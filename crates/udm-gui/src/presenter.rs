// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Applies discovery events to the main window.
//!
//! The presenter runs on the GUI thread only. It receives events that the
//! background worker produced and the binary forwarded with
//! `slint::Weak::upgrade_in_event_loop`.

use std::rc::Rc;

use slint::{ModelRc, VecModel};
use udm_app::{DiscoveryEvent, DiscoveryOutcome, DiscoverySnapshot};
use udm_domain::{Device, IdentityComparison};

use crate::view_model::{device_item, failure, issue_row};
use crate::{DeviceItem, FailureCode, IssueRow, MainWindow, ScreenState};

/// GUI-thread state that mirrors what the window currently shows.
#[derive(Debug, Default)]
pub struct Presenter {
    expected_epoch: u64,
    /// Domain devices in the same order as the window's `devices` model.
    devices: Vec<Device>,
}

impl Presenter {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Epoch whose results are currently accepted.
    #[must_use]
    pub const fn expected_epoch(&self) -> u64 {
        self.expected_epoch
    }

    /// Called when the user picked another data source. Results of the
    /// previous source are discarded from now on, and nothing of the previous
    /// source stays on screen.
    pub fn source_changed(&mut self, ui: &MainWindow, epoch: u64, simulated: bool) {
        self.expected_epoch = epoch;
        self.devices.clear();
        ui.set_devices(ModelRc::from(Rc::new(VecModel::<DeviceItem>::default())));
        ui.set_global_issues(ModelRc::from(Rc::new(VecModel::<IssueRow>::default())));
        ui.set_selected_index(-1);
        ui.set_source_simulated(simulated);
        ui.set_refresh_summary("".into());
        ui.set_state(ScreenState::Loading);
    }

    /// Shows that a pass is running. Existing data stays visible.
    pub fn show_loading(ui: &MainWindow) {
        ui.set_state(ScreenState::Loading);
    }

    /// Shows a failure that happened before the worker could run (e.g. the
    /// worker stopped).
    pub fn show_worker_stopped(&mut self, ui: &MainWindow) {
        ui.set_failure(FailureCode::WorkerStopped);
        ui.set_failure_detail("".into());
        ui.set_state(ScreenState::Failed);
    }

    /// Handles one event from the discovery worker. Events from a previous
    /// source (older epoch) are ignored.
    pub fn handle_event(&mut self, ui: &MainWindow, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::Started { epoch, .. } => {
                if epoch == self.expected_epoch {
                    Self::show_loading(ui);
                }
            }
            DiscoveryEvent::Finished(outcome) => {
                if outcome.epoch() != self.expected_epoch {
                    tracing::debug!(
                        epoch = outcome.epoch(),
                        expected = self.expected_epoch,
                        "ignoring result of a previous data source"
                    );
                    return;
                }
                match outcome {
                    DiscoveryOutcome::Completed(snapshot) => self.show_snapshot(ui, snapshot),
                    DiscoveryOutcome::Failed { error, .. } => {
                        let (code, detail) = failure(&error);
                        self.devices.clear();
                        ui.set_devices(ModelRc::from(Rc::new(VecModel::<DeviceItem>::default())));
                        ui.set_selected_index(-1);
                        ui.set_failure(code);
                        ui.set_failure_detail(detail.into());
                        ui.set_state(ScreenState::Failed);
                    }
                }
            }
        }
    }

    /// Shows a completed snapshot. The selection follows the previously
    /// selected device only if the new observation under the same key is not
    /// an identity mismatch; it never jumps to another device.
    pub fn show_snapshot(&mut self, ui: &MainWindow, snapshot: DiscoverySnapshot) {
        let previous = usize::try_from(ui.get_selected_index())
            .ok()
            .and_then(|i| self.devices.get(i));
        let selected = previous.and_then(|prev| {
            snapshot.devices.iter().position(|d| {
                d.key == prev.key
                    && !matches!(
                        prev.identity.compare(&d.identity),
                        IdentityComparison::Mismatch { .. }
                    )
            })
        });

        let items: Vec<DeviceItem> = snapshot
            .devices
            .iter()
            .map(|d| device_item(d, &snapshot.issues))
            .collect();
        let global: Vec<IssueRow> = snapshot
            .issues
            .iter()
            .filter(|i| i.device.is_none())
            .map(issue_row)
            .collect();

        ui.set_devices(ModelRc::from(Rc::new(VecModel::from(items))));
        ui.set_global_issues(ModelRc::from(Rc::new(VecModel::from(global))));
        ui.set_selected_index(selected.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
        ui.set_source_simulated(snapshot.source.is_simulated());
        ui.set_refresh_summary(
            format!(
                "#{} · {} ms",
                snapshot.generation,
                snapshot.duration.as_millis()
            )
            .into(),
        );
        ui.set_state(ScreenState::Ready);
        self.devices = snapshot.devices;
    }
}
