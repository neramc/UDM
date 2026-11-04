//! Discovery results and typed issues.
//!
//! Discovery is allowed to partially fail: one unreadable attribute must not
//! hide every other device. Partial failures are reported as
//! [`DiscoveryIssue`]s attached to the snapshot instead of being swallowed.

use std::collections::BTreeMap;

use crate::device::{Device, DeviceKey};
use crate::identity::{AttributeKind, AttributeValue, Confidence};
use crate::platform::PlatformKind;

/// Where a snapshot came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverySource {
    /// Simulated devices; `scenario` names the built-in scenario.
    Simulated { scenario: String },
    /// Real, read-only discovery on the running system.
    System { platform: PlatformKind },
}

impl DiscoverySource {
    #[must_use]
    pub const fn is_simulated(&self) -> bool {
        matches!(self, Self::Simulated { .. })
    }
}

/// What a probe was trying to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeTarget {
    Attribute(AttributeKind),
    /// Device state flags (read-only, removable).
    DeviceState,
    Partitions,
    MountTable,
    SwapTable,
    PersistentLinks,
    UsbDescriptor,
    HolderTopology,
}

/// Why a probe failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeFailure {
    PermissionDenied,
    /// The device disappeared while it was being read.
    Vanished,
    /// The data was present but could not be parsed.
    Malformed,
    /// Any other I/O error.
    Io,
}

/// Severity of a discovery issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    Info,
    Warning,
}

/// The kind of a discovery issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryIssueKind {
    /// A probe failed; the affected information is reported as unknown.
    ProbeFailed {
        target: ProbeTarget,
        failure: ProbeFailure,
    },
    /// The same strong identifier was reported by several devices. Its
    /// confidence has been lowered and it is no longer used as evidence.
    DuplicateIdentifier {
        attribute: AttributeKind,
        devices: Vec<DeviceKey>,
    },
    /// The device reported no usable strong identifier (serial number / WWN).
    /// Its identity can never be confirmed with certainty.
    NoStrongIdentifier,
    /// The device holding the running system could not be determined, so
    /// every device is marked [`crate::SystemDiskStatus::Unknown`].
    SystemDiskUndetermined,
    /// Virtual block devices (loop, device-mapper, RAID, zram, ...) were
    /// excluded from the device list.
    VirtualDevicesExcluded { count: u32 },
}

impl DiscoveryIssueKind {
    #[must_use]
    pub const fn severity(&self) -> IssueSeverity {
        match self {
            Self::VirtualDevicesExcluded { .. } => IssueSeverity::Info,
            Self::ProbeFailed { .. }
            | Self::DuplicateIdentifier { .. }
            | Self::NoStrongIdentifier
            | Self::SystemDiskUndetermined => IssueSeverity::Warning,
        }
    }
}

/// An issue found during discovery, optionally tied to a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryIssue {
    pub device: Option<DeviceKey>,
    pub kind: DiscoveryIssueKind,
}

impl DiscoveryIssue {
    #[must_use]
    pub fn for_device(device: &DeviceKey, kind: DiscoveryIssueKind) -> Self {
        Self {
            device: Some(device.clone()),
            kind,
        }
    }

    #[must_use]
    pub const fn global(kind: DiscoveryIssueKind) -> Self {
        Self { device: None, kind }
    }
}

