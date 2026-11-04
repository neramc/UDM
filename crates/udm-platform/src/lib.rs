//! Platform adapters for Universal Drive Manager.
//!
//! Every adapter implements [`DeviceDiscovery`], a **read-only** port: Phase 0
//! contains no adapter method that can modify a device.
//!
//! | Adapter                         | Platforms | Status in Phase 0                       |
//! |---------------------------------|-----------|-----------------------------------------|
//! | [`simulated::SimulatedDiscovery`] | all     | Implemented                             |
//! | `linux::LinuxSysfsDiscovery`    | Linux     | Implemented, read-only, unprivileged    |
//! | [`UnimplementedDiscovery`]      | Windows, macOS, others | Reports `NotImplemented` explicitly |
//!
//! Adapters run on a background worker owned by the application layer and may
//! block; they must never be called from the GUI thread.
//!
//! See `docs/PLATFORM_SUPPORT.md` and `docs/ADR/0008-linux-discovery-via-sysfs.md`.

use std::io;

use udm_domain::{Device, DiscoveryIssue, DiscoverySource, PlatformKind};

#[cfg(target_os = "linux")]
pub mod linux;
pub mod simulated;

/// Result of one successful discovery pass. Partial failures are reported in
/// `issues`; they never cause devices to be silently dropped or invented.
#[derive(Debug, Default)]
pub struct DiscoveryReport {
    pub devices: Vec<Device>,
    pub issues: Vec<DiscoveryIssue>,
}

/// Failure of a whole discovery pass.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    /// The platform has no discovery implementation in this build.
    #[error("device discovery is not implemented for {} in this build", .platform.as_str())]
    NotImplemented { platform: PlatformKind },
    /// The device list itself could not be read because of missing permissions.
    #[error("permission denied while listing storage devices")]
    PermissionDenied {
        #[source]
        source: io::Error,
    },
    /// The device list itself could not be read.
    #[error("failed to list storage devices")]
    Io {
        #[source]
        source: io::Error,
    },
    /// Failure produced on purpose by a simulated scenario.
    #[error("simulated discovery failure")]
    Simulated,
}

impl DiscoveryError {
    /// Classifies an I/O error that prevented listing devices.
    #[must_use]
    pub fn from_listing_error(source: io::Error) -> Self {
        if source.kind() == io::ErrorKind::PermissionDenied {
            Self::PermissionDenied { source }
        } else {
            Self::Io { source }
        }
    }
}

/// Read-only device discovery port.
///
/// Implementations may block (they read OS interfaces) and must therefore only
/// be called from a background worker.
pub trait DeviceDiscovery: Send {
    /// Where results of this adapter come from.
    fn source(&self) -> DiscoverySource;

    /// Performs one discovery pass.
    ///
    /// # Errors
    /// Returns [`DiscoveryError`] when the device list cannot be produced at
    /// all. Problems with individual devices or attributes are reported as
    /// issues inside the [`DiscoveryReport`] instead.
    fn discover(&mut self) -> Result<DiscoveryReport, DiscoveryError>;
}

/// Placeholder adapter for platforms without a discovery implementation.
/// It always fails with [`DiscoveryError::NotImplemented`]; it never returns an
/// empty list, because "no devices" and "cannot look" must stay distinct.
#[derive(Debug, Clone, Copy)]
pub struct UnimplementedDiscovery {
    platform: PlatformKind,
}

impl UnimplementedDiscovery {
    #[must_use]
    pub const fn new(platform: PlatformKind) -> Self {
        Self { platform }
    }
}

impl DeviceDiscovery for UnimplementedDiscovery {
    fn source(&self) -> DiscoverySource {
        DiscoverySource::System {
            platform: self.platform,
        }
    }

    fn discover(&mut self) -> Result<DiscoveryReport, DiscoveryError> {
        Err(DiscoveryError::NotImplemented {
            platform: self.platform,
        })
    }
}

/// The read-only discovery adapter for the running platform.
#[must_use]
pub fn system_discovery() -> Box<dyn DeviceDiscovery> {
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxSysfsDiscovery::new())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Box::new(UnimplementedDiscovery::new(PlatformKind::current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_platform_fails_explicitly_instead_of_returning_no_devices() {
        let mut adapter = UnimplementedDiscovery::new(PlatformKind::Windows);
        let err = adapter.discover().unwrap_err();
        assert!(matches!(
            err,
            DiscoveryError::NotImplemented {
                platform: PlatformKind::Windows
            }
        ));
        assert_eq!(
            err.to_string(),
            "device discovery is not implemented for windows in this build"
        );
    }

    #[test]
    fn system_discovery_reports_the_current_platform() {
        let adapter = system_discovery();
        assert_eq!(
            adapter.source(),
            DiscoverySource::System {
                platform: PlatformKind::current()
            }
        );
    }

    #[test]
    fn listing_errors_are_classified() {
        let denied =
            DiscoveryError::from_listing_error(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(matches!(denied, DiscoveryError::PermissionDenied { .. }));
        let other = DiscoveryError::from_listing_error(io::Error::from(io::ErrorKind::NotFound));
        assert!(matches!(other, DiscoveryError::Io { .. }));
    }
}
