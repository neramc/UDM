// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Device identity as a set of observed attributes.
//!
//! A storage device has no single trustworthy identifier:
//!
//! * OS paths (`/dev/sdb`, `\\.\PhysicalDrive2`, `/dev/disk4`) and disk numbers
//!   are reassigned on reconnect and are **never** identities.
//! * Serial numbers may be missing, duplicated across identical products,
//!   placeholder values, or rewritten by USB bridges.
//! * Model and capacity are shared by every unit of a product line.
//!
//! [`DeviceIdentity`] therefore stores every observed attribute together with
//! its [`Provenance`] and [`Confidence`]. [`DeviceIdentity::compare`] decides
//! whether two observations are consistent with being the same device, and is
//! deliberately conservative: anything short of a matching strong identifier
//! *and* a matching capacity is reported as [`IdentityComparison::Inconclusive`].
//!
//! See `docs/ADR/0003-device-identity-model.md`.

use std::collections::BTreeSet;
use std::fmt;

/// What an identity attribute describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttributeKind {
    /// Vendor string reported by the device or its bridge (e.g. SCSI INQUIRY).
    Vendor,
    /// Model / product string reported by the device or its bridge.
    Model,
    /// Firmware or product revision.
    Revision,
    /// Device serial number (SCSI VPD, NVMe, MMC CID, ...).
    SerialNumber,
    /// World Wide Name / EUI / NGUID style globally unique identifier.
    WorldWideName,
    /// USB idVendor from the USB device descriptor.
    UsbVendorId,
    /// USB idProduct from the USB device descriptor.
    UsbProductId,
    /// USB iManufacturer string.
    UsbManufacturer,
    /// USB iProduct string.
    UsbProduct,
    /// USB iSerialNumber string. Frequently duplicated on cheap devices.
    UsbSerialNumber,
    /// Reported capacity in bytes. Zero when no media is present.
    CapacityBytes,
    /// Logical block (sector) size in bytes.
    LogicalBlockSize,
    /// Physical block size in bytes.
    PhysicalBlockSize,
    /// A persistent OS link derived from device attributes
    /// (e.g. a udev `/dev/disk/by-id/` name).
    OsPersistentLink,
    /// Kernel / OS device name (e.g. `sdb`). Volatile.
    OsDeviceName,
    /// OS device number (e.g. Linux `major:minor`). Volatile.
    OsDeviceNumber,
}

/// Whether an attribute is expected to survive a reconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeStability {
    /// Describes the device itself; expected to be identical on every observation.
    Stable,
    /// Describes how the OS currently exposes the device; may change on reconnect.
    Volatile,
}

impl AttributeKind {
    /// Every attribute kind, in display order.
    pub const ALL: [Self; 16] = [
        Self::Vendor,
        Self::Model,
        Self::Revision,
        Self::SerialNumber,
        Self::WorldWideName,
        Self::UsbVendorId,
        Self::UsbProductId,
        Self::UsbManufacturer,
        Self::UsbProduct,
        Self::UsbSerialNumber,
        Self::CapacityBytes,
        Self::LogicalBlockSize,
        Self::PhysicalBlockSize,
        Self::OsPersistentLink,
        Self::OsDeviceName,
        Self::OsDeviceNumber,
    ];

    /// Stability class of this attribute.
    #[must_use]
    pub const fn stability(self) -> AttributeStability {
        match self {
            Self::OsDeviceName | Self::OsDeviceNumber => AttributeStability::Volatile,
            Self::Vendor
            | Self::Model
            | Self::Revision
            | Self::SerialNumber
            | Self::WorldWideName
            | Self::UsbVendorId
            | Self::UsbProductId
            | Self::UsbManufacturer
            | Self::UsbProduct
            | Self::UsbSerialNumber
            | Self::CapacityBytes
            | Self::LogicalBlockSize
            | Self::PhysicalBlockSize
            | Self::OsPersistentLink => AttributeStability::Stable,
        }
    }

    /// Whether a matching value of this kind (at [`Confidence::Medium`] or
    /// better) may be used as evidence that two observations are the same
    /// physical device.
    #[must_use]
    pub const fn is_strong_identifier(self) -> bool {
        matches!(
            self,
            Self::SerialNumber | Self::WorldWideName | Self::UsbSerialNumber
        )
    }

