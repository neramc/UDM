// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic data patterns for write / verify tests.
//!
//! Patterns are generated and checked in bounded chunks so that tests exercise
//! the same streaming discipline the product must follow (CLAUDE.md rule 11):
//! memory use is independent of the range length.

use std::io;

use crate::block_io::{BlockIo, read_exact_at, write_all_at};

/// Chunk size used for streaming pattern I/O.
pub const CHUNK_SIZE: usize = 64 * 1024;

/// The pattern byte at absolute `offset` for `seed`. Position-dependent, so
/// misplaced (shifted) data is detected as well as corrupted data.
#[must_use]
pub fn pattern_byte(seed: u64, offset: u64) -> u8 {
    let mut x = offset ^ seed.rotate_left(17) ^ 0xA076_1D64_78BD_642F;
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    x ^= x >> 33;
    x.to_le_bytes()[0]
}

fn fill_chunk(seed: u64, start: u64, buf: &mut [u8]) {
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte = pattern_byte(seed, start + i as u64);
    }
}

fn chunk_len(remaining: u64) -> usize {
    usize::try_from(remaining).map_or(CHUNK_SIZE, |r| r.min(CHUNK_SIZE))
}

/// Writes the pattern over `offset..offset + len`.
///
/// # Errors
/// Returns the first I/O error from the device.
pub fn write_pattern<D: BlockIo + ?Sized>(
    device: &mut D,
    seed: u64,
    offset: u64,
    len: u64,
) -> io::Result<()> {
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut done = 0u64;
    while done < len {
        let n = chunk_len(len - done);
        let pos = offset + done;
        fill_chunk(seed, pos, &mut buf[..n]);
        write_all_at(device, pos, &buf[..n])?;
        done += n as u64;
    }
    Ok(())
}

/// Outcome of a pattern verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Match,
    /// First mismatching absolute offset.
    MismatchAt(u64),
}

/// Verifies the pattern over `offset..offset + len`.
///
/// # Errors
/// Returns the first I/O error from the device. A data mismatch is not an
/// error; it is reported as [`Verification::MismatchAt`].
pub fn verify_pattern<D: BlockIo + ?Sized>(
    device: &mut D,
    seed: u64,
    offset: u64,
    len: u64,
) -> io::Result<Verification> {
    let mut actual = vec![0u8; CHUNK_SIZE];
    let mut expected = vec![0u8; CHUNK_SIZE];
    let mut done = 0u64;
    while done < len {
        let n = chunk_len(len - done);
        let pos = offset + done;
        read_exact_at(device, pos, &mut actual[..n])?;
        fill_chunk(seed, pos, &mut expected[..n]);
        if let Some(i) = actual[..n]
            .iter()
            .zip(&expected[..n])
            .position(|(a, e)| a != e)
        {
            return Ok(Verification::MismatchAt(pos + i as u64));
        }
        done += n as u64;
    }
    Ok(Verification::Match)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::{Fault, FaultyDisk};
    use crate::virtual_disk::VirtualDisk;

    #[test]
    fn pattern_round_trip_across_chunk_boundaries() {
        let mut disk = VirtualDisk::new(1 << 20).unwrap();
        let len = (CHUNK_SIZE * 3 + 123) as u64;
        write_pattern(&mut disk, 7, 512, len).unwrap();
        assert_eq!(
            verify_pattern(&mut disk, 7, 512, len).unwrap(),
            Verification::Match
        );
        assert_ne!(
            verify_pattern(&mut disk, 8, 512, len).unwrap(),
            Verification::Match,
            "a different seed must not verify"
        );
    }

    #[test]
    fn silent_corruption_is_located() {
        let mut faulty = FaultyDisk::new(
            VirtualDisk::new(1 << 20).unwrap(),
            vec![Fault::CorruptRead { offset: 200_000 }],
        );
        write_pattern(&mut faulty, 1, 0, 1 << 20).unwrap();
        assert_eq!(
            verify_pattern(&mut faulty, 1, 0, 1 << 20).unwrap(),
            Verification::MismatchAt(200_000)
        );
    }

    #[test]
    fn read_errors_during_verification_are_errors_not_mismatches() {
        let mut faulty = FaultyDisk::new(
            VirtualDisk::new(1 << 20).unwrap(),
            vec![Fault::FailReads {
                range: 300_000..300_001,
                kind: io::ErrorKind::Other,
            }],
        );
        write_pattern(&mut faulty, 1, 0, 1 << 20).unwrap();
        assert!(verify_pattern(&mut faulty, 1, 0, 1 << 20).is_err());
    }

    #[test]
    fn shifted_data_is_detected() {
        let mut disk = VirtualDisk::new(1 << 20).unwrap();
        write_pattern(&mut disk, 3, 1024, 4096).unwrap();
        assert_ne!(
            verify_pattern(&mut disk, 3, 512, 4096).unwrap(),
            Verification::Match
        );
    }
}
