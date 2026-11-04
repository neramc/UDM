// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Devices, partitions and observed device state.
//!
//! Three kinds of status are kept deliberately separate (CLAUDE.md §7):
//!
//! * **Device state** ([`DeviceState`]): what was observed about the device
//!   (media present, write protection, system disk, usage).
//! * **Capabilities** ([`crate::CapabilitySet`]): what the application could do.
//! * **Operation status**: belongs to jobs, which do not exist in Phase 0.

use std::fmt;

use crate::capability::CapabilitySet;
use crate::identity::{AttributeKind, DeviceIdentity};

/// A handle that refers to a device *within one discovery snapshot*.
///
/// It is used for UI selection and for correlating issues with devices.
/// It is **not** an identity: two snapshots may use the same key for different
/// devices (e.g. `linux:sdb` after a replug). Use
/// [`DeviceIdentity::compare`] to decide whether two observations describe the
/// same device.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceKey(String);

impl DeviceKey {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// How the device is attached, as far as it can be determined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    Usb,
    Nvme,
    Ata,
    Scsi,
    /// SD / MMC card on a native (non-USB) host controller.
    Mmc,
    Virtio,
    Unknown,
}

impl Transport {
    /// A stable, non-localized identifier for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Usb => "usb",
            Self::Nvme => "nvme",
            Self::Ata => "ata",
            Self::Scsi => "scsi",
            Self::Mmc => "mmc",
            Self::Virtio => "virtio",
            Self::Unknown => "unknown",
        }
    }
}

/// Whether readable media is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaState {
    Present,
    /// e.g. a card reader without a card. Capacity is reported as zero.
    NoMedia,
    Unknown,
}

/// Write protection as reported by the OS / device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriteProtection {
    Writable,
    ReadOnly,
    Unknown,
}

/// Whether the device holds (part of) the running operating system.
///
/// `Unknown` must be treated like `SystemDisk` by any future destructive
/// workflow: when in doubt, protect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemDiskStatus {
    SystemDisk,
    NotSystemDisk,
    Unknown,
}

/// Observed state of a device. Unknown values stay unknown; adapters must not
/// guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceState {
    pub media: MediaState,
    pub write_protection: WriteProtection,
    /// `None` when the OS does not report it. Note that many USB hard drives
    /// report themselves as non-removable.
    pub removable: Option<bool>,
    pub system_disk: SystemDiskStatus,
}

impl DeviceState {
    /// A state in which nothing is known.
    #[must_use]
    pub const fn unknown() -> Self {
        Self {
            media: MediaState::Unknown,
            write_protection: WriteProtection::Unknown,
            removable: None,
            system_disk: SystemDiskStatus::Unknown,
        }
    }
}

/// How a volume (partition or whole device) is currently in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeUsage {
    /// Mounted, possibly through a stacked device (`via`, e.g. `dm-0`).
    Mounted {
        mount_point: String,
        filesystem: Option<String>,
        via: Option<String>,
    },
    /// Active swap, possibly through a stacked device.
    Swap { via: Option<String> },
    /// Claimed by another block device (device-mapper, RAID, ...) whose own
    /// usage is unknown or not mounted.
    Held { holder: String },
}

/// A partition as observed by the OS. Filesystem contents are not inspected in
/// Phase 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    /// Partition number as reported by the OS.
    pub number: Option<u32>,
    /// OS name (e.g. `sdb1`). Volatile.
    pub os_name: Option<String>,
    pub start_bytes: Option<u64>,
    pub size_bytes: Option<u64>,
    pub usage: Vec<VolumeUsage>,
}

/// One discovered storage device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub key: DeviceKey,
    /// `true` for devices produced by the simulator. The UI must label them.
    pub simulated: bool,
    pub transport: Transport,
    pub identity: DeviceIdentity,
    pub state: DeviceState,
    pub partitions: Vec<Partition>,
    /// Usage of the whole device (filesystem without a partition table, or
    /// stacked devices built directly on the disk).
    pub whole_disk_usage: Vec<VolumeUsage>,
    pub capabilities: CapabilitySet,
}

impl Device {
    /// Reported capacity in bytes, if observed.
    #[must_use]
    pub fn capacity_bytes(&self) -> Option<u64> {
        self.identity.number(AttributeKind::CapacityBytes)
    }

    /// Whether the device or any of its partitions is mounted, used as swap or
    /// held by another block device.
    #[must_use]
    pub fn is_in_use(&self) -> bool {
        !self.whole_disk_usage.is_empty() || self.partitions.iter().any(|p| !p.usage.is_empty())
    }

    /// Every usage of the device and its partitions.
    pub fn all_usage(&self) -> impl Iterator<Item = &VolumeUsage> {
        self.whole_disk_usage
            .iter()
            .chain(self.partitions.iter().flat_map(|p| p.usage.iter()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{AttributeValue, Confidence, Provenance};

    fn device() -> Device {
        Device {
            key: DeviceKey::new("sim:test"),
            simulated: true,
            transport: Transport::Usb,
            identity: DeviceIdentity::new().with(
                AttributeKind::CapacityBytes,
                AttributeValue::Number(1024),
                Provenance::simulated(),
                Confidence::High,
            ),
            state: DeviceState::unknown(),
            partitions: vec![Partition {
                number: Some(1),
                os_name: None,
                start_bytes: Some(512),
                size_bytes: Some(512),
                usage: vec![],
            }],
            whole_disk_usage: vec![],
            capabilities: CapabilitySet::unassessed(),
        }
    }

    #[test]
    fn capacity_comes_from_identity() {
        assert_eq!(device().capacity_bytes(), Some(1024));
    }

    #[test]
    fn usage_includes_partitions_and_whole_disk() {
        let mut d = device();
        assert!(!d.is_in_use());
        d.partitions[0].usage.push(VolumeUsage::Swap { via: None });
        assert!(d.is_in_use());
        d.whole_disk_usage.push(VolumeUsage::Held {
            holder: "dm-0".into(),
        });
        assert_eq!(d.all_usage().count(), 2);
    }

    #[test]
    fn unknown_state_does_not_guess() {
        let s = DeviceState::unknown();
        assert_eq!(s.media, MediaState::Unknown);
        assert_eq!(s.write_protection, WriteProtection::Unknown);
        assert_eq!(s.removable, None);
        assert_eq!(s.system_disk, SystemDiskStatus::Unknown);
    }
}