    /// Whether the value may be sensitive (unique to a person's device) and
    /// should be redacted from logs.
    #[must_use]
    pub const fn is_sensitive(self) -> bool {
        matches!(
            self,
            Self::SerialNumber
                | Self::WorldWideName
                | Self::UsbSerialNumber
                | Self::OsPersistentLink
        )
    }

    /// A stable, non-localized identifier for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vendor => "vendor",
            Self::Model => "model",
            Self::Revision => "revision",
            Self::SerialNumber => "serial_number",
            Self::WorldWideName => "world_wide_name",
            Self::UsbVendorId => "usb_vendor_id",
            Self::UsbProductId => "usb_product_id",
            Self::UsbManufacturer => "usb_manufacturer",
            Self::UsbProduct => "usb_product",
            Self::UsbSerialNumber => "usb_serial_number",
            Self::CapacityBytes => "capacity_bytes",
            Self::LogicalBlockSize => "logical_block_size",
            Self::PhysicalBlockSize => "physical_block_size",
            Self::OsPersistentLink => "os_persistent_link",
            Self::OsDeviceName => "os_device_name",
            Self::OsDeviceNumber => "os_device_number",
        }
    }
}

/// An observed attribute value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttributeValue {
    Text(String),
    Number(u64),
}

impl AttributeValue {
    /// Creates a text value with surrounding whitespace removed. Returns `None`
    /// for values that are empty after trimming: an empty string is "not
    /// reported", not a value.
    #[must_use]
    pub fn text(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Self::Text(trimmed.to_owned()))
        }
    }
}

impl fmt::Display for AttributeValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(text) => f.write_str(text),
            Self::Number(number) => write!(f, "{number}"),
        }
    }
}

/// Where an observation came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservationSource {
    /// Produced by the simulated adapter. Never describes real hardware.
    Simulated,
    /// Read from Linux sysfs (`/sys`).
    LinuxSysfs,
    /// Read from udev-managed symlinks under `/dev/disk/`.
    LinuxUdevLink,
}

impl ObservationSource {
    /// A stable, non-localized identifier for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simulated => "simulated",
            Self::LinuxSysfs => "linux_sysfs",
            Self::LinuxUdevLink => "linux_udev_link",
        }
    }
}

/// Source plus an optional locator (e.g. the sysfs attribute path relative to
/// the device directory).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Provenance {
    pub source: ObservationSource,
    pub locator: Option<String>,
}

impl Provenance {
    #[must_use]
    pub fn new(source: ObservationSource, locator: impl Into<String>) -> Self {
        Self {
            source,
            locator: Some(locator.into()),
        }
    }

    #[must_use]
    pub const fn simulated() -> Self {
        Self {
            source: ObservationSource::Simulated,
            locator: None,
        }
    }
}

/// How much an attribute value can be relied upon.
///
/// Ordered so that `Low < Medium < High`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Confidence {
    /// Heuristic, derived, placeholder-like, or known to collide.
    Low,
    /// Reported by the device or a bridge, but known to be unreliable on some
    /// hardware (e.g. USB serial numbers, bridge-reported model strings).
    Medium,
    /// Reported through a mechanism with strong guarantees (e.g. kernel-reported
    /// capacity, NVMe EUI-64).
    High,
}

/// One observed attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityAttribute {
    pub kind: AttributeKind,
    pub value: AttributeValue,
    pub provenance: Provenance,
    pub confidence: Confidence,
}

/// The identity of a device as a collection of observed attributes.
///
/// The same kind may appear more than once (e.g. several persistent links).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceIdentity {
    attributes: Vec<IdentityAttribute>,
}

/// A stable attribute whose expected and observed values have nothing in common.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeConflict {
    pub kind: AttributeKind,
}

/// Result of comparing an earlier observation with a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityComparison {
    /// At least one strong identifier and the capacity match, and no stable
    /// attribute conflicts.
    Consistent { matched_strong: Vec<AttributeKind> },
    /// Nothing conflicts, but the evidence is insufficient to confirm that
    /// this is the same device (no strong identifier, a strong identifier
    /// disappeared, or capacity could not be compared).
    Inconclusive {
        /// `true` when no strong identifier matched (including the case where
        /// neither observation has one).
        no_strong_match: bool,
        /// Strong identifiers or capacity present in only one observation.
        unverifiable: Vec<AttributeKind>,
    },
    /// At least one stable attribute conflicts. The observations must be
    /// treated as different devices.
    Mismatch { conflicts: Vec<AttributeConflict> },
}

