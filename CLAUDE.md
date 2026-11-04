# CLAUDE.md — Universal Drive Manager

1. Project Mission

Build a high-quality, free and open-source, cross-platform desktop application for managing USB storage devices and external storage.

Target operating systems:

* Windows
* Linux
* macOS

Primary technology:

* Rust stable
* Slint for the desktop GUI
* OS-specific storage adapters
* SQLite for local history and profiles where appropriate

The application is an all-in-one storage management platform, not merely an ISO writer.

Core capabilities planned:

* Device discovery and identity
* Partition and filesystem management
* Storage diagnostics and performance benchmarking
* Image inspection, writing, verification and backup
* Device cloning and restoration
* Security and integrity tools
* Storage usage analysis
* Recovery-assistance imaging
* Operation planning and audit history

The product must prioritize correctness, data safety, maintainability, accessibility, and consistent UX over rapid feature accumulation.

2. Mandatory Rules

1. Never execute destructive disk operations without explicit user authorization.
2. Never trust a device path or device index as a permanent identity.
3. Revalidate device identity, capacity, state, permissions, and operation preconditions immediately before destructive operations.
4. Never silently select a destructive-operation target.
5. Never report an operation as successful unless all required completion and verification steps succeed.
6. Never assume SMART, temperature, secure erase, partition resizing, or recovery capabilities are available on every device or operating system.
7. Never claim guaranteed physical erasure of flash storage unless the actual method and its assurance justify that claim.
8. Never write to a recovery source by default.
9. Never execute arbitrary shell commands or executable paths supplied by the UI or user-controlled configuration without strict validation.
10. Never run the entire GUI with administrator or root privileges just to simplify storage access.
11. Never load an entire disk image into memory. Use bounded, streaming I/O.
12. Never block the GUI thread with disk access, hashing, benchmarking, or other long-running work.
13. Never invent device metadata, diagnostic findings, benchmark results, or supported capabilities.
14. Never hide an unsupported feature behind a generic success message.
15. Never introduce a dependency without considering maintenance, security, platform support, licensing, and necessity.

3. Architecture Principles

Use a modular Rust workspace with clear dependency boundaries.

Presentation

* Slint screens and reusable components
* View models and UI state
* No direct raw-disk access
* No business-critical storage safety logic implemented only in UI code

Application

* Use cases and operation orchestration
* Job lifecycle, progress, cancellation requests, and events
* Operation planning and precondition checks
* Explicit error propagation

Domain

* Device, volume, partition, image, job, diagnostic report, and capability models
* Platform-independent validation rules
* No dependency on Slint or operating-system APIs

Infrastructure

* Image I/O, hashing, database, logging, benchmark orchestration, and report generation
* Streaming I/O and bounded memory usage

Platform adapters

* Windows, Linux, and macOS implementations
* Encapsulate OS-specific APIs and behavior
* Report unsupported or unverified capabilities explicitly

Privileged helper

* Separate process when elevated privileges are required
* Narrow, versioned, structured IPC protocol
* Strict request validation and least privilege
* No arbitrary command execution
* Revalidate targets and operation parameters inside the privileged boundary

Do not create abstractions that have no demonstrated purpose. Keep module interfaces small and document important architectural decisions in docs/ADR/.

4. Storage Safety

Every destructive operation must follow this lifecycle:

1. Discover and identify the device.
2. Construct a typed operation plan.
3. Validate the target and operation parameters.
4. Show the affected device, partitions, data-loss consequences, and relevant warnings.
5. Obtain explicit authorization.
6. Revalidate the device immediately before execution.
7. Execute through the appropriate platform adapter.
8. Handle cancellation, disconnection, permission errors, and partial failure.
9. Flush and verify data where applicable.
10. Persist the result, verification status, and structured error information.

Use explicit types and state machines to prevent invalid transitions.

A cancellation request is not proof that an operation has stopped. Report cancellation only after the worker reaches a safe terminal state.

If a device disconnects or its identity changes, stop safely whenever possible. Never automatically redirect an in-progress destructive operation to a different device.

Prefer virtual-disk fixtures and simulated adapters for automated tests. Never run destructive operations against an arbitrary physical disk in CI.

5. Device Identity and Capabilities

Represent device identity as a collection of observed attributes with provenance and confidence, not as a single supposedly permanent path.

Possible attributes:

* Vendor and product identifiers
* Serial number, when available
* Model and reported capacity
* Bus and transport information
* OS-specific device identifier
* Connection and observation metadata

Serial numbers may be missing, duplicated, or unreliable. Device paths and disk numbers can change.

Use an explicit capability model for operations such as raw reads, raw writes, safe eject, SMART access, partition editing, secure erase, and filesystem checking.

Distinguish:

* Supported
* Limited
* Unavailable
* Not tested

A missing capability must not be interpreted as a healthy device.

6. I/O and Concurrency

* Keep blocking storage I/O off the UI thread.
* Use dedicated blocking workers or a bounded worker pool.
* Stream data through bounded buffers.
* Avoid unbounded queues and unbounded memory growth.
* Track bytes processed, throughput, errors, and stage progress.
* Prevent conflicting destructive jobs against the same device.
* Define cancellation behavior for every long-running operation.
* Treat flush, verification, and finalization as explicit job stages.
* Do not assume direct I/O is always faster; benchmark before adopting it.
* Record short writes, read errors, retries, and verification mismatches.

