// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Operating-system identification used in reports and capability reasons.

/// The operating-system family an observation or adapter belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlatformKind {
    Linux,
    Windows,
    MacOs,
    /// Any target the project does not support (for example a BSD).
    Other,
}

impl PlatformKind {
    /// The platform this binary was compiled for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Other
        }
    }

    /// A stable, non-localized identifier suitable for logs and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::MacOs => "macos",
            Self::Other => "other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_platform_matches_compilation_target() {
        let current = PlatformKind::current();
        #[cfg(target_os = "linux")]
        assert_eq!(current, PlatformKind::Linux);
        #[cfg(target_os = "windows")]
        assert_eq!(current, PlatformKind::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(current, PlatformKind::MacOs);
        assert!(!current.as_str().is_empty());
    }
}
