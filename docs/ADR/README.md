# Architecture Decision Records

Each ADR records one decision, its context, the alternatives considered and
its consequences. ADRs are immutable once **Accepted**; a later ADR may
supersede an earlier one.

Status values: **Proposed** (needs review or an owner decision),
**Accepted**, **Superseded by ADR-NNNN**.

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-workspace-and-crate-boundaries.md) | Workspace layout and crate boundaries | Accepted |
| [0002](0002-gui-toolkit-slint.md) | Slint as GUI toolkit and the project license (GPL-3.0-or-later) | Accepted |
| [0003](0003-device-identity-model.md) | Device identity as attributes with provenance and confidence | Accepted |
| [0004](0004-capability-model.md) | Explicit four-state capability model | Accepted |
| [0005](0005-operation-planning.md) | Typed operation plans and revalidation | Accepted (design; not implemented) |
| [0006](0006-privileged-helper-boundary.md) | Privileged helper process boundary | Accepted (design; not implemented) |
| [0007](0007-concurrency-and-io-model.md) | Concurrency and I/O model | Accepted |
| [0008](0008-linux-discovery-via-sysfs.md) | Read-only Linux discovery via sysfs | Accepted |

Template: copy an existing ADR and keep the sections *Context*, *Decision*,
*Alternatives considered*, *Consequences*.
