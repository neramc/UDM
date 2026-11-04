// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only Linux device discovery via sysfs.
//!
//! This adapter only *reads* world-readable kernel interfaces:
//!
//! * `/sys/block/*` and the sysfs device hierarchy behind it,
//! * `/dev/disk/by-id/*` symlink names (no device node is opened),
//! * `/proc/self/mountinfo` and `/proc/swaps`.
//!
//! It never opens a block device node, so it needs no privileges and cannot
//! cause side effects such as spinning up disks or closing optical trays.
//! See `docs/ADR/0008-linux-discovery-via-sysfs.md` and
//! `docs/PLATFORM_SUPPORT.md` for limitations.

mod tables;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use udm_domain::discovery::{ProbeFailure, ProbeTarget};
use udm_domain::identity::{ObservationSource, classify_serial};
use udm_domain::{
    AttributeKind, AttributeValue, Capability, CapabilityReason, CapabilitySet, CapabilityStatus,
    Confidence, Device, DeviceIdentity, DeviceKey, DeviceState, DiscoveryIssue, DiscoveryIssueKind,
    DiscoverySource, MediaState, Partition, PlatformKind, Provenance, SystemDiskStatus, Transport,
    VolumeUsage, WriteProtection,
};

use crate::{DeviceDiscovery, DiscoveryError, DiscoveryReport};
use tables::{MountTable, parse_mountinfo, parse_swaps};

/// Mount points whose backing device is treated as a system disk.
const SYSTEM_MOUNT_POINTS: [&str; 6] = ["/", "/boot", "/boot/efi", "/efi", "/usr", "/var"];

/// Maximum depth followed through `holders/` (device-mapper, md, ...).
const MAX_HOLDER_DEPTH: usize = 8;

/// The kernel reports `size` in 512-byte units regardless of the logical
/// block size.
const SYSFS_SECTOR: u64 = 512;

/// Locations of the kernel interfaces read by the adapter. Tests point these
/// at a fake tree (see `udm_testkit::fake_sysfs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxPaths {
    pub sys: PathBuf,
    pub dev: PathBuf,
    pub mountinfo: PathBuf,
    pub swaps: PathBuf,
}

impl LinuxPaths {
    /// The real system locations.
    #[must_use]
    pub fn system() -> Self {
        Self {
            sys: PathBuf::from("/sys"),
            dev: PathBuf::from("/dev"),
            mountinfo: PathBuf::from("/proc/self/mountinfo"),
            swaps: PathBuf::from("/proc/swaps"),
        }
    }

    /// Locations below a fake root directory.
    #[must_use]
    pub fn under_root(root: &Path) -> Self {
        Self {
            sys: root.join("sys"),
            dev: root.join("dev"),
            mountinfo: root.join("proc/self/mountinfo"),
            swaps: root.join("proc/swaps"),
        }
    }
}

/// Read-only discovery of physical block devices on Linux.
#[derive(Debug, Clone)]
pub struct LinuxSysfsDiscovery {
    paths: LinuxPaths,
}

impl Default for LinuxSysfsDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSysfsDiscovery {
    /// Discovery against the real `/sys`, `/dev` and `/proc`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_paths(LinuxPaths::system())
    }

    /// Discovery against custom locations (used by tests).
    #[must_use]
    pub const fn with_paths(paths: LinuxPaths) -> Self {
        Self { paths }
    }
}

impl DeviceDiscovery for LinuxSysfsDiscovery {
    fn source(&self) -> DiscoverySource {
        DiscoverySource::System {
            platform: PlatformKind::Linux,
        }
    }

    fn discover(&mut self) -> Result<DiscoveryReport, DiscoveryError> {
        let sys_block = self.paths.sys.join("block");
        let entries = fs::read_dir(&sys_block).map_err(DiscoveryError::from_listing_error)?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(DiscoveryError::from_listing_error)?;
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_owned());
            }
        }
        names.sort();

        let mut issues = Vec::new();
        let context = Context::load(&self.paths, &mut issues);

        let mut devices = Vec::new();
        let mut virtual_count: u32 = 0;
        for name in &names {
            let block_dir = sys_block.join(name);
            // Physical devices have a `device` link to their hardware parent;
            // loop, device-mapper, md, zram, ... do not.
            if !block_dir.join("device").exists() {
                virtual_count = virtual_count.saturating_add(1);
                continue;
            }
            if let Some(device) = context.build_device(name, &block_dir, &mut issues) {
                devices.push(device);
            }
        }

        classify_system_disks(&mut devices, &mut issues);
        if virtual_count > 0 {
            issues.push(DiscoveryIssue::global(
                DiscoveryIssueKind::VirtualDevicesExcluded {
                    count: virtual_count,
                },
            ));
        }
        tracing::debug!(
            devices = devices.len(),
            virtual_excluded = virtual_count,
            issues = issues.len(),
            "linux sysfs discovery finished"
        );
        Ok(DiscoveryReport { devices, issues })
    }
}

