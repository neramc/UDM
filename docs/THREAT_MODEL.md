# Threat Model

Status: Phase 0 (2026-11-04); ADR-0005 and ADR-0006 accepted. Phase 0 performs read-only discovery
only; most threats below concern later phases and drive design decisions now.

## 1. Assets

1. **User data on storage devices** (primary asset). Loss is usually
   irreversible.
2. **Integrity of the running operating system** (system disk, boot
   partitions, swap).
3. **Privileges** of the user and, later, of the privileged helper.
4. **Device metadata** (serial numbers, mount paths containing user names) —
   personal data.
5. **Trust in reported results** (verification status, diagnostics).

## 2. Actors

- The local user (trusted to authorize, but can make mistakes).
- Other local unprivileged processes (untrusted; may try to drive the
  helper).
- Malicious or malformed media (untrusted content: partition tables,
  filesystems, images).
- Malicious or buggy hardware (fake capacity flash, bridges lying about
  identity, devices disappearing mid-operation).
- Supply chain (crates, toolchain, CI).

## 3. Trust boundaries

```
[User] → [GUI process, unprivileged] → [Application/worker threads]
                                           │ (Phase 1+) versioned IPC
                                           ▼
                                  [Privileged helper, per-job]  → [OS storage APIs] → [Device]
```

In Phase 0 only the first two boundaries exist; the GUI refuses to start as
root on Unix.

## 4. Threats and mitigations

| # | Threat | Phase 0 status | Mitigation (existing / planned) |
|---|--------|----------------|----------------------------------|
| T1 | **Wrong target**: writing to a different device than authorized (path reuse after replug, index shift) | No writes exist | Identity model (ADR-0003): paths never identity; `compare` requires strong identifier + capacity. Planned: revalidation immediately before execution and inside the helper (ADR-0005/0006); abort on `Mismatch`; never redirect. UI never auto-selects a destructive target; selection is dropped if the device under a key changes (implemented). |
| T2 | **Identical devices** with duplicated or placeholder serials | Implemented detection | Duplicates downgraded to Low and reported; placeholder serials Low; `NoStrongIdentifier` warning; planned extra confirmation for `Inconclusive`. |
| T3 | **Destroying the running system** | Detection implemented | System disk detection through holders (LUKS/LVM/RAID); `Unknown` when undeterminable, never "not system". Accepted design (ADR-0005): the planner refuses system and `Unknown` targets, with no override in the first implementation. |
| T4 | **TOCTOU** between confirmation and execution | Design | Typestate plan; revalidate right before execution and again in the helper; exclusive open (`O_EXCL` on Linux block devices, volume lock/dismount on Windows, DiskArbitration claim/unmount on macOS — to be verified per OS). |
| T5 | **Privilege escalation via helper** (crafted IPC, command injection) | No helper | Closed request set, no command/paths-to-executables, size limits, version check, peer authentication, helper revalidates everything (ADR-0006). No shell execution anywhere (policy; no `std::process::Command` in the workspace). |
| T6 | **Running the whole GUI elevated** | Enforced on Unix | `geteuid() == 0` → refuse with exit code 2. Windows elevation check not implemented (documented gap). |
| T7 | **Malformed on-disk structures** (partition tables, images) crash or exploit parsers | Not parsed in Phase 0 | Planned: parsers in safe Rust with bounds checks, fuzzing (`cargo fuzz`) before shipping image/partition inspection; never load whole images into memory. |
| T8 | **Malformed OS metadata** (sysfs/mountinfo oddities) | Implemented | Parsers tolerate malformed lines and report `Malformed` issues; mount-point unescaping handles octal escapes; no panics (lint-enforced). |
| T9 | **False success** (reporting success after a failed flush/verify, or after a cancellation that did not stop the worker) | N/A | Planned job stages; success only after verification; cancellation reported only at a safe terminal state (ADR-0007). Harness already injects flush failures, short writes, disconnects and silent corruption. |
| T10 | **Misleading capability or health claims** (e.g. "secure erase" on flash, "healthy" when SMART unavailable) | Implemented model | Four-state capabilities with reasons; `FlashErasureNotGuaranteed`; unassessed = Not tested; UI legend says Not tested ≠ supported. |
| T11 | **Leaking personal data** (serials, user names in mount paths) via logs | Partially | Sensitive attribute kinds flagged; app layer logs counts only; discovery example redacts by default. Planned: redaction helpers for diagnostic reports; no telemetry. |
| T12 | **GUI freeze / denial of service** by hung device I/O | Implemented for discovery | Discovery on a worker; non-blocking commands; bounded queue; shutdown does not hang on stuck read-only worker. |
| T13 | **Supply chain** | Partially | Few direct dependencies; Slint pinned exactly; `Cargo.lock` committed; `cargo deny check` (RustSec advisories, GPLv3-compatible licenses, crates.io only) passes and runs in CI. Planned: reproducible release builds, signed releases. |
| T14 | **Recovery source modified** during recovery imaging | N/A | Planned: recovery sources opened read-only by default; writes to a source require a distinct, explicit flow (CLAUDE.md rule 8). |
| T15 | **Fake-capacity flash** (device reports more capacity than it has) | N/A | Planned: verification by pattern write/read over the full range before trusting capacity; harness has deterministic position-dependent patterns. |

## 5. Assumptions

- The OS kernel and its storage drivers are trusted.
- The user's session is not already compromised (a compromised session can
  invoke the same authorization prompts).
- Device-reported metadata is *untrusted input*: it may be wrong, missing or
  adversarial; it is displayed with provenance, never interpreted as proof.

## 6. Open questions

1. ~~Should `SystemDiskStatus::Unknown` be overridable?~~ Decided in
   ADR-0005 (accepted): no override in the first implementation; a future
   override (e.g. for rescue systems) needs its own ADR.
2. Policy for running under root on rescue distributions that only provide a
   root session (currently: refuse).
3. Windows elevation detection and UAC-based helper launch — security review
   of named-pipe ACLs required.
4. macOS: SMAppService helper signing/notarization requirements for an
   open-source distribution.
5. Whether to sign release binaries and how users verify them.
