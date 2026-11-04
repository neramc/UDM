//! Conversion of domain data into UI view models.
//!
//! Only technical, non-translatable values (numbers, identifiers, mount
//! points, vendor strings) are formatted here. Everything a person reads as
//! prose is selected in the UI from enum codes so it can be localized.

use std::rc::Rc;

use slint::{ModelRc, SharedString, VecModel};
use udm_app::DiscoveryFailure;
use udm_domain::discovery::{IssueSeverity, ProbeFailure, ProbeTarget};
use udm_domain::identity::ObservationSource;
use udm_domain::{
    AttributeKind, Capability, CapabilityReason, CapabilityStatus, Confidence, Device,
    DiscoveryIssue, DiscoveryIssueKind, MediaState, Partition, SystemDiskStatus, Transport,
    VolumeUsage, WriteProtection,
};
use udm_platform::DiscoveryError;

use crate::{
    AttributeName, AttributeRow, CapabilityKind, CapabilityRow, CapabilityState, ConfidenceLevel,
    DeviceItem, FailureCode, IssueCode, IssueRow, MediaKind, PartitionRow, ReasonCode,
    RemovableKind, SourceKind, SystemDiskKind, TransportKind, UsageItem, UsageKind,
    WriteProtectKind,
};

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// Formats a byte count with SI (decimal) units and one decimal place, the
/// convention used on product labels: `32.0 GB`.
#[must_use]
pub fn format_decimal(bytes: u64) -> String {
    format_scaled(bytes, 1000, &["B", "kB", "MB", "GB", "TB", "PB", "EB"])
}

/// Formats a byte count with IEC (binary) units: `29.8 GiB`.
#[must_use]
pub fn format_binary(bytes: u64) -> String {
    format_scaled(
        bytes,
        1024,
        &["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"],
    )
}

fn format_scaled(bytes: u64, base: u64, units: &[&str]) -> String {
    let mut unit = 0;
    let mut divisor: u128 = 1;
    while unit + 1 < units.len() && u128::from(bytes) >= divisor * u128::from(base) {
        divisor *= u128::from(base);
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} {}", units.first().copied().unwrap_or("B"));
    }
    // One decimal place, rounded half up, with integer arithmetic only.
    let mut tenths = (u128::from(bytes) * 10 + divisor / 2) / divisor;
    if tenths >= u128::from(base) * 10 && unit + 1 < units.len() {
        // Rounding reached the next unit (e.g. 999,999,999 B -> 1.0 GB).
        divisor *= u128::from(base);
        unit += 1;
        tenths = (u128::from(bytes) * 10 + divisor / 2) / divisor;
    }
    let name = units.get(unit).copied().unwrap_or("B");
    format!("{}.{} {name}", tenths / 10, tenths % 10)
}

