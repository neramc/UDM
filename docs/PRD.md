# Product Requirements Document — Universal Drive Manager

- Status: Draft v0.1, 2026-11-04 — requirements and priorities to be reviewed
  by the project owner before each phase starts.
- Related: [CLAUDE.md](../CLAUDE.md) (project rules), [ARCHITECTURE](ARCHITECTURE.md),
  [THREAT_MODEL](THREAT_MODEL.md), [PLATFORM_SUPPORT](PLATFORM_SUPPORT.md),
  [ADRs](ADR/README.md).
- License: GPL-3.0-or-later (ADR-0002).

## 1. Vision

A free, open-source, cross-platform desktop application that lets people
manage USB and external storage **safely and honestly**: one place to see
what a device is, what can be done with it on this computer, and to perform
imaging, verification, backup, cloning, partitioning, diagnostics and
secure-erase tasks — with explicit authorization, verification and an audit
trail, and without ever pretending a capability or a result that does not
exist.

## 2. Problem statement

People who work with removable storage typically combine several
single-purpose tools (image writers, partition editors, SMART utilities,
benchmark tools, OS eject dialogs). Recurring problems:

1. **Wrong-target data loss**: device names and numbers change on reconnect;
   identical sticks look the same; tools select targets silently.
2. **Unclear capability**: it is often unclear whether SMART, secure erase,
   or partition editing actually works for a device on a given OS, and tools
   tend to report "success" or "healthy" when a feature was simply
   unavailable.
3. **Unverified results**: writes are reported as complete before data is
   flushed or verified.
4. **Inconsistent cross-platform behaviour** and undocumented limitations.
5. **Privilege handling**: tools ask to run entirely as administrator/root.

## 3. Target users

| Persona | Needs | Key requirements |
|---------|-------|------------------|
| **P1 Everyday user** | Write a bootable image to a USB stick, back up a stick or SD card, eject safely | Clear device identification, guarded confirmation, verification by default, plain-language errors |
| **P2 Technician / power user** | Image, clone, verify and diagnose many devices; keep records | Batch-friendly workflows (later), hashes, reports, history, keyboard operation |
| **P3 Recovery-assisting user** | Copy data off a failing device before it dies | Read-only source handling, retries/skip of bad areas, error maps |
| **P4 Users with accessibility or language needs** | Operate with keyboard and screen readers; Korean UI | WCAG-level contrast, full keyboard paths, Korean and English at launch |

## 4. Goals and non-goals

### Goals

- G1 **No data loss caused by the application selecting or switching the
  wrong target** (CLAUDE.md §4, ADR-0003, ADR-0005).
- G2 **Honest reporting**: every capability is Supported / Limited /
  Unavailable / Not tested with a reason; success only after verification.
- G3 **Cross-platform** on Windows, Linux and macOS, with platform
  differences documented rather than hidden.
- G4 **Least privilege**: unprivileged GUI, narrow privileged helper
  (ADR-0006).
- G5 **Accessible and localized** from the first release.
- G6 **Free software**, no telemetry, local-only history.

### Non-goals (for 1.0)

- File-level data recovery (carving, undelete) — only *recovery-assistance
  imaging* is in scope.
- Filesystem repair beyond invoking validated OS facilities through the
  platform adapter (no own repair engine).
- Managing internal system disks: they are shown, but destructive
  operations on system disks are refused (ADR-0005).
- RAID/LVM/ZFS/Storage Spaces management, network storage, cloud backup.
- Mobile platforms, CLI-first workflows, remote management.
- Guaranteed physical erasure claims for flash media (CLAUDE.md rule 7).

## 5. Feature requirements

Priority: **M** must, **S** should, **C** could. Phase refers to §8.
Each feature must satisfy the safety requirements in §6 and the Definition
of Done in CLAUDE.md §13.

### F1 Device discovery and identity

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| DISC-1 | List removable and external storage devices with capacity, transport, vendor/model as reported, media and write-protection state | M | 0 (Linux) / 1 |
| DISC-2 | Show identity evidence with provenance and confidence; never use a path as identity | M | 0 |
| DISC-3 | Flag devices without a usable strong identifier and duplicate serial numbers | M | 0 |
| DISC-4 | Mark system disks; treat undeterminable cases as system disks | M | 0 (Linux) / 1 |
| DISC-5 | Show partitions and usage (mounted, swap, held by stacked devices) | M | 0 (Linux) / 1 |
| DISC-6 | Update automatically on hot-plug | S | 1 |
| DISC-7 | Show unmounted filesystem type and label | S | 2 |

### F2 Capability reporting

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| CAP-1 | Report all capabilities with one of four states and a localized reason | M | 0 |
| CAP-2 | Never display "healthy" or "supported" for an unassessed capability | M | 0 |
| CAP-3 | Assess capabilities actively per device and OS when the feature exists | M | each feature |

