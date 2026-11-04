//! Prints the result of one read-only discovery pass.
//!
//! Usage:
//!   cargo run -p udm-platform --example discover            # this system (read-only)
//!   cargo run -p udm-platform --example discover -- --simulated
//!   cargo run -p udm-platform --example discover -- --show-sensitive
//!
//! Serial numbers, WWNs and persistent links are redacted unless
//! `--show-sensitive` is given, so the output can be pasted into reports.
//! This tool never opens a device node and never writes anything.

use std::process::ExitCode;

use udm_domain::{VolumeUsage, assess_identities};
use udm_platform::simulated::{Scenario, SimulatedDiscovery};
use udm_platform::{DeviceDiscovery, system_discovery};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_sensitive = args.iter().any(|a| a == "--show-sensitive");
    let mut adapter: Box<dyn DeviceDiscovery> = if args.iter().any(|a| a == "--simulated") {
        Box::new(SimulatedDiscovery::new(Scenario::SampleDevices))
    } else {
        system_discovery()
    };

    println!("source: {:?}", adapter.source());
    let mut report = match adapter.discover() {
        Ok(report) => report,
        Err(e) => {
            eprintln!("discovery failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut issues = assess_identities(&mut report.devices);
    issues.extend(report.issues);

    println!("devices: {}", report.devices.len());
    for device in &report.devices {
        println!(
            "\n[{}] transport={} capacity={} media={:?} write={:?} removable={:?} system_disk={:?}",
            device.key,
            device.transport.as_str(),
            device
                .capacity_bytes()
                .map_or_else(|| "unknown".to_owned(), |c| format!("{c} B")),
            device.state.media,
            device.state.write_protection,
            device.state.removable,
            device.state.system_disk,
        );
        for attribute in device.identity.attributes() {
            let value = if attribute.kind.is_sensitive() && !show_sensitive {
                "<redacted>".to_owned()
            } else {
                attribute.value.to_string()
            };
            println!(
                "  {:<20} {:<40} source={}:{} confidence={:?}",
                attribute.kind.as_str(),
                value,
                attribute.provenance.source.as_str(),
                attribute.provenance.locator.as_deref().unwrap_or("-"),
                attribute.confidence,
            );
        }
        for partition in &device.partitions {
            println!(
                "  partition {:?} {:?} start={:?} size={:?} usage={}",
                partition.number,
                partition.os_name,
                partition.start_bytes,
                partition.size_bytes,
                describe(&partition.usage)
            );
        }
        if !device.whole_disk_usage.is_empty() {
            println!("  whole-disk usage: {}", describe(&device.whole_disk_usage));
        }
        for (capability, status) in device.capabilities.iter() {
            println!(
                "  capability {:<18} {:<12} {}",
                capability.as_str(),
                status.as_str(),
                status.reason().map_or("", |r| r.as_str())
            );
        }
    }
    if !issues.is_empty() {
        println!("\nissues:");
        for issue in &issues {
            println!(
                "  [{:?}] {} {:?}",
                issue.kind.severity(),
                issue.device.as_ref().map_or("-", |k| k.as_str()),
                issue.kind
            );
        }
    }
    ExitCode::SUCCESS
}

fn describe(usage: &[VolumeUsage]) -> String {
    if usage.is_empty() {
        return "unused".to_owned();
    }
    usage
        .iter()
        .map(|u| match u {
            VolumeUsage::Mounted {
                mount_point,
                filesystem,
                via,
            } => format!(
                "mounted {mount_point} ({}){}",
                filesystem.as_deref().unwrap_or("?"),
                via.as_ref().map_or(String::new(), |v| format!(" via {v}"))
            ),
            VolumeUsage::Swap { via } => format!(
                "swap{}",
                via.as_ref().map_or(String::new(), |v| format!(" via {v}"))
            ),
            VolumeUsage::Held { holder } => format!("held by {holder}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
