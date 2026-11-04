# Architecture

Status: Phase 0 draft (2026-11-04). Describes what exists and what is
designed but not implemented. Decisions are recorded in [`ADR/`](ADR/README.md).

## 1. Goals and non-goals of Phase 0

Phase 0 validates the architecture with a **read-only** prototype:

- Modular workspace with enforced layer boundaries.
- Device identity and capability models.
- Simulated discovery and a virtual-disk test harness.
- Read-only discovery on the available OS (Linux).
- A Slint dashboard that keeps the GUI thread free of I/O and shows
  unsupported, empty, loading and error states explicitly.

Non-goals: any write, format, partition, erase, clone, restore, SMART, eject
or benchmarking operation; the privileged helper; persistence.

## 2. Workspace

```
crates/
  udm-domain     Domain: identity, capability, device, discovery issues (no deps)
  udm-platform   Platform adapters: DeviceDiscovery port, simulated, Linux sysfs, "not implemented"
  udm-app        Application: DiscoveryService (background worker, events, epochs)
  udm-gui        Presentation: Slint UI (ui/*.slint), view models, presenter, binary `udm`
  udm-testkit    Tests only: virtual disks, MBR/GPT writers, fault injection, fake sysfs
tools/update_po.py   Korean translation catalog generator
docs/                Architecture, threat model, platform support, testing, UX, ADRs
```

Dependency direction (ADR-0001): `gui → app → platform → domain`; `gui` also
uses `domain` and `platform` types for view-model conversion. `testkit` is a
dev-dependency only.

## 3. Runtime structure

```
 GUI thread (Slint event loop)                   Worker thread "udm-discovery"
 ─────────────────────────────                   ─────────────────────────────
 MainWindow ── callbacks ──► DiscoveryService ── bounded queue (4) ──► run_worker
     ▲                         request_refresh()   SetSource / Refresh      │
     │                         set_source()                                 ▼
     │                                                     adapter.discover()  (blocking, read-only)
     │                                                     assess_identities()
     │                                                              │
 Presenter.handle_event ◄── upgrade_in_event_loop ◄── sink(DiscoveryEvent::{Started, Finished})
   (drops stale epochs, keeps selection only if identity is not a Mismatch)
```

- The GUI thread never calls an adapter or touches the file system.
- Commands use `try_send`; the GUI never blocks on the queue.
- Refresh requests are coalesced; source changes bump an epoch so stale
  results are discarded.
- Adapter panics are caught and reported as failures.
- On shutdown the worker is joined within 2 s or detached (read-only only;
  ADR-0007).

## 4. Domain model

### 4.1 Identity (ADR-0003)

`DeviceIdentity` = list of `IdentityAttribute { kind, value, provenance, confidence }`.
`compare()` yields `Consistent` only with a matching strong identifier
(serial/WWN/USB serial at ≥ Medium confidence) and a matching capacity;
`Mismatch` on any conflicting stable attribute; `Inconclusive` otherwise.
Paths and OS numbers are volatile and never evidence. Duplicate strong
identifiers across devices are downgraded to Low and reported.

### 4.2 Capabilities (ADR-0004)

Seven capabilities × four states (`Supported`, `Limited(reason)`,
`Unavailable(reason)`, `NotTested(reason)`); unassessed = `NotTested`.
Reasons are typed codes for localization. Real devices in Phase 0 report
`Unavailable(NotImplementedInThisBuild)` (or `NoMedia` / `WriteProtected`).

### 4.3 Device state

`DeviceState { media, write_protection, removable, system_disk }` plus
`VolumeUsage` (mounted / swap / held, including through device-mapper
stacks). State, capability and (future) operation status are separate types.
`SystemDiskStatus::Unknown` must be treated like a system disk.

### 4.4 Discovery issues

Partial failures are `DiscoveryIssue`s (`ProbeFailed`, `DuplicateIdentifier`,
`NoStrongIdentifier`, `SystemDiskUndetermined`, `VirtualDevicesExcluded`)
with a severity. A whole-pass failure is a `DiscoveryError`
(`NotImplemented`, `PermissionDenied`, `Io`, `Simulated`). "No devices" and
"cannot look" are never conflated.

## 5. Platform adapters

| Adapter | Status | Notes |
|---------|--------|-------|
| `SimulatedDiscovery` | Implemented | Scenarios: sample devices, no devices, failure, hot-plug; optional latency |
| `LinuxSysfsDiscovery` | Implemented (read-only, unprivileged) | ADR-0008; injectable paths for tests |
| Windows | Not implemented | `UnimplementedDiscovery` → explicit `NotImplemented` error in UI |
| macOS | Not implemented | same |

Planned per-OS designs are in [`PLATFORM_SUPPORT.md`](PLATFORM_SUPPORT.md).

## 6. Presentation

- `ui/theme.slint`: design tokens; light/dark/system (ADR-0002,
  [`UX_DESIGN_SYSTEM.md`](UX_DESIGN_SYSTEM.md)).
- `ui/types.slint`: enums and structs exchanged with Rust. Rust sends codes,
  not prose; `ui/labels.slint` maps codes to `@tr()` strings.
- `ui/components.slint`: Badge, DeviceCard, CapabilityLine, AttributeLine,
  IssueLine, StateMessage, …
- `ui/main.slint`: dashboard (header with source/theme/language, read-only
  notice, device list, detail panel, discovery notes, status bar).
- `src/view_model.rs`: domain → view-model conversion and formatting
  (SI and IEC units, digit grouping) — unit-tested.
- `src/presenter.rs`: applies events, epoch filtering, safe selection.
- `src/main.rs`: startup, refuses to run as root (Unix), logging
  (`UDM_LOG`), language selection (`UDM_LANG`/`LANG`), wiring.
- Translations: `lang/ko/LC_MESSAGES/udm-gui.po`, bundled at build time.

## 7. Designed, not implemented

- Operation planning with typestate and revalidation — ADR-0005.
- Privileged helper with a narrow, versioned protocol — ADR-0006.
- Job engine with stages, progress, cancellation and verification — ADR-0007
  consequences.
- Persistence (SQLite history), image I/O, hashing, benchmarking, reports.

## 8. Cross-cutting rules

- `unsafe_code = "deny"` workspace-wide; one documented exception
  (`geteuid`).
- Strict Clippy set (no `unwrap`/`expect`/`panic` outside tests, cast lints).
- Logs: `tracing`; serials, WWNs and persistent links are flagged
  `is_sensitive()` and are not logged by the application layer. No telemetry.
