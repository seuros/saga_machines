use periscope_machines::{Issuer, sequence_of};

use crate::{Action, Kind, Outcome, Progress, SagaId};

/// The last action a saga resolved, with the outcome it was resolved with.
///
/// It is all the history a saga keeps: enough to honour a repeated
/// [`Saga::resolve`](crate::Saga::resolve) of that one action and to refuse a
/// contradicting one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settlement {
    /// The action that was resolved.
    pub action: Action,
    /// The outcome it was resolved with.
    pub outcome: Outcome,
}

/// Persistable saga state, excluding any unresolved issued attempt.
///
/// Data for the caller to store, read back through the accessors and rebuild
/// with [`Snapshot::new`]; what depends on the number of steps is checked by
/// [`Saga::restored`](crate::Saga::restored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub(crate) id: SagaId,
    pub(crate) scope: u16,
    pub(crate) progress: Progress,
    pub(crate) issued: u64,
    pub(crate) settled: Option<Settlement>,
}

impl Snapshot {
    /// Build saved state, checking what holds whatever the number of steps.
    ///
    /// `None` when `issued` is beyond 2^48 - 1, or when the settlement is
    /// not one a saga of `id` in `scope` can have recorded: its key names
    /// another saga, or its attempt is of another scope or beyond `issued`.
    /// The rest depends on the number of steps and is checked by
    /// [`Saga::restored`](crate::Saga::restored).
    #[must_use]
    pub fn new(
        id: SagaId,
        scope: u16,
        progress: Progress,
        issued: u64,
        settled: Option<Settlement>,
    ) -> Option<Self> {
        let attempts = Issuer::resumed(scope, issued)?;
        let ours = settled.is_none_or(|settled| {
            settled.action.key().saga() == id && attempts.owns(settled.action.attempt().raw())
        });
        ours.then_some(Self {
            id,
            scope,
            progress,
            issued,
            settled,
        })
    }

    /// Workflow identity.
    #[must_use]
    pub const fn id(&self) -> SagaId {
        self.id
    }

    /// The scope the saga issues attempts in.
    #[must_use]
    pub const fn scope(&self) -> u16 {
        self.scope
    }

    /// Progress at the last honoured call.
    #[must_use]
    pub const fn progress(&self) -> Progress {
        self.progress
    }

    /// Attempts issued so far; a restored saga never reissues them.
    #[must_use]
    pub const fn issued(&self) -> u64 {
        self.issued
    }

    /// The last action resolved and its outcome, if any.
    #[must_use]
    pub const fn settled(&self) -> Option<Settlement> {
        self.settled
    }
}

/// Where a saga of `N` steps starts.
pub const fn initial<const N: usize>() -> Progress {
    if N == 0 {
        Progress::Complete
    } else {
        Progress::Forward(0)
    }
}

/// Unwinding from forward step `step`: compensate the one before it.
pub const fn unwound(step: usize) -> Progress {
    match step.checked_sub(1) {
        Some(previous) => Progress::Compensating(previous),
        None => Progress::Aborted,
    }
}

/// Where a saga of `N` steps stands right after `settlement`.
pub const fn advanced<const N: usize>(settlement: Settlement) -> Progress {
    let step = settlement.action.step();
    match (settlement.action.kind(), settlement.outcome) {
        (Kind::Forward, Outcome::Success) if step + 1 == N => Progress::Complete,
        (Kind::Forward, Outcome::Success) => Progress::Forward(step + 1),
        (Kind::Forward, Outcome::Failure) | (Kind::Compensate, Outcome::Success) => unwound(step),
        (Kind::Compensate, Outcome::Failure) => Progress::Compensating(step),
    }
}

/// Fewest attempts a saga of `N` steps has issued when it settles `action`,
/// that attempt included, or `None` when the action is not in the workflow.
///
/// Every step before a forward one succeeded once: `step` attempts, then
/// this one. A compensation of `step` follows at least the forward successes
/// of `0..=step` and a cancel before step `step + 1`, which therefore exists;
/// a forward failure there, or a compensation above, only adds attempts.
/// There is no upper bound: a retry, or an attempt lost to a restart, takes a
/// number and settles nothing.
const fn soonest<const N: usize>(action: Action) -> Option<u64> {
    let step = action.step();
    let attempts = match action.kind() {
        Kind::Forward if step < N => step + 1,
        Kind::Compensate if step < N.saturating_sub(1) => step + 2,
        Kind::Forward | Kind::Compensate => return None,
    };
    Some(attempts as u64)
}

/// Whether a saga standing at `progress` can still issue an attempt.
const fn issuing(progress: Progress) -> bool {
    match progress {
        Progress::Forward(_) | Progress::Compensating(_) => true,
        Progress::Complete | Progress::Aborted => false,
    }
}

/// Whether `snapshot` is a state a saga of `N` steps issuing from `attempts`
/// can have reached: exactly those, no more and no fewer.
///
/// A saga's history is its last settlement, then perhaps a cancel, with
/// attempts issued in between while work remained. So the progress is where
/// the settlement leads (the start, without one) or the cancel of that; the
/// settled attempt is one of this scope within the count and not sooner than
/// its action allows; and the count is exactly the settled attempt's number
/// when the settlement itself finished the saga, since nothing is issued
/// after that.
pub fn sound<const N: usize>(snapshot: &Snapshot, attempts: &Issuer) -> bool {
    let (after, settled_at) = match snapshot.settled {
        None => (initial::<N>(), 0),
        Some(settled) => {
            let action = settled.action;
            let sequence = sequence_of(action.attempt().raw());
            let Some(soonest) = soonest::<N>(action) else {
                return false;
            };
            let ours = action.key().saga() == snapshot.id
                && attempts.owns(action.attempt().raw())
                && sequence >= soonest;
            if !ours {
                return false;
            }
            (advanced::<N>(settled), sequence)
        }
    };
    if snapshot.progress == after {
        return issuing(after) || snapshot.issued == settled_at;
    }
    // Reached by a cancel after the settlement. A cancel is honoured only
    // while going forward, so attempts may have been issued, and lost to a
    // restart, before it, even when it aborts the saga outright.
    match after {
        Progress::Forward(step) => snapshot.progress == unwound(step),
        Progress::Compensating(_) | Progress::Complete | Progress::Aborted => false,
    }
}
