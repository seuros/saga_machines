#![no_std]
#![doc = include_str!("../README.md")]

mod action;
mod id;
mod refusal;
mod snapshot;

use periscope_machines::Issuer;

pub use action::{Action, ActionKey, Kind, Outcome};
pub use dedup_machines::{Claim, ClaimId, Dedup, Refusal as LedgerRefusal};
pub use id::{AttemptId, SagaId};
pub use refusal::Refusal;
pub use snapshot::{Settlement, Snapshot};

/// Workflow progress, excluding any unresolved issued attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Next forward step.
    Forward(usize),
    /// Next completed step to compensate.
    Compensating(usize),
    /// All forward steps succeeded.
    Complete,
    /// All required compensations finished.
    Aborted,
}

/// Honoured outcome of [`Saga::resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// The pending action is resolved and the progress advanced.
    Resolved,
    /// An idempotent repeat: this is the last action resolved, reported with
    /// the same outcome again. Nothing changed.
    AlreadyResolved,
}

/// Honoured outcome of [`Saga::cancel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancellation {
    /// The saga stopped going forward and now unwinds what succeeded.
    Cancelled,
    /// An idempotent repeat: the saga is already compensating or aborted, by
    /// an earlier cancel or by a forward failure. Nothing changed.
    AlreadyUnwinding,
}

/// Fixed-step sequential workflow.
///
/// Equality compares the whole value: identity, issued attempts, progress,
/// the pending action and the last settlement. A refused call leaves the saga
/// equal to a clone taken before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saga<const N: usize> {
    id: SagaId,
    /// Issues the attempt ids; it also holds the saga's scope.
    attempts: Issuer,
    progress: Progress,
    pending: Option<Action>,
    settled: Option<Settlement>,
}

impl<const N: usize> Saga<N> {
    /// Start a fresh workflow in scope zero. `id` must be unique in the dedup
    /// namespace its action keys are claimed in.
    #[must_use]
    pub const fn new(id: SagaId) -> Self {
        Self::with_scope(id, 0)
    }

    /// Start a fresh workflow whose attempt ids carry `scope`.
    ///
    /// An attempt id is a plain integer, so two sagas in the same scope issue
    /// the same ones. Give every saga whose replies could meet (one reply
    /// queue for many sagas, or a new incarnation of this saga that could not
    /// save its count) its own scope: an attempt of another scope is then
    /// always [`Refusal::Stale`]. The scope is no part of the [`ActionKey`],
    /// which stays the same in every scope.
    #[must_use]
    pub const fn with_scope(id: SagaId, scope: u16) -> Self {
        Self {
            id,
            attempts: Issuer::new(scope),
            progress: snapshot::initial::<N>(),
            pending: None,
            settled: None,
        }
    }

    /// Continue the workflow of `scope` saved in `snapshot`.
    ///
    /// The next attempt carries sequence `snapshot.issued() + 1`, so an attempt
    /// that outlived the restart is never issued again. An attempt that was
    /// issued but never resolved is not carried over: it is
    /// [`Refusal::Stale`] here, and the same [`ActionKey`] is issued again
    /// under a fresh attempt. The last settlement is carried over, so a
    /// repeat of it is still honoured.
    ///
    /// `Some` for exactly the snapshots a saga of `N` steps in `scope` can
    /// produce, and `None` for every other. A snapshot is one of those when:
    ///
    /// - it carries `scope`, and `issued` is at most 2^48 - 1;
    /// - the settlement, if any, is this saga's: its key carries the saga id,
    ///   its step is in the workflow (`step < N` forward, `step < N - 1`
    ///   compensating, since the last step is never compensated), its attempt
    ///   is one this scope issued within `issued`, and the attempt's sequence
    ///   number is at least `step + 1` forward or `step + 2` compensating:
    ///   the attempts every earlier step, and that one, needed. Retries and
    ///   attempts lost to a restart take numbers too, so there is no upper
    ///   bound;
    /// - the progress is where the settlement leads (the start, without
    ///   one), or the cancel of that;
    /// - `issued` is exactly the settled attempt's sequence number when the
    ///   settlement itself completed or aborted the saga: nothing is issued
    ///   after it. A saga a cancel aborted before its first settlement may
    ///   have any count, the attempts lost to restarts before the cancel.
    #[must_use]
    pub fn restored(snapshot: Snapshot, scope: u16) -> Option<Self> {
        if snapshot.scope != scope {
            return None;
        }
        let attempts = Issuer::resumed(scope, snapshot.issued)?;
        snapshot::sound::<N>(&snapshot, &attempts).then_some(Self {
            id: snapshot.id,
            attempts,
            progress: snapshot.progress,
            pending: None,
            settled: snapshot.settled,
        })
    }

    /// Persistable state; take it after each honoured call and hand it back
    /// to [`Saga::restored`]. The pending action is not part of it.
    #[must_use]
    pub const fn snapshot(&self) -> Snapshot {
        Snapshot {
            id: self.id,
            scope: self.attempts.scope(),
            progress: self.progress,
            issued: self.attempts.issued(),
            settled: self.settled,
        }
    }

