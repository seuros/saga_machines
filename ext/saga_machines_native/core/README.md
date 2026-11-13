# saga-machines

> **AMS Garcia-Molina**, NCC-4217, *Garcia-Molina*-class recovery carrier.

Engine-room flight manual: chart forward maneuvers and compensating return legs.

Allocation-free `no_std` sequential saga decisions. Successful forward steps are
compensated in reverse order after a known forward failure. A failed compensation
stays at the same step for a fresh retry attempt; a stale reply cannot advance
progress. Every step is assumed compensatable. Cancellation is allowed only
between forward actions, never by forgetting an unresolved external effect.

```rust
use core::time::Duration;
use saga_machines::{Claim, Dedup, Kind, Outcome, Progress, Resolution, Saga, SagaId};

let mut saga = Saga::<2>::new(SagaId::from_raw(99));
let mut ledger = Dedup::<saga_machines::ActionKey, 4>::new();

let Ok(Some(first)) = saga.next_action() else {
    panic!("two steps to run");
};
assert_eq!((first.step(), first.kind()), (0, Kind::Forward));
// The key is the same for every retry of this step: claim it before the effect.
let ttl = Duration::from_secs(30);
assert!(matches!(first.claim_in(&mut ledger, Duration::ZERO, ttl), Ok(Claim::Acquired(_))));
assert_eq!(saga.resolve(first, Outcome::Success), Ok(Resolution::Resolved));
// A redelivered reply is honoured and changes nothing.
assert_eq!(saga.resolve(first, Outcome::Success), Ok(Resolution::AlreadyResolved));
assert_eq!(saga.progress(), Progress::Forward(1));
```

A forward failure must mean **no effect occurred**; uncertain effects must be
reconciled before reporting. This model does not make effects or progress
durable: persist actions before executing them and outcomes before advancing,
and provide transactional or idempotent effects externally. Compensation is not
atomic rollback. No payloads, no retry timing, no wire format.

## Contract

**Two identities.** An `Action` carries both, and they do different jobs.

- `ActionKey { saga, step, kind }` is the **idempotency identity**. It is built
  from the caller's `SagaId` and is the same for every attempt at that work:
  across retries, across restarts, in every scope. It is what the outside world
  deduplicates the effect by, and the key `Action::claim_in` claims. It is not
  an issued id: anyone can build one and it proves nothing to the saga.
- `AttemptId` is the **handle for `resolve`**. The saga issues one per try
  through a `periscope_machines::Issuer` and never issues it again. A reply is
  honoured only for an attempt the saga issued, under the key it issued it for.

`SagaId` is caller-supplied (`from_raw`) or derived from a name
(`from_bytes`, FNV-1a, the same in every process). The caller owns collision
avoidance within one ledger.

**Refusals.** `Refusal` is the one refusal type. A refused call changed
nothing: not the progress, the pending action, the last settlement or the
issued count; the saga equals a clone taken before the call.

| Call | Honoured | Refusals |
| --- | --- | --- |
| `next_action()` | `Some(Action)`; `None` once complete or aborted | `Exhausted` |
| `resolve(action, outcome)` | `Resolved`, `AlreadyResolved` | `Stale`, `Mismatch` |
| `cancel()` | `Cancelled`, `AlreadyUnwinding` | `Concluded`, `Busy` |
| `Action::claim_in(ledger, now, ttl)` | the ledger's `Claim` | the ledger's `LedgerRefusal` |
| `Saga::restored(snapshot, scope)` | `Some(Saga)` | `None` for unsound saved state |

- `Exhausted`: work remains but the scope's 2^48 - 1 attempts are spent. It is
  not the terminal answer: a finished saga answers `Ok(None)`.
- `Stale`: the attempt carries another scope or was never issued (answered
  before anything is looked up), or it is an earlier attempt that is neither
  pending nor the last one resolved.
- `Mismatch`: the attempt is the pending one under another key, or the last
  settled one under another key or with another outcome. The record stands.
- `Busy`: a forward action is unresolved, so the saga cannot be cancelled.
- `Concluded`: the saga is complete and cannot be cancelled.

`resolve` checks in this order: `Stale` (never issued), `Mismatch`, `Stale`
(issued, but no longer current).

**Repeats.** Only what the saga can prove is honoured, and a repeat changes
nothing.

- `next_action` while an action is pending answers that same action, same
  attempt, and issues nothing.
- `resolve` of the last settled action with the same outcome is
  `AlreadyResolved`. The saga keeps exactly one `Settlement` (the last action
  resolved and its outcome, `Saga::settled`), so this is the one repeat it can
  prove, and it stays provable after a cancel, while a newer action is pending,
  and across a restart. The same action with the other outcome is `Mismatch`.
  Anything older is `Stale`: no further history is kept.
- `cancel` on a saga that is already compensating or aborted is
  `AlreadyUnwinding`, whether an earlier cancel or a forward failure put it
  there: the effect asked for (go no further, undo what succeeded) holds and is
  read straight off the progress.

**Scopes.** `Saga::new(id)` issues attempts in scope zero;
`Saga::with_scope(id, scope)` picks another. An attempt is a plain integer
(`AttemptId::raw`, `from_raw`, `scope`): the scope in the top 16 bits above a
sequence from 1. Two sagas in one scope issue the same integers, so give every
saga whose replies could meet its own scope, and an attempt of another scope is
`Stale` before anything is looked up. The scope is no part of the `ActionKey`:
the idempotency identity must survive a change of scope.

**Restarts.** `Saga::snapshot` is the state to save: id, scope, progress,
issued count and last settlement. `Saga::restored(snapshot, scope)` takes the
scope explicitly and answers `Some` for exactly the snapshots a saga of `N`
steps in that scope can produce, `None` for every other: a snapshot of another
scope, a count beyond 2^48 - 1, a progress that does not fit `N` steps, or a
settlement that is not this saga's or does not lead to the progress, directly
or through a cancel. The settled attempt must be one the scope issued within
the count, and not sooner than its action allows: sequence number at least
`step + 1` for a forward action and `step + 2` for a compensation (retries and
attempts lost to a restart only raise it). When the settlement itself
completed or aborted the saga the count must equal that sequence number, since
no attempt is issued afterwards; only a saga cancelled before it settled
anything is aborted with a free count. The pending action is not
saved: after a restart it is `Stale`, and the same `ActionKey` is issued again
under a fresh attempt, so the ledger decides whether the effect already
happened. A saga that cannot save its count takes a scope no earlier
incarnation used.

**The ledger seam.** `Dedup`, `Claim`, `ClaimId` and the ledger's refusal
(`LedgerRefusal`, which is `dedup_machines::Refusal`) are re-exported.
`claim_in` is named for what it does: it claims the action's key *in* a sibling
ledger and writes to it, so it is not a `_from` reader. It changes nothing in
the saga, and the ledger's answer comes back unchanged, honoured or refused.

**Time.** The saga reads no clock and takes none. `claim_in` passes `now` and
`ttl` to the ledger as `core::time::Duration`.