/// System-wide tables read once per discovery pass.
struct Context {
    sys_block: PathBuf,
    /// Canonical `sys/devices`; USB ancestors are only searched below it.
    sys_devices: PathBuf,
    mounts: MountTable,
    swaps: HashSet<String>,
    by_id: HashMap<String, Vec<String>>,
}

fn probe_failure(error: &io::Error) -> ProbeFailure {
    match error.kind() {
        io::ErrorKind::PermissionDenied => ProbeFailure::PermissionDenied,
        io::ErrorKind::NotFound => ProbeFailure::Vanished,
        io::ErrorKind::InvalidData => ProbeFailure::Malformed,
        _ => ProbeFailure::Io,
    }
}

/// Reads a sysfs attribute. `Ok(None)` means "not reported" (file absent).
fn read_attr(path: &Path) -> io::Result<Option<String>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).trim().to_owned())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Lists the entry names of a directory; an absent directory is empty.
fn list_names(path: &Path) -> io::Result<Vec<String>> {
    match fs::read_dir(path) {
        Ok(entries) => {
            let mut names = Vec::new();
            for entry in entries {
                if let Some(name) = entry?.file_name().to_str() {
                    names.push(name.to_owned());
                }
            }
            names.sort();
            Ok(names)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Collects per-device attributes and probe failures.
struct DeviceProbe<'a> {
    key: DeviceKey,
    identity: DeviceIdentity,
    issues: &'a mut Vec<DiscoveryIssue>,
}

impl DeviceProbe<'_> {
    fn fail(&mut self, target: ProbeTarget, error: &io::Error) {
        self.issues.push(DiscoveryIssue::for_device(
            &self.key,
            DiscoveryIssueKind::ProbeFailed {
                target,
                failure: probe_failure(error),
            },
        ));
    }

    fn read(&mut self, path: &Path, target: ProbeTarget) -> Option<String> {
        match read_attr(path) {
            Ok(value) => value,
            Err(e) => {
                self.fail(target, &e);
                None
            }
        }
    }

    fn malformed(&mut self, target: ProbeTarget) {
        self.issues.push(DiscoveryIssue::for_device(
            &self.key,
            DiscoveryIssueKind::ProbeFailed {
                target,
                failure: ProbeFailure::Malformed,
            },
        ));
    }

    fn read_u64(&mut self, path: &Path, kind: AttributeKind) -> Option<u64> {
        let text = self.read(path, ProbeTarget::Attribute(kind))?;
        let parsed = text.parse::<u64>().ok();
        if parsed.is_none() {
            self.malformed(ProbeTarget::Attribute(kind));
        }
        parsed
    }

    /// Reads a `0`/`1` sysfs flag.
    fn read_flag(&mut self, path: &Path) -> Option<bool> {
        let text = self.read(path, ProbeTarget::DeviceState)?;
        match text.as_str() {
            "0" => Some(false),
            "1" => Some(true),
            _ => {
                self.malformed(ProbeTarget::DeviceState);
                None
            }
        }
    }

    fn add(
        &mut self,
        kind: AttributeKind,
        value: AttributeValue,
        locator: &str,
        confidence: Confidence,
    ) {
        self.identity.push(udm_domain::IdentityAttribute {
            kind,
            value,
            provenance: Provenance::new(ObservationSource::LinuxSysfs, locator),
            confidence,
        });
    }

    /// Reads a text attribute and records it if non-empty.
    fn text(
        &mut self,
        dir: &Path,
        file: &str,
        kind: AttributeKind,
        locator: &str,
        confidence: Confidence,
    ) {
        if let Some(raw) = self.read(&dir.join(file), ProbeTarget::Attribute(kind))
            && let Some(value) = AttributeValue::text(&raw)
        {
            self.add(kind, value, locator, confidence);
        }
    }

    /// Reads a serial-number attribute, lowering confidence for placeholders.
    fn serial(&mut self, dir: &Path, file: &str, kind: AttributeKind, locator: &str) {
        if let Some(raw) = self.read(&dir.join(file), ProbeTarget::Attribute(kind))
            && let Some((value, confidence)) = classify_serial(&raw, Confidence::Medium)
        {
            self.add(kind, value, locator, confidence);
        }
    }
}

impl Context {
    fn load(paths: &LinuxPaths, issues: &mut Vec<DiscoveryIssue>) -> Self {
        let mounts = match fs::read_to_string(&paths.mountinfo) {
            Ok(text) => {
                let (table, malformed) = parse_mountinfo(&text);
                if malformed > 0 {
                    issues.push(DiscoveryIssue::global(DiscoveryIssueKind::ProbeFailed {
                        target: ProbeTarget::MountTable,
                        failure: ProbeFailure::Malformed,
                    }));
                }
                table
            }
            Err(e) => {
                issues.push(DiscoveryIssue::global(DiscoveryIssueKind::ProbeFailed {
                    target: ProbeTarget::MountTable,
                    failure: probe_failure(&e),
                }));
                MountTable::new()
            }
        };

        let swaps = match fs::read_to_string(&paths.swaps) {
            Ok(text) => parse_swaps(&text)
                .iter()
                .filter_map(|path| swap_kernel_name(&paths.dev, path))
                .collect(),
            Err(e) => {
                issues.push(DiscoveryIssue::global(DiscoveryIssueKind::ProbeFailed {
                    target: ProbeTarget::SwapTable,
                    failure: probe_failure(&e),
                }));
                HashSet::new()
            }
        };

        let by_id = match read_by_id_links(&paths.dev.join("disk/by-id")) {
            Ok(map) => map,
            Err(e) => {
                issues.push(DiscoveryIssue::global(DiscoveryIssueKind::ProbeFailed {
                    target: ProbeTarget::PersistentLinks,
                    failure: probe_failure(&e),
                }));
                HashMap::new()
            }
        };

        let sys_devices = paths.sys.join("devices");
        Self {
            sys_block: paths.sys.join("block"),
            sys_devices: fs::canonicalize(&sys_devices).unwrap_or(sys_devices),
            mounts,
            swaps,
            by_id,
        }
    }

    fn build_device(
        &self,
        name: &str,
        block_dir: &Path,
        issues: &mut Vec<DiscoveryIssue>,
    ) -> Option<Device> {
        let key = DeviceKey::new(format!("linux:{name}"));
        let canonical = match fs::canonicalize(block_dir) {
            Ok(path) => path,
            Err(e) => {
                issues.push(DiscoveryIssue::for_device(
                    &key,
                    DiscoveryIssueKind::ProbeFailed {
                        target: ProbeTarget::Attribute(AttributeKind::OsDeviceName),
                        failure: probe_failure(&e),
                    },
                ));
                return None;
            }
        };
        let hardware = fs::canonicalize(block_dir.join("device")).ok();

        let mut probe = DeviceProbe {
            key: key.clone(),
            identity: DeviceIdentity::new(),
            issues,
        };

        // Kernel-reported geometry: high confidence.
        let size_sectors = match read_attr(&block_dir.join("size")) {
            Ok(Some(text)) => {
                let parsed = text.parse::<u64>().ok();
                if parsed.is_none() {
                    probe.malformed(ProbeTarget::Attribute(AttributeKind::CapacityBytes));
                }
                parsed
            }
            Ok(None) => {
                // The device directory lost its `size`: it vanished mid-scan.
                probe.fail(
                    ProbeTarget::Attribute(AttributeKind::CapacityBytes),
                    &io::Error::from(io::ErrorKind::NotFound),
                );
                return None;
            }
            Err(e) => {
                probe.fail(ProbeTarget::Attribute(AttributeKind::CapacityBytes), &e);
                None
            }
        };
        if let Some(capacity) = size_sectors.and_then(|s| s.checked_mul(SYSFS_SECTOR)) {
            probe.add(
                AttributeKind::CapacityBytes,
                AttributeValue::Number(capacity),
                "size",
                Confidence::High,
            );
        }
        for (file, kind) in [
            ("queue/logical_block_size", AttributeKind::LogicalBlockSize),
            (
                "queue/physical_block_size",
                AttributeKind::PhysicalBlockSize,
            ),
        ] {
            if let Some(value) = probe.read_u64(&block_dir.join(file), kind) {
                probe.add(kind, AttributeValue::Number(value), file, Confidence::High);
            }
        }

        // Volatile OS bindings.
        probe.add(
            AttributeKind::OsDeviceName,
            AttributeValue::Text(name.to_owned()),
            "block",
            Confidence::High,
        );
        let devnum = probe.read(
            &block_dir.join("dev"),
            ProbeTarget::Attribute(AttributeKind::OsDeviceNumber),
        );
        if let Some(devnum) = devnum.as_deref().and_then(AttributeValue::text) {
            probe.add(
                AttributeKind::OsDeviceNumber,
                devnum,
                "dev",
                Confidence::High,
            );
        }

        // Device- or bridge-reported strings: medium confidence.
        if let Some(hw) = &hardware {
            probe.text(
                hw,
                "vendor",
                AttributeKind::Vendor,
                "device/vendor",
                Confidence::Medium,
            );
            probe.text(
                hw,
                "model",
                AttributeKind::Model,
                "device/model",
                Confidence::Medium,
            );
            if read_attr(&hw.join("model")).ok().flatten().is_none() {
                // MMC/SD cards expose their product name as `name`.
                probe.text(
                    hw,
                    "name",
                    AttributeKind::Model,
                    "device/name",
                    Confidence::Medium,
                );
            }
            probe.text(
                hw,
                "rev",
                AttributeKind::Revision,
                "device/rev",
                Confidence::Medium,
            );
            probe.text(
                hw,
                "firmware_rev",
                AttributeKind::Revision,
                "device/firmware_rev",
                Confidence::Medium,
            );
            probe.serial(hw, "serial", AttributeKind::SerialNumber, "device/serial");
        }
        // virtio-blk exposes the serial on the block device itself.
        probe.serial(block_dir, "serial", AttributeKind::SerialNumber, "serial");
        for (dir, locator) in [
            (Some(block_dir.to_path_buf()), "wwid"),
            (hardware.clone(), "device/wwid"),
        ] {
            if let Some(dir) = dir
                && let Some(raw) = probe.read(
                    &dir.join("wwid"),
                    ProbeTarget::Attribute(AttributeKind::WorldWideName),
                )
                && let Some(value) = AttributeValue::text(&raw)
            {
                let confidence = if raw.starts_with("naa.") || raw.starts_with("eui.") {
                    Confidence::High
                } else {
                    // `t10.` and kernel-synthesized `nvme.` identifiers embed
                    // vendor-controlled strings.
                    Confidence::Medium
                };
                probe.add(AttributeKind::WorldWideName, value, locator, confidence);
            }
        }

        // USB descriptor of the nearest USB ancestor.
        if let Some(hw) = &hardware
            && let Some(usb_dir) = hw
                .ancestors()
                .take_while(|dir| dir.starts_with(&self.sys_devices))
                .find(|dir| dir.join("idVendor").is_file())
        {
            for (file, kind, confidence) in [
                ("idVendor", AttributeKind::UsbVendorId, Confidence::High),
                ("idProduct", AttributeKind::UsbProductId, Confidence::High),
                (
                    "manufacturer",
                    AttributeKind::UsbManufacturer,
                    Confidence::Medium,
                ),
                ("product", AttributeKind::UsbProduct, Confidence::Medium),
            ] {
                if let Some(raw) = probe.read(&usb_dir.join(file), ProbeTarget::UsbDescriptor)
                    && let Some(value) = AttributeValue::text(&raw)
                {
                    probe.add(kind, value, &format!("usb/{file}"), confidence);
                }
            }
            if let Some(raw) = probe.read(&usb_dir.join("serial"), ProbeTarget::UsbDescriptor)
                && let Some((value, confidence)) = classify_serial(&raw, Confidence::Medium)
            {
                probe.add(
                    AttributeKind::UsbSerialNumber,
                    value,
                    "usb/serial",
                    confidence,
                );
            }
        }

        // udev persistent links (names only; nothing is opened).
        for link in self.by_id.get(name).into_iter().flatten() {
            probe.identity.push(udm_domain::IdentityAttribute {
                kind: AttributeKind::OsPersistentLink,
                value: AttributeValue::Text(link.clone()),
                provenance: Provenance::new(ObservationSource::LinuxUdevLink, "disk/by-id"),
                confidence: Confidence::Medium,
            });
        }

        let read_only = probe.read_flag(&block_dir.join("ro"));
        let removable = probe.read_flag(&block_dir.join("removable"));
        let identity = probe.identity;

        let write_protection = match read_only {
            Some(false) => WriteProtection::Writable,
            Some(true) => WriteProtection::ReadOnly,
            None => WriteProtection::Unknown,
        };
        let media = match (size_sectors, removable) {
            (Some(0), Some(true)) => MediaState::NoMedia,
            (Some(0) | None, _) => MediaState::Unknown,
            (Some(_), _) => MediaState::Present,
        };
        let state = DeviceState {
            media,
            write_protection,
            removable,
            system_disk: SystemDiskStatus::Unknown,
        };

        let partitions = self.partitions(&key, block_dir, issues);
        let whole_disk_usage = self.usage(&key, block_dir, name, 0, issues);

        Some(Device {
            capabilities: phase0_capabilities(&state),
            key,
            simulated: false,
            transport: classify_transport(&canonical),
            identity,
            state,
            partitions,
            whole_disk_usage,
        })
    }

    fn partitions(
        &self,
        key: &DeviceKey,
        block_dir: &Path,
        issues: &mut Vec<DiscoveryIssue>,
    ) -> Vec<Partition> {
        let names = match list_names(block_dir) {
            Ok(names) => names,
            Err(e) => {
                issues.push(DiscoveryIssue::for_device(
                    key,
                    DiscoveryIssueKind::ProbeFailed {
                        target: ProbeTarget::Partitions,
                        failure: probe_failure(&e),
                    },
                ));
                return Vec::new();
            }
        };
        let mut partitions = Vec::new();
        for name in names {
            let dir = block_dir.join(&name);
            // Partitions are subdirectories containing a `partition` file. The
            // block directory also holds plain attribute files (`size`, `ro`,
            // ...) and unrelated subdirectories, which must be skipped without
            // reporting errors.
            if !dir.is_dir() {
                continue;
            }
            let number = match read_attr(&dir.join("partition")) {
                Ok(Some(text)) => text.parse::<u32>().ok(),
                Ok(None) => continue, // not a partition directory
                Err(e) => {
                    issues.push(DiscoveryIssue::for_device(
                        key,
                        DiscoveryIssueKind::ProbeFailed {
                            target: ProbeTarget::Partitions,
                            failure: probe_failure(&e),
                        },
                    ));
                    continue;
                }
            };
            let sectors = |file: &str| {
                read_attr(&dir.join(file))
                    .ok()
                    .flatten()
                    .and_then(|t| t.parse::<u64>().ok())
                    .and_then(|s| s.checked_mul(SYSFS_SECTOR))
            };
            partitions.push(Partition {
                number,
                start_bytes: sectors("start"),
                size_bytes: sectors("size"),
                usage: self.usage(key, &dir, &name, 0, issues),
                os_name: Some(name),
            });
        }
        partitions.sort_by_key(|p| p.number);
        partitions
    }

    /// Usage of a block node (disk or partition), following `holders/`.
    fn usage(
        &self,
        key: &DeviceKey,
        node_dir: &Path,
        name: &str,
        depth: usize,
        issues: &mut Vec<DiscoveryIssue>,
    ) -> Vec<VolumeUsage> {
        let mut usage = Vec::new();
        let via = (depth > 0).then(|| name.to_owned());
        if let Ok(Some(devnum)) = read_attr(&node_dir.join("dev")) {
            for entry in self.mounts.get(&devnum).into_iter().flatten() {
                usage.push(VolumeUsage::Mounted {
                    mount_point: entry.mount_point.clone(),
                    filesystem: Some(entry.filesystem.clone()),
                    via: via.clone(),
                });
            }
        }
        if self.swaps.contains(name) {
            usage.push(VolumeUsage::Swap { via: via.clone() });
        }
        let holders = match list_names(&node_dir.join("holders")) {
            Ok(holders) => holders,
            Err(e) => {
                issues.push(DiscoveryIssue::for_device(
                    key,
                    DiscoveryIssueKind::ProbeFailed {
                        target: ProbeTarget::HolderTopology,
                        failure: probe_failure(&e),
                    },
                ));
                Vec::new()
            }
        };
        for holder in holders {
            let nested = if depth + 1 >= MAX_HOLDER_DEPTH {
                Vec::new()
            } else {
                self.usage(
                    key,
                    &self.sys_block.join(&holder),
                    &holder,
                    depth + 1,
                    issues,
                )
            };
            if nested.is_empty() {
                usage.push(VolumeUsage::Held { holder });
            } else {
                usage.extend(nested);
            }
        }
        usage
    }
}

/// Maps a `/proc/swaps` path to a kernel block device name, if it is one.
fn swap_kernel_name(dev_root: &Path, path: &str) -> Option<String> {
    let relative = path.strip_prefix("/dev/")?;
    if let Ok(resolved) = fs::canonicalize(dev_root.join(relative))
        && let Some(name) = resolved.file_name().and_then(|n| n.to_str())
    {
        return Some(name.to_owned());
    }
    Path::new(relative)
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
}

/// Reads `/dev/disk/by-id` link names, grouped by the kernel name they point to.
fn read_by_id_links(dir: &Path) -> io::Result<HashMap<String, Vec<String>>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for link in list_names(dir)? {
        let Ok(target) = fs::read_link(dir.join(&link)) else {
            continue;
        };
        if let Some(kernel_name) = target.file_name().and_then(|n| n.to_str()) {
            map.entry(kernel_name.to_owned()).or_default().push(link);
        }
    }
    Ok(map)
}

