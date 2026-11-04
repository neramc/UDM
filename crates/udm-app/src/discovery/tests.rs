// SPDX-FileCopyrightText: 2026 Universal Drive Manager contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the background discovery service.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use udm_domain::{DiscoveryIssueKind, DiscoverySource, PlatformKind};
use udm_platform::simulated::{Scenario, SimulatedDiscovery};
use udm_platform::{DeviceDiscovery, DiscoveryError, DiscoveryReport, UnimplementedDiscovery};

use super::*;

const TIMEOUT: Duration = Duration::from_secs(10);

fn start(adapter: Box<dyn DeviceDiscovery>) -> (DiscoveryService, Receiver<DiscoveryEvent>) {
    let (tx, rx) = mpsc::channel();
    let service = DiscoveryService::start(adapter, move |event| {
        let _ = tx.send(event);
    })
    .unwrap();
    (service, rx)
}

fn next_outcome(rx: &Receiver<DiscoveryEvent>) -> DiscoveryOutcome {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining).expect("event before timeout") {
            DiscoveryEvent::Started { .. } => {}
            DiscoveryEvent::Finished(outcome) => return outcome,
        }
    }
}

/// Records the thread it runs on and optionally blocks or panics.
struct ProbeAdapter {
    threads: Arc<Mutex<Vec<ThreadId>>>,
    delay: Duration,
    panic: bool,
}

impl DeviceDiscovery for ProbeAdapter {
    fn source(&self) -> DiscoverySource {
        DiscoverySource::Simulated {
            scenario: "probe".into(),
        }
    }

    fn discover(&mut self) -> Result<DiscoveryReport, DiscoveryError> {
        self.threads.lock().unwrap().push(thread::current().id());
        thread::sleep(self.delay);
        assert!(!self.panic, "probe adapter panics on purpose");
        Ok(DiscoveryReport::default())
    }
}

fn probe(delay: Duration, panic: bool) -> (Box<ProbeAdapter>, Arc<Mutex<Vec<ThreadId>>>) {
    let threads = Arc::new(Mutex::new(Vec::new()));
    (
        Box::new(ProbeAdapter {
            threads: Arc::clone(&threads),
            delay,
            panic,
        }),
        threads,
    )
}

#[test]
fn discovery_runs_on_the_worker_thread_not_the_caller() {
    let (adapter, threads) = probe(Duration::ZERO, false);
    let (service, rx) = start(adapter);
    service.request_refresh().unwrap();
    assert!(matches!(next_outcome(&rx), DiscoveryOutcome::Completed(_)));
    let threads = threads.lock().unwrap();
    assert_eq!(threads.len(), 1);
    assert_ne!(threads[0], thread::current().id());
}

#[test]
fn requesting_a_refresh_never_blocks_on_a_slow_adapter() {
    let (adapter, _) = probe(Duration::from_millis(500), false);
    let (service, rx) = start(adapter);
    let started = Instant::now();
    for _ in 0..20 {
        service.request_refresh().unwrap();
    }
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "request_refresh blocked for {:?}",
        started.elapsed()
    );
    drop(rx);
}

#[test]
fn refresh_requests_are_coalesced() {
    let (adapter, threads) = probe(Duration::from_millis(200), false);
    let (service, rx) = start(adapter);
    let mut queued = 0;
    for _ in 0..10 {
        if let RefreshRequest::Queued { .. } = service.request_refresh().unwrap() {
            queued += 1;
        }
    }
    assert!((1..=2).contains(&queued), "queued {queued}");
    for _ in 0..queued {
        next_outcome(&rx);
    }
    assert!(
        rx.recv_timeout(Duration::from_millis(400)).is_err(),
        "no extra passes"
    );
    assert_eq!(threads.lock().unwrap().len(), queued);
}

