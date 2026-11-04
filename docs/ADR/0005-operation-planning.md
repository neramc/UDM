# ADR-0005: Typed operation plans and revalidation

- Status: **Accepted** (design approved by the project owner on 2026-11-04)
- Date: 2026-11-04
- Scope of acceptance: this ADR is the binding design for operation
  planning. It does not by itself enable any destructive operation on a
  physical device: each operation still needs its implementation, tests
  (virtual disks, fault injection, revalidation failures) and a review of
  its concrete confirmation UI before it is enabled (CLAUDE.md §14).

## Context

CLAUDE.md §4 defines a ten-step lifecycle for destructive operations
(discover → plan → validate → show consequences → authorize → revalidate →
execute → handle failure → flush/verify → persist). The type system should
make skipping a step impossible rather than merely discouraged.

## Decision

### Plan lifecycle as typestate

```rust
// Sketch — not implemented in Phase 0.
struct OperationPlan<S> { id: PlanId, kind: OperationKind, target: TargetSnapshot, state: S }

struct Draft;                                   // constructed from a snapshot
struct Validated { warnings: Vec<PlanWarning> } // preconditions checked
struct Authorized { token: AuthorizationToken } // explicit user confirmation
struct Ready { revalidated_at: Instant }        // identity re-checked just now

impl OperationPlan<Draft>      { fn validate(self, policy: &Policy) -> Result<OperationPlan<Validated>, PlanError>; }
impl OperationPlan<Validated>  { fn authorize(self, confirmation: UserConfirmation) -> Result<OperationPlan<Authorized>, PlanError>; }
impl OperationPlan<Authorized> { fn revalidate(self, fresh: &Device) -> Result<OperationPlan<Ready>, RevalidationError>; }
// Only OperationPlan<Ready> can be handed to an executor.
```

- `TargetSnapshot` stores the full `DeviceIdentity`, capacity, partition
  layout and usage observed when the plan was drafted — never just a path.
- `UserConfirmation` is created only by the UI after showing the affected
  device, partitions, data-loss consequences and warnings. For devices whose
  identity is `Inconclusive` (no strong identifier), the confirmation must
  include an extra explicit step (e.g. typing the capacity or a label).
- `revalidate` re-runs discovery for the target immediately before execution
  and requires `IdentityComparison::Consistent`, or — for devices without a
  strong identifier — `Inconclusive` with an unchanged volatile binding and a
  fresh confirmation. `Mismatch` always aborts. A changed OS path aborts; the
  plan is never silently re-pointed.

### Policy checks (in `validate` and again in `revalidate`)

- Target is not a system disk. `SystemDiskStatus::Unknown` is treated as a
  system disk. **The first implementation offers no override**; allowing one
  (e.g. for rescue systems) requires a separate ADR.
- No partition is mounted, active swap, or held by device-mapper/RAID, unless
  the operation itself unmounts through the platform adapter with consent.
- Capability for the operation is `Supported` or `Limited` (with the
  limitation shown); never `NotTested`.
- No other destructive job targets the same device (job registry lock keyed
  by identity, not path).
- Source ≠ target for clone/restore; recovery sources are opened read-only.

### Execution and results

- Executors receive `OperationPlan<Ready>` and run on a dedicated worker
  (ADR-0007) or inside the privileged helper (ADR-0006).
- Flush and verification are explicit job stages; success is reported only
  after verification completes. Cancellation is reported only after the
  worker reaches a safe terminal state.
- Every result (including partial failure and verification status) is
  persisted with a correlation ID.

## Alternatives considered

- Runtime state enum with checks in each method: easy to bypass by
  constructing a plan in the wrong state; typestate makes it a compile error.
- Trusting the UI selection as target: rejected; dashboard selection is for
  viewing and is never an implicit destructive target.

## Consequences

- The privileged helper must revalidate again inside its own boundary; the
  unprivileged `Ready` state is not trusted by the helper (ADR-0006).
- Requires a design review with concrete UI flows before implementation.
