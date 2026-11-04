# ADR-0006: Privileged helper process boundary

- Status: **Accepted** (design approved by the project owner on 2026-11-04)
- Date: 2026-11-04
- Scope of acceptance: the process boundary, protocol rules and
  revalidation requirement are binding. The per-OS mechanisms in item 4 are
  the intended direction and must be confirmed by prototypes; deviations
  need an ADR update. No helper exists yet, and the GUI keeps refusing to
  start with root privileges.

## Context

Raw device access, partitioning and unmount/eject often need elevated
privileges. CLAUDE.md forbids running the whole GUI elevated and forbids
arbitrary command execution. Each OS offers different elevation mechanisms
(see `docs/PLATFORM_SUPPORT.md`).

## Decision

1. A separate executable `udm-helper` performs only privileged operations.
   The GUI and application layer run unprivileged.
2. **Narrow, versioned, structured protocol** (`udm-helper-protocol` crate):
   - Length-prefixed frames with a protocol version and request ID.
   - Closed set of typed requests, e.g. `OpenForRead { target }`,
     `WriteImage { plan }`, `Unmount { target }`, `Eject { target }`,
     `Cancel { request_id }`, `Shutdown`. No "run command", no paths to
     executables, no shell strings.
   - `target` is a `TargetSnapshot` (identity attributes + expected capacity +
     OS binding), never a bare path.
   - Bounded message sizes; unknown fields or versions are rejected.
3. **Revalidation inside the boundary**: the helper re-discovers the target
   itself and repeats identity, capacity, system-disk and usage checks before
   acting. It does not trust any state computed by the unprivileged side.
4. **Least privilege per OS** (to be prototyped):
   - Linux: prefer udisks2 over D-Bus (`OpenForBackup`/`OpenForRestore`,
     `Unmount`, `PowerOff`) with polkit authorization, so that no custom
     setuid/root binary is needed for common operations; a pkexec-launched
     helper only if udisks2 cannot cover an operation.
   - Windows: helper launched on demand with UAC elevation (`runas`) per job,
     communicating over a named pipe with an ACL restricted to the invoking
     user; no permanently installed service in the first iteration.
   - macOS: helper registered via `SMAppService` (macOS 13+), code-signed,
     communicating over XPC; `authopen` considered for read-only raw access.
5. The helper's lifetime is bounded to one job (or an idle timeout), and it
   logs with the same correlation ID as the job.

## Threats addressed

Confused deputy (helper writing to a device the user did not authorize),
privilege escalation via crafted requests, TOCTOU between authorization and
execution. See `docs/THREAT_MODEL.md`.

## Open questions (to resolve during implementation)

- Exact authentication of the GUI to the helper on each OS (peer credentials
  on Unix sockets / D-Bus sender, named-pipe client PID and token on Windows,
  XPC code-signing requirement on macOS).
- Packaging impact: Flatpak/Snap sandboxes block raw `/dev` access; whether
  udisks2 access through the sandbox is sufficient is unverified.
- Code-signing and notarization requirements and cost (macOS, Windows
  SmartScreen) for an open-source project.

## Consequences

- Phase 1 must start with a protocol crate plus a fake helper used in tests,
  before any real privileged code.
