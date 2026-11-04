//! Tests for the Linux sysfs adapter against fake `/sys` trees.

use std::path::Path;

use udm_domain::discovery::{ProbeFailure, ProbeTarget};
use udm_domain::identity::ObservationSource;
use udm_domain::{
    AttributeKind, Capability, CapabilityReason, CapabilityStatus, Confidence, Device,
    DiscoveryIssueKind, MediaState, SystemDiskStatus, Transport, VolumeUsage, WriteProtection,
};
use udm_testkit::fake_sysfs::{
    FakeBlockDevice, FakeMount, FakePartition, FakeSystem, FakeUsbDevice,
};

use super::{LinuxPaths, LinuxSysfsDiscovery, classify_transport};
use crate::{DeviceDiscovery, DiscoveryError, DiscoveryReport};

const USB_PORT: &str = "pci0000:00/0000:00:14.0/usb2/2-1";
const USB_SCSI: &str = "pci0000:00/0000:00:14.0/usb2/2-1/2-1:1.0/host6/target6:0:0/6:0:0:0";
const NVME_CTRL: &str = "pci0000:00/0000:00:1d.0/0000:3d:00.0/nvme/nvme0";

fn discover(sys: &FakeSystem) -> DiscoveryReport {
    LinuxSysfsDiscovery::with_paths(LinuxPaths::under_root(sys.root()))
        .discover()
        .expect("discovery succeeds")
}

fn device<'a>(report: &'a DiscoveryReport, name: &str) -> &'a Device {
    report
        .devices
        .iter()
        .find(|d| d.key.as_str() == format!("linux:{name}"))
        .unwrap_or_else(|| panic!("device {name} not found"))
}

fn usb_stick(sys: &FakeSystem, serial: &str) {
    sys.add_usb_device(&FakeUsbDevice {
        dir: USB_PORT.into(),
        id_vendor: "f055".into(),
        id_product: "0032".into(),
        manufacturer: Some("Acme".into()),
        product: Some("FlashLine 32".into()),
        serial: Some(serial.into()),
        speed: Some("5000".into()),
    })
    .unwrap();
    sys.add_block_device(&FakeBlockDevice {
        name: "sdb".into(),
        devpath: format!("{USB_SCSI}/block/sdb"),
        device_dir: Some(USB_SCSI.into()),
        dev: "8:16".into(),
        size_sectors: 62_531_584,
        removable: true,
        device_attributes: vec![
            ("vendor".into(), "Acme    ".into()),
            ("model".into(), "FlashLine 32    ".into()),
            ("rev".into(), "1.00".into()),
        ],
        partitions: vec![
            FakePartition {
                name: "sdb2".into(),
                number: 2,
                start_sectors: 4096,
                size_sectors: 1024,
                dev: "8:18".into(),
                holders: vec![],
            },
            FakePartition {
                name: "sdb1".into(),
                number: 1,
                start_sectors: 2048,
                size_sectors: 2048,
                dev: "8:17".into(),
                holders: vec![],
            },
        ],
        ..FakeBlockDevice::default()
    })
    .unwrap();
    sys.add_by_id_link("usb-Acme_FlashLine_32_A1B2C3D4-0:0", "sdb")
        .unwrap();
    sys.add_by_id_link("usb-Acme_FlashLine_32_A1B2C3D4-0:0-part1", "sdb1")
        .unwrap();
}

