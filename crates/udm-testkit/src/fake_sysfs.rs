//! Fake Linux `/sys`, `/dev` and `/proc` trees for discovery tests.
//!
//! The layout mirrors the parts of the real kernel interfaces that the Linux
//! discovery adapter reads:
//!
//! ```text
//! <root>/sys/block/<name>            -> symlink to ../devices/<devpath>
//! <root>/sys/devices/<devpath>/size, ro, removable, dev, queue/*, <partitions>/
//! <root>/sys/devices/<devpath>/device -> symlink to the parent hardware device
//! <root>/sys/devices/<usb device>/idVendor, idProduct, serial, ...
//! <root>/dev/disk/by-id/<link>       -> symlink to ../../<name>
//! <root>/proc/self/mountinfo, <root>/proc/swaps
//! ```
//!
//! Everything lives in a temporary directory that is removed on drop.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A partition of a fake block device.
#[derive(Debug, Clone, Default)]
pub struct FakePartition {
    pub name: String,
    pub number: u32,
    pub start_sectors: u64,
    pub size_sectors: u64,
    /// `major:minor`.
    pub dev: String,
    /// Names of block devices stacked on this partition (e.g. `dm-0`).
    pub holders: Vec<String>,
}

/// A fake block device under `/sys/block`.
#[derive(Debug, Clone)]
pub struct FakeBlockDevice {
    /// Kernel name, e.g. `sdb`.
    pub name: String,
    /// Path of the block device directory relative to `sys/devices`, e.g.
    /// `pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0/host6/target6:0:0/6:0:0:0/block/sdb`.
    pub devpath: String,
    /// Target of the `device` link relative to `sys/devices`. `None` for
    /// virtual devices (loop, device-mapper, ...), which have no such link.
    pub device_dir: Option<String>,
    /// `major:minor`.
    pub dev: String,
    /// Size in 512-byte sectors (the kernel always uses 512-byte units here).
    pub size_sectors: u64,
    pub logical_block_size: u32,
    pub physical_block_size: u32,
    pub removable: bool,
    pub read_only: bool,
    /// Files written into the `device` directory (e.g. `vendor`, `model`).
    pub device_attributes: Vec<(String, String)>,
    /// Files written into the block device directory itself (e.g. `wwid`).
    pub own_attributes: Vec<(String, String)>,
    pub partitions: Vec<FakePartition>,
    pub holders: Vec<String>,
    pub slaves: Vec<String>,
}

impl Default for FakeBlockDevice {
    fn default() -> Self {
        Self {
            name: String::new(),
            devpath: String::new(),
            device_dir: None,
            dev: String::new(),
            size_sectors: 0,
            logical_block_size: 512,
            physical_block_size: 512,
            removable: false,
            read_only: false,
            device_attributes: Vec::new(),
            own_attributes: Vec::new(),
            partitions: Vec::new(),
            holders: Vec::new(),
            slaves: Vec::new(),
        }
    }
}

/// USB device descriptor attributes, written into a USB device directory.
#[derive(Debug, Clone, Default)]
pub struct FakeUsbDevice {
    /// Directory relative to `sys/devices`, e.g. `pci0000:00/0000:00:14.0/usb1/1-2`.
    pub dir: String,
    pub id_vendor: String,
    pub id_product: String,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    /// Speed in Mbit/s as reported by sysfs (e.g. `480`, `5000`).
    pub speed: Option<String>,
}

/// One line of `/proc/self/mountinfo`.
#[derive(Debug, Clone)]
pub struct FakeMount {
    /// `major:minor` of the mounted block device.
    pub dev: String,
    pub mount_point: String,
    pub fstype: String,
    pub source: String,
}

/// A fake root containing `sys`, `dev` and `proc`.
#[derive(Debug)]
pub struct FakeSystem {
    dir: TempDir,
}

