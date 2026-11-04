// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Simulated device discovery.
//!
//! The simulator produces devices that **do not exist**. Every device it
//! returns has `simulated == true` and [`ObservationSource::Simulated`]
//! provenance, and the UI must label it accordingly.
//!
//! Capability statuses of simulated devices are *illustrative*: they show how
//! the capability model represents typical hardware situations (empty card
//! reader, write-protected card, USB bridge without SMART pass-through, ...).
//! They do not describe real hardware and do not mean that this build can
//! perform the operation.
//!
//! Vendor and product names are fictional.

use std::thread;
use std::time::Duration;

use udm_domain::identity::ObservationSource;
use udm_domain::{
    AttributeKind, AttributeValue, Capability, CapabilityReason, CapabilitySet, CapabilityStatus,
    Confidence, Device, DeviceIdentity, DeviceKey, DeviceState, DiscoverySource, MediaState,
    Partition, Provenance, SystemDiskStatus, Transport, VolumeUsage, WriteProtection,
};

use crate::{DeviceDiscovery, DiscoveryError, DiscoveryReport};

/// Built-in simulated scenarios.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scenario {
    /// A varied set of devices covering the main capability and state cases.
    SampleDevices,
    /// Discovery succeeds but finds nothing.
    NoDevices,
    /// Discovery fails as a whole.
    DiscoveryFailure,
    /// Devices appear and disappear across successive discovery passes.
    Hotplug,
}

impl Scenario {
    pub const ALL: [Self; 4] = [
        Self::SampleDevices,
        Self::NoDevices,
        Self::DiscoveryFailure,
        Self::Hotplug,
    ];

    /// Stable identifier used in [`DiscoverySource::Simulated`].
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::SampleDevices => "sample-devices",
            Self::NoDevices => "no-devices",
            Self::DiscoveryFailure => "discovery-failure",
            Self::Hotplug => "hotplug",
        }
    }
}

/// Discovery adapter that returns simulated devices.
#[derive(Debug, Clone)]
pub struct SimulatedDiscovery {
    scenario: Scenario,
    passes: u32,
    latency: Duration,
}

impl SimulatedDiscovery {
    #[must_use]
    pub const fn new(scenario: Scenario) -> Self {
        Self {
            scenario,
            passes: 0,
            latency: Duration::ZERO,
        }
    }

    /// Adds an artificial delay to every pass, to exercise loading states and
    /// to verify that discovery never runs on the GUI thread.
    #[must_use]
    pub const fn with_latency(mut self, latency: Duration) -> Self {
        self.latency = latency;
        self
    }

    #[must_use]
    pub const fn scenario(&self) -> Scenario {
        self.scenario
    }
}

impl DeviceDiscovery for SimulatedDiscovery {
    fn source(&self) -> DiscoverySource {
        DiscoverySource::Simulated {
            scenario: self.scenario.id().to_owned(),
        }
    }

    fn discover(&mut self) -> Result<DiscoveryReport, DiscoveryError> {
        if !self.latency.is_zero() {
            thread::sleep(self.latency);
        }
        let pass = self.passes;
        self.passes = self.passes.saturating_add(1);
        let devices = match self.scenario {
            Scenario::SampleDevices => sample_devices(),
            Scenario::NoDevices => Vec::new(),
            Scenario::DiscoveryFailure => return Err(DiscoveryError::Simulated),
            Scenario::Hotplug => match pass {
                0 => vec![usb_stick("usb-stick-a", "0123456789ABCDEF", true)],
                1 => vec![
                    usb_stick("usb-stick-a", "0123456789ABCDEF", true),
                    nvme_enclosure(),
                ],
                _ => vec![nvme_enclosure()],
            },
        };
        Ok(DiscoveryReport {
            devices,
            issues: Vec::new(),
        })
    }
}

/// The devices of [`Scenario::SampleDevices`].
#[must_use]
pub fn sample_devices() -> Vec<Device> {
    vec![
        internal_ssd(),
        usb_stick("usb-stick-a", "0123456789ABCDEF", true),
        usb_stick("usb-stick-b", "0123456789ABCDEF", false),
        nvme_enclosure(),
        empty_card_reader(),
        locked_sd_card(),
        unidentified_device(),
    ]
}

const GB: u64 = 1_000_000_000;

fn text(s: &str) -> AttributeValue {
    AttributeValue::Text(s.to_owned())
}

struct IdentityBuilder(DeviceIdentity);

impl IdentityBuilder {
    fn new() -> Self {
        Self(DeviceIdentity::new())
    }

    fn text(self, kind: AttributeKind, value: &str, confidence: Confidence) -> Self {
        Self(
            self.0
                .with(kind, text(value), Provenance::simulated(), confidence),
        )
    }