/// Infers the transport from the canonical sysfs path. USB is checked first
/// because USB storage also has SCSI `host*` components.
fn classify_transport(canonical: &Path) -> Transport {
    let components: Vec<String> = canonical
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => s.to_str().map(str::to_owned),
            _ => None,
        })
        .collect();
    let any = |prefix: &str| components.iter().any(|c| c.starts_with(prefix));
    if any("usb") {
        Transport::Usb
    } else if any("nvme") {
        Transport::Nvme
    } else if any("mmc_host") || any("mmc") {
        Transport::Mmc
    } else if any("virtio") {
        Transport::Virtio
    } else if any("ata") {
        Transport::Ata
    } else if any("host") && any("target") {
        Transport::Scsi
    } else {
        Transport::Unknown
    }
}

/// Capability assessment for real devices in Phase 0.
///
/// Phase 0 implements no device operation, so every capability is
/// `Unavailable(NotImplementedInThisBuild)` unless a device-level fact gives a
/// more specific reason (no media, write protection).
fn phase0_capabilities(state: &DeviceState) -> CapabilitySet {
    let mut set = CapabilitySet::unassessed();
    for capability in Capability::ALL {
        let reason = if state.media == MediaState::NoMedia && capability != Capability::SafeEject {
            CapabilityReason::NoMedia
        } else if state.write_protection == WriteProtection::ReadOnly && capability.is_destructive()
        {
            CapabilityReason::WriteProtected
        } else {
            CapabilityReason::NotImplementedInThisBuild
        };
        set.set(capability, CapabilityStatus::Unavailable(reason));
    }
    set
}

/// Marks system disks. If the root filesystem cannot be traced to any
/// discovered device (container, network root, unsupported stacking), no
/// device is declared "not a system disk".
fn classify_system_disks(devices: &mut [Device], issues: &mut Vec<DiscoveryIssue>) {
    let is_system_usage = |usage: &VolumeUsage| match usage {
        VolumeUsage::Mounted { mount_point, .. } => {
            SYSTEM_MOUNT_POINTS.contains(&mount_point.as_str())
        }
        VolumeUsage::Swap { .. } => true,
        VolumeUsage::Held { .. } => false,
    };
    let root_found = devices.iter().any(|d| {
        d.all_usage()
            .any(|u| matches!(u, VolumeUsage::Mounted { mount_point, .. } if mount_point == "/"))
    });
    for device in devices.iter_mut() {
        device.state.system_disk = if device.all_usage().any(is_system_usage) {
            SystemDiskStatus::SystemDisk
        } else if root_found {
            SystemDiskStatus::NotSystemDisk
        } else {
            SystemDiskStatus::Unknown
        };
    }
    if !root_found {
        issues.push(DiscoveryIssue::global(
            DiscoveryIssueKind::SystemDiskUndetermined,
        ));
    }
}

#[cfg(test)]
mod tests;