impl IdentityComparison {
    /// Only [`IdentityComparison::Consistent`] may be treated as "same device".
    #[must_use]
    pub const fn is_consistent(&self) -> bool {
        matches!(self, Self::Consistent { .. })
    }
}

impl DeviceIdentity {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            attributes: Vec::new(),
        }
    }

    /// Adds an attribute.
    pub fn push(&mut self, attribute: IdentityAttribute) {
        self.attributes.push(attribute);
    }

    /// Builder-style helper used by adapters and fixtures.
    #[must_use]
    pub fn with(
        mut self,
        kind: AttributeKind,
        value: AttributeValue,
        provenance: Provenance,
        confidence: Confidence,
    ) -> Self {
        self.push(IdentityAttribute {
            kind,
            value,
            provenance,
            confidence,
        });
        self
    }

    /// All attributes in observation order.
    #[must_use]
    pub fn attributes(&self) -> &[IdentityAttribute] {
        &self.attributes
    }

    /// The first attribute of `kind`, if any.
    #[must_use]
    pub fn first(&self, kind: AttributeKind) -> Option<&IdentityAttribute> {
        self.attributes.iter().find(|a| a.kind == kind)
    }

    /// The first text value of `kind`, if any.
    #[must_use]
    pub fn text(&self, kind: AttributeKind) -> Option<&str> {
        self.attributes
            .iter()
            .filter(|a| a.kind == kind)
            .find_map(|a| match &a.value {
                AttributeValue::Text(text) => Some(text.as_str()),
                AttributeValue::Number(_) => None,
            })
    }

    /// The first numeric value of `kind`, if any.
    #[must_use]
    pub fn number(&self, kind: AttributeKind) -> Option<u64> {
        self.attributes
            .iter()
            .filter(|a| a.kind == kind)
            .find_map(|a| match a.value {
                AttributeValue::Number(number) => Some(number),
                AttributeValue::Text(_) => None,
            })
    }

    /// Whether at least one strong identifier is present with at least
    /// [`Confidence::Medium`].
    #[must_use]
    pub fn has_strong_identifier(&self) -> bool {
        self.attributes
            .iter()
            .any(|a| a.kind.is_strong_identifier() && a.confidence >= Confidence::Medium)
    }

    /// Lowers the confidence of every attribute of `kind` with the given value
    /// to at most `ceiling`. Used when a value turns out to be shared by
    /// several devices.
    pub fn cap_confidence(
        &mut self,
        kind: AttributeKind,
        value: &AttributeValue,
        ceiling: Confidence,
    ) {
        for attribute in &mut self.attributes {
            if attribute.kind == kind && &attribute.value == value && attribute.confidence > ceiling
            {
                attribute.confidence = ceiling;
            }
        }
    }

    /// Values of a stable attribute kind that are reliable enough to compare.
    fn comparable_values(&self, kind: AttributeKind) -> BTreeSet<&AttributeValue> {
        self.attributes
            .iter()
            .filter(|a| a.kind == kind && a.confidence >= Confidence::Medium)
            .map(|a| &a.value)
            .collect()
    }

    /// Compares an `expected` identity (e.g. the one captured when an operation
    /// was planned) with a fresh observation.
    ///
    /// Rules (see ADR-0003):
    /// 1. Only stable attributes with confidence `Medium` or better are compared.
    ///    Volatile attributes (OS names and numbers) are ignored: a changed path
    ///    is not evidence of a different device, and an unchanged path is not
    ///    evidence of the same device.
    /// 2. A stable kind present on both sides whose value sets do not intersect
    ///    is a conflict. Any conflict yields [`IdentityComparison::Mismatch`].
    /// 3. Without conflicts, the result is [`IdentityComparison::Consistent`] only
    ///    when at least one strong identifier matches, no strong identifier is
    ///    present on just one side, and the capacity was compared and matched.
    ///    Everything else is [`IdentityComparison::Inconclusive`].
    #[must_use]
    pub fn compare(&self, observed: &Self) -> IdentityComparison {
        let mut conflicts = Vec::new();
        let mut matched_strong = Vec::new();
        let mut unverifiable = Vec::new();
        let mut capacity_matched = false;

        for kind in AttributeKind::ALL {
            if kind.stability() == AttributeStability::Volatile {
                continue;
            }
            let expected = self.comparable_values(kind);
            let actual = observed.comparable_values(kind);
            match (expected.is_empty(), actual.is_empty()) {
                (true, true) => {}
                (false, false) => {
                    if expected.is_disjoint(&actual) {
                        conflicts.push(AttributeConflict { kind });
                    } else if kind.is_strong_identifier() {
                        matched_strong.push(kind);
                    } else if kind == AttributeKind::CapacityBytes {
                        capacity_matched = true;
                    }
                }
                _ => {
                    if kind.is_strong_identifier() || kind == AttributeKind::CapacityBytes {
                        unverifiable.push(kind);
                    }
                }
            }
        }

        if !conflicts.is_empty() {
            return IdentityComparison::Mismatch { conflicts };
        }
        if !capacity_matched && !unverifiable.contains(&AttributeKind::CapacityBytes) {
            unverifiable.push(AttributeKind::CapacityBytes);
        }
        if matched_strong.is_empty() || !unverifiable.is_empty() {
            return IdentityComparison::Inconclusive {
                no_strong_match: matched_strong.is_empty(),
                unverifiable,
            };
        }
        IdentityComparison::Consistent { matched_strong }
    }
}

