// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-independent domain model for Universal Drive Manager.
//!
//! This crate must not depend on Slint, on operating-system APIs, or on any I/O.
//! It defines the vocabulary the rest of the workspace uses to talk about
//! storage devices:
//!
//! * [`identity`]: a device identity is a *collection of observed attributes*,
//!   each with provenance and confidence. A device path is never an identity.
//! * [`capability`]: every operation is described by an explicit
//!   [`capability::CapabilityStatus`]; a missing entry means "not tested",
//!   never "supported" and never "healthy".
//! * [`device`]: devices, partitions and observed device state.
//! * [`discovery`]: the result of one discovery pass, including partial
//!   failures reported as typed issues.
//!
//! See `docs/ARCHITECTURE.md` and `docs/ADR/` for the reasoning behind these
//! types.

pub mod capability;
pub mod device;
pub mod discovery;
pub mod identity;
pub mod platform;

pub use capability::{Capability, CapabilityReason, CapabilitySet, CapabilityStatus};
pub use device::{
    Device, DeviceKey, DeviceState, MediaState, Partition, SystemDiskStatus, Transport,
    VolumeUsage, WriteProtection,
};
pub use discovery::{
    DiscoveryIssue, DiscoveryIssueKind, DiscoverySource, IssueSeverity, ProbeFailure, ProbeTarget,
    assess_identities,
};
pub use identity::{
    AttributeKind, AttributeValue, Confidence, DeviceIdentity, IdentityAttribute,
    IdentityComparison, ObservationSource, Provenance,
};
pub use platform::PlatformKind;