Use Tokio only where asynchronous orchestration is useful. Do not force all low-level I/O into async APIs when a bounded blocking worker is more appropriate.

7. UI and Design System

Use the custom design language “Precision Minimalism”.

Requirements:

* Dashboard-first navigation
* Light and dark themes, plus an optional system-following mode
* Reusable design tokens for color, typography, spacing, radius, and elevation
* Clear distinction between ordinary actions and destructive actions
* Device state, measurement status, and operation status must remain distinct
* Accessible focus states and keyboard navigation
* Support for localization, including Korean and English
* No hardcoded styling values scattered across unrelated screens
* No decorative animation that delays or obscures critical feedback
* Long-running operations must expose progress, stage, cancellation status, and errors
* Empty, loading, unavailable, permission-denied, disconnected, and error states must be designed explicitly

Do not copy the visual identity of another application. Use references only to understand general interaction patterns.

8. Error Handling and Logging

Use typed errors in domain and infrastructure crates. Add contextual information at application boundaries.

Do not:

* Swallow errors
* Replace failures with fake success values
* Log sensitive data unnecessarily
* expose raw internal errors as the only user-facing explanation

Provide actionable user messages while preserving detailed diagnostics in logs.

Logs should support correlation between a user-visible job and its internal stages. Redact sensitive paths and identifiers where appropriate. Do not collect telemetry by default.

9. Testing Requirements

For every feature, add relevant tests before marking it complete.

Expected categories:

* Unit tests for domain rules and state transitions
* Integration tests for application use cases
* Platform adapter tests
* Fault-injection tests for I/O failures and disconnections
* Safety tests for incorrect targets and invalid parameters
* UI tests for important interaction paths
* Performance tests for streaming, hashing, and benchmarking
* Regression tests for every fixed bug

Use virtual devices, temporary files, fixtures, and simulated platform adapters.

A feature is not complete merely because it compiles.

10. Code Quality and Dependencies

* Use stable Rust unless an explicit decision documents otherwise.
* Run cargo fmt --check.
* Run appropriate Clippy checks.
* Run relevant tests before reporting completion.
* Avoid unnecessary unsafe; document and minimize every justified unsafe boundary.
* Do not introduce a large framework or dependency for a trivial helper.
* Review dependency licenses and platform compatibility.
* Keep the dependency graph and feature flags understandable.
* Do not upgrade major dependencies without checking compatibility and running tests.
* Do not edit generated files manually unless the generation workflow requires it.

11. Required Documentation

Keep these documents current as the project evolves:

* docs/PRD.md
* docs/ARCHITECTURE.md
* docs/THREAT_MODEL.md
* docs/PLATFORM_SUPPORT.md
* docs/UX_DESIGN_SYSTEM.md
* docs/TESTING_STRATEGY.md
* docs/ADR/
* SECURITY.md
* CONTRIBUTING.md

Document platform-specific limitations instead of hiding them.

12. Claude Code Workflow

Before implementing a substantial feature:

1. Inspect the existing repository and relevant documentation.
2. Identify the responsible module and its dependencies.
3. State the intended behavior, safety implications, and acceptance criteria.
4. Check whether the operation is supported by the target operating systems.
5. Propose the smallest coherent implementation plan.
6. Implement in small, reviewable changes.
7. Add tests and update documentation.
8. Run formatting, static checks, and relevant tests.
9. Review security, failure behavior, and data-loss risks.
10. Summarize files changed, tests executed, known limitations, and remaining work.

Do not rewrite unrelated modules to complete a localized task.

Do not silently change public interfaces, storage formats, security policies, or architecture decisions. Propose an ADR when a meaningful architectural decision changes.

Do not claim tests passed unless they were actually executed and their results were observed.

If platform behavior is uncertain, investigate and document the uncertainty. Do not invent APIs or claim unsupported parity.

13. Definition of Done

A feature is complete only when:

* Requirements and acceptance criteria are met.
* Error and failure paths are handled.
* Safety checks are present where required.
* Tests cover meaningful success and failure cases.
* Relevant supported platforms have been tested or limitations are documented.
* UI states and accessibility have been considered.
* Logs and user-facing errors are appropriate.
* Documentation is updated.
* Formatting, linting, and relevant tests have been run.
* No known critical data-loss or security issue remains unaddressed.

14. First Task

Do not implement the entire product immediately.

Start with Phase 0:

1. Inspect the repository.
2. Propose a concrete workspace and dependency plan.
3. Create architecture, threat-model, and platform-support drafts.
4. Build a minimal Slint application.
5. Implement simulated device discovery and a virtual-disk test harness.
6. Prototype read-only device discovery for each target operating system.
7. Document privilege requirements and raw-I/O constraints.
8. Produce a test report and a list of unresolved technical risks.

Do not enable destructive physical-device operations until the relevant design, tests, and explicit authorization flow have been reviewed.
