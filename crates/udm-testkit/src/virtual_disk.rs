// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sparse, fixed-capacity, file-backed virtual disks.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::block_io::BlockIo;

/// A virtual disk backed by a sparse temporary file.
///
/// * The capacity is fixed at creation; writes past it fail with
///   [`io::ErrorKind::InvalidInput`], like a real block device (a plain file
///   would silently grow).
/// * The backing file is created sparse, so a multi-gigabyte virtual disk costs
///   almost no real disk space until data is written.
/// * The backing directory is deleted when the value is dropped.
#[derive(Debug)]
pub struct VirtualDisk {
    _dir: TempDir,
    path: PathBuf,
    file: File,
    capacity: u64,
    logical_block_size: u32,
}

impl VirtualDisk {
    /// Creates a zero-filled virtual disk with 512-byte logical blocks.
    ///
    /// # Errors
    /// Returns an error if the temporary file cannot be created, or if
    /// `capacity` is zero or not a multiple of the block size.
    pub fn new(capacity: u64) -> io::Result<Self> {
        Self::with_block_size(capacity, 512)
    }

    /// Creates a zero-filled virtual disk with the given logical block size.
    ///
    /// # Errors
    /// Returns an error if the temporary file cannot be created, or if the
    /// geometry is invalid (zero capacity, block size that is not a power of
    /// two of at least 512, or capacity not a multiple of the block size).
    pub fn with_block_size(capacity: u64, logical_block_size: u32) -> io::Result<Self> {
        if capacity == 0
            || logical_block_size < 512
            || !logical_block_size.is_power_of_two()
            || !capacity.is_multiple_of(u64::from(logical_block_size))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "capacity must be a non-zero multiple of a power-of-two block size >= 512",
            ));
        }
        let dir = tempfile::Builder::new().prefix("udm-vdisk-").tempdir()?;
        let path = dir.path().join("disk.img");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.set_len(capacity)?;
        Ok(Self {
            _dir: dir,
            path,
            file,
            capacity,
            logical_block_size,
        })
    }

    /// Path of the backing image file. Valid while `self` is alive.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of logical blocks.
    #[must_use]
    pub fn block_count(&self) -> u64 {
        self.capacity / u64::from(self.logical_block_size)
    }

    /// Bytes actually allocated on the host file system (Unix only; elsewhere
    /// returns `None`).
    ///
    /// # Errors
    /// Returns an error if the file metadata cannot be read.
    pub fn allocated_bytes(&self) -> io::Result<Option<u64>> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta = self.file.metadata()?;
            Ok(meta.blocks().checked_mul(512))
        }
        #[cfg(not(unix))]
        {
            Ok(None)
        }
    }

    fn check_range(&self, offset: u64, len: usize) -> io::Result<()> {
        let len = u64::try_from(len)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "length overflow"))?;
        match offset.checked_add(len) {
            Some(end) if end <= self.capacity => Ok(()),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "write extends past the end of the virtual disk",
            )),
        }
    }
}

impl BlockIo for VirtualDisk {
    fn capacity(&self) -> u64 {
        self.capacity
    }

    fn logical_block_size(&self) -> u32 {
        self.logical_block_size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        if offset >= self.capacity || buf.is_empty() {
            return Ok(0);
        }
        let remaining = self.capacity - offset;
        let len = usize::try_from(remaining).map_or(buf.len(), |r| r.min(buf.len()));
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read(&mut buf[..len])
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<usize> {
        self.check_range(offset, buf.len())?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.sync_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_io::{read_exact_at, write_all_at};

    #[test]
    fn rejects_invalid_geometry() {
        assert!(VirtualDisk::new(0).is_err());
        assert!(VirtualDisk::new(1000).is_err());
        assert!(VirtualDisk::with_block_size(4096, 1000).is_err());
        assert!(VirtualDisk::with_block_size(4096, 256).is_err());
        assert!(VirtualDisk::with_block_size(8192, 4096).is_ok());
    }

    #[test]
    fn starts_zeroed_and_round_trips_data() {
        let mut disk = VirtualDisk::new(1 << 20).unwrap();
        let mut buf = [0xAAu8; 512];
        read_exact_at(&mut disk, 4096, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0));

        write_all_at(&mut disk, 4096, &[7u8; 512]).unwrap();
        disk.flush().unwrap();
        read_exact_at(&mut disk, 4096, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 7));
    }

    #[test]
    fn never_grows_past_capacity() {
        let mut disk = VirtualDisk::new(4096).unwrap();
        let err = disk.write_at(4000, &[1u8; 512]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::metadata(disk.path()).unwrap().len(), 4096);
    }

    #[test]
    fn reads_are_truncated_at_the_end() {
        let mut disk = VirtualDisk::new(4096).unwrap();
        let mut buf = [0u8; 1024];
        assert_eq!(disk.read_at(3584, &mut buf).unwrap(), 512);
        assert_eq!(disk.read_at(4096, &mut buf).unwrap(), 0);
        let err = read_exact_at(&mut disk, 3584, &mut buf).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn large_disks_are_sparse() {
        // 16 GiB of virtual capacity must not consume 16 GiB of host disk.
        let disk = VirtualDisk::new(16 << 30).unwrap();
        assert_eq!(disk.block_count(), (16 << 30) / 512);
        if let Some(allocated) = disk.allocated_bytes().unwrap() {
            assert!(allocated < 1 << 20, "allocated {allocated} bytes");
        }
    }

    #[test]
    fn backing_file_is_removed_on_drop() {
        let disk = VirtualDisk::new(4096).unwrap();
        let path = disk.path().to_path_buf();
        assert!(path.exists());
        drop(disk);
        assert!(!path.exists());
    }
}
