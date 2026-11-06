# saga-machines

> **AMS Garcia-Molina**, NCC-4217, *Garcia-Molina*-class recovery carrier.

Engine-room flight manual: chart forward maneuvers and compensating return legs.

Allocation-free `no_std` sequential saga decisions. Successful forward steps are
compensated in reverse order after a known forward failure. Failed compensations
remain at the same step for a fresh retry attempt; stale/duplicate replies cannot
advance progress. Every step is assumed compensatable. Cancellation is allowed
only between actions, never by forgetting an unresolved external effect.

Action keys remain stable across retries and can claim an external `dedup-machines`
ledger. Attempt handles change to reject stale replies. A forward failure must mean
**no effect occurred**; uncertain effects must be reconciled before reporting.
Handles are scoped to one workflow lifetime.

This model does not make effects or progress durable. Persist actions before
executing them and outcomes before advancing; provide transactional/idempotent
effects and crash recovery externally. Compensation is not atomic rollback.
No payloads, retry timing, wire checkpoint, or Ruby binding.
