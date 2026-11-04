# ADR-0008: Read-only Linux discovery via sysfs

- Status: Accepted
- Date: 2026-11-04

## Context

Linux exposes block device metadata through several interfaces: sysfs,
udev (libudev / `/run/udev/data`), udisks2 (D-Bus), `lsblk` (CLI) and ioctls
on device nodes.

## Decision

Phase 0 discovery reads only:

- `/sys/block/<name>` and the device hierarchy behind it (`size`, `ro`,
  `removable`, `dev`, `queue/*_block_size`, `device/{vendor,model,rev,serial,
  firmware_rev,wwid,name}`, `wwid`, `serial`, partitions, `holders/`),
- USB descriptor attributes of the nearest USB ancestor (`idVendor`,
  `idProduct`, `manufacturer`, `product`, `serial`),
- `/dev/disk/by-id/*` link *names* (`readlink` only),
- `/proc/self/mountinfo` and `/proc/swaps`.

It never opens a block device node. Virtual block devices (no `device` link:
loop, dm, md, zram, nbd) are excluded from the list, counted, and followed
only through `holders/` to determine usage (LUKS/LVM/RAID stacks).

System disk detection: a device is a system disk if any of its partitions (or
the whole disk) is mounted at `/`, `/boot`, `/boot/efi`, `/efi`, `/usr`,
`/var` — directly or through holders — or is active swap. If the root
filesystem cannot be traced to any discovered device (containers, overlay,
network root), every device is marked `Unknown`, never "not a system disk".

All paths are injectable (`LinuxPaths`), so the adapter is tested against
fake trees built by `udm_testkit::fake_sysfs`.

## Alternatives considered

- **libudev**: richer (`ID_FS_TYPE`, `ID_SERIAL`), but adds a C dependency and
  is unavailable in some containers; its database format under `/run/udev`
  is internal.
- **udisks2 over D-Bus**: best for privileged operations (ADR-0006) and
  filesystem metadata, but not always running (servers, containers), and
  would add a D-Bus dependency for a read-only listing.
- **Parsing `lsblk --json`**: spawning external commands is avoided by
  policy; output is version-dependent.
- **Opening device nodes for ioctls**: needs permissions, can have side
  effects (spinning up disks, optical tray), not needed for listing.

## Consequences

- Filesystem type and label of unmounted partitions are unknown in Phase 0.
- Vendor strings for virtio disks are numeric PCI vendor IDs (e.g. `0x1af4`)
  as reported by sysfs; they are shown with their provenance and not
  "beautified".
- udisks2 integration is deferred to the privileged-operations work.
