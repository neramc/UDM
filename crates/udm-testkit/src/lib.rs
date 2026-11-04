// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Test harness for Universal Drive Manager.
//!
//! Everything in this crate operates on **temporary files and directories
//! only**. Nothing here opens, reads or writes a physical device, and nothing
//! here should ever be pointed at one.
//!
//! * [`virtual_disk`]: fixed-capacity, sparse, file-backed disks that behave
//!   like block devices (they never grow past their capacity).
//! * [`layout`]: writers for MBR and GPT partition tables on virtual disks.
//! * [`fault`]: a fault-injecting wrapper for read errors, short transfers,
//!   silent corruption, flush failures and disconnects.
//! * [`pattern`]: deterministic data patterns for write/verify tests, using
//!   bounded buffers.
//! * [`fake_sysfs`] (Unix only): builders for fake Linux `/sys`, `/dev` and
//!   `/proc` trees so the Linux discovery adapter can be tested without
//!   real hardware.
//!
//! See `docs/TESTING_STRATEGY.md`.

pub mod block_io;
pub mod crc32;
#[cfg(unix)]
pub mod fake_sysfs;
pub mod fault;
pub mod layout;
pub mod pattern;
pub mod virtual_disk;

pub use block_io::{BlockIo, read_exact_at, write_all_at};
pub use fault::{Fault, FaultyDisk};
pub use virtual_disk::VirtualDisk;
