# ADR-0002: Slint as GUI toolkit and its licensing implications

- Status: **Proposed** — the toolkit choice follows CLAUDE.md; the project
  license and the resulting Slint license option need an owner decision.
- Date: 2026-11-04

## Context

CLAUDE.md names Slint as the desktop GUI toolkit. The project is described as
free and open source, but the repository has no `LICENSE` file yet.

Slint 1.18.1 (the version pinned in Phase 0) is offered under
`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`
(from the crate metadata; full texts ship in the crate's `LICENSES/`
directory).

The Royalty-free Desktop, Mobile and Web Applications License v2.0 grants use
in desktop applications on condition that the application either displays
the `AboutSlint` widget in an About screen reachable from the top-level menu
(or the splash screen), or that a public download page displays the
"Made with Slint" attribution badge. It does not permit use in embedded
systems or distributing an application that exposes Slint's APIs.

## Decision

1. Use Slint 1.18.1, pinned with `=` because the headless testing backend
   (`i-slint-backend-testing`) must match exactly and does not follow semver.
2. Enable only: `std`, `backend-winit`, `renderer-femtovg`,
   `renderer-software`, `accessibility`, `compat-1-2`. The Qt backend and the
   system-tray feature are disabled to keep the dependency graph small.
3. Use the `fluent` widget style explicitly (`build.rs`) so the look does not
   change with the host platform; all custom styling goes through design
   tokens (`ui/theme.slint`).
4. Bundle translations into the binary (`with_bundled_translations`), so no
   gettext runtime dependency is needed.

## Open question for the project owner

Choose the project license. The options are compatible with Slint as follows:

| Project license | Slint option | Obligation |
|-----------------|--------------|------------|
| GPL-3.0-only / GPL-3.0-or-later | GPL-3.0-only | Whole application distributed under GPLv3 terms |
| Permissive (MIT / Apache-2.0) or MPL-2.0 | Royalty-free 2.0 | AboutSlint widget in About dialog **or** badge on the download page |
| Any | Commercial (Software 3.0) | Per contract |

Until decided, no `license` field is set in `Cargo.toml` and no `LICENSE`
file is added. A dependency license audit (e.g. `cargo deny`) should be
introduced together with that decision.

## Alternatives considered

- egui, iced, GTK, Qt, Tauri: not evaluated further because CLAUDE.md fixes
  Slint. Recorded here only so that a future change of toolkit is an explicit
  superseding ADR.

## Consequences

- An About dialog containing `AboutSlint` is likely required (Royalty-free
  path) — tracked as Phase 1 UI work.
- Slint upgrades must update both `slint`, `slint-build` and
  `i-slint-backend-testing` together and rerun UI tests.
