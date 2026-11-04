# Testing Strategy

Status: Phase 0 draft (2026-11-04).

## 1. Principles

- **No test touches a physical device.** Destructive code paths are tested
  only against virtual disks (sparse temporary files) and simulated adapters.
  CI must never run destructive operations against a real disk.
- Read-only tests against the real OS are allowed (e.g. listing `/sys`), and
  only assert invariants, never machine-specific values.
- A feature is complete only with tests for success *and* failure paths.
- Every fixed bug gets a regression test that fails without the fix.

## 2. Test layers

| Layer | Where | Examples (Phase 0) |
|-------|-------|--------------------|
| Domain unit tests | `udm-domain/src/**` | identity comparison (path change, swapped device, capacity change, missing serial, low confidence), capability defaults, duplicate detection |
| Adapter tests with fakes | `udm-platform/src/linux/tests.rs` | fake sysfs trees: USB stick, NVMe with LUKS+LVM holders, overlay root, empty card reader, write-protected SD, placeholder serial, malformed/vanished attributes, missing tables |
| Real-OS smoke (read-only) | `real_system_discovery_is_read_only_and_well_formed` | runs `LinuxSysfsDiscovery::new()` against `/sys`, asserts invariants |
| Application tests | `udm-app/src/discovery/tests.rs` | off-thread execution, non-blocking requests, coalescing, epochs, adapter errors, panics, shutdown with a stuck adapter |
| Harness self-tests | `udm-testkit/src/**` | sparse capacity, never growing past capacity, MBR/GPT validity (CRC), fault injection, pattern verify |
| View-model tests | `udm-gui/src/view_model.rs` | unit formatting, titles never invented, issue mapping, failure mapping |
| Headless UI tests | `udm-gui/tests/ui_dashboard.rs` | Slint testing backend: simulated labels, four capability states, failure/empty/loading states, stale epochs ignored, selection safety, keyboard navigation, no destructive buttons, theme switching, Korean bundle |
| Manual GUI run | Xvfb + xdotool | see test report |

## 3. Virtual-disk harness (`udm-testkit`)

- `VirtualDisk`: sparse temp file with fixed capacity and logical block size
  (512 B / 4 KiB); writes past the end fail like a real device; deleted on
  drop.
- `layout`: MBR and GPT (protective MBR, primary/backup headers, CRC32)
  writers; fixtures independently validated with `sfdisk`, `sgdisk` and
  `fdisk -b 4096` (see test report). `examples/write_fixtures.rs` writes
  fixtures to a new directory for such cross-checks.
- `FaultyDisk<D>`: read/write errors in ranges, short reads/writes, silent
  corruption, flush failure, disconnect after N bytes (cuts the crossing
  transfer short, then fails everything). Injected errors carry a typed
  payload (`InjectedError`) so tests distinguish them from host errors.
- `pattern`: position-dependent deterministic data written/verified in
  64 KiB chunks (bounded memory); detects corruption and shifted data.
- `fake_sysfs`: builder for `/sys`, `/dev/disk/by-id`, `/proc` trees (Unix).

## 4. Planned additions

- **Fault-injection tests for the job engine** using `FaultyDisk`: short
  writes must be completed, disconnects must stop the job and report partial
  progress, flush failure must not report success, verification mismatches
  must fail the job.
- **Property tests** (proptest) for identity comparison and partition-table
  parsing; **fuzzing** (`cargo fuzz`) for every on-disk parser.
- **Performance tests** (criterion) for streaming copy/hash throughput and
  memory bounds on multi-GiB sparse virtual disks.
- **Platform adapter tests** on Windows/macOS with recorded API responses
  (fixtures) plus read-only smoke tests on CI runners.
- **Snapshot tests** of the UI (software renderer screenshots) for both
  themes and both languages.
- Accessibility checks: every interactive element has an accessible role and
  label; keyboard paths for all flows.

## 5. Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p udm-platform --example discover            # read-only, redacts serials
cargo run -p udm-testkit --example write_fixtures -- <new-dir>
python3 -I tools/update_po.py                            # Korean catalog; fails on missing strings
```

Running the GUI headless (Linux):

```sh
Xvfb :99 -screen 0 1280x860x24 &
DISPLAY=:99 SLINT_BACKEND=winit-software cargo run -p udm-gui   # as a non-root user
```

## 6. CI

`.github/workflows/ci.yml` runs fmt, clippy and tests on Ubuntu, Windows and
macOS. Results for Windows/macOS are only claimed once a run has been
observed (see the test report).
