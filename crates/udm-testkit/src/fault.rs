//! Fault injection for block I/O.
//!
//! [`FaultyDisk`] wraps any [`BlockIo`] and injects failures that real
//! removable storage produces: read/write errors in a range, short transfers,
//! silent corruption, flush failures, and a disconnect after a number of bytes.

use std::fmt;
use std::io;
use std::ops::Range;

use crate::block_io::BlockIo;

/// Marker payload carried by injected errors so tests can tell injected
/// failures from genuine host I/O errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectedError {
    ReadFailure,
    WriteFailure,
    FlushFailure,
    Disconnected,
}

impl fmt::Display for InjectedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::ReadFailure => "injected read failure",
            Self::WriteFailure => "injected write failure",
            Self::FlushFailure => "injected flush failure",
            Self::Disconnected => "injected disconnect: device is gone",
        };
        f.write_str(text)
    }
}

impl std::error::Error for InjectedError {}

impl InjectedError {
    /// Returns the injected error carried by `error`, if any.
    #[must_use]
    pub fn from_io(error: &io::Error) -> Option<Self> {
        error.get_ref()?.downcast_ref::<Self>().copied()
    }
}

/// A fault to inject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// Reads overlapping `range` fail with `kind`.
    FailReads {
        range: Range<u64>,
        kind: io::ErrorKind,
    },
    /// Writes overlapping `range` fail with `kind`.
    FailWrites {
        range: Range<u64>,
        kind: io::ErrorKind,
    },
    /// Every read transfers at most `max_bytes` (must be non-zero).
    ShortReads { max_bytes: usize },
    /// Every write transfers at most `max_bytes` (must be non-zero).
    ShortWrites { max_bytes: usize },
    /// The byte at `offset` is returned with all bits inverted (silent
    /// corruption; the read itself succeeds).
    CorruptRead { offset: u64 },
    /// `flush` fails.
    FailFlush,
    /// After `bytes` bytes have been transferred in total (reads + writes),
    /// the device disconnects: the transfer that crosses the threshold is cut
    /// short and every later operation fails.
    DisconnectAfter { bytes: u64 },
}

/// A [`BlockIo`] wrapper that injects [`Fault`]s.
#[derive(Debug)]
pub struct FaultyDisk<D> {
    inner: D,
    faults: Vec<Fault>,
    transferred: u64,
    disconnected: bool,
    injected: u64,
}

impl<D: BlockIo> FaultyDisk<D> {
    #[must_use]
    pub fn new(inner: D, faults: Vec<Fault>) -> Self {
        Self {
            inner,
            faults,
            transferred: 0,
            disconnected: false,
            injected: 0,
        }
    }

    /// Number of faults that have fired so far.
    #[must_use]
    pub const fn injected_count(&self) -> u64 {
        self.injected
    }

    /// Whether a simulated disconnect has happened.
    #[must_use]
    pub const fn is_disconnected(&self) -> bool {
        self.disconnected
    }

    /// Total bytes transferred successfully.
    #[must_use]
    pub const fn transferred(&self) -> u64 {
        self.transferred
    }

    /// Returns the wrapped device.
    pub fn into_inner(self) -> D {
        self.inner
    }

    fn fail(&mut self, kind: io::ErrorKind, error: InjectedError) -> io::Error {
        self.injected += 1;
        io::Error::new(kind, error)
    }

    fn overlapping(range: &Range<u64>, offset: u64, len: usize) -> bool {
        let end = offset.saturating_add(u64::try_from(len).unwrap_or(u64::MAX));
        offset < range.end && range.start < end
    }

    /// Applies short-transfer and disconnect limits; returns the permitted length.
    fn permitted_len(&mut self, requested: usize, short_limit: Option<usize>) -> io::Result<usize> {
        if self.disconnected {
            return Err(self.fail(io::ErrorKind::NotConnected, InjectedError::Disconnected));
        }
        let mut len = requested;
        if let Some(max) = short_limit
            && max > 0
            && max < len
        {
            self.injected += 1;
            len = max;
        }
        let disconnect_at = self.faults.iter().find_map(|f| match f {
            Fault::DisconnectAfter { bytes } => Some(*bytes),
            _ => None,
        });
        if let Some(limit) = disconnect_at {
            let remaining = limit.saturating_sub(self.transferred);
            if remaining == 0 {
                self.disconnected = true;
                return Err(self.fail(io::ErrorKind::NotConnected, InjectedError::Disconnected));
            }
            if let Ok(remaining) = usize::try_from(remaining)
                && remaining < len
            {
                len = remaining;
            }
        }
        Ok(len)
    }

    fn record_transfer(&mut self, n: usize) {
        self.transferred = self
            .transferred
            .saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
    }
}

impl<D: BlockIo> BlockIo for FaultyDisk<D> {
    fn capacity(&self) -> u64 {
        self.inner.capacity()
    }