impl FakeSystem {
    /// Creates an empty fake root with `sys/block`, `sys/devices`,
    /// `dev/disk/by-id`, an empty mount table and an empty swap table.
    ///
    /// # Errors
    /// Returns an error if the temporary directory cannot be populated.
    pub fn new() -> io::Result<Self> {
        let dir = tempfile::Builder::new().prefix("udm-fakesys-").tempdir()?;
        let this = Self { dir };
        fs::create_dir_all(this.sys().join("block"))?;
        fs::create_dir_all(this.sys().join("devices"))?;
        fs::create_dir_all(this.dev().join("disk/by-id"))?;
        this.set_mountinfo(&[])?;
        this.set_swaps(&[])?;
        Ok(this)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    #[must_use]
    pub fn sys(&self) -> PathBuf {
        self.root().join("sys")
    }

    #[must_use]
    pub fn dev(&self) -> PathBuf {
        self.root().join("dev")
    }

    #[must_use]
    pub fn mountinfo(&self) -> PathBuf {
        self.root().join("proc/self/mountinfo")
    }

    #[must_use]
    pub fn swaps(&self) -> PathBuf {
        self.root().join("proc/swaps")
    }

    /// Writes `contents` to `relative` (relative to the fake root), creating
    /// parent directories.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn write(&self, relative: impl AsRef<Path>, contents: &str) -> io::Result<()> {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }

    /// Creates a symlink at `relative` pointing to `target`, creating parent
    /// directories. The target does not need to exist.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn symlink(&self, relative: impl AsRef<Path>, target: impl AsRef<Path>) -> io::Result<()> {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        symlink(target, path)
    }

    /// Adds a block device, its partitions, holders and slaves.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn add_block_device(&self, device: &FakeBlockDevice) -> io::Result<()> {
        let devices = self.sys().join("devices");
        let block_dir = devices.join(&device.devpath);
        fs::create_dir_all(&block_dir)?;
        let put = |dir: &Path, name: &str, value: &str| -> io::Result<()> {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, format!("{value}\n"))
        };

        put(&block_dir, "dev", &device.dev)?;
        put(&block_dir, "size", &device.size_sectors.to_string())?;
        put(&block_dir, "ro", if device.read_only { "1" } else { "0" })?;
        put(
            &block_dir,
            "removable",
            if device.removable { "1" } else { "0" },
        )?;
        put(
            &block_dir,
            "queue/logical_block_size",
            &device.logical_block_size.to_string(),
        )?;
        put(
            &block_dir,
            "queue/physical_block_size",
            &device.physical_block_size.to_string(),
        )?;
        for (name, value) in &device.own_attributes {
            put(&block_dir, name, value)?;
        }
        if let Some(device_dir) = &device.device_dir {
            let hw = devices.join(device_dir);
            fs::create_dir_all(&hw)?;
            for (name, value) in &device.device_attributes {
                put(&hw, name, value)?;
            }
            symlink(&hw, block_dir.join("device"))?;
        }
        fs::create_dir_all(block_dir.join("holders"))?;
        fs::create_dir_all(block_dir.join("slaves"))?;
        for holder in &device.holders {
            symlink(
                devices.join("virtual/block").join(holder),
                block_dir.join("holders").join(holder),
            )?;
        }
        for slave in &device.slaves {
            symlink(
                self.sys().join("block").join(slave),
                block_dir.join("slaves").join(slave),
            )?;
        }
        for partition in &device.partitions {
            let part_dir = block_dir.join(&partition.name);
            fs::create_dir_all(part_dir.join("holders"))?;
            put(&part_dir, "partition", &partition.number.to_string())?;
            put(&part_dir, "start", &partition.start_sectors.to_string())?;
            put(&part_dir, "size", &partition.size_sectors.to_string())?;
            put(&part_dir, "dev", &partition.dev)?;
            put(&part_dir, "ro", "0")?;
            for holder in &partition.holders {
                symlink(
                    devices.join("virtual/block").join(holder),
                    part_dir.join("holders").join(holder),
                )?;
            }
        }
        symlink(&block_dir, self.sys().join("block").join(&device.name))
    }

    /// Writes USB descriptor attributes into a USB device directory.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn add_usb_device(&self, usb: &FakeUsbDevice) -> io::Result<()> {
        let dir = self.sys().join("devices").join(&usb.dir);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("idVendor"), format!("{}\n", usb.id_vendor))?;
        fs::write(dir.join("idProduct"), format!("{}\n", usb.id_product))?;
        let optional = [
            ("manufacturer", &usb.manufacturer),
            ("product", &usb.product),
            ("serial", &usb.serial),
            ("speed", &usb.speed),
        ];
        for (name, value) in optional {
            if let Some(value) = value {
                fs::write(dir.join(name), format!("{value}\n"))?;
            }
        }
        Ok(())
    }

    /// Adds `/dev/disk/by-id/<link>` pointing at `/dev/<kernel_name>`.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn add_by_id_link(&self, link: &str, kernel_name: &str) -> io::Result<()> {
        self.symlink(
            Path::new("dev/disk/by-id").join(link),
            Path::new("../..").join(kernel_name),
        )
    }

    /// Replaces the mount table.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn set_mountinfo(&self, mounts: &[FakeMount]) -> io::Result<()> {
        let mut text = String::new();
        for (i, m) in mounts.iter().enumerate() {
            let id = 20 + i;
            text.push_str(&format!(
                "{id} 1 {} / {} rw,relatime shared:1 - {} {} rw\n",
                m.dev,
                escape_mount_field(&m.mount_point),
                m.fstype,
                escape_mount_field(&m.source),
            ));
        }
        self.write("proc/self/mountinfo", &text)
    }

    /// Replaces the swap table with the given swap device paths.
    ///
    /// # Errors
    /// Returns any I/O error.
    pub fn set_swaps(&self, devices: &[&str]) -> io::Result<()> {
        let mut text = String::from("Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n");
        for device in devices {
            text.push_str(&format!("{device}\t\t\t\tpartition\t1048572\t\t0\t\t-2\n"));
        }
        self.write("proc/swaps", &text)
    }
}

