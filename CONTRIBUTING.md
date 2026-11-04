# Contributing to Universal Drive Manager

Thank you for helping. This project manipulates storage devices, so
correctness and data safety come before speed of feature delivery. Please
read this guide, [CLAUDE.md](CLAUDE.md) (the binding project rules — they
apply to human and AI-assisted contributions alike) and the
[architecture overview](docs/ARCHITECTURE.md) before starting larger work.

## License of contributions

The project is licensed under **GPL-3.0-or-later** ([LICENSE](LICENSE),
[ADR-0002](docs/ADR/0002-gui-toolkit-slint.md)). By submitting a
contribution you agree that it is licensed under the same terms
(inbound = outbound). There is no contributor license agreement. Only
submit work you have the right to contribute.

Every new source file starts with:

```rust
// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later
```

(`#` comments for Python, TOML and YAML.)

## Safety rules for contributors

1. **Never run destructive code against a physical disk** while developing
   or testing. Use `udm-testkit` virtual disks, `FaultyDisk` and the
   simulated adapter. CI must never write to a real disk.
2. **Never run the GUI as root/administrator.** The binary refuses to start
   as root on Unix by design.
3. **No shell or external command execution** from application code
   (`std::process::Command` is not used anywhere; discuss in an issue first
   if you believe it is needed).
4. Destructive features follow the operation-planning lifecycle in
   [ADR-0005](docs/ADR/0005-operation-planning.md) and need a design review
   of their confirmation UI before they are enabled on physical devices.
5. Never invent device metadata, diagnostic results or capability claims.
   Unknown stays unknown; unsupported is reported as unsupported.

## Before you start

- **Small fixes**: open a pull request directly.
- **Features or behaviour changes**: open an issue first describing intended
  behaviour, safety implications, platform support and acceptance criteria
  (CLAUDE.md §12).
- **Architectural changes** (public interfaces, storage formats, security
  policy, crate boundaries, new major dependencies): propose an ADR in
  `docs/ADR/` (copy an existing one) and get it accepted first.
- **Security issues**: do not open a public issue — see [SECURITY.md](SECURITY.md).

## Development setup

- Rust stable, at least the `rust-version` in `Cargo.toml` (1.92).
- Linux build packages (Debian/Ubuntu): `libfontconfig1-dev libxkbcommon-dev`;
  runtime for X11: `libxkbcommon-x11-0`.
- Optional: `cargo install cargo-deny` for the dependency policy check.

```sh
cargo run -p udm-gui                                   # dashboard (simulated devices by default)
cargo run -p udm-platform --example discover           # read-only discovery, serials redacted
cargo run -p udm-testkit --example write_fixtures -- /tmp/new-dir   # MBR/GPT fixtures
```

Headless GUI on Linux: see [docs/TESTING_STRATEGY.md](docs/TESTING_STRATEGY.md).

## Required checks

All of these must pass before a pull request is merged; the CI workflow is
configured to run them on Linux, Windows and macOS:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check                     # licenses (GPLv3-compatible only), advisories, sources
python3 -I tools/update_po.py        # Korean catalog complete; commit the regenerated .po
```

Never report or claim that tests passed unless you ran them and saw the
result. If you could not test on a platform, say so in the pull request.

## Code guidelines

- **Layers**: respect the crate boundaries in
  [ADR-0001](docs/ADR/0001-workspace-and-crate-boundaries.md). The domain
  crate has no dependencies on UI or OS APIs; OS-specific code lives in
  `udm-platform` behind `#[cfg(target_os = …)]`.
- **Errors**: typed errors (`thiserror`) in libraries; no `unwrap`,
  `expect` or `panic!` outside tests (enforced by Clippy). Do not swallow
  errors or replace failures with fake success values.
- **Unsafe**: `unsafe_code` is denied workspace-wide. A justified exception
  needs a local `#[allow(unsafe_code)]`, a `// SAFETY:` comment and review.
- **Concurrency**: no blocking I/O on the GUI thread; bounded queues;
  every long-running operation defines cancellation (ADR-0007).
- **I/O**: stream through bounded buffers; never load a whole image or
  device into memory.
- **Logging**: use `tracing`; do not log serial numbers, WWNs or personal
  paths at info level; no telemetry.
- **Dependencies**: justify every new dependency (maintenance, security,
  platform support, license, necessity). It must pass `cargo deny check`;
  adding a license to the allow-list requires a GPLv3-compatibility check
  and an ADR-0002 update.

## Tests

- Add tests for success **and** failure paths of every change.
- Every bug fix gets a regression test that fails without the fix.
- Use virtual disks and fault injection for I/O paths, fake sysfs trees for
  Linux discovery, and the Slint testing backend for UI behaviour
  (`crates/udm-gui/tests/`).

## UI, design system and translations

- Use the tokens in `crates/udm-gui/ui/theme.slint`; no hard-coded colors,
  sizes or spacing in screens. Check contrast for new color pairs
  ([UX_DESIGN_SYSTEM](docs/UX_DESIGN_SYSTEM.md)).
- Every user-visible string uses `@tr()`. Rust code passes enum codes, not
  prose, to the UI.
- Run `python3 -I tools/update_po.py` and add Korean translations to the
  table in that script. If you cannot translate, say so in the pull request;
  a maintainer will add the translation before merging (the check fails
  until then).
- Every interactive element needs an accessible role and label and must be
  reachable by keyboard. Design empty, loading, error, permission-denied and
  disconnected states explicitly.

## Commits and pull requests

- Keep changes small and focused; do not reformat or rewrite unrelated code.
- Write commit messages that explain *why*, not only *what*.
- Pull request description: intended behaviour, safety implications,
  platforms tested (and not tested), tests added, documentation updated.
- Update the relevant documents (`docs/`, ADRs, `PLATFORM_SUPPORT.md` for
  platform limitations) in the same pull request.
- Definition of Done: CLAUDE.md §13.

## AI-assisted contributions

AI tools may be used. The contributor remains responsible for the result:
review every line, run the checks yourself, and do not submit generated
claims about tests, platforms or hardware that were not verified.