/// Classifies a raw serial-number string.
///
/// Returns `None` when nothing usable was reported, and caps the confidence at
/// [`Confidence::Low`] for values that look like placeholders (very short, or a
/// single repeated character such as `000000000000`).
#[must_use]
pub fn classify_serial(
    raw: &str,
    reported_confidence: Confidence,
) -> Option<(AttributeValue, Confidence)> {
    let text = raw.trim();
    let mut chars = text.chars();
    let first = chars.next()?;
    let single_repeated = chars.all(|c| c == first);
    let too_short = text.chars().count() < 4;
    let confidence = if single_repeated || too_short {
        Confidence::Low
    } else {
        reported_confidence
    };
    Some((AttributeValue::Text(text.to_owned()), confidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Provenance {
        Provenance::simulated()
    }

    fn text(s: &str) -> AttributeValue {
        AttributeValue::Text(s.to_owned())
    }

    fn stick(serial: &str, capacity: u64, os_name: &str) -> DeviceIdentity {
        DeviceIdentity::new()
            .with(
                AttributeKind::Vendor,
                text("Acme"),
                sim(),
                Confidence::Medium,
            )
            .with(
                AttributeKind::Model,
                text("Stick"),
                sim(),
                Confidence::Medium,
            )
            .with(
                AttributeKind::SerialNumber,
                text(serial),
                sim(),
                Confidence::Medium,
            )
            .with(
                AttributeKind::CapacityBytes,
                AttributeValue::Number(capacity),
                sim(),
                Confidence::High,
            )
            .with(
                AttributeKind::OsDeviceName,
                text(os_name),
                sim(),
                Confidence::High,
            )
    }

    #[test]
    fn same_device_on_a_different_path_is_consistent() {
        let before = stick("A1B2C3D4", 32_000_000_000, "sdb");
        let after = stick("A1B2C3D4", 32_000_000_000, "sdc");
        assert_eq!(
            before.compare(&after),
            IdentityComparison::Consistent {
                matched_strong: vec![AttributeKind::SerialNumber]
            }
        );
    }

    #[test]
    fn different_device_on_the_same_path_is_a_mismatch() {
        let before = stick("A1B2C3D4", 32_000_000_000, "sdb");
        let after = stick("ZZZZ9999", 32_000_000_000, "sdb");
        let result = before.compare(&after);
        assert_eq!(
            result,
            IdentityComparison::Mismatch {
                conflicts: vec![AttributeConflict {
                    kind: AttributeKind::SerialNumber
                }]
            }
        );
        assert!(!result.is_consistent());
    }

    #[test]
    fn capacity_change_is_a_mismatch_even_with_matching_serial() {
        // e.g. a card reader whose card was swapped for a different size.
        let before = stick("A1B2C3D4", 32_000_000_000, "sdb");
        let after = stick("A1B2C3D4", 64_000_000_000, "sdb");
        assert!(matches!(
            before.compare(&after),
            IdentityComparison::Mismatch { .. }
        ));
    }

    #[test]
    fn device_without_strong_identifier_is_never_consistent() {
        let no_serial = DeviceIdentity::new()
            .with(
                AttributeKind::Model,
                text("Stick"),
                sim(),
                Confidence::Medium,
            )
            .with(
                AttributeKind::CapacityBytes,
                AttributeValue::Number(8),
                sim(),
                Confidence::High,
            );
        let result = no_serial.compare(&no_serial.clone());
        assert_eq!(
            result,
            IdentityComparison::Inconclusive {
                no_strong_match: true,
                unverifiable: vec![]
            }
        );
        assert!(!result.is_consistent());
    }

    #[test]
    fn disappearing_serial_is_inconclusive() {
        let before = stick("A1B2C3D4", 32, "sdb");
        let mut after = DeviceIdentity::new();
        for attribute in before.attributes() {
            if attribute.kind != AttributeKind::SerialNumber {
                after.push(attribute.clone());
            }
        }
        assert_eq!(
            before.compare(&after),
            IdentityComparison::Inconclusive {
                no_strong_match: true,
                unverifiable: vec![AttributeKind::SerialNumber]
            }
        );
    }

    #[test]
    fn missing_capacity_is_inconclusive() {
        let a = DeviceIdentity::new().with(
            AttributeKind::SerialNumber,
            text("A1B2C3D4"),
            sim(),
            Confidence::Medium,
        );
        assert_eq!(
            a.compare(&a.clone()),
            IdentityComparison::Inconclusive {
                no_strong_match: false,
                unverifiable: vec![AttributeKind::CapacityBytes]
            }
        );
    }

    #[test]
    fn low_confidence_values_are_not_used_as_evidence() {
        let mut before = stick("A1B2C3D4", 32, "sdb");
        let mut after = stick("A1B2C3D4", 32, "sdc");
        before.cap_confidence(
            AttributeKind::SerialNumber,
            &text("A1B2C3D4"),
            Confidence::Low,
        );
        after.cap_confidence(
            AttributeKind::SerialNumber,
            &text("A1B2C3D4"),
            Confidence::Low,
        );
        assert!(!before.has_strong_identifier());
        assert!(matches!(
            before.compare(&after),
            IdentityComparison::Inconclusive { .. }
        ));
    }

    #[test]
    fn os_name_alone_never_establishes_identity() {
        let a = DeviceIdentity::new().with(
            AttributeKind::OsDeviceName,
            text("sdb"),
            sim(),
            Confidence::High,
        );
        assert!(!a.compare(&a.clone()).is_consistent());
    }

    #[test]
    fn text_values_are_trimmed_and_empty_is_not_a_value() {
        assert_eq!(AttributeValue::text("  SanDisk  "), Some(text("SanDisk")));
        assert_eq!(AttributeValue::text("   "), None);
        assert_eq!(AttributeValue::text(""), None);
    }

    #[test]
    fn placeholder_serials_get_low_confidence() {
        assert_eq!(classify_serial("   ", Confidence::Medium), None);
        assert_eq!(
            classify_serial("000000000000", Confidence::Medium),
            Some((text("000000000000"), Confidence::Low))
        );
        assert_eq!(
            classify_serial("12", Confidence::Medium),
            Some((text("12"), Confidence::Low))
        );
        assert_eq!(
            classify_serial(" 4C530001234 ", Confidence::Medium),
            Some((text("4C530001234"), Confidence::Medium))
        );
    }

    #[test]
    fn accessors_return_first_matching_value() {
        let id = stick("A1B2C3D4", 42, "sdb");
        assert_eq!(id.text(AttributeKind::Model), Some("Stick"));
        assert_eq!(id.number(AttributeKind::CapacityBytes), Some(42));
        assert_eq!(id.number(AttributeKind::Model), None);
        assert!(id.has_strong_identifier());
    }

    #[test]
    fn sensitive_kinds_are_flagged_for_redaction() {
        assert!(AttributeKind::SerialNumber.is_sensitive());
        assert!(AttributeKind::UsbSerialNumber.is_sensitive());
        assert!(!AttributeKind::Model.is_sensitive());
    }
}
