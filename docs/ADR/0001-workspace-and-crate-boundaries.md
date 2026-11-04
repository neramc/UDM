# ADR-0001: Workspace layout and crate boundaries

- Status: Accepted
- Date: 2026-11-04

## Context

CLAUDE.md requires a modular Rust workspace with clear dependency boundaries
between presentation, application, domain, infrastructure, platform adapters
and a privileged helper, without abstractions that have no demonstrated
purpose. Phase 0 needs only read-only discovery, a dashboard and a test
harness.

## Decision

Phase 0 uses five crates. Arrows point from a crate to what it depends on.

```
udm-gui ──► udm-app ──► udm-platform ──► udm-domain
   │           │                            ▲
   └───────────┴────────────────────────────┘
udm-testkit  (dev-dependency only; depends on nothing in the workspace)
```

| Crate | Layer | Responsibility | May depend on |
|-------|-------|----------------|---------------|
| `udm-domain` | Domain | Identity, capability, device, discovery-issue models and pure rules | std only (no third-party dependency) |
| `udm-platform` | Platform adapters | `DeviceDiscovery` port; simulated, Linux sysfs and explicit "not implemented" adapters | `udm-domain`, `thiserror`, `tracing` |
| `udm-app` | Application | Background discovery service, event delivery, identity post-processing | `udm-domain`, `udm-platform` |
| `udm-gui` | Presentation | Slint UI, view models, presenter, binary `udm` | all of the above, `slint` |
| `udm-testkit` | Test infrastructure | Virtual disks, fault injection, fake sysfs | `tempfile` |

Rules:

1. `udm-domain` never depends on Slint, OS APIs, or I/O.
2. Only `udm-platform` contains OS-specific code, behind `#[cfg(target_os)]`.
   OS-specific dependencies are declared as target-specific dependencies.
3. The GUI never calls an adapter directly; it goes through `udm-app`.
4. `udm-testkit` is only ever a dev-dependency and never touches devices.
5. `unsafe_code` is denied workspace-wide; the single exception (the
   `geteuid` call in the GUI binary) is annotated and justified locally.

Planned crates, created only when their first feature lands:

- `udm-io` (infrastructure): streaming image I/O, hashing, verification.
- `udm-store` (infrastructure): SQLite history and profiles.
- `udm-helper-protocol` + `udm-helper` (privileged helper, ADR-0006).
- Splitting `udm-platform` into per-OS crates if the OS-specific dependency
  graphs become large (e.g. the `windows` crate).

## Alternatives considered

- **One crate with modules**: simplest, but boundaries would only be enforced
  by convention, and the GUI toolkit would be compiled into domain tests.
- **Per-OS adapter crates now**: clearer dependency isolation, but premature
  while two of three adapters are stubs.
- **Port traits in `udm-domain`**: purer hexagonal layout, but the port's
  error type is I/O-flavoured; keeping it in `udm-platform` avoids an extra
  crate with no behaviour.

## Consequences

- Domain tests compile in seconds and run on every platform.
- Adding Windows/macOS discovery only touches `udm-platform`.
- When the privileged helper arrives, the helper protocol crate must not
  depend on `udm-gui` or `udm-app`.
