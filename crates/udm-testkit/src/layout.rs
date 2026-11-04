// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Partition-table writers for virtual disks.
//!
//! These produce well-formed MBR and GPT layouts so that future partition
//! inspection, imaging and verification code can be tested against known
//! fixtures. They only ever write to a [`BlockIo`] implementation supplied by
//! the caller — in practice a [`crate::VirtualDisk`].

use std::io;

use crate::block_io::{BlockIo, write_all_at};
use crate::crc32::crc32;

/// One MBR primary partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MbrPartition {
    /// MBR partition type byte (e.g. `0x0C` FAT32 LBA, `0x83` Linux, `0x07` NTFS/exFAT).
    pub partition_type: u8,
    pub bootable: bool,
    pub start_lba: u32,
    pub sector_count: u32,
}

/// Writes an MBR with up to four primary partitions.
///
/// # Errors
/// Returns [`io::ErrorKind::InvalidInput`] for more than four partitions,
/// zero-sized or out-of-range partitions, or overlapping partitions; otherwise
/// any I/O error from the device.
pub fn write_mbr<D: BlockIo + ?Sized>(
    device: &mut D,
    disk_signature: u32,
    partitions: &[MbrPartition],
) -> io::Result<()> {
    if partitions.len() > 4 {
        return Err(invalid("MBR supports at most four primary partitions"));
    }
    let block = u64::from(device.logical_block_size());
    let total_blocks = device.capacity() / block;
    let mut ranges: Vec<(u64, u64)> = Vec::with_capacity(partitions.len());
    for p in partitions {
        let start = u64::from(p.start_lba);
        let end = start + u64::from(p.sector_count);
        if p.start_lba == 0 || p.sector_count == 0 || end > total_blocks {
            return Err(invalid("MBR partition is empty or outside the disk"));
        }
        if ranges.iter().any(|&(s, e)| start < e && s < end) {
            return Err(invalid("MBR partitions overlap"));
        }
        ranges.push((start, end));
    }

    let mut sector = [0u8; 512];
    sector[440..444].copy_from_slice(&disk_signature.to_le_bytes());
    for (i, p) in partitions.iter().enumerate() {
        let entry = &mut sector[446 + i * 16..446 + (i + 1) * 16];
        entry[0] = if p.bootable { 0x80 } else { 0x00 };
        // CHS fields are set to the conventional "use LBA" placeholder.
        entry[1..4].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        entry[4] = p.partition_type;
        entry[5..8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        entry[8..12].copy_from_slice(&p.start_lba.to_le_bytes());
        entry[12..16].copy_from_slice(&p.sector_count.to_le_bytes());
    }
    sector[510] = 0x55;
    sector[511] = 0xAA;
    write_all_at(device, 0, &sector)
}

/// A GUID in its on-disk (mixed-endian) byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// Parses the canonical `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX` text form
    /// and converts it to GPT on-disk byte order (first three fields
    /// little-endian).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let hex: String = text.chars().filter(|c| *c != '-').collect();
        if hex.len() != 32 || text.len() != 36 {
            return None;
        }
        let mut be = [0u8; 16];
        for (i, byte) in be.iter_mut().enumerate() {
            *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
        }
        let mut disk = be;
        disk[0..4].reverse();
        disk[4..6].reverse();
        disk[6..8].reverse();
        Some(Self(disk))
    }

    /// A deterministic pseudo-unique GUID for fixtures (RFC 4122 variant,
    /// version 4 bits set). Not random: fixtures must be reproducible.
    #[must_use]
    pub fn deterministic(seed: u64) -> Self {
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        let mut bytes = [0u8; 16];
        for chunk in bytes.chunks_mut(8) {
            state = splitmix64(state);
            chunk.copy_from_slice(&state.to_le_bytes()[..chunk.len()]);
        }
        bytes[7] = (bytes[7] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Self(bytes)
    }
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Well-known GPT partition type GUIDs.
pub mod gpt_types {
    /// EFI System Partition.
    pub const EFI_SYSTEM: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";
    /// Microsoft basic data (FAT, exFAT, NTFS).
    pub const MICROSOFT_BASIC_DATA: &str = "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7";
    /// Linux filesystem data.
    pub const LINUX_FILESYSTEM: &str = "0FC63DAF-8483-4772-8E79-3D69D8477DE4";
}

/// One GPT partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptPartition {
    pub type_guid: Guid,
    pub unique_guid: Guid,
    pub first_lba: u64,
    /// Inclusive, as stored on disk.
    pub last_lba: u64,
    pub attributes: u64,
    /// Up to 36 UTF-16 code units.
    pub name: String,
}

