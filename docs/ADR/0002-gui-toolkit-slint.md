# ADR-0002: Slint as GUI toolkit and the project license

- Status: **Accepted**
- Date: 2026-11-04 (proposed); 2026-11-04 (license decided by the project owner)

## Context

CLAUDE.md names Slint as the desktop GUI toolkit. The project is free and
open-source software.

Slint 1.18.1 (pinned in Phase 0) is offered under
`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`
(crate metadata; full texts ship in the crate's `LICENSES/` directory).
The royalty-free option would have required an `AboutSlint` widget or a
"Made with Slint" badge; the commercial option requires a contract.

## Decision

### Project license

The project owner chose the **GNU General Public License**.

- Project source code: **`GPL-3.0-or-later`** (SPDX). Version 3 is required
  because Slint's GPL option is `GPL-3.0-only`, which is incompatible with
  GPL-2.0-only. "Or later" follows the FSF's recommended notice and keeps
  the project's own code relicensable to later GPL versions.
- **Slint is used under its `GPL-3.0-only` option.** Distributed binaries
  that contain Slint are therefore governed by GPLv3 terms; a future GPLv4
  would only apply to binaries if Slint also offered it.
- `LICENSE` contains the unmodified GPLv3 text (byte-identical to the
  FSF's `gpl-3.0.txt`). Every source file carries
  `SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors` and
  `SPDX-License-Identifier: GPL-3.0-or-later`. `Cargo.toml` sets
  `license = "GPL-3.0-or-later"` for all crates.
- Contributions are accepted under the same license (inbound = outbound,
  no CLA; see `CONTRIBUTING.md`).

### GPL obligations implemented

- **Appropriate Legal Notices** (GPLv3 §0, §5(d)): the GUI's About dialog
  shows the copyright notice, the absence of warranty, the license terms,
  where to find a copy of the license, and the source-code location
  (`crates/udm-gui/ui/about.slint`, covered by UI tests).
- **Dependency license policy**: `deny.toml` allows only GPLv3-compatible
  licenses (GPL-3.0, MIT, Apache-2.0, BSD-2/3, ISC, Zlib, BSL-1.0,
  Unicode-3.0, 0BSD, CC0-1.0, Unlicense). Slint's proprietary `LicenseRef`
  options are intentionally not allowed, so the check fails if Slint ever
  stopped offering GPL. Verified on 2026-11-04 with cargo-deny 0.20.2 for
  Linux, Windows and macOS targets: `licenses ok`, `advisories ok`. A CI job
  runs the same check.

### Toolkit configuration (unchanged)

1. Slint 1.18.1 pinned with `=` (the headless testing backend must match
   exactly and does not follow semver).
2. Features: `std`, `backend-winit`, `renderer-femtovg`, `renderer-software`,
   `accessibility`, `compat-1-2`. No Qt backend, no system tray.
3. `fluent` widget style set explicitly; custom styling via design tokens.
4. Translations bundled into the binary.

## Alternatives considered

- **Permissive license + Slint royalty-free option**: rejected by the owner.
  Would have imposed Slint's attribution conditions and a non-OSI license on
  the GUI layer.
- **GPL-3.0-only for the project code**: equally valid; "or later" was chosen
  for the project's own code. Switching is a one-line change in
  `Cargo.toml` plus the SPDX headers.

## Consequences

- Anyone distributing binaries must offer the corresponding source code
  under GPLv3 (the public repository satisfies this for official builds) and
  keep the About dialog's legal notices.
- Release packages must also ship the license notices of permissively
  licensed dependencies (MIT/BSD/Apache require it). A generated
  third-party notices file (e.g. with `cargo-about`) is a release
  requirement, not yet implemented.
- New dependencies must pass `cargo deny check`; adding a license to the
  allow-list requires checking GPLv3 compatibility and updating this ADR.
- Slint upgrades must update `slint`, `slint-build` and
  `i-slint-backend-testing` together and rerun UI tests.
