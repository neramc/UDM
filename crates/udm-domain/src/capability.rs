//! Explicit capability model.
//!
//! Every [`Capability`] of every device has a [`CapabilityStatus`]. The four
//! states mirror the project rules:
//!
//! * `Supported`   – verified to work for this device on this platform.
//! * `Limited`     – works with restrictions; the reason says which.
//! * `Unavailable` – known not to work; the reason says why.
//! * `NotTested`   – nobody has established either way.
//!
//! A capability that was never assessed is reported as
//! `NotTested(NotAssessed)`. It must never be read as `Supported`, and a
//! missing diagnostic capability must never be read as a healthy device.
//!
//! Reasons are typed codes rather than free text so that the UI can localize
//! them. See `docs/ADR/0004-capability-model.md`.

use std::collections::BTreeMap;

/// Operations whose availability is tracked per device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    /// Read raw sectors from the whole device.
    RawRead,
    /// Write raw sectors to the whole device (destructive).
    RawWrite,
    /// Flush, unmount and request the OS to prepare the device for removal.
    SafeEject,
    /// Read SMART / health data.
    SmartAccess,
    /// Create, delete or resize partitions (destructive).
    PartitionEdit,
    /// Erase with an assurance level stronger than a plain overwrite (destructive).
    SecureErase,
    /// Check a filesystem for consistency.
    FilesystemCheck,
}

impl Capability {
    /// Every capability, in display order.
    pub const ALL: [Self; 7] = [
        Self::RawRead,
        Self::RawWrite,
        Self::SafeEject,
        Self::SmartAccess,
        Self::PartitionEdit,
        Self::SecureErase,
        Self::FilesystemCheck,
    ];

    /// Whether exercising this capability can destroy data.
    #[must_use]
    pub const fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::RawWrite | Self::PartitionEdit | Self::SecureErase
        )
    }

    /// A stable, non-localized identifier for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RawRead => "raw_read",
            Self::RawWrite => "raw_write",
            Self::SafeEject => "safe_eject",
            Self::SmartAccess => "smart_access",
            Self::PartitionEdit => "partition_edit",
            Self::SecureErase => "secure_erase",
            Self::FilesystemCheck => "filesystem_check",
        }
    }
}

/// Why a capability is not plainly `Supported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityReason {
    /// Nobody has assessed this capability for this device.
    NotAssessed,
    /// The feature is not implemented in this build (Phase 0 is read-only).
    NotImplementedInThisBuild,
    /// Requires the privileged helper, which does not exist yet.
    RequiresPrivilegedHelper,
    /// The current user lacks permission to access the device node.
    PermissionDenied,
    /// The device reports that it is write-protected.
    WriteProtected,
    /// No media is present (e.g. an empty card reader).
    NoMedia,
    /// The transport does not carry this kind of command (e.g. SMART over a
    /// plain USB mass-storage bridge).
    TransportUnsupported,
    /// A USB bridge may or may not pass the command through; unverified.
    BridgePassthroughUnverified,
    /// Overwriting flash storage cannot guarantee physical erasure because of
    /// wear levelling and over-provisioning.
    FlashErasureNotGuaranteed,
}

impl CapabilityReason {
    /// A stable, non-localized identifier for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotAssessed => "not_assessed",
            Self::NotImplementedInThisBuild => "not_implemented_in_this_build",
            Self::RequiresPrivilegedHelper => "requires_privileged_helper",
            Self::PermissionDenied => "permission_denied",
            Self::WriteProtected => "write_protected",
            Self::NoMedia => "no_media",
            Self::TransportUnsupported => "transport_unsupported",
            Self::BridgePassthroughUnverified => "bridge_passthrough_unverified",
            Self::FlashErasureNotGuaranteed => "flash_erasure_not_guaranteed",
        }
    }
}

/// The assessed status of one capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityStatus {
    Supported,
    Limited(CapabilityReason),
    Unavailable(CapabilityReason),
    NotTested(CapabilityReason),
}

impl CapabilityStatus {
    /// The status of a capability nobody has assessed.
    pub const UNASSESSED: Self = Self::NotTested(CapabilityReason::NotAssessed);

    /// The reason attached to a non-`Supported` status.
    #[must_use]
    pub const fn reason(self) -> Option<CapabilityReason> {
        match self {
            Self::Supported => None,
            Self::Limited(reason) | Self::Unavailable(reason) | Self::NotTested(reason) => {
                Some(reason)
            }
        }
    }

    /// A stable, non-localized identifier for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::Limited(_) => "limited",
            Self::Unavailable(_) => "unavailable",
            Self::NotTested(_) => "not_tested",
        }
    }
}