### F3 Safe eject

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| EJ-1 | Unmount all volumes of a device, flush, and request removal through the OS | M | 2 |
| EJ-2 | Report OS vetoes (open files, processes) with the reason, never as success | M | 2 |

### F4 Image inspection, writing and verification

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| IMG-1 | Inspect raw images (`.img`, `.iso`): size, partition table, hash | M | 2 |
| IMG-2 | Write an image to a removable device through the full safety lifecycle (§6) | M | 2 |
| IMG-3 | Verify written data by read-back comparison by default; report mismatches with offsets | M | 2 |
| IMG-4 | Compute and compare SHA-256 (user-supplied expected hash optional) | M | 2 |
| IMG-5 | Stream with bounded memory; never load a whole image | M | 2 |
| IMG-6 | Compressed images (`.gz`, `.xz`, `.zst`) | S | 3 |

### F5 Backup, clone and restore

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| BAK-1 | Read a whole device into an image file with hash and verification | M | 3 |
| BAK-2 | Restore an image to a device (same lifecycle as IMG-2) | M | 3 |
| BAK-3 | Clone device to device; source opened read-only; refuse same device | M | 3 |
| BAK-4 | Sparse/used-blocks-only backup where the filesystem is understood | C | 5 |

### F6 Partition and filesystem management

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| PART-1 | Create a new partition table (GPT/MBR) and a single partition | M | 4 |
| PART-2 | Format with common filesystems (FAT32, exFAT; NTFS/ext4 where the OS supports) using OS facilities | M | 4 |
| PART-3 | Delete partitions | S | 4 |
| PART-4 | Resize/move partitions | C | 6 |
| PART-5 | Filesystem check through OS facilities, reported with capability state | S | 4 |

### F7 Diagnostics and benchmarking

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| DIAG-1 | SMART / NVMe health where available; explicit Unavailable/Limited otherwise | S | 4 |
| DIAG-2 | Read-only surface scan with error map and timing | S | 4 |
| DIAG-3 | Capacity authenticity check (detect fake-capacity flash) — destructive variant only with full authorization; read-only heuristics clearly labelled | S | 4 |
| DIAG-4 | Sequential and random read benchmark; write benchmark only with explicit authorization or on a file in free space | S | 4 |
| DIAG-5 | Report measurement conditions (block size, queue depth, caching) with every result | M | 4 |

### F8 Security and integrity tools

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| SEC-1 | Erase by overwrite with honest assurance text for flash media | S | 5 |
| SEC-2 | Device-native sanitize/secure erase (ATA, NVMe) only where verified to be supported; report assurance level | C | 6 |
| SEC-3 | Hash files and devices (SHA-256) | S | 2 |

### F9 Storage usage analysis

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| USE-1 | Show used/free space per mounted volume | S | 2 |
| USE-2 | Directory size breakdown for a mounted volume (read-only, cancellable) | C | 5 |

### F10 Recovery-assistance imaging

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| REC-1 | Image a failing device read-only with retries, skipping and an error map | S | 5 |
| REC-2 | Resume an interrupted recovery image | C | 5 |
| REC-3 | Never write to the recovery source by default | M | 5 |

### F11 Operation planning, jobs and history

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| JOB-1 | Typed operation plans with validation, consequence summary, authorization and revalidation (ADR-0005) | M | 1 |
| JOB-2 | Job progress with stage, bytes, throughput, ETA, cancellation state and errors | M | 1 |
| JOB-3 | Prevent conflicting destructive jobs on the same device | M | 1 |
| JOB-4 | Persist job results, verification status and structured errors in local SQLite history | M | 2 |
| JOB-5 | Export a job report (text/JSON) with redaction options | S | 3 |

### F12 Settings and profiles

| ID | Requirement | Pri | Phase |
|----|-------------|-----|-------|
| SET-1 | Theme (system/light/dark) and language persisted | S | 1 |
| SET-2 | Verification and hashing defaults | S | 2 |
| SET-3 | Saved operation profiles (e.g. "write image + verify + eject") | C | 5 |

## 6. Safety requirements

These apply to every destructive operation (CLAUDE.md §2 and §4).

| ID | Requirement |
|----|-------------|
| SAF-1 | Only typed plans in the `Ready` state can be executed (ADR-0005). |
| SAF-2 | The confirmation screen shows the device (identity evidence, capacity, transport), affected partitions and mounted volumes, and the data-loss consequence. |
| SAF-3 | Devices without a strong identifier require an extra confirmation step. |
| SAF-4 | Identity, capacity, system-disk status and usage are revalidated immediately before execution and again inside the privileged helper; mismatch aborts; the target is never redirected. |
| SAF-5 | System disks and `Unknown` system-disk status are refused (no override in the first implementation). |
| SAF-6 | Cancellation is reported only after the worker reached a safe terminal state; partial writes are reported as such. |
| SAF-7 | Success is reported only after flush and verification succeeded. |
| SAF-8 | Disconnects, permission errors, short writes and read errors are surfaced with structured detail and persisted. |
| SAF-9 | Automated tests use virtual disks and simulated adapters only; CI never writes to a physical disk. |

