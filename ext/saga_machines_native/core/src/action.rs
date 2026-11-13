use core::time::Duration;

use dedup_machines::{Claim, Dedup};

use crate::{AttemptId, LedgerRefusal, SagaId};

/// Direction of a saga action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Perform a forward step.
    Forward,
    /// Undo a known completed step.
    Compensate,
}

/// Known external effect outcome, never an uncertain timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// The requested effect is complete.
    Success,
    /// The requested effect did not occur.
    Failure,
}

/// Idempotency identity of one piece of work: which saga, step and direction.
///
/// It is the same for every attempt at that work, before and after a restart,
/// in every scope, so it is the key to claim in a dedup ledger.
///
/// It is built from the caller's [`SagaId`] and is not an issued id: anyone
/// may build one, and it proves nothing to [`Saga::resolve`](crate::Saga::resolve).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionKey {
    saga: SagaId,
    step: usize,
    kind: Kind,
}

impl ActionKey {
    /// The key of `kind` work on `step` of `saga`.
    #[must_use]
    pub const fn new(saga: SagaId, step: usize, kind: Kind) -> Self {
        Self { saga, step, kind }
    }

    /// The saga the work belongs to.
    #[must_use]
    pub const fn saga(self) -> SagaId {
        self.saga
    }

    /// Zero-based workflow step.
    #[must_use]
    pub const fn step(self) -> usize {
        self.step
    }

    /// Forward or compensating work.
    #[must_use]
    pub const fn kind(self) -> Kind {
        self.kind
    }
}

/// One attempt at one piece of work: the stable [`ActionKey`] plus the
/// [`AttemptId`] issued for this try.
///
/// Plain data, so it can be persisted before the effect runs and rebuilt with
/// [`Action::new`] when the reply arrives. The saga honours it only if the
/// attempt is one it issued and the key is the one it issued it for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Action {
    key: ActionKey,
    attempt: AttemptId,
}

impl Action {
    /// Rebuild an action from its persisted parts.
    #[must_use]
    pub const fn new(key: ActionKey, attempt: AttemptId) -> Self {
        Self { key, attempt }
    }

    /// Stable idempotency identity: the same across retries and restarts.
    #[must_use]
    pub const fn key(self) -> ActionKey {
        self.key
    }

    /// The handle of this try: fresh for every retry, never reissued.
    #[must_use]
    pub const fn attempt(self) -> AttemptId {
        self.attempt
    }

    /// Zero-based workflow step.
    #[must_use]
    pub const fn step(self) -> usize {
        self.key.step
    }

    /// Forward or compensating work.
    #[must_use]
    pub const fn kind(self) -> Kind {
        self.key.kind
    }

    /// Claim this action's [`ActionKey`] in a sibling dedup ledger, so a retry
    /// or a restarted saga learns whether the effect already ran.
    ///
    /// The saga is not involved and nothing of it changes: the call is
    /// `ledger.claim(self.key(), now, ttl)`, and the answer, honoured or
    /// refused, is the ledger's own.
    ///
    /// # Errors
    /// The ledger's refusal, unchanged: see
    /// [`Ledger::claim`](dedup_machines::Ledger::claim).
    pub fn claim_in<const N: usize, V>(
        self,
        ledger: &mut Dedup<ActionKey, N, V>,
        now: Duration,
        ttl: Duration,
    ) -> Result<Claim<'_, V>, LedgerRefusal> {
        ledger.claim(self.key, now, ttl)
    }
}