    fn logical_block_size(&self) -> u32 {
        self.inner.logical_block_size()
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let failing = self.faults.iter().find_map(|f| match f {
            Fault::FailReads { range, kind } if Self::overlapping(range, offset, buf.len()) => {
                Some(*kind)
            }
            _ => None,
        });
        if let Some(kind) = failing {
            return Err(self.fail(kind, InjectedError::ReadFailure));
        }
        let short = self.faults.iter().find_map(|f| match f {
            Fault::ShortReads { max_bytes } => Some(*max_bytes),
            _ => None,
        });
        let len = self.permitted_len(buf.len(), short)?;
        let n = self.inner.read_at(offset, &mut buf[..len])?;
        self.record_transfer(n);

        let corrupt: Vec<u64> = self
            .faults
            .iter()
            .filter_map(|f| match f {
                Fault::CorruptRead { offset } => Some(*offset),
                _ => None,
            })
            .collect();
        for target in corrupt {
            if target >= offset
                && let Ok(index) = usize::try_from(target - offset)
                && index < n
            {
                buf[index] = !buf[index];
                self.injected += 1;
            }
        }
        Ok(n)
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> io::Result<usize> {
        let failing = self.faults.iter().find_map(|f| match f {
            Fault::FailWrites { range, kind } if Self::overlapping(range, offset, buf.len()) => {
                Some(*kind)
            }
            _ => None,
        });
        if let Some(kind) = failing {
            return Err(self.fail(kind, InjectedError::WriteFailure));
        }
        let short = self.faults.iter().find_map(|f| match f {
            Fault::ShortWrites { max_bytes } => Some(*max_bytes),
            _ => None,
        });
        let len = self.permitted_len(buf.len(), short)?;
        let n = self.inner.write_at(offset, &buf[..len])?;
        self.record_transfer(n);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.disconnected {
            return Err(self.fail(io::ErrorKind::NotConnected, InjectedError::Disconnected));
        }
        if self.faults.contains(&Fault::FailFlush) {
            return Err(self.fail(io::ErrorKind::Other, InjectedError::FlushFailure));
        }
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_io::{read_exact_at, write_all_at};
    use crate::virtual_disk::VirtualDisk;

    fn disk(faults: Vec<Fault>) -> FaultyDisk<VirtualDisk> {
        FaultyDisk::new(VirtualDisk::new(1 << 20).unwrap(), faults)
    }

    #[test]
    fn read_errors_fire_only_in_range() {
        let mut d = disk(vec![Fault::FailReads {
            range: 4096..8192,
            kind: io::ErrorKind::Other,
        }]);
        let mut buf = [0u8; 512];
        assert!(d.read_at(0, &mut buf).is_ok());
        let err = d.read_at(4000, &mut buf).unwrap_err();
        assert_eq!(
            InjectedError::from_io(&err),
            Some(InjectedError::ReadFailure)
        );
        assert!(d.read_at(8192, &mut buf).is_ok());
        assert_eq!(d.injected_count(), 1);
    }

    #[test]
    fn write_errors_fire_only_in_range() {
        let mut d = disk(vec![Fault::FailWrites {
            range: 0..512,
            kind: io::ErrorKind::PermissionDenied,
        }]);
        let err = d.write_at(0, &[1u8; 16]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            InjectedError::from_io(&err),
            Some(InjectedError::WriteFailure)
        );
        assert_eq!(d.write_at(512, &[1u8; 16]).unwrap(), 16);
    }

    #[test]
    fn short_writes_are_completed_by_write_all_at() {
        let mut d = disk(vec![Fault::ShortWrites { max_bytes: 100 }]);
        assert_eq!(d.write_at(0, &[9u8; 512]).unwrap(), 100);
        write_all_at(&mut d, 0, &[9u8; 4096]).unwrap();
        let mut inner = d.into_inner();
        let mut buf = [0u8; 4096];
        read_exact_at(&mut inner, 0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 9));
    }

    #[test]
    fn short_reads_are_completed_by_read_exact_at() {
        let mut d = disk(vec![Fault::ShortReads { max_bytes: 7 }]);
        let mut buf = [1u8; 100];
        assert_eq!(d.read_at(0, &mut buf).unwrap(), 7);
        read_exact_at(&mut d, 0, &mut buf).unwrap();
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn corruption_is_silent_but_detectable() {
        let mut d = disk(vec![Fault::CorruptRead { offset: 10 }]);
        let mut buf = [0u8; 32];
        read_exact_at(&mut d, 0, &mut buf).unwrap();
        assert_eq!(buf[10], 0xFF);
        assert_eq!(buf[9], 0);
    }

    #[test]
    fn disconnect_cuts_transfer_and_fails_everything_after() {
        let mut d = disk(vec![Fault::DisconnectAfter { bytes: 1000 }]);
        assert_eq!(d.write_at(0, &[1u8; 512]).unwrap(), 512);
        // Crossing the threshold: only the remaining 488 bytes go through.
        assert_eq!(d.write_at(512, &[1u8; 512]).unwrap(), 488);
        assert_eq!(d.transferred(), 1000);
        let err = d.write_at(1000, &[1u8; 512]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotConnected);
        assert!(d.is_disconnected());
        let mut buf = [0u8; 16];
        assert!(d.read_at(0, &mut buf).is_err());
        assert_eq!(
            InjectedError::from_io(&d.flush().unwrap_err()),
            Some(InjectedError::Disconnected)
        );
        // write_all_at must surface the disconnect, not loop forever.
        assert!(write_all_at(&mut d, 0, &[0u8; 64]).is_err());
    }

    #[test]
    fn flush_failure_is_reported() {
        let mut d = disk(vec![Fault::FailFlush]);
        assert_eq!(
            InjectedError::from_io(&d.flush().unwrap_err()),
            Some(InjectedError::FlushFailure)
        );
    }

    #[test]
    fn genuine_errors_are_not_mistaken_for_injected_ones() {
        let mut d = disk(vec![]);
        let err = d.write_at(u64::MAX - 1, &[0u8; 4]).unwrap_err();
        assert_eq!(InjectedError::from_io(&err), None);
    }
}