/// NVMe system disk with LUKS (dm-0) and LVM (dm-1, dm-2) on partition 2.
fn nvme_system_disk(sys: &FakeSystem) {
    sys.add_block_device(&FakeBlockDevice {
        name: "nvme0n1".into(),
        devpath: format!("{NVME_CTRL}/nvme0n1"),
        device_dir: Some(NVME_CTRL.into()),
        dev: "259:0".into(),
        size_sectors: 1_000_215_216,
        device_attributes: vec![
            ("model".into(), "Lumen NVMe 512".into()),
            ("serial".into(), "LMN0000000042".into()),
            ("firmware_rev".into(), "1B2QEXM7".into()),
        ],
        own_attributes: vec![("wwid".into(), "eui.0025388b91c0a1b2".into())],
        partitions: vec![
            FakePartition {
                name: "nvme0n1p1".into(),
                number: 1,
                start_sectors: 2048,
                size_sectors: 1_048_576,
                dev: "259:1".into(),
                holders: vec![],
            },
            FakePartition {
                name: "nvme0n1p2".into(),
                number: 2,
                start_sectors: 1_050_624,
                size_sectors: 999_000_000,
                dev: "259:2".into(),
                holders: vec!["dm-0".into()],
            },
        ],
        ..FakeBlockDevice::default()
    })
    .unwrap();
    for (name, dev, holders, slaves) in [
        ("dm-0", "253:0", vec!["dm-1", "dm-2"], vec!["nvme0n1p2"]),
        ("dm-1", "253:1", vec![], vec!["dm-0"]),
        ("dm-2", "253:2", vec![], vec!["dm-0"]),
    ] {
        sys.add_block_device(&FakeBlockDevice {
            name: name.into(),
            devpath: format!("virtual/block/{name}"),
            device_dir: None,
            dev: dev.into(),
            size_sectors: 1000,
            holders: holders.into_iter().map(String::from).collect(),
            slaves: slaves.into_iter().map(String::from).collect(),
            ..FakeBlockDevice::default()
        })
        .unwrap();
    }
    sys.write("dev/dm-2", "").unwrap();
    sys.set_swaps(&["/dev/dm-2"]).unwrap();
}

fn mount(dev: &str, mount_point: &str, fstype: &str) -> FakeMount {
    FakeMount {
        dev: dev.into(),
        mount_point: mount_point.into(),
        fstype: fstype.into(),
        source: "/dev/x".into(),
    }
}

#[test]
fn usb_stick_is_discovered_with_provenance_and_partitions() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    sys.set_mountinfo(&[mount("8:17", "/media/user/MY STICK", "vfat")])
        .unwrap();

    let report = discover(&sys);
    let stick = device(&report, "sdb");

    assert!(!stick.simulated);
    assert_eq!(stick.transport, Transport::Usb);
    assert_eq!(stick.capacity_bytes(), Some(62_531_584 * 512));
    assert_eq!(stick.identity.text(AttributeKind::Vendor), Some("Acme"));
    assert_eq!(
        stick.identity.text(AttributeKind::Model),
        Some("FlashLine 32")
    );
    assert_eq!(
        stick.identity.text(AttributeKind::UsbVendorId),
        Some("f055")
    );
    assert_eq!(
        stick.identity.text(AttributeKind::UsbProductId),
        Some("0032")
    );
    assert_eq!(
        stick.identity.text(AttributeKind::UsbSerialNumber),
        Some("A1B2C3D4")
    );
    assert_eq!(
        stick.identity.text(AttributeKind::OsDeviceName),
        Some("sdb")
    );
    assert_eq!(
        stick.identity.text(AttributeKind::OsDeviceNumber),
        Some("8:16")
    );
    assert_eq!(
        stick.identity.text(AttributeKind::OsPersistentLink),
        Some("usb-Acme_FlashLine_32_A1B2C3D4-0:0")
    );
    let capacity = stick.identity.first(AttributeKind::CapacityBytes).unwrap();
    assert_eq!(capacity.confidence, Confidence::High);
    assert_eq!(capacity.provenance.source, ObservationSource::LinuxSysfs);
    let serial = stick
        .identity
        .first(AttributeKind::UsbSerialNumber)
        .unwrap();
    assert_eq!(serial.confidence, Confidence::Medium);
    assert_eq!(serial.provenance.locator.as_deref(), Some("usb/serial"));
    assert!(stick.identity.has_strong_identifier());

    assert_eq!(stick.state.media, MediaState::Present);
    assert_eq!(stick.state.write_protection, WriteProtection::Writable);
    assert_eq!(stick.state.removable, Some(true));

    // Partitions are sorted by number and carry their usage.
    let numbers: Vec<_> = stick.partitions.iter().map(|p| p.number).collect();
    assert_eq!(numbers, vec![Some(1), Some(2)]);
    assert_eq!(stick.partitions[0].start_bytes, Some(2048 * 512));
    assert_eq!(stick.partitions[0].size_bytes, Some(2048 * 512));
    assert_eq!(
        stick.partitions[0].usage,
        vec![VolumeUsage::Mounted {
            mount_point: "/media/user/MY STICK".into(),
            filesystem: Some("vfat".into()),
            via: None,
        }]
    );
    assert!(stick.partitions[1].usage.is_empty());
}

