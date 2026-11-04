# ADR-0007: Concurrency and I/O model

- Status: Accepted
- Date: 2026-11-04

## Context

The GUI thread must never block on disk access, hashing or benchmarking.
Storage I/O is blocking by nature (sysfs reads, raw device reads, ioctls);
streaming must use bounded memory; long operations need progress and safe
cancellation.

## Decision

- **No Tokio in Phase 0.** The only background work is discovery, which is
  served by one dedicated, named `std::thread` worker per
  `DiscoveryService`.
- **Bounded command queue** (`std::sync::mpsc::sync_channel`, capacity 4)
  with `try_send`: GUI calls never block. Refresh requests are coalesced with
  an atomic flag.
- **Events** flow worker → GUI through a callback that the GUI implements
  with `slint::Weak::upgrade_in_event_loop`, so all UI mutation happens on
  the UI thread.
- **Epochs**: every source change increments an epoch; results of older
  epochs are dropped by the presenter.
- **Panics** inside adapters are caught (`catch_unwind`) and reported as a
  failure; the worker survives.
- **Shutdown**: dropping the service disconnects the queue and waits up to
  2 s for the worker; a worker stuck in a *read-only* discovery call is
  detached. This policy is explicitly **not** valid for future write jobs,
  which must reach a safe terminal state (flush, release exclusive handles)
  before the application reports cancellation or exits.
- **Streaming I/O discipline** (exercised by `udm-testkit`): fixed-size
  chunks (64 KiB in the harness), short reads/writes looped explicitly,
  `Interrupted` retried, device end and disconnects surfaced as errors.
  Direct I/O and buffer sizes will be chosen by benchmark, not assumed.

## Alternatives considered

- Tokio runtime for everything: adds a large dependency and pushes blocking
  I/O into `spawn_blocking` anyway. May be introduced later where async
  orchestration (helper IPC, multiple concurrent jobs) benefits from it.
- Rayon / thread pool for discovery: discovery is a single sequential pass;
  a pool adds nothing.

## Consequences

- Job execution (Phase 1+) will need a job registry with per-device locks,
  explicit stages (prepare, transfer, flush, verify, finalize), progress
  events with bytes/throughput, and cancellation tokens checked between
  chunks.