    fn number(self, kind: AttributeKind, value: u64, confidence: Confidence) -> Self {
        Self(self.0.with(
            kind,
            AttributeValue::Number(value),
            Provenance::simulated(),
            confidence,
        ))
    }

    fn capacity(self, bytes: u64, block: u64) -> Self {
        self.number(AttributeKind::CapacityBytes, bytes, Confidence::High)
            .number(AttributeKind::LogicalBlockSize, block, Confidence::High)
    }

    fn build(self) -> DeviceIdentity {
        debug_assert!(
            self.0
                .attributes()
                .iter()
                .all(|a| a.provenance.source == ObservationSource::Simulated)
        );
        self.0
    }
}

fn device(
    key: &str,
    transport: Transport,
    identity: DeviceIdentity,
    state: DeviceState,
    partitions: Vec<Partition>,
    capabilities: CapabilitySet,
) -> Device {
    Device {
        key: DeviceKey::new(format!("sim:{key}")),
        simulated: true,
        transport,
        identity,
        state,
        partitions,
        whole_disk_usage: Vec::new(),
        capabilities,
    }
}

fn partition(number: u32, start: u64, size: u64, usage: Vec<VolumeUsage>) -> Partition {
    Partition {
        number: Some(number),
        os_name: None,
        start_bytes: Some(start),
        size_bytes: Some(size),
        usage,
    }
}

fn mounted(mount_point: &str, filesystem: &str) -> VolumeUsage {
    VolumeUsage::Mounted {
        mount_point: mount_point.to_owned(),
        filesystem: Some(filesystem.to_owned()),
        via: None,
    }
}

fn present(write_protection: WriteProtection, removable: bool) -> DeviceState {
    DeviceState {
        media: MediaState::Present,
        write_protection,
        removable: Some(removable),
        system_disk: SystemDiskStatus::NotSystemDisk,
    }
}

fn internal_ssd() -> Device {
    let capacity = 512_110_190_592;
    let identity = IdentityBuilder::new()
        .text(AttributeKind::Vendor, "ATA", Confidence::Medium)
        .text(AttributeKind::Model, "Lumen SSD 512", Confidence::Medium)
        .text(
            AttributeKind::SerialNumber,
            "LMN512A0000042",
            Confidence::Medium,
        )
        .text(
            AttributeKind::WorldWideName,
            "naa.5000000000000042",
            Confidence::High,
        )
        .capacity(capacity, 512)
        .build();
    let mut state = present(WriteProtection::Writable, false);
    state.system_disk = SystemDiskStatus::SystemDisk;
    let partitions = vec![
        partition(1, 1 << 20, 536_870_912, vec![mounted("/boot/efi", "vfat")]),
        partition(
            2,
            537_919_488,
            8 * GB,
            vec![VolumeUsage::Swap { via: None }],
        ),
        partition(
            3,
            8_537_919_488,
            capacity - 8_537_919_488 - (1 << 20),
            vec![mounted("/", "ext4")],
        ),
    ];
    let capabilities = CapabilitySet::unassessed()
        .with(Capability::RawRead, CapabilityStatus::Supported)
        .with(Capability::RawWrite, CapabilityStatus::Supported)
        .with(
            Capability::SafeEject,
            CapabilityStatus::Unavailable(CapabilityReason::TransportUnsupported),
        )
        .with(
            Capability::SmartAccess,
            CapabilityStatus::Limited(CapabilityReason::RequiresPrivilegedHelper),
        )
        .with(Capability::PartitionEdit, CapabilityStatus::Supported)
        .with(
            Capability::SecureErase,
            CapabilityStatus::NotTested(CapabilityReason::NotAssessed),
        )
        .with(Capability::FilesystemCheck, CapabilityStatus::Supported);
    device(
        "internal-ssd",
        Transport::Ata,
        identity,
        state,
        partitions,
        capabilities,
    )
}

