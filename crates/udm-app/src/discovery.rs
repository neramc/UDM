//! Background device discovery.
//!
//! [`DiscoveryService`] owns one worker thread and one adapter at a time.
//! Callers (the GUI thread) only enqueue commands through a **bounded**
//! channel and receive [`DiscoveryEvent`]s through a callback invoked on the
//! worker thread. No call on the service blocks on discovery.
//!
//! * Refresh requests are coalesced: while a request is queued, further
//!   requests return [`RefreshRequest::AlreadyPending`].
//! * Every source change increments an *epoch*. Results carry the epoch they
//!   were produced under, so consumers can discard results from a previous
//!   source.
//! * Adapter panics are caught and reported as
//!   [`DiscoveryFailure::AdapterPanicked`]; the worker keeps running.
//! * On drop, the service waits a bounded time for the worker to finish. A
//!   worker stuck in a read-only discovery call is detached rather than
//!   blocking application exit. (Future write jobs must not use this policy;
//!   see ADR-0007.)

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use udm_domain::{Device, DiscoveryIssue, DiscoverySource, assess_identities};
use udm_platform::{DeviceDiscovery, DiscoveryError};

/// Capacity of the command queue. Small on purpose: commands are coalesced.
const COMMAND_QUEUE_CAPACITY: usize = 4;

/// How long `Drop` waits for the worker before detaching it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);

/// A completed discovery pass.
#[derive(Debug, Clone)]
pub struct DiscoverySnapshot {
    pub generation: u64,
    pub epoch: u64,
    pub source: DiscoverySource,
    pub devices: Vec<Device>,
    /// Adapter issues followed by identity-assessment issues.
    pub issues: Vec<DiscoveryIssue>,
    pub duration: Duration,
}

/// Why a discovery pass produced no snapshot.
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryFailure {
    #[error(transparent)]
    Adapter(#[from] DiscoveryError),
    #[error("the discovery adapter panicked")]
    AdapterPanicked,
}

/// Result of one discovery pass.
#[derive(Debug)]
pub enum DiscoveryOutcome {
    Completed(DiscoverySnapshot),
    Failed {
        generation: u64,
        epoch: u64,
        source: DiscoverySource,
        error: DiscoveryFailure,
    },
}

impl DiscoveryOutcome {
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        match self {
            Self::Completed(snapshot) => snapshot.epoch,
            Self::Failed { epoch, .. } => *epoch,
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        match self {
            Self::Completed(snapshot) => snapshot.generation,
            Self::Failed { generation, .. } => *generation,
        }
    }
}

/// Events delivered to the sink, on the worker thread.
#[derive(Debug)]
pub enum DiscoveryEvent {
    Started { generation: u64, epoch: u64 },
    Finished(DiscoveryOutcome),
}

/// Answer to [`DiscoveryService::request_refresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshRequest {
    /// A new pass was queued with this generation number.
    Queued { generation: u64 },
    /// A pass is already queued; its result will reflect this request too.
    AlreadyPending,
}

/// The service cannot accept a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ServiceError {
    #[error("the discovery queue is full")]
    Busy,
    #[error("the discovery worker has stopped")]
    Stopped,
}

enum Command {
    SetSource {
        adapter: Box<dyn DeviceDiscovery>,
        epoch: u64,
    },
    Refresh {
        generation: u64,
    },
}

/// Runs discovery on a dedicated background thread.
pub struct DiscoveryService {
    sender: Option<SyncSender<Command>>,
    pending_refresh: Arc<AtomicBool>,
    next_generation: AtomicU64,
    epoch: AtomicU64,
    worker: Option<JoinHandle<()>>,
    done: Receiver<()>,
}

