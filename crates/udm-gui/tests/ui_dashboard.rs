//! Headless UI tests for the Phase 0 dashboard.
//!
//! They use Slint's testing backend (no window system, no rendering) and the
//! same presenter code as the application, fed with simulated snapshots.

// Test-only code: failing loudly is the desired behaviour.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use i_slint_backend_testing::{ElementHandle, ElementQuery};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Model, SharedString};
use udm_app::{DiscoveryEvent, DiscoveryFailure, DiscoveryOutcome, DiscoverySnapshot};
use udm_domain::{
    AttributeKind, AttributeValue, Confidence, DiscoverySource, PlatformKind, Provenance,
    assess_identities,
};
use udm_gui::presenter::Presenter;
use udm_gui::{MainWindow, ScreenState, Theme};
use udm_platform::DiscoveryError;
use udm_platform::simulated::sample_devices;

fn window() -> MainWindow {
    i_slint_backend_testing::init_no_event_loop();
    let ui = MainWindow::new().expect("window can be created with the testing backend");
    // Tall enough that no list or detail content is clipped by scroll views.
    ui.window().set_size(slint::PhysicalSize::new(1400, 3000));
    ui
}

fn sample_snapshot(generation: u64, epoch: u64) -> DiscoverySnapshot {
    let mut devices = sample_devices();
    let issues = assess_identities(&mut devices);
    DiscoverySnapshot {
        generation,
        epoch,
        source: DiscoverySource::Simulated {
            scenario: "sample-devices".into(),
        },
        devices,
        issues,
        duration: Duration::from_millis(3),
    }
}

fn labels(ui: &MainWindow, label: &str) -> usize {
    ElementHandle::find_by_accessible_label(ui, label).count()
}

/// Accessible labels of every visible element (Text elements expose their
/// text as accessible label).
fn texts(ui: &MainWindow) -> Vec<SharedString> {
    ElementQuery::from_root(ui)
        .match_descendants()
        .find_all()
        .into_iter()
        .filter_map(|element| element.accessible_label())
        .collect()
}

#[test]
fn simulated_devices_are_listed_and_labelled() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.show_snapshot(&ui, sample_snapshot(1, 0));

    assert_eq!(ui.get_state(), ScreenState::Ready);
    assert_eq!(ui.get_devices().row_count(), 7);
    assert!(ui.get_source_simulated());
    // Every card carries a "Simulated" badge.
    assert!(labels(&ui, "Simulated") >= 7);
    // The notice makes the read-only nature explicit.
    let all = texts(&ui);
    assert!(
        all.iter()
            .any(|t| t.contains("This build cannot write, format, partition or erase any device.")),
        "notice missing"
    );
    // Global issues (duplicate serials) are shown.
    assert!(ui.get_global_issues().row_count() >= 1);
    // Nothing is selected until the person selects something.
    assert_eq!(ui.get_selected_index(), -1);
    assert!(all.iter().any(|t| t == "No device selected"));
}

#[test]
fn device_detail_shows_all_four_capability_states() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.show_snapshot(&ui, sample_snapshot(1, 0));
    let stick = (0..ui.get_devices().row_count())
        .find(|&i| ui.get_devices().row_data(i).unwrap().key == "sim:usb-stick-a")
        .unwrap();
    ui.set_selected_index(i32::try_from(stick).unwrap());

    for state in ["Supported", "Limited", "Unavailable", "Not tested"] {
        assert!(
            labels(&ui, state) >= 1,
            "capability state {state} not shown"
        );
    }
    let all = texts(&ui);
    assert!(all.iter().any(|t| t == "Identity evidence"));
    assert!(all.iter().any(|t| t.starts_with("Simulated device.")));
}

#[test]
fn failure_state_is_explicit_and_not_an_empty_list() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.handle_event(
        &ui,
        DiscoveryEvent::Finished(DiscoveryOutcome::Failed {
            generation: 1,
            epoch: 0,
            source: DiscoverySource::System {
                platform: PlatformKind::Windows,
            },
            error: DiscoveryFailure::Adapter(DiscoveryError::NotImplemented {
                platform: PlatformKind::Windows,
            }),
        }),
    );
    assert_eq!(ui.get_state(), ScreenState::Failed);
    let all = texts(&ui);
    assert!(all.iter().any(|t| t == "Devices could not be listed"));
    assert!(all.iter().any(|t| t.starts_with(
        "Device discovery is not implemented for this operating system in this build."
    )));
    assert!(!all.iter().any(|t| t == "No storage devices found"));
    assert_eq!(labels(&ui, "Try again"), 1);
}

#[test]
fn empty_and_loading_states_are_distinct() {
    let ui = window();
    let mut presenter = Presenter::new();
    // Initial state: loading with no data.
    assert_eq!(ui.get_state(), ScreenState::Loading);
    assert!(texts(&ui).iter().any(|t| t == "Discovering devices…"));

    let mut empty = sample_snapshot(1, 0);
    empty.devices.clear();
    empty.issues.clear();
    presenter.show_snapshot(&ui, empty);
    let all = texts(&ui);
    assert!(all.iter().any(|t| t == "No storage devices found"));
    assert!(!all.iter().any(|t| t == "Discovering devices…"));
}