## 7. Non-functional requirements

| Area | Requirement |
|------|-------------|
| Responsiveness | No disk I/O, hashing or benchmarking on the GUI thread; the UI keeps responding to input during every operation. |
| Memory | Streaming with bounded buffers; memory use independent of image/device size (target: below 256 MiB during transfers, to be validated by benchmarks). |
| Throughput | Image write/verify throughput should be close to the device's sustained rate; buffer sizes and direct I/O chosen by benchmark (ADR-0007). |
| Reliability | Every long-running job defines cancellation; crash or disconnect never leaves a "success" record. |
| Security | Unprivileged GUI; narrow, versioned helper protocol; no arbitrary command execution; dependencies restricted to vetted, GPLv3-compatible licenses and checked against RustSec advisories. |
| Privacy | No telemetry. History is local. Serials and user paths are redacted from logs and exported reports by default. |
| Accessibility | WCAG 2.1 AA contrast (measured, see UX_DESIGN_SYSTEM), full keyboard operation, accessible names/roles for all controls, screen-reader testing on all three OSes before 1.0. |
| Localization | All UI text translatable; English and Korean at launch; units and technical identifiers not translated. |
| Portability | Proposed minimums (to confirm by prototypes): Windows 10 22H2 / Windows 11; macOS 13+ (needed for `SMAppService`); Linux with a current LTS kernel, X11 or Wayland; x86_64 and arm64. |
| Maintainability | Modular workspace, ADRs for architectural decisions, strict lints, tests for every feature and regression. |

## 8. Phases and exit criteria

| Phase | Scope | Exit criteria |
|-------|-------|---------------|
| **0 — Architecture validation** (done) | Workspace, identity & capability models, simulated + Linux read-only discovery, virtual-disk harness, dashboard, docs, ADRs | Test report, accepted ADRs 0001–0008, license decided |
| **1 — Platform foundation** | Windows and macOS read-only discovery; hot-plug; helper protocol crate + fake helper; job engine (stages, progress, cancellation) running only against virtual disks; operation-planning typestate | Discovery verified on all three OSes; job engine fault-injection tests green; no physical writes possible |
| **2 — First destructive feature** | Image inspection, write + verify to removable devices, safe eject, hashing, SQLite history | Design review of the confirmation UI; revalidation tests; physical-device testing on each OS by maintainers |
| **3 — Backup, restore, clone** | BAK-1..3, compressed images, job reports | Same safety review as Phase 2 |
| **4 — Partitioning and diagnostics** | PART-1..3/5, DIAG-1..5 | Per-OS capability assessment documented |
| **5 — Erase, recovery, usage** | SEC-1, REC-1..3, USE-2, profiles | Assurance texts reviewed |
| **6 — Advanced** | Resize/move, native sanitize | Separate ADRs |

## 9. Success measures

The project collects no telemetry, so success is measured through tests,
reviews and voluntary reports:

- Zero known defects in which the application wrote to a device other than
  the authorized one (tracked as critical security issues, see
  [SECURITY.md](../SECURITY.md)).
- 100 % of destructive features have revalidation and fault-injection tests
  before release.
- Every capability shown is either assessed or explicitly "Not tested".
- Screen-reader and keyboard walkthroughs pass for all primary flows on each
  OS before 1.0.
- Translation catalog complete for every release (CI-enforced).

## 10. Risks and open questions

See the risk table in [PHASE0_TEST_REPORT.md](PHASE0_TEST_REPORT.md) §8.
Product-level open questions:

1. Release channels and packaging formats per OS; code-signing for an
   open-source project.
2. Whether batch operations (many devices at once, P2) belong in 1.0.
3. Which filesystems to support for formatting on each OS (depends on OS
   facilities and licensing of tools).
4. Minimum OS versions (§7) — to be confirmed by prototypes.

## 11. Glossary

- **Strong identifier**: serial number, WWN or USB serial with at least medium
  confidence and not duplicated among observed devices.
- **Revalidation**: re-observing a device right before execution and
  comparing it with the planned target (ADR-0003, ADR-0005).
- **Privileged helper**: separate process performing only privileged
  operations on behalf of the unprivileged GUI (ADR-0006).
- **Capability**: an operation whose availability for a device on the
  current OS is reported as Supported / Limited / Unavailable / Not tested.
