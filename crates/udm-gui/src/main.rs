//! Universal Drive Manager desktop application (Phase 0, read-only).

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use slint::ComponentHandle;
use tracing_subscriber::EnvFilter;
use udm_app::{DiscoveryEvent, DiscoveryService, RefreshRequest, ServiceError};
use udm_gui::MainWindow;
use udm_gui::presenter::Presenter;
use udm_platform::DeviceDiscovery;
use udm_platform::simulated::{Scenario, SimulatedDiscovery};

/// Artificial latency for simulated sources, so the loading state is visible
/// and the asynchronous path is exercised even in the simulator.
const SIMULATED_LATENCY: Duration = Duration::from_millis(250);

/// Index of "This computer (read-only)" in the data-source selector.
const SYSTEM_SOURCE_INDEX: i32 = 4;

thread_local! {
    /// GUI-thread state. Only touched from the Slint event loop thread.
    static PRESENTER: RefCell<Presenter> = RefCell::new(Presenter::new());
}

fn main() -> ExitCode {
    init_logging();

    if running_with_elevated_privileges() {
        eprintln!(
            "Universal Drive Manager must not run as root/administrator.\n\
             Start it as a regular user. Operations that need privileges will use a \
             separate, narrowly scoped helper process (not part of this build)."
        );
        return ExitCode::from(2);
    }

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "application failed");
            eprintln!("Universal Drive Manager could not start: {error}");
            ExitCode::FAILURE
        }
    }
}

fn init_logging() {
    let filter = EnvFilter::try_from_env("UDM_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("user interface: {0}")]
    Ui(#[source] slint::PlatformError),
    #[error("discovery worker: {0}")]
    Worker(#[source] std::io::Error),
}

fn adapter_for(source_index: i32) -> Box<dyn DeviceDiscovery> {
    let scenario = match source_index {
        0 => Scenario::SampleDevices,
        1 => Scenario::NoDevices,
        2 => Scenario::DiscoveryFailure,
        3 => Scenario::Hotplug,
        _ => return udm_platform::system_discovery(),
    };
    Box::new(SimulatedDiscovery::new(scenario).with_latency(SIMULATED_LATENCY))
}

fn initial_language() -> &'static str {
    let from_env = ["UDM_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()));
    match from_env {
        Some(value) if value.starts_with("ko") => "ko",
        _ => "en",
    }
}

fn select_language(ui: &MainWindow, index: i32) {
    let language = if index == 1 { "ko" } else { "en" };
    match slint::select_bundled_translation(language) {
        Ok(()) => ui.set_language_index(index),
        Err(error) => tracing::warn!(%error, language, "translation not available"),
    }
}

fn run() -> Result<(), StartupError> {
    let ui = MainWindow::new().map_err(StartupError::Ui)?;
    select_language(&ui, i32::from(initial_language() == "ko"));

    let weak = ui.as_weak();
    let service = DiscoveryService::start(adapter_for(0), move |event: DiscoveryEvent| {
        // Runs on the worker thread: hand the event to the GUI thread.
        if weak
            .upgrade_in_event_loop(move |ui| {
                PRESENTER.with_borrow_mut(|p| p.handle_event(&ui, event));
            })
            .is_err()
        {
            tracing::debug!("event loop gone; dropping discovery event");
        }
    })
    .map_err(StartupError::Worker)?;
    let service = Rc::new(service);

    let refresh = {
        let service = Rc::clone(&service);
        let weak = ui.as_weak();
        move || {
            let Some(ui) = weak.upgrade() else { return };
            match service.request_refresh() {
                Ok(RefreshRequest::Queued { .. }) => Presenter::show_loading(&ui),
                Ok(RefreshRequest::AlreadyPending) => {}
                Err(ServiceError::Busy) => tracing::debug!("refresh ignored: queue full"),
                Err(ServiceError::Stopped) => {
                    PRESENTER.with_borrow_mut(|p| p.show_worker_stopped(&ui));
                }
            }
        }
    };

    ui.on_refresh({
        let refresh = refresh.clone();
        move || refresh()
    });

    ui.on_source_changed({
        let service = Rc::clone(&service);
        let weak = ui.as_weak();
        let refresh = refresh.clone();
        move |index| {
            let Some(ui) = weak.upgrade() else { return };
            match service.set_source(adapter_for(index)) {
                Ok(epoch) => {
                    let simulated = index != SYSTEM_SOURCE_INDEX;
                    PRESENTER.with_borrow_mut(|p| p.source_changed(&ui, epoch, simulated));
                    refresh();
                }
                Err(ServiceError::Busy) => tracing::warn!("source change ignored: queue full"),
                Err(ServiceError::Stopped) => {
                    PRESENTER.with_borrow_mut(|p| p.show_worker_stopped(&ui));
                }
            }
        }
    });

    ui.on_language_changed({
        let weak = ui.as_weak();
        move |index| {
            if let Some(ui) = weak.upgrade() {
                select_language(&ui, index);
            }
        }
    });

    refresh();
    let result = ui.run().map_err(StartupError::Ui);
    drop(service);
    result
}

/// Phase 0 policy (CLAUDE.md rule 10): the GUI never runs elevated.
#[cfg(unix)]
#[allow(unsafe_code)]
fn running_with_elevated_privileges() -> bool {
    // SAFETY: geteuid() takes no arguments, cannot fail, and has no memory
    // safety preconditions (POSIX.1-2008).
    let euid = unsafe { libc::geteuid() };
    euid == 0
}

/// Windows elevation detection (token elevation query) is not implemented in
/// Phase 0; see docs/PLATFORM_SUPPORT.md.
#[cfg(not(unix))]
fn running_with_elevated_privileges() -> bool {
    false
}