#[test]
fn snapshot_includes_identity_assessment() {
    let (service, rx) = start(Box::new(SimulatedDiscovery::new(Scenario::SampleDevices)));
    let RefreshRequest::Queued { generation } = service.request_refresh().unwrap() else {
        panic!("first request must be queued");
    };
    let DiscoveryOutcome::Completed(snapshot) = next_outcome(&rx) else {
        panic!("sample scenario succeeds");
    };
    assert_eq!(snapshot.generation, generation);
    assert_eq!(snapshot.epoch, 0);
    assert!(snapshot.source.is_simulated());
    assert_eq!(snapshot.devices.len(), 7);
    assert!(
        snapshot
            .issues
            .iter()
            .any(|i| matches!(i.kind, DiscoveryIssueKind::DuplicateIdentifier { .. }))
    );
}

#[test]
fn source_changes_bump_the_epoch_and_switch_adapters() {
    let (service, rx) = start(Box::new(SimulatedDiscovery::new(Scenario::SampleDevices)));
    let epoch = service
        .set_source(Box::new(SimulatedDiscovery::new(Scenario::NoDevices)))
        .unwrap();
    assert_eq!(epoch, 1);
    assert_eq!(service.current_epoch(), 1);
    service.request_refresh().unwrap();
    let DiscoveryOutcome::Completed(snapshot) = next_outcome(&rx) else {
        panic!("no-devices scenario succeeds");
    };
    assert_eq!(snapshot.epoch, 1);
    assert!(snapshot.devices.is_empty());
    assert_eq!(
        snapshot.source,
        DiscoverySource::Simulated {
            scenario: "no-devices".into()
        }
    );
}

#[test]
fn adapter_errors_are_reported_not_swallowed() {
    let (service, rx) = start(Box::new(UnimplementedDiscovery::new(PlatformKind::MacOs)));
    service.request_refresh().unwrap();
    match next_outcome(&rx) {
        DiscoveryOutcome::Failed { error, source, .. } => {
            assert!(matches!(
                error,
                DiscoveryFailure::Adapter(DiscoveryError::NotImplemented {
                    platform: PlatformKind::MacOs
                })
            ));
            assert_eq!(
                source,
                DiscoverySource::System {
                    platform: PlatformKind::MacOs
                }
            );
        }
        DiscoveryOutcome::Completed(_) => panic!("must not report success"),
    }
}

#[test]
fn adapter_panic_is_contained_and_the_worker_survives() {
    let (adapter, _) = probe(Duration::ZERO, true);
    let (service, rx) = start(adapter);
    service.request_refresh().unwrap();
    assert!(matches!(
        next_outcome(&rx),
        DiscoveryOutcome::Failed {
            error: DiscoveryFailure::AdapterPanicked,
            ..
        }
    ));
    service
        .set_source(Box::new(SimulatedDiscovery::new(Scenario::NoDevices)))
        .unwrap();
    service.request_refresh().unwrap();
    assert!(matches!(next_outcome(&rx), DiscoveryOutcome::Completed(_)));
}

#[test]
fn started_event_precedes_finished_event() {
    let (service, rx) = start(Box::new(SimulatedDiscovery::new(Scenario::NoDevices)));
    service.request_refresh().unwrap();
    assert!(matches!(
        rx.recv_timeout(TIMEOUT).unwrap(),
        DiscoveryEvent::Started {
            generation: 1,
            epoch: 0
        }
    ));
    assert!(matches!(
        rx.recv_timeout(TIMEOUT).unwrap(),
        DiscoveryEvent::Finished(_)
    ));
}

#[test]
fn drop_does_not_hang_on_a_stuck_adapter() {
    let (adapter, _) = probe(Duration::from_secs(30), false);
    let (service, rx) = start(adapter);
    service.request_refresh().unwrap();
    // Wait until the pass is running.
    assert!(matches!(
        rx.recv_timeout(TIMEOUT).unwrap(),
        DiscoveryEvent::Started { .. }
    ));
    let started = Instant::now();
    drop(service);
    let elapsed = started.elapsed();
    assert!(
        elapsed < SHUTDOWN_GRACE + Duration::from_secs(1),
        "drop took {elapsed:?}"
    );
}

#[test]
fn drop_joins_an_idle_worker_promptly() {
    let (service, _rx) = start(Box::new(SimulatedDiscovery::new(Scenario::NoDevices)));
    let started = Instant::now();
    drop(service);
    assert!(started.elapsed() < Duration::from_millis(500));
}
