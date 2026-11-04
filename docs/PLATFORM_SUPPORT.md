# Platform Support

Status: Phase 0 draft (2026-11-04).

Legend: **Implemented** = code exists and was tested; **Planned** = design
direction, not implemented; **Unverified** = believed from platform
documentation or prior knowledge, must be confirmed by a prototype on the
real OS before it is relied on. Nothing in this document claims parity
between operating systems.

## 1. Feature matrix (Phase 0)

| Feature | Linux | Windows | macOS |
|---------|-------|---------|-------|
| Build + clippy + automated tests | Tested locally (x86_64) and in CI | Pass in CI (x86_64-msvc) | Pass in CI (arm64) |
| GUI (Slint, winit) | Tested under Xvfb (software renderer) | Not run | Not run |
| Simulated discovery | Tested | Unit/UI tests pass in CI; GUI not run | Unit/UI tests pass in CI; GUI not run |
| Read-only device discovery | **Implemented** (sysfs) | **Not implemented** – explicit error, confirmed in CI | **Not implemented** – explicit error, confirmed in CI |
| Refuse to run GUI elevated | Implemented (`geteuid`) | Not implemented | Implemented (`geteuid`), not run |
| Raw read / write, eject, SMART, partitioning, erase | Not implemented | Not implemented | Not implemented |

## 2. Linux

### 2.1 Discovery (Implemented, ADR-0008)

Reads `/sys/block`, sysfs device attributes, USB descriptor attributes,
`/dev/disk/by-id` link names, `/proc/self/mountinfo` and `/proc/swaps`. No
device node is opened; no privileges needed.

Known limitations:

- Filesystem type/label of **unmounted** partitions is not reported (would
  require libudev/udisks2 or reading superblocks).
- Virtual block devices are excluded from the list but followed through
  `holders/` to detect usage.
- `removable` is what the kernel reports; many USB hard drives and NVMe
  enclosures report `0`.
- Virtio disks report a numeric PCI vendor (`0x1af4`) and no model; shown as
  reported.
- Transport is inferred from the sysfs path (USB is checked first, so a
  USB-to-NVMe bridge is reported as USB). Heuristic, documented as such.
- System disk detection covers `/`, `/boot`, `/boot/efi`, `/efi`, `/usr`,
  `/var` and active swap, also through dm/md holders. Btrfs multi-device
  and ZFS pools are **unverified**. If `/` cannot be traced (containers,
  overlay, NFS root) every device is `Unknown`.
- Hot-plug notification (netlink uevents) is not implemented; refresh is
  manual.

Tested environment: Linux 6.18 (Firecracker VM), 6 virtio disks, `/` on
`vda`, no udev `/dev/disk/by-id` directory. No physical USB device was
available. See `PHASE0_TEST_REPORT.md`.

### 2.2 Raw block access and permissions (Planned / Unverified)

- `/dev/sdX`, `/dev/nvmeXnY`, `/dev/mmcblkX` are typically `root:disk 0660`.
  Membership in `disk` is equivalent to root for storage and must not be
  recommended.
- Preferred path: **udisks2 over D-Bus** with polkit authorization
  (`org.freedesktop.UDisks2.Block.OpenForBackup` / `OpenForRestore`,
  `Filesystem.Unmount`, `Drive.PowerOff`), which hands back a file
  descriptor without a custom root binary. Availability varies (servers,
  minimal systems, containers) — Unverified.
- Fallback: a pkexec-launched `udm-helper` (ADR-0006).
- Exclusive access: opening a block device with `O_EXCL` fails with `EBUSY`
  while it is mounted or claimed by the kernel — to be confirmed per kernel
  version and used as an extra guard.
- After writing a partition table: `BLKRRPART` / partition rescan is needed;
  may fail while partitions are in use.
- Direct I/O (`O_DIRECT`) requires aligned buffers (logical block size); use
  only after benchmarking (ADR-0007).

### 2.3 Safe eject (Planned / Unverified)

Unmount all filesystems on the device (and close dm/LUKS mappings), flush
(`fsync`/`BLKFLSBUF`), then power off through udisks2 `Drive.PowerOff`, or
`/sys/.../remove` / USB port power-off. Requires privileges or polkit.

### 2.4 SMART (Planned / Unverified)

ATA via `SG_IO` with SAT pass-through and NVMe admin commands via
`NVME_IOCTL_ADMIN_CMD` normally require root/`CAP_SYS_RAWIO`. Many USB
bridges do not pass SAT commands. Must be reported as `Limited`/`NotTested`,
never "healthy".

