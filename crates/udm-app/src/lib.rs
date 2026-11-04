//! Application layer for Universal Drive Manager.
//!
//! Phase 0 contains a single use case: read-only device discovery, executed on
//! a dedicated background worker so that the GUI thread never touches the
//! file system or a device ([`discovery::DiscoveryService`]).
//!
//! Operation planning, jobs and the privileged helper are designed in
//! `docs/ADR/0005-operation-planning.md` and
//! `docs/ADR/0006-privileged-helper-boundary.md` but intentionally not
//! implemented yet.

pub mod discovery;

pub use discovery::{
    DiscoveryEvent, DiscoveryFailure, DiscoveryOutcome, DiscoveryService, DiscoverySnapshot,
    RefreshRequest, ServiceError,
};