/// Geometry of a written GPT, for assertions in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GptGeometry {
    pub primary_header_lba: u64,
    pub primary_entries_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub backup_entries_lba: u64,
    pub backup_header_lba: u64,
    pub entry_count: u32,
    pub entry_size: u32,
}

const GPT_ENTRY_COUNT: u32 = 128;
const GPT_ENTRY_SIZE: u32 = 128;
const GPT_HEADER_SIZE: u32 = 92;

/// Computes the GPT geometry for a device.
///
/// # Errors
/// Returns [`io::ErrorKind::InvalidInput`] if the device is too small to hold
/// a GPT with usable space.
pub fn gpt_geometry<D: BlockIo + ?Sized>(device: &D) -> io::Result<GptGeometry> {
    let block = u64::from(device.logical_block_size());
    let total = device.capacity() / block;
    let entries_bytes = u64::from(GPT_ENTRY_COUNT) * u64::from(GPT_ENTRY_SIZE);
    let entries_blocks = entries_bytes.div_ceil(block);
    // LBA0 PMBR, LBA1 header, entries, ..., backup entries, backup header.
    let min_blocks = 2 + 2 * entries_blocks + 1;
    if total < min_blocks + 1 {
        return Err(invalid("device too small for GPT"));
    }
    let last = total - 1;
    Ok(GptGeometry {
        primary_header_lba: 1,
        primary_entries_lba: 2,
        first_usable_lba: 2 + entries_blocks,
        last_usable_lba: last - 1 - entries_blocks,
        backup_entries_lba: last - entries_blocks,
        backup_header_lba: last,
        entry_count: GPT_ENTRY_COUNT,
        entry_size: GPT_ENTRY_SIZE,
    })
}

