// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Writes MBR and GPT virtual-disk fixtures into a directory so they can be
//! cross-checked with independent tools (e.g. `fdisk -l`, `sfdisk --json`).
//!
//! Usage: `cargo run -p udm-testkit --example write_fixtures -- <new-directory>`
//!
//! Safety: the output directory must not exist yet, and images are only ever
//! created as new regular files inside it. This tool never opens a device.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use udm_testkit::layout::{
    GptPartition, Guid, MbrPartition, gpt_geometry, gpt_types, write_gpt, write_mbr,
};
use udm_testkit::{BlockIo, VirtualDisk};

fn main() -> ExitCode {
    let Some(out) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: write_fixtures <new-directory>");
        return ExitCode::from(2);
    };
    match run(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(out: &Path) -> io::Result<()> {
    // `create_dir` (not `create_dir_all`) fails if the path already exists.
    fs::create_dir(out)?;

    let mut mbr = VirtualDisk::new(64 << 20)?;
    write_mbr(
        &mut mbr,
        0x1234_5678,
        &[
            MbrPartition {
                partition_type: 0x0C,
                bootable: false,
                start_lba: 2048,
                sector_count: 65536,
            },
            MbrPartition {
                partition_type: 0x83,
                bootable: false,
                start_lba: 67584,
                sector_count: 63488,
            },
        ],
    )?;
    persist(&mbr, out, "mbr-512.img")?;

    for block_size in [512u32, 4096] {
        let mut disk = VirtualDisk::with_block_size(256 << 20, block_size)?;
        let g = gpt_geometry(&disk)?;
        let first = g
            .first_usable_lba
            .next_multiple_of(2048 * 512 / u64::from(block_size));
        let efi_blocks = (64 << 20) / u64::from(disk.logical_block_size());
        write_gpt(
            &mut disk,
            Guid::deterministic(7),
            &[
                GptPartition {
                    type_guid: Guid::parse(gpt_types::EFI_SYSTEM).unwrap_or(Guid([0; 16])),
                    unique_guid: Guid::deterministic(1),
                    first_lba: first,
                    last_lba: first + efi_blocks - 1,
                    attributes: 0,
                    name: "EFI system".into(),
                },
                GptPartition {
                    type_guid: Guid::parse(gpt_types::MICROSOFT_BASIC_DATA)
                        .unwrap_or(Guid([0; 16])),
                    unique_guid: Guid::deterministic(2),
                    first_lba: first + efi_blocks,
                    last_lba: g.last_usable_lba,
                    attributes: 0,
                    name: "Data".into(),
                },
            ],
        )?;
        persist(&disk, out, &format!("gpt-{block_size}.img"))?;
    }
    println!("fixtures written to {}", out.display());
    Ok(())
}

fn persist(disk: &VirtualDisk, out: &Path, name: &str) -> io::Result<()> {
    let target = out.join(name);
    let mut source = fs::File::open(disk.path())?;
    let mut dest = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?;
    io::copy(&mut source, &mut dest)?;
    println!(
        "{name}: {} bytes, {} byte blocks",
        disk.capacity(),
        disk.logical_block_size()
    );
    Ok(())
}