#[test]
fn results_from_a_previous_source_are_ignored() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.source_changed(&ui, 2, true);
    presenter.handle_event(
        &ui,
        DiscoveryEvent::Finished(DiscoveryOutcome::Completed(sample_snapshot(1, 1))),
    );
    assert_eq!(ui.get_devices().row_count(), 0);
    assert_eq!(ui.get_state(), ScreenState::Loading);
    presenter.handle_event(
        &ui,
        DiscoveryEvent::Finished(DiscoveryOutcome::Completed(sample_snapshot(2, 2))),
    );
    assert_eq!(ui.get_devices().row_count(), 7);
}

#[test]
fn selection_never_moves_to_a_different_device_under_the_same_key() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.show_snapshot(&ui, sample_snapshot(1, 0));
    let nvme = (0..ui.get_devices().row_count())
        .find(|&i| ui.get_devices().row_data(i).unwrap().key == "sim:nvme-enclosure")
        .unwrap();
    ui.set_selected_index(i32::try_from(nvme).unwrap());

    // Same snapshot again: selection is kept.
    presenter.show_snapshot(&ui, sample_snapshot(2, 0));
    assert_eq!(ui.get_selected_index(), i32::try_from(nvme).unwrap());

    // Same key, different serial number: a different physical device.
    let mut swapped = sample_snapshot(3, 0);
    let device = swapped
        .devices
        .iter_mut()
        .find(|d| d.key.as_str() == "sim:nvme-enclosure")
        .unwrap();
    let mut identity = udm_domain::DeviceIdentity::new();
    for attribute in device.identity.attributes() {
        if attribute.kind != AttributeKind::UsbSerialNumber {
            identity.push(attribute.clone());
        }
    }
    device.identity = identity.with(
        AttributeKind::UsbSerialNumber,
        AttributeValue::Text("OTHER-DEVICE-1".into()),
        Provenance::simulated(),
        Confidence::Medium,
    );
    presenter.show_snapshot(&ui, swapped);
    assert_eq!(ui.get_selected_index(), -1);
}

#[test]
fn keyboard_navigation_moves_the_selection() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.show_snapshot(&ui, sample_snapshot(1, 0));

    // Activate the first card through its accessibility action (as a screen
    // reader would), which also focuses the list.
    let first = ElementHandle::find_by_accessible_label(&ui, "ATA Lumen SSD 512, 512.1 GB")
        .next()
        .expect("first card is exposed to accessibility");
    first.invoke_accessible_default_action();
    assert_eq!(ui.get_selected_index(), 0);

    let press = |key: Key| {
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
    };
    press(Key::DownArrow);
    press(Key::DownArrow);
    assert_eq!(ui.get_selected_index(), 2);
    press(Key::End);
    assert_eq!(ui.get_selected_index(), 6);
    press(Key::DownArrow);
    assert_eq!(ui.get_selected_index(), 6, "selection stays within bounds");
    press(Key::Home);
    assert_eq!(ui.get_selected_index(), 0);
    press(Key::UpArrow);
    assert_eq!(ui.get_selected_index(), 0);
}

#[test]
fn no_destructive_action_is_offered() {
    let ui = window();
    let mut presenter = Presenter::new();
    presenter.show_snapshot(&ui, sample_snapshot(1, 0));
    ui.set_selected_index(1);
    let buttons: Vec<SharedString> = ElementHandle::find_by_element_type_name(&ui, "Button")
        .filter_map(|b| b.accessible_label())
        .collect();
    for button in &buttons {
        let lower = button.to_lowercase();
        for word in [
            "erase",
            "format",
            "write",
            "wipe",
            "partition",
            "clone",
            "restore",
        ] {
            assert!(
                !lower.contains(word),
                "unexpected destructive button: {button}"
            );
        }
    }
    assert!(buttons.iter().any(|b| b == "Refresh"));
}

#[test]
fn theme_selection_switches_tokens() {
    let ui = window();
    ui.invoke_apply_theme(2);
    assert!(ui.global::<Theme>().get_dark());
    let dark_canvas = ui.global::<Theme>().get_canvas();
    ui.invoke_apply_theme(1);
    assert!(!ui.global::<Theme>().get_dark());
    assert_ne!(ui.global::<Theme>().get_canvas(), dark_canvas);
    assert_eq!(ui.get_theme_index(), 1);
}

#[test]
fn korean_translation_is_bundled() {
    let ui = window();
    slint::select_bundled_translation("ko").expect("Korean translation is bundled");
    let all = texts(&ui);
    assert!(
        all.iter().any(|t| t == "데이터 소스"),
        "Korean label missing"
    );
    assert!(
        all.iter().any(|t| t == "장치를 검색하는 중…"),
        "Korean loading state missing"
    );
    assert!(!all.iter().any(|t| t == "Data source"));
    slint::select_bundled_translation("en").expect("English is always available");
    assert!(texts(&ui).iter().any(|t| t == "Data source"));
}