/// Regression test: attribute files in the block directory (`size`, `ro`,
/// `dev`, ...) were probed as partition directories, producing one spurious
/// `ProbeFailed { Partitions, Io }` issue per file on real systems.
#[test]
fn well_formed_device_produces_no_device_issues() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    sys.write(
        format!("sys/devices/{USB_SCSI}/block/sdb/alignment_offset"),
        "0\n",
    )
    .unwrap();
    sys.symlink(
        format!("sys/devices/{USB_SCSI}/block/sdb/bdi"),
        sys.sys().join("devices/virtual/bdi/8:16"),
    )
    .unwrap();
    sys.set_mountinfo(&[mount("8:17", "/media/user/STICK", "vfat")])
        .unwrap();
    let report = discover(&sys);
    let device_issues: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.device.is_some())
        .collect();
    assert!(
        device_issues.is_empty(),
        "unexpected issues: {device_issues:?}"
    );
    assert_eq!(device(&report, "sdb").partitions.len(), 2);
}

#[test]
fn phase0_reports_every_capability_as_unavailable_not_implemented() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    let report = discover(&sys);
    for (capability, status) in device(&report, "sdb").capabilities.iter() {
        assert_eq!(
            status,
            CapabilityStatus::Unavailable(CapabilityReason::NotImplementedInThisBuild),
            "{capability:?}"
        );
    }
}

#[test]
fn system_disk_is_traced_through_luks_and_lvm_holders() {
    let sys = FakeSystem::new().unwrap();
    nvme_system_disk(&sys);
    usb_stick(&sys, "A1B2C3D4");
    sys.set_mountinfo(&[
        mount("253:1", "/", "ext4"),
        mount("259:1", "/boot/efi", "vfat"),
    ])
    .unwrap();

    let report = discover(&sys);
    let nvme = device(&report, "nvme0n1");
    assert_eq!(nvme.transport, Transport::Nvme);
    assert_eq!(nvme.state.system_disk, SystemDiskStatus::SystemDisk);
    assert_eq!(
        nvme.partitions[1].usage,
        vec![
            VolumeUsage::Mounted {
                mount_point: "/".into(),
                filesystem: Some("ext4".into()),
                via: Some("dm-1".into()),
            },
            VolumeUsage::Swap {
                via: Some("dm-2".into())
            },
        ]
    );
    assert_eq!(
        nvme.identity
            .first(AttributeKind::WorldWideName)
            .map(|a| a.confidence),
        Some(Confidence::High)
    );
    assert_eq!(
        nvme.identity.text(AttributeKind::Revision),
        Some("1B2QEXM7")
    );
    assert_eq!(
        nvme.identity.text(AttributeKind::SerialNumber),
        Some("LMN0000000042")
    );

    let stick = device(&report, "sdb");
    assert_eq!(stick.state.system_disk, SystemDiskStatus::NotSystemDisk);

    // The three device-mapper nodes are virtual and excluded, but reported.
    assert_eq!(report.devices.len(), 2);
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == DiscoveryIssueKind::VirtualDevicesExcluded { count: 3 })
    );
    assert!(
        !report
            .issues
            .iter()
            .any(|i| i.kind == DiscoveryIssueKind::SystemDiskUndetermined)
    );
}

#[test]
fn unresolvable_root_marks_every_device_unknown_never_safe() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    // Container-style overlay root on an anonymous device number.
    sys.set_mountinfo(&[mount("0:30", "/", "overlay")]).unwrap();

    let report = discover(&sys);
    assert_eq!(
        device(&report, "sdb").state.system_disk,
        SystemDiskStatus::Unknown
    );
    assert!(
        report
            .issues
            .iter()
            .any(|i| i.kind == DiscoveryIssueKind::SystemDiskUndetermined)
    );
}

#[test]
fn unmounted_holder_is_reported_as_held() {
    let sys = FakeSystem::new().unwrap();
    nvme_system_disk(&sys);
    sys.set_swaps(&[]).unwrap();
    let report = discover(&sys);
    let nvme = device(&report, "nvme0n1");
    assert_eq!(
        nvme.partitions[1].usage,
        vec![
            VolumeUsage::Held {
                holder: "dm-1".into()
            },
            VolumeUsage::Held {
                holder: "dm-2".into()
            },
        ]
    );
    assert!(nvme.is_in_use());
}