/// Escapes a mountinfo field the way the kernel does (space, tab, newline and
/// backslash as octal escapes).
#[must_use]
pub fn escape_mount_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    for c in field.chars() {
        match c {
            ' ' => out.push_str("\\040"),
            '\t' => out.push_str("\\011"),
            '\n' => out.push_str("\\012"),
            '\\' => out.push_str("\\134"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_layout() {
        let sys = FakeSystem::new().unwrap();
        sys.add_block_device(&FakeBlockDevice {
            name: "sdb".into(),
            devpath: "pci0/usb1/1-2/host6/6:0:0:0/block/sdb".into(),
            device_dir: Some("pci0/usb1/1-2/host6/6:0:0:0".into()),
            dev: "8:16".into(),
            size_sectors: 1000,
            device_attributes: vec![("model".into(), "Stick".into())],
            partitions: vec![FakePartition {
                name: "sdb1".into(),
                number: 1,
                start_sectors: 2048,
                size_sectors: 100,
                dev: "8:17".into(),
                holders: vec![],
            }],
            ..FakeBlockDevice::default()
        })
        .unwrap();
        sys.add_by_id_link("usb-Stick-0:0", "sdb").unwrap();

        let block = sys.sys().join("block/sdb");
        assert_eq!(fs::read_to_string(block.join("size")).unwrap(), "1000\n");
        assert_eq!(
            fs::read_to_string(block.join("device/model")).unwrap(),
            "Stick\n"
        );
        assert_eq!(
            fs::read_to_string(block.join("sdb1/partition")).unwrap(),
            "1\n"
        );
        let link = fs::read_link(sys.dev().join("disk/by-id/usb-Stick-0:0")).unwrap();
        assert_eq!(link, Path::new("../../sdb"));
    }

    #[test]
    fn mount_fields_are_escaped() {
        assert_eq!(escape_mount_field("/media/a b"), "/media/a\\040b");
        assert_eq!(escape_mount_field("a\\b"), "a\\134b");
    }
}