fn usb_stick(key: &str, serial: &str, is_mounted: bool) -> Device {
    let capacity = 32_015_679_488;
    let identity = IdentityBuilder::new()
        .text(AttributeKind::Vendor, "Acme", Confidence::Medium)
        .text(AttributeKind::Model, "FlashLine 32", Confidence::Medium)
        .text(AttributeKind::UsbVendorId, "f055", Confidence::High)
        .text(AttributeKind::UsbProductId, "0032", Confidence::High)
        .text(AttributeKind::UsbSerialNumber, serial, Confidence::Medium)
        .capacity(capacity, 512)
        .build();
    let usage = if is_mounted {
        vec![mounted("/media/user/ACME", "exfat")]
    } else {
        Vec::new()
    };
    let partitions = vec![partition(1, 1 << 20, capacity - (1 << 20), usage)];
    let capabilities = CapabilitySet::unassessed()
        .with(Capability::RawRead, CapabilityStatus::Supported)
        .with(Capability::RawWrite, CapabilityStatus::Supported)
        .with(Capability::SafeEject, CapabilityStatus::Supported)
        .with(
            Capability::SmartAccess,
            CapabilityStatus::Unavailable(CapabilityReason::TransportUnsupported),
        )
        .with(
            Capability::SecureErase,
            CapabilityStatus::Limited(CapabilityReason::FlashErasureNotGuaranteed),
        );
    device(
        key,
        Transport::Usb,
        identity,
        present(WriteProtection::Writable, true),
        partitions,
        capabilities,
    )
}

fn nvme_enclosure() -> Device {
    let capacity = 1_000_204_886_016;
    let identity = IdentityBuilder::new()
        .text(AttributeKind::Vendor, "Orbit", Confidence::Medium)
        .text(AttributeKind::Model, "NVMe Enclosure", Confidence::Medium)
        .text(AttributeKind::UsbVendorId, "f0ab", Confidence::High)
        .text(AttributeKind::UsbProductId, "9210", Confidence::High)
        .text(
            AttributeKind::UsbSerialNumber,
            "ORB9210-77D1C3",
            Confidence::Medium,
        )
        .capacity(capacity, 512)
        .build();
    let partitions = vec![
        partition(1, 1 << 20, 200 * GB, Vec::new()),
        partition(2, 200 * GB + (1 << 20), 700 * GB, Vec::new()),
    ];
    let capabilities = CapabilitySet::unassessed()
        .with(
            Capability::RawRead,
            CapabilityStatus::Limited(CapabilityReason::RequiresPrivilegedHelper),
        )
        .with(
            Capability::RawWrite,
            CapabilityStatus::Limited(CapabilityReason::RequiresPrivilegedHelper),
        )
        .with(Capability::SafeEject, CapabilityStatus::Supported)
        .with(
            Capability::SmartAccess,
            CapabilityStatus::NotTested(CapabilityReason::BridgePassthroughUnverified),
        );
    device(
        "nvme-enclosure",
        Transport::Usb,
        identity,
        present(WriteProtection::Writable, false),
        partitions,
        capabilities,
    )
}

fn empty_card_reader() -> Device {
    let identity = IdentityBuilder::new()
        .text(AttributeKind::Vendor, "Kestrel", Confidence::Medium)
        .text(
            AttributeKind::Model,
            "Multi-Card Reader",
            Confidence::Medium,
        )
        .text(AttributeKind::UsbVendorId, "f0c1", Confidence::High)
        .text(AttributeKind::UsbProductId, "0400", Confidence::High)
        .text(
            AttributeKind::UsbSerialNumber,
            "KST-0400-1187",
            Confidence::Medium,
        )
        .capacity(0, 512)
        .build();
    let state = DeviceState {
        media: MediaState::NoMedia,
        write_protection: WriteProtection::Unknown,
        removable: Some(true),
        system_disk: SystemDiskStatus::NotSystemDisk,
    };
    let no_media = CapabilityStatus::Unavailable(CapabilityReason::NoMedia);
    let mut capabilities = CapabilitySet::unassessed();
    for capability in Capability::ALL {
        capabilities.set(capability, no_media);
    }
    capabilities.set(Capability::SafeEject, CapabilityStatus::Supported);
    device(
        "card-reader-empty",
        Transport::Usb,
        identity,
        state,
        Vec::new(),
        capabilities,
    )
}

fn locked_sd_card() -> Device {
    let capacity = 63_864_569_856;
    let identity = IdentityBuilder::new()
        .text(AttributeKind::Vendor, "Kestrel", Confidence::Medium)
        .text(AttributeKind::Model, "SD Reader", Confidence::Medium)
        .text(AttributeKind::UsbVendorId, "f0c1", Confidence::High)
        .text(AttributeKind::UsbProductId, "0410", Confidence::High)
        .text(
            AttributeKind::UsbSerialNumber,
            "KST-0410-5530",
            Confidence::Medium,
        )
        .capacity(capacity, 512)
        .build();
    let partitions = vec![partition(
        1,
        4 << 20,
        capacity - (4 << 20),
        vec![mounted("/media/user/CAMERA", "exfat")],
    )];
    let mut capabilities = CapabilitySet::unassessed()
        .with(Capability::RawRead, CapabilityStatus::Supported)
        .with(Capability::RawWrite, CapabilityStatus::Supported)
        .with(Capability::PartitionEdit, CapabilityStatus::Supported)
        .with(
            Capability::SecureErase,
            CapabilityStatus::Limited(CapabilityReason::FlashErasureNotGuaranteed),
        )
        .with(Capability::SafeEject, CapabilityStatus::Supported)
        .with(
            Capability::SmartAccess,
            CapabilityStatus::Unavailable(CapabilityReason::TransportUnsupported),
        );
    capabilities.restrict_destructive(CapabilityReason::WriteProtected);
    device(
        "sd-card-locked",
        Transport::Usb,
        identity,
        present(WriteProtection::ReadOnly, true),
        partitions,
        capabilities,
    )
}