#[test]
fn empty_card_reader_has_no_media() {
    let sys = FakeSystem::new().unwrap();
    sys.add_block_device(&FakeBlockDevice {
        name: "sdc".into(),
        devpath: format!("{USB_SCSI}/block/sdc"),
        device_dir: Some(USB_SCSI.into()),
        dev: "8:32".into(),
        size_sectors: 0,
        removable: true,
        ..FakeBlockDevice::default()
    })
    .unwrap();
    let report = discover(&sys);
    let reader = device(&report, "sdc");
    assert_eq!(reader.state.media, MediaState::NoMedia);
    assert_eq!(reader.capacity_bytes(), Some(0));
    assert_eq!(
        reader.capabilities.get(Capability::RawRead),
        CapabilityStatus::Unavailable(CapabilityReason::NoMedia)
    );
    assert_eq!(
        reader.capabilities.get(Capability::SafeEject),
        CapabilityStatus::Unavailable(CapabilityReason::NotImplementedInThisBuild)
    );
}

#[test]
fn write_protected_device_reports_write_protection_reason() {
    let sys = FakeSystem::new().unwrap();
    sys.add_block_device(&FakeBlockDevice {
        name: "mmcblk0".into(),
        devpath: "platform/soc/mmc_host/mmc0/mmc0:0001/block/mmcblk0".into(),
        device_dir: Some("platform/soc/mmc_host/mmc0/mmc0:0001".into()),
        dev: "179:0".into(),
        size_sectors: 124_735_488,
        removable: false,
        read_only: true,
        device_attributes: vec![
            ("name".into(), "SD64G".into()),
            ("serial".into(), "0x1a2b3c4d".into()),
            ("type".into(), "SD".into()),
        ],
        ..FakeBlockDevice::default()
    })
    .unwrap();
    let report = discover(&sys);
    let card = device(&report, "mmcblk0");
    assert_eq!(card.transport, Transport::Mmc);
    assert_eq!(card.identity.text(AttributeKind::Model), Some("SD64G"));
    assert_eq!(card.state.write_protection, WriteProtection::ReadOnly);
    for capability in Capability::ALL {
        let expected = if capability.is_destructive() {
            CapabilityReason::WriteProtected
        } else {
            CapabilityReason::NotImplementedInThisBuild
        };
        assert_eq!(
            card.capabilities.get(capability),
            CapabilityStatus::Unavailable(expected),
            "{capability:?}"
        );
    }
}

#[test]
fn placeholder_serial_gets_low_confidence() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "000000000000");
    let report = discover(&sys);
    let stick = device(&report, "sdb");
    let serial = stick
        .identity
        .first(AttributeKind::UsbSerialNumber)
        .unwrap();
    assert_eq!(serial.confidence, Confidence::Low);
    assert!(!stick.identity.has_strong_identifier());
}

#[test]
fn virtio_serial_on_block_directory_is_read() {
    let sys = FakeSystem::new().unwrap();
    sys.add_block_device(&FakeBlockDevice {
        name: "vda".into(),
        devpath: "pci0000:00/0000:00:02.0/virtio1/block/vda".into(),
        device_dir: Some("pci0000:00/0000:00:02.0/virtio1".into()),
        dev: "254:0".into(),
        size_sectors: 2048,
        own_attributes: vec![("serial".into(), "rootfs-1".into())],
        ..FakeBlockDevice::default()
    })
    .unwrap();
    sys.set_mountinfo(&[mount("254:0", "/", "ext4")]).unwrap();
    let report = discover(&sys);
    let vda = device(&report, "vda");
    assert_eq!(vda.transport, Transport::Virtio);
    assert_eq!(
        vda.identity.text(AttributeKind::SerialNumber),
        Some("rootfs-1")
    );
    assert_eq!(vda.state.system_disk, SystemDiskStatus::SystemDisk);
    // Whole-disk filesystem (no partition table).
    assert!(vda.partitions.is_empty());
    assert!(matches!(
        vda.whole_disk_usage.as_slice(),
        [VolumeUsage::Mounted { mount_point, .. }] if mount_point == "/"
    ));
}