/// Writes a protective MBR, primary and backup GPT headers, and both
/// partition entry arrays.
///
/// # Errors
/// Returns [`io::ErrorKind::InvalidInput`] for too many partitions, names that
/// are too long, partitions outside the usable range or overlapping
/// partitions; otherwise any I/O error from the device.
pub fn write_gpt<D: BlockIo + ?Sized>(
    device: &mut D,
    disk_guid: Guid,
    partitions: &[GptPartition],
) -> io::Result<GptGeometry> {
    let geometry = gpt_geometry(device)?;
    if partitions.len() > GPT_ENTRY_COUNT as usize {
        return Err(invalid("too many GPT partitions"));
    }
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for p in partitions {
        if p.first_lba < geometry.first_usable_lba
            || p.last_lba > geometry.last_usable_lba
            || p.first_lba > p.last_lba
        {
            return Err(invalid("GPT partition outside the usable range"));
        }
        if ranges
            .iter()
            .any(|&(s, e)| p.first_lba <= e && s <= p.last_lba)
        {
            return Err(invalid("GPT partitions overlap"));
        }
        ranges.push((p.first_lba, p.last_lba));
    }

    let block = u64::from(device.logical_block_size());
    let entry_size = GPT_ENTRY_SIZE as usize;
    let mut entries = vec![0u8; GPT_ENTRY_COUNT as usize * entry_size];
    for (i, p) in partitions.iter().enumerate() {
        let e = &mut entries[i * entry_size..(i + 1) * entry_size];
        e[0..16].copy_from_slice(&p.type_guid.0);
        e[16..32].copy_from_slice(&p.unique_guid.0);
        e[32..40].copy_from_slice(&p.first_lba.to_le_bytes());
        e[40..48].copy_from_slice(&p.last_lba.to_le_bytes());
        e[48..56].copy_from_slice(&p.attributes.to_le_bytes());
        let units: Vec<u16> = p.name.encode_utf16().collect();
        if units.len() > 36 {
            return Err(invalid("GPT partition name longer than 36 UTF-16 units"));
        }
        for (j, unit) in units.iter().enumerate() {
            e[56 + j * 2..58 + j * 2].copy_from_slice(&unit.to_le_bytes());
        }
    }
    let entries_crc = crc32(&entries);

    // Protective MBR.
    let total_blocks = device.capacity() / block;
    let pmbr_size = u32::try_from(total_blocks - 1).unwrap_or(u32::MAX);
    let mut pmbr = [0u8; 512];
    let entry = &mut pmbr[446..462];
    entry[1..4].copy_from_slice(&[0x00, 0x02, 0x00]);
    entry[4] = 0xEE;
    entry[5..8].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
    entry[8..12].copy_from_slice(&1u32.to_le_bytes());
    entry[12..16].copy_from_slice(&pmbr_size.to_le_bytes());
    pmbr[510] = 0x55;
    pmbr[511] = 0xAA;
    write_all_at(device, 0, &pmbr)?;

    let header = |current: u64, backup: u64, entries_lba: u64| -> Vec<u8> {
        let mut h = vec![0u8; GPT_HEADER_SIZE as usize];
        h[0..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        h[12..16].copy_from_slice(&GPT_HEADER_SIZE.to_le_bytes());
        // 16..20 header CRC (filled below), 20..24 reserved.
        h[24..32].copy_from_slice(&current.to_le_bytes());
        h[32..40].copy_from_slice(&backup.to_le_bytes());
        h[40..48].copy_from_slice(&geometry.first_usable_lba.to_le_bytes());
        h[48..56].copy_from_slice(&geometry.last_usable_lba.to_le_bytes());
        h[56..72].copy_from_slice(&disk_guid.0);
        h[72..80].copy_from_slice(&entries_lba.to_le_bytes());
        h[80..84].copy_from_slice(&GPT_ENTRY_COUNT.to_le_bytes());
        h[84..88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        h[88..92].copy_from_slice(&entries_crc.to_le_bytes());
        let crc = crc32(&h);
        h[16..20].copy_from_slice(&crc.to_le_bytes());
        h
    };

    let primary = header(
        geometry.primary_header_lba,
        geometry.backup_header_lba,
        geometry.primary_entries_lba,
    );
    let backup = header(
        geometry.backup_header_lba,
        geometry.primary_header_lba,
        geometry.backup_entries_lba,
    );
    write_all_at(device, geometry.primary_header_lba * block, &primary)?;
    write_all_at(device, geometry.primary_entries_lba * block, &entries)?;
    write_all_at(device, geometry.backup_entries_lba * block, &entries)?;
    write_all_at(device, geometry.backup_header_lba * block, &backup)?;
    device.flush()?;
    Ok(geometry)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_io::read_exact_at;
    use crate::virtual_disk::VirtualDisk;

    fn read_block(disk: &mut VirtualDisk, lba: u64) -> Vec<u8> {
        let block = u64::from(disk.logical_block_size());
        let mut buf = vec![0u8; usize::try_from(block).unwrap()];
        read_exact_at(disk, lba * block, &mut buf).unwrap();
        buf
    }

    fn le_u32(b: &[u8]) -> u32 {
        u32::from_le_bytes(b.try_into().unwrap())
    }

    fn le_u64(b: &[u8]) -> u64 {
        u64::from_le_bytes(b.try_into().unwrap())
    }

    #[test]
    fn guid_parse_uses_mixed_endian_disk_order() {
        let g = Guid::parse(gpt_types::EFI_SYSTEM).unwrap();
        assert_eq!(
            g.0,
            [
                0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E,
                0xC9, 0x3B
            ]
        );
        assert_eq!(Guid::parse("not-a-guid"), None);
        assert_eq!(Guid::deterministic(1), Guid::deterministic(1));
        assert_ne!(Guid::deterministic(1), Guid::deterministic(2));
    }

    #[test]
    fn mbr_layout_is_well_formed() {
        let mut disk = VirtualDisk::new(64 << 20).unwrap();
        write_mbr(
            &mut disk,
            0xDEAD_BEEF,
            &[
                MbrPartition {
                    partition_type: 0x0C,
                    bootable: true,
                    start_lba: 2048,
                    sector_count: 65536,
                },
                MbrPartition {
                    partition_type: 0x83,
                    bootable: false,
                    start_lba: 67584,
                    sector_count: 2048,
                },
            ],
        )
        .unwrap();
        let s = read_block(&mut disk, 0);
        assert_eq!(&s[510..512], &[0x55, 0xAA]);
        assert_eq!(le_u32(&s[440..444]), 0xDEAD_BEEF);
        assert_eq!(s[446], 0x80);
        assert_eq!(s[446 + 4], 0x0C);
        assert_eq!(le_u32(&s[446 + 8..446 + 12]), 2048);
        assert_eq!(le_u32(&s[446 + 12..446 + 16]), 65536);
        assert_eq!(s[462 + 4], 0x83);
        assert_eq!(s[478 + 4], 0, "unused entries stay empty");
    }

    #[test]
    fn mbr_rejects_invalid_layouts() {
        let mut disk = VirtualDisk::new(1 << 20).unwrap();
        let p = |start, count| MbrPartition {
            partition_type: 0x83,
            bootable: false,
            start_lba: start,
            sector_count: count,
        };
        assert!(write_mbr(&mut disk, 0, &[p(0, 10)]).is_err());
        assert!(write_mbr(&mut disk, 0, &[p(10, 0)]).is_err());
        assert!(write_mbr(&mut disk, 0, &[p(2000, 100)]).is_err());
        assert!(write_mbr(&mut disk, 0, &[p(100, 100), p(150, 10)]).is_err());
        assert!(write_mbr(&mut disk, 0, &[p(1, 1), p(2, 1), p(3, 1), p(4, 1), p(5, 1)]).is_err());
    }

    fn gpt_fixture(block_size: u32) -> (VirtualDisk, GptGeometry, Guid) {
        let mut disk = VirtualDisk::with_block_size(256 << 20, block_size).unwrap();
        let disk_guid = Guid::deterministic(42);
        let geometry = gpt_geometry(&disk).unwrap();
        let first = geometry.first_usable_lba.next_multiple_of(256);
        let parts = vec![
            GptPartition {
                type_guid: Guid::parse(gpt_types::EFI_SYSTEM).unwrap(),
                unique_guid: Guid::deterministic(1),
                first_lba: first,
                last_lba: first + 1023,
                attributes: 0,
                name: "EFI system".into(),
            },
            GptPartition {
                type_guid: Guid::parse(gpt_types::MICROSOFT_BASIC_DATA).unwrap(),
                unique_guid: Guid::deterministic(2),
                first_lba: first + 1024,
                last_lba: geometry.last_usable_lba,
                attributes: 0,
                name: "데이터".into(),
            },
        ];
        let written = write_gpt(&mut disk, disk_guid, &parts).unwrap();
        assert_eq!(written, geometry);
        (disk, geometry, disk_guid)
    }

    fn check_header(
        header_block: &[u8],
        expected_current: u64,
        expected_backup: u64,
        disk_guid: Guid,
    ) -> (u64, u32) {
        assert_eq!(&header_block[0..8], b"EFI PART");
        let size = le_u32(&header_block[12..16]) as usize;
        let mut h = header_block[..size].to_vec();
        let stored_crc = le_u32(&h[16..20]);
        h[16..20].fill(0);
        assert_eq!(crc32(&h), stored_crc, "header CRC");
        assert_eq!(le_u64(&h[24..32]), expected_current);
        assert_eq!(le_u64(&h[32..40]), expected_backup);
        assert_eq!(&h[56..72], &disk_guid.0);
        (le_u64(&h[72..80]), le_u32(&h[88..92]))
    }

    #[test]
    fn gpt_layout_is_well_formed_for_512_and_4k_sectors() {
        for block_size in [512u32, 4096] {
            let (mut disk, g, disk_guid) = gpt_fixture(block_size);
            let block = u64::from(block_size);

            let pmbr = read_block(&mut disk, 0);
            assert_eq!(pmbr[446 + 4], 0xEE, "protective MBR type");
            assert_eq!(&pmbr[510..512], &[0x55, 0xAA]);

            let primary = read_block(&mut disk, 1);
            let (entries_lba, entries_crc) =
                check_header(&primary, 1, g.backup_header_lba, disk_guid);
            assert_eq!(entries_lba, 2);

            let backup = read_block(&mut disk, g.backup_header_lba);
            let (backup_entries_lba, backup_entries_crc) =
                check_header(&backup, g.backup_header_lba, 1, disk_guid);
            assert_eq!(backup_entries_lba, g.backup_entries_lba);
            assert_eq!(entries_crc, backup_entries_crc);
            assert_eq!(g.backup_header_lba, disk.block_count() - 1);

            let mut entries = vec![0u8; 128 * 128];
            read_exact_at(&mut disk, entries_lba * block, &mut entries).unwrap();
            assert_eq!(crc32(&entries), entries_crc, "entries CRC");
            assert_eq!(
                &entries[0..16],
                &Guid::parse(gpt_types::EFI_SYSTEM).unwrap().0
            );
            assert_eq!(le_u64(&entries[128 + 40..128 + 48]), g.last_usable_lba);
            assert!(entries[256..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn gpt_rejects_overlap_and_out_of_range() {
        let mut disk = VirtualDisk::new(16 << 20).unwrap();
        let g = gpt_geometry(&disk).unwrap();
        let part = |first, last| GptPartition {
            type_guid: Guid::parse(gpt_types::LINUX_FILESYSTEM).unwrap(),
            unique_guid: Guid::deterministic(first),
            first_lba: first,
            last_lba: last,
            attributes: 0,
            name: String::new(),
        };
        assert!(write_gpt(&mut disk, Guid::deterministic(0), &[part(1, 100)]).is_err());
        assert!(
            write_gpt(
                &mut disk,
                Guid::deterministic(0),
                &[part(g.first_usable_lba, g.last_usable_lba + 1)]
            )
            .is_err()
        );
        assert!(
            write_gpt(
                &mut disk,
                Guid::deterministic(0),
                &[part(2048, 4095), part(4000, 5000)]
            )
            .is_err()
        );
    }

    #[test]
    fn gpt_requires_minimum_size() {
        let disk = VirtualDisk::new(16 * 1024).unwrap();
        assert!(gpt_geometry(&disk).is_err());
    }
}
