#![no_std]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Progress {
    /// Next forward step.
    Forward(usize),
    /// Next completed step to compensate.
    Compensating(usize),
    /// All forward steps succeeded.
    Complete,
    /// All required compensations finished.
    Aborted,
    /// The compensation of this step was reported [`Outcome::Stuck`]: nothing
    /// more is issued and the saga is parked for the caller, until
    /// [`Saga::unstick`] resumes the compensation.
    Stuck(usize),
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
/// the pending action, the skipped steps and the last settlement. A refused call leaves the saga
/// equal to a clone taken before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saga<const N: usize> {
    id: SagaId,
    /// Issues the attempt ids; it also holds the saga's scope.
    attempts: Issuer,
    progress: Progress,
    pending: Option<Action>,
    /// Bit `i` is set when forward step `i` was skipped: never compensated.
    skipped: u64,
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
        const { assert!(N <= 64, "a saga tracks skipped steps in 64 bits") };
        Self {
            id,
            attempts: Issuer::new(scope),
            progress: snapshot::initial::<N>(),
            pending: None,
            skipped: 0,
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
    /// A saved [`Progress::Stuck`] is restored as it was and can be resumed
    /// with [`Saga::unstick`], however long it stayed parked.
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
    ///   one), or the cancel of that, or the retry of a stuck compensation
    ///   (`Compensating(step)` after a stuck settlement of `step`);
    /// - `issued` is exactly the settled attempt's sequence number when the
    ///   settlement itself completed or aborted the saga: nothing is issued
    ///   after it. A saga a cancel aborted before its first settlement may
    ///   have any count, the attempts lost to restarts before the cancel.
    #[must_use]
    pub fn restored(snapshot: Snapshot, scope: u16) -> Option<Self> {
        const { assert!(N <= 64, "a saga tracks skipped steps in 64 bits") };
        if snapshot.scope != scope {
            return None;
        }
        let attempts = Issuer::resumed(scope, snapshot.issued)?;
        snapshot::sound::<N>(&snapshot, &attempts).then_some(Self {
            id: snapshot.id,
            attempts,
            progress: snapshot.progress,
            pending: None,
            skipped: snapshot.skipped,
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
            skipped: self.skipped,
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

    /// The forward steps resolved [`Outcome::Skipped`]: bit `i` is set for
    /// step `i`. They are never compensated.
    #[must_use]
    pub const fn skipped(&self) -> u64 {
        self.skipped
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
    /// `Ok(None)` once the saga is [`Progress::Complete`],
    /// [`Progress::Aborted`] or [`Progress::Stuck`]: nothing is left (a stuck
    /// saga issues again once [`Saga::unstick`] resumes it). While
    /// an action is pending, the call is an idempotent repeat: it answers
    /// that same action, same attempt, and changes nothing. Otherwise it
    /// issues a fresh attempt for the step the progress points at. A step
    /// resolved [`Outcome::Skipped`] is never issued as a compensation.
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
            Progress::Complete | Progress::Aborted | Progress::Stuck(_) => return Ok(None),
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
    /// [`Outcome::Skipped`] answers a forward action: the step is recorded as
    /// done without effect, the progress advances as for success, and the
    /// unwind steps over it. [`Outcome::Stuck`] answers a compensation: the
    /// progress becomes [`Progress::Stuck`] at that step and the saga issues
    /// nothing more. Either way the action is the last settlement, so the
    /// same attempt is stale protection as for any other outcome: repeating
    /// it with the same outcome is `AlreadyResolved`, with another
    /// `Mismatch`, and once a newer action settles it is `Stale`. A stuck
    /// saga has no pending action, so every reply but the repeat of the stuck
    /// one is refused.
    ///
    /// # Errors
    /// In this order: [`Refusal::Stale`] for an attempt this saga never
    /// issued (another scope, or a sequence it has not reached), before
    /// anything is looked up; [`Refusal::Inapplicable`] for an outcome that
    /// cannot answer the action's kind, whatever the attempt; [`Refusal::Mismatch`] when the attempt is the
    /// pending one under another key, or the last settled one under another
    /// key or with another outcome; [`Refusal::Stale`] for any other attempt
    /// (an earlier one, or one that was pending when a snapshot was taken).
    pub fn resolve(&mut self, action: Action, outcome: Outcome) -> Result<Resolution, Refusal> {
        if !self.attempts.owns(action.attempt().raw()) {
            return Err(Refusal::Stale);
        }
        if !outcome.applies_to(action.kind()) {
            return Err(Refusal::Inapplicable);
        }
        let report = Settlement { action, outcome };
        if let Some(pending) = self.pending
            && pending.attempt() == action.attempt()
        {
            if pending != action {
                return Err(Refusal::Mismatch);
            }
            let skipped = snapshot::skipped_after(self.skipped, report);
            let Some(progress) = snapshot::advanced::<N>(report, skipped) else {
                return Err(Refusal::Inapplicable);
            };
            self.pending = None;
            self.skipped = skipped;
            self.progress = progress;
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
    /// [`Progress::Aborted`] when none did; steps that were skipped did not
    /// succeed with an effect and are passed over. On a saga that is already
    /// compensating or aborted the call is the idempotent repeat
    /// [`Cancellation::AlreadyUnwinding`], pending compensation or not;
    /// nothing changes.
    ///
    /// # Errors
    /// [`Refusal::Concluded`] for a complete or stuck saga; [`Refusal::Busy`] while a
    /// forward action is unresolved.
    pub const fn cancel(&mut self) -> Result<Cancellation, Refusal> {
        match self.progress {
            Progress::Compensating(_) | Progress::Aborted => Ok(Cancellation::AlreadyUnwinding),
            Progress::Complete | Progress::Stuck(_) => Err(Refusal::Concluded),
            Progress::Forward(_) if self.pending.is_some() => Err(Refusal::Busy),
            Progress::Forward(step) => {
                self.progress = snapshot::unwound(step, self.skipped);
                Ok(Cancellation::Cancelled)
            }
        }
    }

    /// Resume the compensation a stuck saga was parked at.
    ///
    /// The progress turns from `Progress::Stuck(step)` back to
    /// `Progress::Compensating(step)`; the next [`Saga::next_action`]
    /// issues a fresh attempt for that step. The stuck settlement stays the
    /// last settlement, so a repeat of it is still the idempotent
    /// [`Resolution::AlreadyResolved`] and changes nothing; an attempt of
    /// the parked compensation can never resolve the new one. Nothing is
    /// issued here, and a snapshot of a stuck saga, saved and restored,
    /// resumes the same way.
    ///
    /// # Errors
    /// [`Refusal::NotStuck`] unless the saga is [`Progress::Stuck`], a
    /// saga already resumed included.
    pub const fn unstick(&mut self) -> Result<(), Refusal> {
        match self.progress {
            Progress::Stuck(step) => {
                self.progress = Progress::Compensating(step);
                Ok(())
            }
            Progress::Forward(_)
            | Progress::Compensating(_)
            | Progress::Complete
            | Progress::Aborted => Err(Refusal::NotStuck),
        }
    }

    /// Record the known outcome of the step the saga is on, without a
    /// fresh action: the way to settle in-flight work after a restart.
    ///
    /// A snapshot never holds the pending action, so a restored saga cannot
    /// be told what happened to work that was running when the process died.
    /// The caller finds out in the real world (the effect is idempotent by
    /// its [`ActionKey`]) and reports it here as `outcome` of `kind` work on
    /// `step`. The call is [`Saga::next_action`] followed by
    /// [`Saga::resolve`] of that action, in one honoured step: the pending
    /// action is the one settled if there is one, otherwise a fresh attempt
    /// is issued and settled at once. Either way the progress advances as
    /// resolving it does, [`Outcome::Skipped`] and [`Outcome::Stuck`]
    /// included, and the settled action is returned.
    ///
    /// Only the work the progress points at can be in flight: `kind` is
    /// [`Kind::Forward`] on `Progress::Forward(step)` and
    /// [`Kind::Compensate`] on `Progress::Compensating(step)`. Nothing
    /// can be in flight on a saga that is complete, aborted or stuck.
    ///
    /// # Errors
    /// In this order: [`Refusal::Inapplicable`] for an outcome that cannot
    /// answer `kind`; [`Refusal::Concluded`] for a complete, aborted or
    /// stuck saga; [`Refusal::Mismatch`] when `step` and `kind` are not the
    /// work the progress points at; [`Refusal::Exhausted`] when a fresh
    /// attempt is needed and every attempt of the scope has been issued.
    pub fn reconcile(
        &mut self,
        step: usize,
        kind: Kind,
        outcome: Outcome,
    ) -> Result<Action, Refusal> {
        if !outcome.applies_to(kind) {
            return Err(Refusal::Inapplicable);
        }
        match (self.progress, kind) {
            (Progress::Forward(at), Kind::Forward)
            | (Progress::Compensating(at), Kind::Compensate)
                if at == step => {}
            (Progress::Forward(_) | Progress::Compensating(_), _) => return Err(Refusal::Mismatch),
            (Progress::Complete | Progress::Aborted | Progress::Stuck(_), _) => {
                return Err(Refusal::Concluded);
            }
        }
        let mut issuing = self.clone();
        let Some(action) = issuing.next_action()? else {
            return Err(Refusal::Concluded);
        };
        issuing.resolve(action, outcome)?;
        *self = issuing;
        Ok(action)
    }
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