### 2.5 Packaging constraints

- Runtime libraries for the winit X11 backend: `libxkbcommon-x11.so` was
  missing in the test container and the app aborted at startup until
  `libxkbcommon-x11-0` was installed. Packages must declare it (and
  `libfontconfig1`, plus Wayland libraries if enabled).
- Under Xvfb without GPU the default femtovg (OpenGL) renderer produced a
  black window; `SLINT_BACKEND=winit-software` worked. Renderer fallback
  behaviour on real desktops needs testing.
- Flatpak/Snap sandboxes block `/dev` access; udisks2 through portals is
  Unverified.

## 3. Windows (Not implemented)

### 3.1 Discovery plan (Unverified)

- Enumerate disk interfaces with SetupAPI (`SetupDiGetClassDevs` with
  `GUID_DEVINTERFACE_DISK`) or `CM_Get_Device_Interface_List`.
- Open each interface or `\\.\PhysicalDriveN` with **zero desired access**
  (metadata queries only) and query:
  `IOCTL_STORAGE_QUERY_PROPERTY` (`StorageDeviceProperty`: vendor, product,
  revision, serial offsets, `BusType`), `IOCTL_STORAGE_GET_DEVICE_NUMBER`,
  `IOCTL_DISK_GET_DRIVE_GEOMETRY_EX` (capacity; access requirements to be
  verified), `IOCTL_DISK_GET_DRIVE_LAYOUT_EX` (partitions; may need read
  access).
- Map volumes to disks with `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS`; detect
  the system disk via the Windows/boot volume.
- `PhysicalDriveN` numbers are **not stable** across reboots or replug.
- Crate candidate: `windows` / `windows-sys` (Microsoft, MIT/Apache-2.0).

### 3.2 Raw access and permissions (Unverified)

- Raw reads/writes to `\\.\PhysicalDriveN` require administrator rights.
- Writing sectors that belong to a mounted volume is blocked by the OS;
  volumes must be locked and dismounted first (`FSCTL_LOCK_VOLUME`,
  `FSCTL_DISMOUNT_VOLUME`).
- The GUI must run with `asInvoker`; elevation only for the helper via UAC
  (ADR-0006). Elevation detection of the GUI itself (token elevation query)
  is not implemented in Phase 0.

### 3.3 Safe eject (Unverified)

`CM_Request_Device_Eject` on the device's removable parent devnode, after
dismounting volumes; the OS may veto if handles are open — the veto reason
must be surfaced.

## 4. macOS (Not implemented)

### 4.1 Discovery plan (Unverified)

- DiskArbitration (`DASessionCreate`, `DARegisterDiskAppearedCallback`,
  `DADiskCopyDescription`: media size, whole/partition, removable, writable,
  protocol, vendor/model, mount path) plus IOKit (`IOMedia` matching, parent
  `IOUSBHostDevice` properties for USB serial / VID / PID).
- `/dev/diskN` numbers are **not stable**.
- No shelling out to `diskutil` (policy: no external command execution).

### 4.2 Raw access and permissions (Unverified)

- `/dev/diskN` and `/dev/rdiskN` are `root:operator`; non-root users cannot
  open them. `authopen` can provide an authorized file descriptor; a
  privileged helper via `SMAppService` (macOS 13+) is the long-term path.
- Disks must be unmounted (`DADiskUnmount` with whole-disk option) before
  raw writes; Spotlight/FSEvents may remount — needs a claim/approval
  callback strategy.
- The sealed system volume and SIP protect the internal system disk.
- Full Disk Access / TCC interactions for raw access are Unverified.

### 4.3 Safe eject (Unverified)

`DADiskUnmount` followed by `DADiskEject`; dissenters must be reported.

## 5. Privilege requirements summary

| Operation | Linux | Windows | macOS |
|-----------|-------|---------|-------|
| List devices & metadata | None (sysfs) — Implemented | Probably none with zero-access handles — Unverified | Probably none (DiskArbitration) — Unverified |
| Raw read | root / polkit via udisks2 | Administrator | root / authopen |
| Raw write | root / polkit via udisks2; unmounted | Administrator; volumes locked & dismounted | root; unmounted |
| Unmount / eject | polkit (udisks2) or root | User for removable media (veto possible) | User via DiskArbitration (Unverified) |
| SMART | root / CAP_SYS_RAWIO | Administrator | Limited; vendor/driver dependent |