/// Capability statuses for one device.
///
/// Lookups of capabilities that were never set return
/// [`CapabilityStatus::UNASSESSED`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    entries: BTreeMap<Capability, CapabilityStatus>,
}

impl CapabilitySet {
    /// A set in which every capability is unassessed.
    #[must_use]
    pub fn unassessed() -> Self {
        Self::default()
    }

    /// Sets the status of one capability, replacing any previous status.
    pub fn set(&mut self, capability: Capability, status: CapabilityStatus) {
        self.entries.insert(capability, status);
    }

    /// Builder-style variant of [`CapabilitySet::set`].
    #[must_use]
    pub fn with(mut self, capability: Capability, status: CapabilityStatus) -> Self {
        self.set(capability, status);
        self
    }

    /// The status of `capability`; unassessed capabilities are `NotTested`.
    #[must_use]
    pub fn get(&self, capability: Capability) -> CapabilityStatus {
        self.entries
            .get(&capability)
            .copied()
            .unwrap_or(CapabilityStatus::UNASSESSED)
    }

    /// Every capability with its status, in [`Capability::ALL`] order.
    /// Always yields exactly one entry per capability.
    pub fn iter(&self) -> impl Iterator<Item = (Capability, CapabilityStatus)> + '_ {
        Capability::ALL.into_iter().map(|c| (c, self.get(c)))
    }

    /// Downgrades every destructive capability that is currently `Supported`
    /// or `Limited` to `Unavailable(reason)`. Used for device-level facts that
    /// rule out writing, such as write protection or missing media.
    ///
    /// Policy decisions (system disk, mounted volumes, ...) are *not* encoded
    /// as capabilities; they belong to operation planning (ADR-0005).
    pub fn restrict_destructive(&mut self, reason: CapabilityReason) {
        for capability in Capability::ALL {
            if !capability.is_destructive() {
                continue;
            }
            match self.get(capability) {
                CapabilityStatus::Supported | CapabilityStatus::Limited(_) => {
                    self.set(capability, CapabilityStatus::Unavailable(reason));
                }
                CapabilityStatus::Unavailable(_) | CapabilityStatus::NotTested(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_capability_is_not_tested_never_supported() {
        let set = CapabilitySet::unassessed();
        for capability in Capability::ALL {
            assert_eq!(
                set.get(capability),
                CapabilityStatus::NotTested(CapabilityReason::NotAssessed)
            );
        }
    }

    #[test]
    fn iter_always_lists_every_capability_once() {
        let set =
            CapabilitySet::unassessed().with(Capability::RawRead, CapabilityStatus::Supported);
        let listed: Vec<_> = set.iter().map(|(c, _)| c).collect();
        assert_eq!(listed, Capability::ALL.to_vec());
        assert_eq!(set.get(Capability::RawRead), CapabilityStatus::Supported);
    }

    #[test]
    fn restrict_destructive_only_touches_destructive_capabilities() {
        let mut set = CapabilitySet::unassessed()
            .with(Capability::RawRead, CapabilityStatus::Supported)
            .with(Capability::RawWrite, CapabilityStatus::Supported)
            .with(
                Capability::SecureErase,
                CapabilityStatus::Limited(CapabilityReason::FlashErasureNotGuaranteed),
            );
        set.restrict_destructive(CapabilityReason::WriteProtected);
        assert_eq!(set.get(Capability::RawRead), CapabilityStatus::Supported);
        assert_eq!(
            set.get(Capability::RawWrite),
            CapabilityStatus::Unavailable(CapabilityReason::WriteProtected)
        );
        assert_eq!(
            set.get(Capability::SecureErase),
            CapabilityStatus::Unavailable(CapabilityReason::WriteProtected)
        );
        // Not-tested stays not-tested: restricting never invents an assessment.
        assert_eq!(
            set.get(Capability::PartitionEdit),
            CapabilityStatus::UNASSESSED
        );
    }

    #[test]
    fn destructive_classification() {
        assert!(Capability::RawWrite.is_destructive());
        assert!(Capability::PartitionEdit.is_destructive());
        assert!(Capability::SecureErase.is_destructive());
        assert!(!Capability::RawRead.is_destructive());
        assert!(!Capability::SafeEject.is_destructive());
        assert!(!Capability::SmartAccess.is_destructive());
        assert!(!Capability::FilesystemCheck.is_destructive());
    }

    #[test]
    fn reasons_are_exposed() {
        assert_eq!(CapabilityStatus::Supported.reason(), None);
        assert_eq!(
            CapabilityStatus::Limited(CapabilityReason::NoMedia).reason(),
            Some(CapabilityReason::NoMedia)
        );
    }
}