impl std::fmt::Debug for DiscoveryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscoveryService")
            .field("epoch", &self.epoch.load(Ordering::Relaxed))
            .field(
                "pending_refresh",
                &self.pending_refresh.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl DiscoveryService {
    /// Starts the worker with an initial adapter (epoch 0). No discovery runs
    /// until [`DiscoveryService::request_refresh`] is called.
    ///
    /// # Errors
    /// Returns an error if the worker thread cannot be spawned.
    pub fn start(
        adapter: Box<dyn DeviceDiscovery>,
        sink: impl Fn(DiscoveryEvent) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (done_tx, done) = mpsc::channel();
        let pending_refresh = Arc::new(AtomicBool::new(false));
        let worker_pending = Arc::clone(&pending_refresh);
        let worker = thread::Builder::new()
            .name("udm-discovery".into())
            .spawn(move || {
                run_worker(adapter, &receiver, &worker_pending, &sink);
                // The receiver may already be gone if the service was dropped
                // and timed out; that is fine.
                let _ = done_tx.send(());
            })?;
        Ok(Self {
            sender: Some(sender),
            pending_refresh,
            next_generation: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            worker: Some(worker),
            done,
        })
    }

    /// The epoch of the most recently requested source.
    #[must_use]
    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    /// Requests a discovery pass. Never blocks.
    ///
    /// # Errors
    /// [`ServiceError::Busy`] if the command queue is full,
    /// [`ServiceError::Stopped`] if the worker is gone.
    pub fn request_refresh(&self) -> Result<RefreshRequest, ServiceError> {
        if self.pending_refresh.swap(true, Ordering::SeqCst) {
            return Ok(RefreshRequest::AlreadyPending);
        }
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        match self.send(Command::Refresh { generation }) {
            Ok(()) => Ok(RefreshRequest::Queued { generation }),
            Err(e) => {
                self.pending_refresh.store(false, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    /// Replaces the adapter for subsequent passes and returns the new epoch.
    /// Never blocks.
    ///
    /// # Errors
    /// [`ServiceError::Busy`] if the command queue is full,
    /// [`ServiceError::Stopped`] if the worker is gone.
    pub fn set_source(&self, adapter: Box<dyn DeviceDiscovery>) -> Result<u64, ServiceError> {
        let epoch = self.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.send(Command::SetSource { adapter, epoch })?;
        Ok(epoch)
    }

    fn send(&self, command: Command) -> Result<(), ServiceError> {
        let sender = self.sender.as_ref().ok_or(ServiceError::Stopped)?;
        sender.try_send(command).map_err(|e| match e {
            TrySendError::Full(_) => ServiceError::Busy,
            TrySendError::Disconnected(_) => ServiceError::Stopped,
        })
    }
}

impl Drop for DiscoveryService {
    fn drop(&mut self) {
        // Disconnecting the channel makes the worker exit after its current pass.
        self.sender = None;
        match self.done.recv_timeout(SHUTDOWN_GRACE) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                if let Some(worker) = self.worker.take()
                    && worker.join().is_err()
                {
                    tracing::error!("discovery worker panicked during shutdown");
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                tracing::warn!(
                    "discovery worker did not stop within {SHUTDOWN_GRACE:?}; detaching read-only worker"
                );
            }
        }
    }
}

fn run_worker(
    mut adapter: Box<dyn DeviceDiscovery>,
    receiver: &Receiver<Command>,
    pending_refresh: &AtomicBool,
    sink: &dyn Fn(DiscoveryEvent),
) {
    let mut epoch = 0;
    while let Ok(command) = receiver.recv() {
        match command {
            Command::SetSource {
                adapter: next,
                epoch: next_epoch,
            } => {
                adapter = next;
                epoch = next_epoch;
                tracing::debug!(epoch, source = ?adapter.source(), "discovery source changed");
            }
            Command::Refresh { generation } => {
                // Requests arriving from now on need a new pass.
                pending_refresh.store(false, Ordering::SeqCst);
                sink(DiscoveryEvent::Started { generation, epoch });
                let outcome = run_pass(adapter.as_mut(), generation, epoch);
                sink(DiscoveryEvent::Finished(outcome));
            }
        }
    }
    tracing::debug!("discovery worker stopped");
}

fn run_pass(adapter: &mut dyn DeviceDiscovery, generation: u64, epoch: u64) -> DiscoveryOutcome {
    let source = adapter.source();
    let started = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| adapter.discover()));
    let duration = started.elapsed();
    match result {
        Ok(Ok(report)) => {
            let mut devices = report.devices;
            let mut issues = report.issues;
            issues.extend(assess_identities(&mut devices));
            tracing::info!(
                generation,
                epoch,
                devices = devices.len(),
                issues = issues.len(),
                elapsed_ms = duration.as_millis(),
                "discovery pass completed"
            );
            DiscoveryOutcome::Completed(DiscoverySnapshot {
                generation,
                epoch,
                source,
                devices,
                issues,
                duration,
            })
        }
        Ok(Err(error)) => {
            tracing::warn!(generation, epoch, %error, "discovery pass failed");
            DiscoveryOutcome::Failed {
                generation,
                epoch,
                source,
                error: DiscoveryFailure::Adapter(error),
            }
        }
        Err(_) => {
            tracing::error!(generation, epoch, "discovery adapter panicked");
            DiscoveryOutcome::Failed {
                generation,
                epoch,
                source,
                error: DiscoveryFailure::AdapterPanicked,
            }
        }
    }
}

#[cfg(test)]
mod tests;