/// Formats an exact byte count with digit grouping: `32,015,679,488`.
#[must_use]
pub fn group_digits(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Human-readable device title from reported strings only. Returns an empty
/// string when nothing was reported; the UI then shows "Unidentified device".
#[must_use]
pub fn device_title(device: &Device) -> String {
    let id = &device.identity;
    let pick = |a: AttributeKind, b: AttributeKind| {
        id.text(a)
            .or_else(|| id.text(b))
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let vendor = pick(AttributeKind::Vendor, AttributeKind::UsbManufacturer);
    let model = pick(AttributeKind::Model, AttributeKind::UsbProduct);
    match (vendor.is_empty(), model.is_empty()) {
        (true, true) => String::new(),
        (false, true) => vendor,
        (true, false) => model,
        (false, false) if model.starts_with(&vendor) => model,
        (false, false) => format!("{vendor} {model}"),
    }
}

const fn transport(t: Transport) -> TransportKind {
    match t {
        Transport::Usb => TransportKind::Usb,
        Transport::Nvme => TransportKind::Nvme,
        Transport::Ata => TransportKind::Ata,
        Transport::Scsi => TransportKind::Scsi,
        Transport::Mmc => TransportKind::Mmc,
        Transport::Virtio => TransportKind::Virtio,
        Transport::Unknown => TransportKind::Unknown,
    }
}

const fn capability(c: Capability) -> CapabilityKind {
    match c {
        Capability::RawRead => CapabilityKind::RawRead,
        Capability::RawWrite => CapabilityKind::RawWrite,
        Capability::SafeEject => CapabilityKind::SafeEject,
        Capability::SmartAccess => CapabilityKind::SmartAccess,
        Capability::PartitionEdit => CapabilityKind::PartitionEdit,
        Capability::SecureErase => CapabilityKind::SecureErase,
        Capability::FilesystemCheck => CapabilityKind::FilesystemCheck,
    }
}

const fn reason(r: Option<CapabilityReason>) -> ReasonCode {
    match r {
        None => ReasonCode::None,
        Some(CapabilityReason::NotAssessed) => ReasonCode::NotAssessed,
        Some(CapabilityReason::NotImplementedInThisBuild) => ReasonCode::NotImplemented,
        Some(CapabilityReason::RequiresPrivilegedHelper) => ReasonCode::RequiresHelper,
        Some(CapabilityReason::PermissionDenied) => ReasonCode::PermissionDenied,
        Some(CapabilityReason::WriteProtected) => ReasonCode::WriteProtected,
        Some(CapabilityReason::NoMedia) => ReasonCode::NoMedia,
        Some(CapabilityReason::TransportUnsupported) => ReasonCode::TransportUnsupported,
        Some(CapabilityReason::BridgePassthroughUnverified) => ReasonCode::BridgeUnverified,
        Some(CapabilityReason::FlashErasureNotGuaranteed) => ReasonCode::FlashErasureNotGuaranteed,
    }
}

const fn capability_state(s: CapabilityStatus) -> CapabilityState {
    match s {
        CapabilityStatus::Supported => CapabilityState::Supported,
        CapabilityStatus::Limited(_) => CapabilityState::Limited,
        CapabilityStatus::Unavailable(_) => CapabilityState::Unavailable,
        CapabilityStatus::NotTested(_) => CapabilityState::NotTested,
    }
}

const fn attribute_name(kind: AttributeKind) -> AttributeName {
    match kind {
        AttributeKind::Vendor => AttributeName::Vendor,
        AttributeKind::Model => AttributeName::Model,
        AttributeKind::Revision => AttributeName::Revision,
        AttributeKind::SerialNumber => AttributeName::SerialNumber,
        AttributeKind::WorldWideName => AttributeName::WorldWideName,
        AttributeKind::UsbVendorId => AttributeName::UsbVendorId,
        AttributeKind::UsbProductId => AttributeName::UsbProductId,
        AttributeKind::UsbManufacturer => AttributeName::UsbManufacturer,
        AttributeKind::UsbProduct => AttributeName::UsbProduct,
        AttributeKind::UsbSerialNumber => AttributeName::UsbSerialNumber,
        AttributeKind::CapacityBytes => AttributeName::Capacity,
        AttributeKind::LogicalBlockSize => AttributeName::LogicalBlockSize,
        AttributeKind::PhysicalBlockSize => AttributeName::PhysicalBlockSize,
        AttributeKind::OsPersistentLink => AttributeName::PersistentLink,
        AttributeKind::OsDeviceName => AttributeName::OsDeviceName,
        AttributeKind::OsDeviceNumber => AttributeName::OsDeviceNumber,
    }
}

const fn confidence(c: Confidence) -> ConfidenceLevel {
    match c {
        Confidence::Low => ConfidenceLevel::Low,
        Confidence::Medium => ConfidenceLevel::Medium,
        Confidence::High => ConfidenceLevel::High,
    }
}

const fn source(s: ObservationSource) -> SourceKind {
    match s {
        ObservationSource::Simulated => SourceKind::Simulated,
        ObservationSource::LinuxSysfs => SourceKind::LinuxSysfs,
        ObservationSource::LinuxUdevLink => SourceKind::LinuxUdevLink,
    }
}

fn usage_item(usage: &VolumeUsage) -> UsageItem {
    match usage {
        VolumeUsage::Mounted {
            mount_point,
            filesystem,
            via,
        } => {
            let extra = [filesystem.as_deref(), via.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
            UsageItem {
                kind: UsageKind::Mounted,
                detail: mount_point.into(),
                extra: extra.into(),
            }
        }
        VolumeUsage::Swap { via } => UsageItem {
            kind: UsageKind::Swap,
            detail: SharedString::default(),
            extra: via.as_deref().unwrap_or_default().into(),
        },
        VolumeUsage::Held { holder } => UsageItem {
            kind: UsageKind::Held,
            detail: holder.into(),
            extra: SharedString::default(),
        },
    }
}

fn partition_row(partition: &Partition) -> PartitionRow {
    PartitionRow {
        number: partition
            .number
            .map_or_else(|| "?".to_owned(), |n| n.to_string())
            .into(),
        os_name: partition.os_name.as_deref().unwrap_or_default().into(),
        start: partition
            .start_bytes
            .map(format_binary)
            .unwrap_or_default()
            .into(),
        size: partition
            .size_bytes
            .map(format_decimal)
            .unwrap_or_default()
            .into(),
        usage: model(partition.usage.iter().map(usage_item).collect()),
    }
}

const fn probe_target(t: ProbeTarget) -> &'static str {
    match t {
        ProbeTarget::Attribute(kind) => kind.as_str(),
        ProbeTarget::DeviceState => "device_state",
        ProbeTarget::Partitions => "partitions",
        ProbeTarget::MountTable => "mount_table",
        ProbeTarget::SwapTable => "swap_table",
        ProbeTarget::PersistentLinks => "persistent_links",
        ProbeTarget::UsbDescriptor => "usb_descriptor",
        ProbeTarget::HolderTopology => "holder_topology",
    }
}

const fn probe_failure(f: ProbeFailure) -> &'static str {
    match f {
        ProbeFailure::PermissionDenied => "permission denied",
        ProbeFailure::Vanished => "not present",
        ProbeFailure::Malformed => "malformed",
        ProbeFailure::Io => "I/O error",
    }
}

/// Converts a discovery issue into a row. `detail` carries only technical
/// identifiers.
#[must_use]
pub fn issue_row(issue: &DiscoveryIssue) -> IssueRow {
    let (code, detail) = match &issue.kind {
        DiscoveryIssueKind::ProbeFailed { target, failure } => (
            IssueCode::ProbeFailed,
            format!("{}: {}", probe_target(*target), probe_failure(*failure)),
        ),
        DiscoveryIssueKind::DuplicateIdentifier { attribute, devices } => (
            IssueCode::DuplicateIdentifier,
            format!(
                "{}: {}",
                attribute.as_str(),
                devices
                    .iter()
                    .map(|d| d.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        DiscoveryIssueKind::NoStrongIdentifier => (IssueCode::NoStrongIdentifier, String::new()),
        DiscoveryIssueKind::SystemDiskUndetermined => {
            (IssueCode::SystemDiskUndetermined, String::new())
        }
        DiscoveryIssueKind::VirtualDevicesExcluded { count } => {
            (IssueCode::VirtualDevicesExcluded, format!("{count}"))
        }
    };
    IssueRow {
        code,
        warning: issue.kind.severity() == IssueSeverity::Warning,
        detail: detail.into(),
        device: issue
            .device
            .as_ref()
            .map(|d| d.as_str())
            .unwrap_or_default()
            .into(),
    }
}

/// Converts a device and the issues that concern it into a list item.
#[must_use]
pub fn device_item(device: &Device, issues: &[DiscoveryIssue]) -> DeviceItem {
    let id = &device.identity;
    let capacity = device.capacity_bytes();
    let block_size = match (
        id.number(AttributeKind::LogicalBlockSize),
        id.number(AttributeKind::PhysicalBlockSize),
    ) {
        (Some(l), Some(p)) if l != p => format!("{l} B / {p} B"),
        (Some(l), _) => format!("{l} B"),
        (None, Some(p)) => format!("{p} B"),
        (None, None) => String::new(),
    };
    let attributes = id
        .attributes()
        .iter()
        .map(|a| AttributeRow {
            name: attribute_name(a.kind),
            value: match (a.kind, &a.value) {
                (AttributeKind::CapacityBytes, udm_domain::AttributeValue::Number(n)) => {
                    format!("{} B", group_digits(*n))
                }
                (_, value) => value.to_string(),
            }
            .into(),
            source: source(a.provenance.source),
            locator: a.provenance.locator.as_deref().unwrap_or_default().into(),
            confidence: confidence(a.confidence),
        })
        .collect();
    let capabilities = device
        .capabilities
        .iter()
        .map(|(c, s)| CapabilityRow {
            capability: capability(c),
            state: capability_state(s),
            reason: reason(s.reason()),
            destructive: c.is_destructive(),
        })
        .collect();
    DeviceItem {
        key: device.key.as_str().into(),
        title: device_title(device).into(),
        os_name: id
            .text(AttributeKind::OsDeviceName)
            .unwrap_or_default()
            .into(),
        capacity: capacity.map(format_decimal).unwrap_or_default().into(),
        capacity_detail: capacity
            .map(|c| format!("{} B · {}", group_digits(c), format_binary(c)))
            .unwrap_or_default()
            .into(),
        transport: transport(device.transport),
        simulated: device.simulated,
        media: match device.state.media {
            MediaState::Present => MediaKind::Present,
            MediaState::NoMedia => MediaKind::NoMedia,
            MediaState::Unknown => MediaKind::Unknown,
        },
        write_protect: match device.state.write_protection {
            WriteProtection::Writable => WriteProtectKind::Writable,
            WriteProtection::ReadOnly => WriteProtectKind::ReadOnly,
            WriteProtection::Unknown => WriteProtectKind::Unknown,
        },
        removable: match device.state.removable {
            Some(true) => RemovableKind::Yes,
            Some(false) => RemovableKind::No,
            None => RemovableKind::Unknown,
        },
        system_disk: match device.state.system_disk {
            SystemDiskStatus::SystemDisk => SystemDiskKind::System,
            SystemDiskStatus::NotSystemDisk => SystemDiskKind::NotSystem,
            SystemDiskStatus::Unknown => SystemDiskKind::Unknown,
        },
        in_use: device.is_in_use(),
        strong_identity: id.has_strong_identifier(),
        block_size: block_size.into(),
        attributes: model(attributes),
        partitions: model(device.partitions.iter().map(partition_row).collect()),
        whole_disk_usage: model(device.whole_disk_usage.iter().map(usage_item).collect()),
        capabilities: model(capabilities),
        issues: model(
            issues
                .iter()
                .filter(|i| i.device.as_ref() == Some(&device.key))
                .map(issue_row)
                .collect(),
        ),
    }
}

/// Maps a discovery failure to a code and a technical detail line.
#[must_use]
pub fn failure(error: &DiscoveryFailure) -> (FailureCode, String) {
    match error {
        DiscoveryFailure::AdapterPanicked => (FailureCode::AdapterPanicked, String::new()),
        DiscoveryFailure::Adapter(error) => match error {
            DiscoveryError::NotImplemented { platform } => {
                (FailureCode::NotImplemented, platform.as_str().to_owned())
            }
            DiscoveryError::PermissionDenied { source } => {
                (FailureCode::PermissionDenied, source.to_string())
            }
            DiscoveryError::Io { source } => (FailureCode::Io, source.to_string()),
            DiscoveryError::Simulated => (FailureCode::Simulated, String::new()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;
    use udm_domain::{DeviceKey, assess_identities};
    use udm_platform::simulated::sample_devices;

    #[test]
    fn decimal_and_binary_formatting() {
        assert_eq!(format_decimal(0), "0 B");
        assert_eq!(format_decimal(999), "999 B");
        assert_eq!(format_decimal(1000), "1.0 kB");
        assert_eq!(format_decimal(32_015_679_488), "32.0 GB");
        assert_eq!(format_decimal(1_000_204_886_016), "1.0 TB");
        assert_eq!(format_decimal(u64::MAX), "18.4 EB");
        assert_eq!(format_binary(32_015_679_488), "29.8 GiB");
        assert_eq!(format_binary(1023), "1023 B");
        assert_eq!(format_binary(1536), "1.5 KiB");
        // Rounding must carry into the next unit instead of printing "1000.0 MB".
        assert_eq!(format_decimal(999_999_999), "1.0 GB");
        assert_eq!(format_decimal(999_949_999), "999.9 MB");
    }

    #[test]
    fn digit_grouping() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(32_015_679_488), "32,015,679,488");
    }

    #[test]
    fn titles_never_invent_names() {
        let devices = sample_devices();
        let titles: Vec<String> = devices.iter().map(device_title).collect();
        assert!(titles.contains(&"Acme FlashLine 32".to_owned()));
        let unidentified = devices
            .iter()
            .find(|d| d.key == DeviceKey::new("sim:unidentified"))
            .unwrap();
        assert_eq!(device_title(unidentified), "");
    }

    #[test]
    fn device_item_exposes_every_capability_and_device_issues() {
        let mut devices = sample_devices();
        let issues = assess_identities(&mut devices);
        let unidentified = devices
            .iter()
            .find(|d| d.key == DeviceKey::new("sim:unidentified"))
            .unwrap();
        let item = device_item(unidentified, &issues);
        assert_eq!(item.capabilities.row_count(), Capability::ALL.len());
        assert!(item.simulated);
        assert!(!item.strong_identity);
        assert_eq!(item.issues.row_count(), 1);
        assert_eq!(
            item.issues.row_data(0).unwrap().code,
            IssueCode::NoStrongIdentifier
        );
        assert_eq!(item.capacity.as_str(), "8.1 GB");
        assert_eq!(item.system_disk, SystemDiskKind::Unknown);
    }

    #[test]
    fn issue_rows_carry_only_technical_detail() {
        let row = issue_row(&DiscoveryIssue::global(
            DiscoveryIssueKind::VirtualDevicesExcluded { count: 9 },
        ));
        assert_eq!(row.code, IssueCode::VirtualDevicesExcluded);
        assert!(!row.warning, "informational notes are not warnings");
        assert_eq!(row.detail.as_str(), "9");
        assert_eq!(row.device.as_str(), "");
    }

    #[test]
    fn failures_map_to_codes() {
        let (code, detail) = failure(&DiscoveryFailure::Adapter(DiscoveryError::NotImplemented {
            platform: udm_domain::PlatformKind::Windows,
        }));
        assert_eq!(code, FailureCode::NotImplemented);
        assert_eq!(detail, "windows");
        assert_eq!(
            failure(&DiscoveryFailure::AdapterPanicked).0,
            FailureCode::AdapterPanicked
        );
    }
}
