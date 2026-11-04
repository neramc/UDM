# Phase 0 Test Report

Date: 2026-11-04 (updated after the license decision and ADR-0005/0006 acceptance) · Branch: `code/ecstatic-noether-6rz2w3`

Only results that were actually executed and observed are reported as
passing. Platforms that were not available are listed as untested.

## 1. Environment

| Item | Value |
|------|-------|
| OS | Linux 6.18.44 (Firecracker VM), x86_64, Ubuntu 24.04 userland |
| Toolchain | rustc 1.97.0, cargo 1.97.0 (stable) |
| GUI stack | Slint 1.18.1, winit backend, software renderer, Xvfb 21.1.12 |
| Storage visible to the VM | 6 virtio disks (`vda` root ext4, `vdb`–`vdf` read-only), 9 loop devices; no USB/NVMe/SD hardware; no udev `/dev/disk/by-id` |
| User | root (container); the GUI was run as `nobody` |

## 2. Automated checks (Linux, executed)

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` | Pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass (0 warnings; Slint build-script warnings also 0 after replacing deprecated `viewport-*` properties) |
| `cargo test --workspace --locked` | **112 passed, 0 failed, 0 ignored** |
| `python3 -I tools/update_po.py` | 141 strings, 141 translated |
| `cargo deny check` (cargo-deny 0.20.2; Linux, Windows, macOS targets) | `advisories ok, bans ok, licenses ok, sources ok` (warnings only for duplicate transitive crate versions) |
| SPDX header check (all `.rs`, `.slint`, `.py`, `.toml`, workflow files) | Every file has `SPDX-License-Identifier: GPL-3.0-or-later` |

Per crate:

| Crate / target | Tests | Highlights |
|----------------|-------|-----------|
| `udm-domain` | 25 | identity comparison (path change = consistent, swapped device = mismatch, capacity change = mismatch, no serial / missing capacity / disappeared serial = inconclusive, OS name never evidence), placeholder serials, duplicate-serial downgrade, capability defaults (`NotTested`), destructive classification |
| `udm-platform` | 31 | Linux adapter on fake sysfs trees (USB stick, NVMe with LUKS→LVM holders, overlay root → `Unknown`, empty reader, write-protected MMC, virtio whole-disk root, malformed/vanished attributes, missing mount/swap tables, stable ordering, transport heuristics), real `/sys` read-only smoke test, simulator scenarios, unimplemented-platform error |
| `udm-app` | 10 | discovery off the caller thread, `request_refresh` < 100 ms with a 500 ms adapter, coalescing, epochs, adapter errors surfaced, adapter panic contained, drop does not hang on a 30 s stuck adapter (bounded by 2 s grace) |
| `udm-testkit` | 28 | sparse 16 GiB virtual disk uses < 1 MiB, never grows past capacity, MBR/GPT structure and CRC32, 512 B and 4 KiB sectors, fault injection (range errors, short I/O, corruption, flush failure, disconnect mid-transfer), pattern verification across chunk boundaries |
| `udm-gui` (unit) | 6 | SI/IEC formatting incl. rounding carry (999,999,999 B → 1.0 GB), digit grouping, titles never invented, failure/issue mapping |
| `udm-gui` (headless UI) | 12 | simulated labels on all cards, all four capability states rendered, failure ≠ empty ≠ loading, stale-epoch results ignored, selection dropped when the device under a key changes, Up/Down/Home/End navigation, no destructive buttons, theme switching, Korean bundle, About dialog GPL notices + Escape/Close |

## 3. Independent validation of virtual-disk fixtures (executed)

`cargo run -p udm-testkit --example write_fixtures -- <dir>`, then:

| Fixture | Tool | Result |
|---------|------|--------|
| `mbr-512.img` | `sfdisk -l`, `sfdisk --verify` (util-linux 2.39.3) | 2 partitions as written (FAT32 LBA, Linux); "No errors detected" |
| `gpt-512.img` | `sfdisk --verify`; `sgdisk -v` (GPT fdisk 1.0.10) | "No errors detected"; "No problems found" (informational note: partition 2 end not 2048-aligned) |
| `gpt-4096.img` | `fdisk -b 4096 -l` | GPT recognised, disk GUID and both partitions as written |

## 4. Read-only discovery prototype (executed on Linux)

`cargo run -p udm-platform --example discover` (serials redacted): 6 devices,
`vda` 274,877,906,944 B virtio, mounted at `/`, **SystemDisk**; `vdb`–`vdf`
write-protected (`ro=1`), so destructive capabilities report
`WriteProtected`; all others `NotImplementedInThisBuild`; 9 virtual devices
excluded (info note). Wall time of the example process ≈ 0.28 s; discovery
pass 2–4 ms in the GUI.

**Bug found and fixed:** on the real system every attribute file in a block
directory (`size`, `ro`, …) was probed as a partition directory, producing
one spurious `ProbeFailed { Partitions, Io }` warning per file. Fixed by
probing only directories; regression test
`well_formed_device_produces_no_device_issues` was confirmed to **fail
without the fix and pass with it**.

## 5. GUI run (executed, Linux/Xvfb, as unprivileged user)

| Check | Result |
|-------|--------|
| Start as root | Refused with message, exit code 2 |
| Start as `nobody` with default renderer (femtovg/OpenGL) | Window black under Xvfb (no GPU) |
| Start as `nobody` with `SLINT_BACKEND=winit-software` | Renders correctly |
| Missing `libxkbcommon-x11.so` | App aborted at startup (winit panic) until `libxkbcommon-x11-0` was installed → packaging dependency |
| Simulated sample devices, mouse selection, scrolling detail | Works (screenshots in `docs/images/`) |
| Keyboard Up/Down in the list | Works once the X window has input focus (no window manager in Xvfb) |
| Theme System → Dark, language → 한국어 | Works; Hangul renders via system fonts |
| "Simulated: discovery failure" | Explicit failure state with "Try again"; log `discovery pass failed … simulated discovery failure` |
| "This computer (read-only)" | 6 virtio devices; `vda` shows "Holds the running system" |
| Layout defect found in the run | Cards overflowed the sidebar (scroll view sized to content); fixed by binding content width to visible width; identity provenance column misaligned; fixed with a fixed-width column |
| About dialog (GPLv3 Appropriate Legal Notices) | Opens from the header; shows version 0.0.1 and repository URL from Cargo metadata, copyright, no-warranty text, license location; Escape closes it (screen pixel-identical to before opening) |
| GUI responsiveness | Discovery always runs on the `udm-discovery` thread (unit-tested); a 250 ms simulated latency shows the loading state without blocking input |

## 6. Untested platforms and features

| Item | Status |
|------|--------|
| Windows GUI and device discovery | GUI **not run** (no Windows desktop available). Discovery is not implemented; CI confirmed it reports `device discovery is not implemented for windows in this build` instead of an empty list. Build and automated tests: see §6a. |
| macOS GUI and device discovery | GUI **not run**. Discovery not implemented; CI confirmed the explicit `not implemented for macos` error. Build and automated tests: see §6a. |
| Linux with real USB / NVMe / SD hardware, udev by-id links | **Not tested** — only fake sysfs trees and virtio disks. |
| Linux Wayland session, real GPU renderer | Not tested. |
| Screen readers (Orca, Narrator, VoiceOver) | Not tested; accessibility tree checked only through Slint's testing backend. |
| ARM64 builds | Not tested. |
| Any write / destructive operation | Not implemented by design. |

## 6a. CI results (GitHub Actions, observed 2026-11-04)

GitHub Actions became active after PR #1 was merged. Both observed runs
concluded **success**:

| Run | Commit | Jobs |
|-----|--------|------|
| [#1](https://github.com/neramc/UDM/actions/runs/41826252860) | `main` @ `6dfd4a6` (Phase 0 code) | fmt ✓, translations ✓, clippy + test on ubuntu-latest ✓, windows-latest ✓, macos-latest ✓ |
| [#2](https://github.com/neramc/UDM/actions/runs/41828931658) | branch @ `ebd2dea` (GPL + docs) | fmt ✓, translations ✓, cargo-deny ✓, clippy + test on ubuntu-latest ✓, windows-latest ✓, macos-latest ✓ |

Details observed in the logs:

- **Windows** (x86_64-pc-windows-msvc, Rust 1.99.1, run #2): clippy with
  `-D warnings` clean; all test binaries passed, including 12 headless UI
  tests, 11 platform tests and 26 testkit tests (the two Unix-only fake-sysfs
  tests are compiled out). The `geteuid` root check is Unix-only, so the
  Windows build exercised the non-Unix code path.
- **macOS** (aarch64-apple-darwin, Rust 1.99.1, run #1): 90 tests passed
  (10 app, 25 domain, 6 view-model, 10 UI, 11 platform, 28 testkit); the
  Linux adapter tests are compiled out.
- **Ubuntu**: clippy, tests and the read-only discovery example passed
  (the test step took 8–17 min, dominated by compiling dependencies at
  `opt-level = 2`).
- Discovery example on Windows/macOS exits with the explicit
  `NotImplemented` error (`continue-on-error` in CI by design).
- CI warning: `actions/checkout@v4` targets the deprecated Node.js 20
  runtime; update the action version.

## 7. Blockers

- No Windows or macOS machine in this environment → Windows/macOS discovery
  prototypes could not be written and validated responsibly (writing
  untested FFI code would violate "do not invent results"). They are
  designed in `PLATFORM_SUPPORT.md` and remain explicit `NotImplemented`.
- No physical removable devices → USB/SD/NVMe behaviour on real hardware
  unverified.
- ~~Project license not chosen~~ — resolved: GPL-3.0-or-later, Slint under
  GPL-3.0-only (ADR-0002 accepted).

## 8. Unresolved technical risks

| # | Risk | Impact | Next step |
|---|------|--------|-----------|
| R1 | Windows: metadata IOCTLs with zero-access handles may not return everything (capacity/layout may need read access, i.e. admin) | Discovery might need the helper on Windows | Prototype on Windows 10/11 |
| R2 | macOS: raw access requires root/authopen; helper needs `SMAppService` + code signing/notarization | Distribution cost, UX of authorization | Prototype + signing plan |
| R3 | Linux: udisks2 availability (servers, containers, Flatpak) | Fallback to pkexec helper needed | Prototype udisks2 path and detection |
| R4 | System-disk detection gaps (btrfs multi-device, ZFS, bcache, network roots) | Mis-classification risk mitigated by `Unknown`, but may over-block | Add fixtures; treat Unknown as protected |
| R5 | Identity of devices without serial numbers cannot be confirmed | Requires extra confirmation UX for destructive ops | Design in ADR-0005 review |
| R6 | Renderer fallback: femtovg black window without GL | Users on VMs/remote desktops could see a blank window | Auto-fallback to software renderer or document `SLINT_BACKEND` |
| R7 | Runtime library packaging (`libxkbcommon-x11`, fontconfig, Wayland libs) | Startup crash on minimal systems | Package dependencies; graceful error message |
| R8 | Slint testing backend is an internal, non-semver crate pinned with `=` | Upgrades may break UI tests | Upgrade Slint crates together |
| R9 | Transport heuristic from sysfs path | Mislabelled bus type in edge cases | Prefer udev/udisks properties when available |
| R10 | Fake-capacity flash and USB bridges that lie about identity | Wrong capacity/identity trust | Full-surface verification feature; display provenance |

## 9. Unresolved questions (licensing, packaging, accessibility, security)

- **Licensing**: resolved — GPL-3.0-or-later; `cargo deny` license policy in
  place. Still open: generating third-party license notices for release
  packages (MIT/BSD/Apache notice requirements) and font licensing if fonts
  are ever bundled for CJK.
- **Packaging**: formats per OS (AppImage/deb/rpm/Flatpak, MSI/MSIX,
  notarized DMG); how the privileged helper is installed and updated;
  code-signing for an open-source project.
- **Accessibility**: screen-reader testing on all three OSes; high-contrast
  mode; text scaling; locale-aware number formatting.
- **Security**: GUI-to-helper authentication per OS; whether `Unknown`
  system-disk status may ever be overridden; root-only rescue systems;
  Windows elevation detection for the GUI.

## 10. Status of decisions and next steps

Decided on 2026-11-04 by the project owner:

- License: GPL-3.0-or-later (ADR-0002 accepted; `LICENSE`, SPDX headers,
  About dialog, `deny.toml`).
- ADR-0005 (operation planning) and ADR-0006 (privileged helper) accepted as
  binding designs. No destructive operation is implemented or enabled.
- `docs/PRD.md`, `SECURITY.md` and `CONTRIBUTING.md` written.

Next steps:

1. ~~Observe CI on Windows and macOS~~ — done (§6a). Update
   `actions/checkout` off the deprecated Node.js 20 runtime.
2. Enable GitHub private vulnerability reporting, which `SECURITY.md` points
   to (repository Settings → Security).
3. Phase 1 (PRD §8): Windows and macOS read-only discovery prototypes on
   real machines; helper protocol crate with a fake helper; job engine and
   operation-planning typestate against virtual disks only.
4. Third-party license notices for release packages; renderer fallback;
   uevent-based hot-plug refresh on Linux.