    /// The workflow identity.
    #[must_use]
    pub const fn id(&self) -> SagaId {
        self.id
    }

    /// The scope carried by every attempt id this saga issues.
    #[must_use]
    pub const fn scope(&self) -> u16 {
        self.attempts.scope()
    }

    /// How many attempts this saga has issued; also the sequence number of
    /// the latest one.
    #[must_use]
    pub const fn issued(&self) -> u64 {
        self.attempts.issued()
    }

    /// Current workflow progress.
    #[must_use]
    pub const fn progress(&self) -> Progress {
        self.progress
    }

    /// The issued action that is not resolved yet, if any.
    #[must_use]
    pub const fn pending(&self) -> Option<Action> {
        self.pending
    }

    /// The last action resolved and its outcome, if any: the one repeat
    /// [`Saga::resolve`] can prove.
    #[must_use]
    pub const fn settled(&self) -> Option<Settlement> {
        self.settled
    }

    /// The next piece of work: a drain over the workflow.
    ///
    /// `Ok(None)` once the saga is [`Progress::Complete`] or
    /// [`Progress::Aborted`]: nothing is left. While an action is pending,
    /// the call is an idempotent repeat: it answers that same action, same
    /// attempt, and changes nothing. Otherwise it issues a fresh attempt for
    /// the step the progress points at.
    ///
    /// # Errors
    /// [`Refusal::Exhausted`] when work remains but every attempt of the
    /// scope has been issued.
    pub const fn next_action(&mut self) -> Result<Option<Action>, Refusal> {
        if self.pending.is_some() {
            return Ok(self.pending);
        }
        let (step, kind) = match self.progress {
            Progress::Forward(step) => (step, Kind::Forward),
            Progress::Compensating(step) => (step, Kind::Compensate),
            Progress::Complete | Progress::Aborted => return Ok(None),
        };
        let attempt = match self.attempts.issue() {
            Ok(raw) => AttemptId::issued(raw),
            Err(periscope_machines::Refusal::Exhausted) => return Err(Refusal::Exhausted),
        };
        let action = Action::new(ActionKey::new(self.id, step, kind), attempt);
        self.pending = Some(action);
        Ok(self.pending)
    }

    /// Report the known outcome of an issued action.
    ///
    /// [`Resolution::Resolved`] for the pending action: the progress advances
    /// and the action becomes the last settlement. Reporting the last
    /// settled action again with the same outcome is the idempotent repeat
    /// [`Resolution::AlreadyResolved`], whatever happened since (a cancel, a
    /// newer pending action); nothing changes.
    ///
    /// # Errors
    /// In this order: [`Refusal::Stale`] for an attempt this saga never
    /// issued (another scope, or a sequence it has not reached), before
    /// anything is looked up; [`Refusal::Mismatch`] when the attempt is the
    /// pending one under another key, or the last settled one under another
    /// key or with another outcome; [`Refusal::Stale`] for any other attempt
    /// (an earlier one, or one that was pending when a snapshot was taken).
    pub fn resolve(&mut self, action: Action, outcome: Outcome) -> Result<Resolution, Refusal> {
        if !self.attempts.owns(action.attempt().raw()) {
            return Err(Refusal::Stale);
        }
        let report = Settlement { action, outcome };
        if let Some(pending) = self.pending
            && pending.attempt() == action.attempt()
        {
            if pending != action {
                return Err(Refusal::Mismatch);
            }
            self.pending = None;
            self.progress = snapshot::advanced::<N>(report);
            self.settled = Some(report);
            return Ok(Resolution::Resolved);
        }
        match self.settled {
            Some(settled) if settled == report => Ok(Resolution::AlreadyResolved),
            Some(settled) if settled.action.attempt() == action.attempt() => Err(Refusal::Mismatch),
            Some(_) | None => Err(Refusal::Stale),
        }
    }

    /// Stop going forward and unwind what succeeded.
    ///
    /// [`Cancellation::Cancelled`] between forward actions: the progress
    /// turns to compensating the last step that succeeded, or straight to
    /// [`Progress::Aborted`] when none did. On a saga that is already
    /// compensating or aborted the call is the idempotent repeat
    /// [`Cancellation::AlreadyUnwinding`], pending compensation or not;
    /// nothing changes.
    ///
    /// # Errors
    /// [`Refusal::Concluded`] for a complete saga; [`Refusal::Busy`] while a
    /// forward action is unresolved.
    pub const fn cancel(&mut self) -> Result<Cancellation, Refusal> {
        match self.progress {
            Progress::Compensating(_) | Progress::Aborted => Ok(Cancellation::AlreadyUnwinding),
            Progress::Complete => Err(Refusal::Concluded),
            Progress::Forward(_) if self.pending.is_some() => Err(Refusal::Busy),
            Progress::Forward(step) => {
                self.progress = snapshot::unwound(step);
                Ok(Cancellation::Cancelled)
            }
        }
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