fn unidentified_device() -> Device {
    let identity = IdentityBuilder::new().capacity(8_053_063_680, 512).build();
    let capabilities = CapabilitySet::unassessed().with(
        Capability::RawRead,
        CapabilityStatus::Unavailable(CapabilityReason::PermissionDenied),
    );
    device(
        "unidentified",
        Transport::Unknown,
        identity,
        DeviceState {
            media: MediaState::Present,
            write_protection: WriteProtection::Unknown,
            removable: None,
            system_disk: SystemDiskStatus::Unknown,
        },
        Vec::new(),
        capabilities,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use udm_domain::{DiscoveryIssueKind, assess_identities};

    #[test]
    fn every_simulated_device_is_labelled_and_has_simulated_provenance() {
        for device in sample_devices() {
            assert!(device.simulated, "{}", device.key);
            assert!(device.key.as_str().starts_with("sim:"));
            for attribute in device.identity.attributes() {
                assert_eq!(attribute.provenance.source, ObservationSource::Simulated);
            }
        }
    }

    #[test]
    fn device_keys_are_unique() {
        let devices = sample_devices();
        let mut keys: Vec<_> = devices.iter().map(|d| d.key.clone()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), devices.len());
    }

    #[test]
    fn sample_covers_all_four_capability_states() {
        let statuses: Vec<_> = sample_devices()
            .iter()
            .flat_map(|d| {
                d.capabilities
                    .iter()
                    .map(|(_, s)| s.as_str())
                    .collect::<Vec<_>>()
            })
            .collect();
        for expected in ["supported", "limited", "unavailable", "not_tested"] {
            assert!(statuses.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn sample_contains_duplicate_serials_and_a_device_without_identity() {
        let mut devices = sample_devices();
        let issues = assess_identities(&mut devices);
        assert!(issues.iter().any(|i| matches!(
            &i.kind,
            DiscoveryIssueKind::DuplicateIdentifier { devices, .. } if devices.len() == 2
        )));
        assert!(issues.iter().any(|i| {
            i.kind == DiscoveryIssueKind::NoStrongIdentifier
                && i.device == Some(DeviceKey::new("sim:unidentified"))
        }));
    }

    #[test]
    fn write_protected_card_has_no_destructive_capability() {
        let card = locked_sd_card();
        for capability in Capability::ALL.into_iter().filter(|c| c.is_destructive()) {
            assert_eq!(
                card.capabilities.get(capability),
                CapabilityStatus::Unavailable(CapabilityReason::WriteProtected)
            );
        }
    }

    #[test]
    fn empty_reader_reports_no_media_and_zero_capacity() {
        let reader = empty_card_reader();
        assert_eq!(reader.state.media, MediaState::NoMedia);
        assert_eq!(reader.capacity_bytes(), Some(0));
        assert_eq!(
            reader.capabilities.get(Capability::RawRead),
            CapabilityStatus::Unavailable(CapabilityReason::NoMedia)
        );
    }

    #[test]
    fn scenarios_behave_as_documented() {
        assert_eq!(
            SimulatedDiscovery::new(Scenario::NoDevices)
                .discover()
                .unwrap()
                .devices
                .len(),
            0
        );
        assert!(matches!(
            SimulatedDiscovery::new(Scenario::DiscoveryFailure).discover(),
            Err(DiscoveryError::Simulated)
        ));
        let mut hotplug = SimulatedDiscovery::new(Scenario::Hotplug);
        let counts: Vec<usize> = (0..4)
            .map(|_| hotplug.discover().unwrap().devices.len())
            .collect();
        assert_eq!(counts, vec![1, 2, 1, 1]);
        assert_eq!(
            hotplug.source(),
            DiscoverySource::Simulated {
                scenario: "hotplug".into()
            }
        );
    }

    #[test]
    fn latency_is_applied() {
        let mut slow =
            SimulatedDiscovery::new(Scenario::NoDevices).with_latency(Duration::from_millis(30));
        let start = Instant::now();
        slow.discover().unwrap();
        assert!(start.elapsed() >= Duration::from_millis(30));
    }
}
