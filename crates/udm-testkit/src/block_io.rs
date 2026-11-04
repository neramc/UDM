//! Minimal positional block I/O interface used by the harness.
//!
//! This trait exists so that fault injection ([`crate::FaultyDisk`]) can wrap
//! a [`crate::VirtualDisk`] transparently. When the first streaming feature
//! (image write / verify) is implemented, the production equivalent will live
//! in an infrastructure crate and this trait will be aligned with it
//! (see `docs/ADR/0007-concurrency-and-io-model.md`).

use std::io;

/// A fixed-capacity, positionally addressed block device.
///
/// Implementations may transfer fewer bytes than requested (short reads and
/// writes). Callers that need the full transfer must loop, e.g. with
/// [`read_exact_at`] and [`write_all_at`].
pub trait BlockIo {
    /// Capacity in bytes. Never changes.
    fn capacity(&self) -> u64;

    /// Logical block size in bytes.
    fn logical_block_size(&self) -> u32;

    /// Reads up to `buf.len()` bytes at `offset`. Returns `Ok(0)` at or beyond
    /// the end of the device.
    ///
    /// # Errors
    /// Returns any I/O error reported by the underlying storage.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;

    /// Writes up to `buf.len()` bytes at `offset`.
    ///
    /// # Errors
    /// Returns [`io::ErrorKind::InvalidInput`] if the write would extend past
    /// the capacity, or any I/O error reported by the underlying storage.
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<usize>;

    /// Flushes written data to stable storage.
    ///
    /// # Errors
    /// Returns any I/O error reported by the underlying storage.
    fn flush(&mut self) -> io::Result<()>;
}

/// Reads exactly `buf.len()` bytes at `offset`, retrying short reads and
/// [`io::ErrorKind::Interrupted`].
///
/// # Errors
/// Returns [`io::ErrorKind::UnexpectedEof`] if the device ends first, or the
/// first non-interrupt error.
pub fn read_exact_at<D: BlockIo + ?Sized>(
    device: &mut D,
    mut offset: u64,
    mut buf: &mut [u8],
) -> io::Result<()> {
    while !buf.is_empty() {
        match device.read_at(offset, buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "device ended before the requested range was read",
                ));
            }
            Ok(n) => {
                offset = advance(offset, n)?;
                buf = &mut buf[n..];
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Writes all of `buf` at `offset`, retrying short writes and
/// [`io::ErrorKind::Interrupted`].
///
/// # Errors
/// Returns [`io::ErrorKind::WriteZero`] if the device stops accepting data, or
/// the first non-interrupt error.
pub fn write_all_at<D: BlockIo + ?Sized>(
    device: &mut D,
    mut offset: u64,
    mut buf: &[u8],
) -> io::Result<()> {
    while !buf.is_empty() {
        match device.write_at(offset, buf) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "device accepted no data",
                ));
            }
            Ok(n) => {
                offset = advance(offset, n)?;
                buf = &buf[n..];
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn advance(offset: u64, n: usize) -> io::Result<u64> {
    u64::try_from(n)
        .ok()
        .and_then(|n| offset.checked_add(n))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "offset overflow"))
}
