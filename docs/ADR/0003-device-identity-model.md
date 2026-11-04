# ADR-0003: Device identity as attributes with provenance and confidence

- Status: Accepted
- Date: 2026-11-04

## Context

Destructive operations must target exactly the device the person authorized.
OS paths (`/dev/sdb`, `\\.\PhysicalDrive2`, `/dev/disk4`) and disk numbers
are reassigned on reconnect. Serial numbers can be missing, placeholders
(`000000000000`), duplicated across identical cheap devices, or replaced by a
USB bridge's own serial. Model and capacity are shared by every unit of a
product line.

## Decision

`udm_domain::DeviceIdentity` is a list of `IdentityAttribute`s, each with:

- `kind` (`AttributeKind`): vendor, model, revision, serial number, WWN, USB
  VID/PID/manufacturer/product/serial, capacity, block sizes, persistent OS
  link, OS device name, OS device number.
- `value`: text (trimmed; empty means "not reported") or number.
- `provenance`: source (`Simulated`, `LinuxSysfs`, `LinuxUdevLink`, ...) and
  a locator (e.g. `device/model`).
- `confidence`: `Low < Medium < High`.

Attribute kinds are **stable** (describe the device) or **volatile** (OS name
and number). Serial number, WWN and USB serial are **strong identifiers**.

`DeviceIdentity::compare(expected, observed)` returns:

- `Mismatch` if any stable attribute with confidence ≥ Medium is present on
  both sides with disjoint values (e.g. different serial, different capacity).
- `Consistent` only if, without conflicts, at least one strong identifier
  matches, no strong identifier is present on only one side, and capacity was
  compared and matched.
- `Inconclusive` otherwise (including every device without a strong
  identifier).

Volatile attributes never count as evidence either way.

Post-processing after every discovery pass (`assess_identities`):

- A strong identifier reported by more than one device is capped at `Low`
  confidence and a `DuplicateIdentifier` issue is raised.
- Devices without a usable strong identifier get a `NoStrongIdentifier`
  issue.
- Adapters classify placeholder-looking serials (very short or one repeated
  character) as `Low` (`classify_serial`).

`DeviceKey` (e.g. `linux:sdb`, `sim:usb-stick-a`) is only a handle within one
snapshot and is documented as *not* an identity.

## Alternatives considered

- **Single "device ID" string** (e.g. udev by-id link): fails for duplicated
  or missing serials and hides why identification is weak.
- **Hash of all attributes**: any benign change (firmware revision string,
  extra udev link) would look like a different device; a hash also cannot
  explain itself to the user.

## Consequences

- Future destructive workflows (ADR-0005) must treat `Inconclusive` as "ask
  again with explicit confirmation" and `Mismatch` as "abort".
- The dashboard shows identity evidence with provenance and confidence and
  explains when a device cannot be told apart from an identical one.
- The UI keeps a selection across refreshes only if the device under the
  same key is not a `Mismatch` (`Presenter::show_snapshot`).