/// Post-processing applied to every discovery result, independent of the
/// adapter that produced it:
///
/// 1. Strong identifiers reported by more than one device have their
///    confidence capped at [`Confidence::Low`] and an issue is raised.
/// 2. Devices without a usable strong identifier get a
///    [`DiscoveryIssueKind::NoStrongIdentifier`] issue.
pub fn assess_identities(devices: &mut [Device]) -> Vec<DiscoveryIssue> {
    let mut issues = Vec::new();

    let mut seen: BTreeMap<(AttributeKind, AttributeValue), Vec<usize>> = BTreeMap::new();
    for (index, device) in devices.iter().enumerate() {
        for attribute in device.identity.attributes() {
            if attribute.kind.is_strong_identifier() && attribute.confidence >= Confidence::Medium {
                let owners = seen
                    .entry((attribute.kind, attribute.value.clone()))
                    .or_default();
                if !owners.contains(&index) {
                    owners.push(index);
                }
            }
        }
    }

    for ((kind, value), owners) in seen {
        if owners.len() < 2 {
            continue;
        }
        for &index in &owners {
            if let Some(device) = devices.get_mut(index) {
                device
                    .identity
                    .cap_confidence(kind, &value, Confidence::Low);
            }
        }
        issues.push(DiscoveryIssue::global(
            DiscoveryIssueKind::DuplicateIdentifier {
                attribute: kind,
                devices: owners
                    .iter()
                    .filter_map(|&i| devices.get(i).map(|d| d.key.clone()))
                    .collect(),
            },
        ));
    }

    for device in devices.iter() {
        if !device.identity.has_strong_identifier() {
            issues.push(DiscoveryIssue::for_device(
                &device.key,
                DiscoveryIssueKind::NoStrongIdentifier,
            ));
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySet;
    use crate::device::{DeviceState, Transport};
    use crate::identity::{DeviceIdentity, Provenance};

    fn device(key: &str, serial: Option<&str>) -> Device {
        let mut identity = DeviceIdentity::new();
        if let Some(serial) = serial {
            identity = identity.with(
                AttributeKind::UsbSerialNumber,
                AttributeValue::Text(serial.into()),
                Provenance::simulated(),
                Confidence::Medium,
            );
        }
        Device {
            key: DeviceKey::new(key),
            simulated: true,
            transport: Transport::Usb,
            identity,
            state: DeviceState::unknown(),
            partitions: vec![],
            whole_disk_usage: vec![],
            capabilities: CapabilitySet::unassessed(),
        }
    }

    #[test]
    fn duplicate_serials_are_downgraded_and_reported() {
        let mut devices = vec![
            device("a", Some("0123456789ABCDEF")),
            device("b", Some("0123456789ABCDEF")),
            device("c", Some("UNIQUE-1")),
        ];
        let issues = assess_identities(&mut devices);

        assert!(!devices[0].identity.has_strong_identifier());
        assert!(!devices[1].identity.has_strong_identifier());
        assert!(devices[2].identity.has_strong_identifier());

        assert!(issues.contains(&DiscoveryIssue::global(
            DiscoveryIssueKind::DuplicateIdentifier {
                attribute: AttributeKind::UsbSerialNumber,
                devices: vec![DeviceKey::new("a"), DeviceKey::new("b")],
            }
        )));
        // After downgrading, both duplicates also lack a strong identifier.
        let no_strong: Vec<_> = issues
            .iter()
            .filter(|i| i.kind == DiscoveryIssueKind::NoStrongIdentifier)
            .filter_map(|i| i.device.clone())
            .collect();
        assert_eq!(no_strong, vec![DeviceKey::new("a"), DeviceKey::new("b")]);
    }

    #[test]
    fn device_without_serial_is_reported() {
        let mut devices = vec![device("x", None)];
        let issues = assess_identities(&mut devices);
        assert_eq!(
            issues,
            vec![DiscoveryIssue::for_device(
                &DeviceKey::new("x"),
                DiscoveryIssueKind::NoStrongIdentifier
            )]
        );
    }

    #[test]
    fn unique_identifiers_raise_no_issue() {
        let mut devices = vec![device("a", Some("SERIAL-A")), device("b", Some("SERIAL-B"))];
        assert!(assess_identities(&mut devices).is_empty());
    }

    #[test]
    fn severities() {
        assert_eq!(
            DiscoveryIssueKind::VirtualDevicesExcluded { count: 1 }.severity(),
            IssueSeverity::Info
        );
        assert_eq!(
            DiscoveryIssueKind::SystemDiskUndetermined.severity(),
            IssueSeverity::Warning
        );
    }
}
