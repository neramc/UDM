# ADR-0004: Explicit four-state capability model

- Status: Accepted
- Date: 2026-11-04

## Context

Not every operation is available on every device and OS: SMART over USB
bridges, secure erase on flash, partition editing on locked media, raw access
without privileges. CLAUDE.md requires distinguishing Supported, Limited,
Unavailable and Not tested, and forbids treating a missing capability as a
healthy device.

## Decision

- `Capability`: `RawRead`, `RawWrite`, `SafeEject`, `SmartAccess`,
  `PartitionEdit`, `SecureErase`, `FilesystemCheck`. `RawWrite`,
  `PartitionEdit` and `SecureErase` are flagged destructive.
- `CapabilityStatus`: `Supported`, `Limited(reason)`, `Unavailable(reason)`,
  `NotTested(reason)`. Every non-supported status carries a typed
  `CapabilityReason` (localizable code, never free text).
- `CapabilitySet::get` returns `NotTested(NotAssessed)` for anything never
  set; `iter` always yields all seven capabilities, so the UI cannot omit one.
- Capabilities describe **what this build can do with this device on this
  platform**. Consequently the Linux adapter reports every capability of a
  real device as `Unavailable(NotImplementedInThisBuild)` in Phase 0, unless
  a device fact gives a more specific reason (`NoMedia`, `WriteProtected`).
- Simulated devices carry *illustrative* statuses to exercise the model and
  the UI. They are always labelled as simulated, with an explanatory note.
- Policy restrictions (system disk, mounted volumes) are **not** encoded as
  capabilities; they are checked by operation planning (ADR-0005).

## Alternatives considered

- `bool` per capability: cannot express "limited" or "not tested", and makes
  "unknown" indistinguishable from "no".
- Free-text reasons: cannot be localized and invite inconsistent wording.

## Consequences

- When a feature is implemented, its adapter must actively assess the
  capability; the default remains "not tested".
- Adding a capability or reason requires updating the UI label mapping
  (`ui/labels.slint`) and the Korean catalog (`tools/update_po.py`);
  exhaustive `match`es in `view_model.rs` make the compiler flag omissions.