#[test]
fn missing_tables_are_reported_and_devices_still_listed() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    std::fs::remove_file(sys.mountinfo()).unwrap();
    std::fs::remove_file(sys.swaps()).unwrap();
    let report = discover(&sys);
    assert_eq!(report.devices.len(), 1);
    for target in [ProbeTarget::MountTable, ProbeTarget::SwapTable] {
        assert!(report.issues.iter().any(|i| i.kind
            == DiscoveryIssueKind::ProbeFailed {
                target,
                failure: ProbeFailure::Vanished
            }));
    }
}

#[test]
fn malformed_size_keeps_device_with_unknown_capacity() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    sys.write(
        format!("sys/devices/{USB_SCSI}/block/sdb/size"),
        "garbage\n",
    )
    .unwrap();
    let report = discover(&sys);
    let stick = device(&report, "sdb");
    assert_eq!(stick.capacity_bytes(), None);
    assert_eq!(stick.state.media, MediaState::Unknown);
    assert!(report.issues.iter().any(|i| i.kind
        == DiscoveryIssueKind::ProbeFailed {
            target: ProbeTarget::Attribute(AttributeKind::CapacityBytes),
            failure: ProbeFailure::Malformed
        }));
}

#[test]
fn vanished_device_is_skipped_with_an_issue() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    std::fs::remove_file(
        sys.root()
            .join(format!("sys/devices/{USB_SCSI}/block/sdb/size")),
    )
    .unwrap();
    let report = discover(&sys);
    assert!(report.devices.is_empty());
    assert!(report.issues.iter().any(|i| i.kind
        == DiscoveryIssueKind::ProbeFailed {
            target: ProbeTarget::Attribute(AttributeKind::CapacityBytes),
            failure: ProbeFailure::Vanished
        }));
}

#[test]
fn missing_sys_block_is_a_discovery_error() {
    let sys = FakeSystem::new().unwrap();
    std::fs::remove_dir_all(sys.sys()).unwrap();
    let result = LinuxSysfsDiscovery::with_paths(LinuxPaths::under_root(sys.root())).discover();
    assert!(matches!(result, Err(DiscoveryError::Io { .. })));
}

#[test]
fn devices_are_reported_in_stable_order() {
    let sys = FakeSystem::new().unwrap();
    usb_stick(&sys, "A1B2C3D4");
    nvme_system_disk(&sys);
    let names: Vec<_> = discover(&sys)
        .devices
        .iter()
        .map(|d| d.key.as_str().to_owned())
        .collect();
    assert_eq!(names, vec!["linux:nvme0n1", "linux:sdb"]);
}

#[test]
fn transport_classification() {
    let t = |p: &str| classify_transport(Path::new(p));
    assert_eq!(
        t(&format!("/sys/devices/{USB_SCSI}/block/sdb")),
        Transport::Usb
    );
    assert_eq!(
        t(&format!("/sys/devices/{NVME_CTRL}/nvme0n1")),
        Transport::Nvme
    );
    assert_eq!(
        t("/sys/devices/pci0000:00/0000:00:17.0/ata1/host0/target0:0:0/0:0:0:0/block/sda"),
        Transport::Ata
    );
    assert_eq!(
        t("/sys/devices/pci0000:00/0000:00:10.0/host2/target2:0:0/2:0:0:0/block/sdd"),
        Transport::Scsi
    );
    assert_eq!(
        t("/sys/devices/platform/soc/mmc_host/mmc0/mmc0:0001/block/mmcblk0"),
        Transport::Mmc
    );
    assert_eq!(
        t("/sys/devices/pci0000:00/0000:00:02.0/virtio1/block/vda"),
        Transport::Virtio
    );
    assert_eq!(
        t("/sys/devices/platform/something/block/xyz"),
        Transport::Unknown
    );
}

/// Read-only smoke test against the real `/sys` of the machine running the
/// tests. It asserts invariants only; it never opens a device node.
#[test]
fn real_system_discovery_is_read_only_and_well_formed() {
    if !Path::new("/sys/block").is_dir() {
        return;
    }
    let report = LinuxSysfsDiscovery::new()
        .discover()
        .expect("real /sys is listable");
    for device in &report.devices {
        assert!(!device.simulated);
        assert!(device.key.as_str().starts_with("linux:"));
        assert!(device.identity.text(AttributeKind::OsDeviceName).is_some());
        for (_, status) in device.capabilities.iter() {
            assert!(matches!(status, CapabilityStatus::Unavailable(_)));
        }
    }
}
