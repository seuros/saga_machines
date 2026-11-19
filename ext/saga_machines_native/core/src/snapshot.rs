use periscope_machines::{Issuer, sequence_of};

use crate::{Action, Kind, Outcome, Progress, SagaId};

/// The last action a saga resolved, with the outcome it was resolved with.
///
/// It is all the history a saga keeps: enough to honour a repeated
/// [`Saga::resolve`](crate::Saga::resolve) of that one action and to refuse a
/// contradicting one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Snapshot {
    pub(crate) id: SagaId,
    pub(crate) scope: u16,
    pub(crate) progress: Progress,
    pub(crate) issued: u64,
    pub(crate) skipped: u64,
    pub(crate) settled: Option<Settlement>,
}

impl Snapshot {
    /// Build saved state, checking what holds whatever the number of steps.
    ///
    /// `skipped` has bit `i` set when forward step `i` was skipped.
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
        skipped: u64,
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
            skipped,
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

    /// The forward steps that were skipped: bit `i` is set for step `i`.
    #[must_use]
    pub const fn skipped(&self) -> u64 {
        self.skipped
    }

    /// The last action resolved and its outcome, if any.
    #[must_use]
    pub const fn settled(&self) -> Option<Settlement> {
        self.settled
    }
}

/// A snapshot as stored, before [`Snapshot::new`] checks it.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
#[serde(rename = "Snapshot", deny_unknown_fields)]
struct Stored {
    id: SagaId,
    scope: u16,
    progress: Progress,
    issued: u64,
    skipped: u64,
    settled: Option<Settlement>,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Snapshot {
    /// The stored fields, checked as [`Snapshot::new`] checks them; what
    /// depends on the number of steps is left to
    /// [`Saga::restored`](crate::Saga::restored).
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = Stored::deserialize(deserializer)?;
        Self::new(
            stored.id,
            stored.scope,
            stored.progress,
            stored.issued,
            stored.skipped,
            stored.settled,
        )
        .ok_or_else(|| serde::de::Error::custom("a snapshot no saga of this id and scope can save"))
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

/// The bit of step `step` in a skipped set; zero past the 64 steps it holds.
const fn bit(step: usize) -> u64 {
    if step < 64 { 1 << step } else { 0 }
}

/// The bits of steps `0..steps`.
const fn below(steps: usize) -> u64 {
    if steps < 64 {
        (1 << steps) - 1
    } else {
        u64::MAX
    }
}

/// Unwinding from forward step `step`: compensate the nearest step before it
/// that was not skipped, or abort when there is none.
pub const fn unwound(step: usize, skipped: u64) -> Progress {
    let mut next = step;
    while let Some(previous) = next.checked_sub(1) {
        if skipped & bit(previous) == 0 {
            return Progress::Compensating(previous);
        }
        next = previous;
    }
    Progress::Aborted
}

/// The skipped set once `settlement` is recorded.
pub const fn skipped_after(skipped: u64, settlement: Settlement) -> u64 {
    match (settlement.action.kind(), settlement.outcome) {
        (Kind::Forward, Outcome::Skipped) => skipped | bit(settlement.action.step()),
        _ => skipped,
    }
}

/// Where a saga of `N` steps stands right after `settlement`, with `skipped`
/// the set it left; `None` for an outcome that cannot answer the action.
pub const fn advanced<const N: usize>(settlement: Settlement, skipped: u64) -> Option<Progress> {
    // Only an action this saga issued, or one `soonest` placed in the
    // workflow, is settled: `step < N`, so `step + 1` cannot overflow.
    let step = settlement.action.step();
    Some(match (settlement.action.kind(), settlement.outcome) {
        (Kind::Forward, Outcome::Success | Outcome::Skipped) if step + 1 == N => Progress::Complete,
        (Kind::Forward, Outcome::Success | Outcome::Skipped) => Progress::Forward(step + 1),
        (Kind::Forward, Outcome::Failure) | (Kind::Compensate, Outcome::Success) => {
            unwound(step, skipped)
        }
        (Kind::Compensate, Outcome::Failure) => Progress::Compensating(step),
        (Kind::Compensate, Outcome::Stuck) => Progress::Stuck(step),
        (Kind::Forward, Outcome::Stuck) | (Kind::Compensate, Outcome::Skipped) => return None,
    })
}

/// Whether `skipped` is the set a saga of `N` steps can hold right after it
/// settled `settlement`: the settlement's own step is skipped exactly when it
/// says so, and nothing is skipped beyond what a forward settlement reached
/// or, for a compensation, beyond the steps a failure could have unwound.
const fn coherent<const N: usize>(settlement: Settlement, skipped: u64) -> bool {
    let step = settlement.action.step();
    match settlement.action.kind() {
        Kind::Forward => {
            let skips = matches!(settlement.outcome, Outcome::Skipped);
            (skipped & bit(step) != 0) == skips && skipped & !below(step + 1) == 0
        }
        Kind::Compensate => skipped & bit(step) == 0 && skipped & !below(N.saturating_sub(1)) == 0,
    }
}

/// Fewest attempts a saga of `N` steps has issued when it settles `action`,
/// that attempt included, or `None` when the action is not in the workflow.
///
/// Every step before a forward one succeeded once: `step` attempts, then
/// this one. A compensation of `step` follows at least the forward successes
/// of `0..=step` and a cancel before step `step + 1`, which therefore exists;
/// a forward failure there, or a compensation above, only adds attempts. A
/// skipped step took its forward attempt too, so the highest skipped step,
/// if above `step`, raises the floor the same way.
/// There is no upper bound: a retry, or an attempt lost to a restart, takes a
/// number and settles nothing.
const fn soonest<const N: usize>(action: Action, skipped: u64) -> Option<u64> {
    let step = action.step();
    // Each arm's guard keeps `step` below `N`, or below `N - 1` for `+ 2`.
    let attempts = match action.kind() {
        Kind::Forward if step < N => step + 1,
        Kind::Compensate if step < N.saturating_sub(1) => {
            // Forward attempts up to the highest of `step` and a skipped step.
            let reached = skipped.bit_width() as usize;
            (if reached > step + 1 {
                reached
            } else {
                step + 1
            }) + 1
        }
        Kind::Forward | Kind::Compensate => return None,
    };
    Some(attempts as u64)
}

/// Whether a saga standing at `progress` can still issue an attempt.
const fn issuing(progress: Progress) -> bool {
    match progress {
        Progress::Forward(_) | Progress::Compensating(_) => true,
        Progress::Complete | Progress::Aborted | Progress::Stuck(_) => false,
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
    if snapshot.skipped & !below(N) != 0 {
        return false;
    }
    let (after, settled_at) = match snapshot.settled {
        None if snapshot.skipped != 0 => return false,
        None => (initial::<N>(), 0),
        Some(settled) => {
            let action = settled.action;
            let sequence = sequence_of(action.attempt().raw());
            let Some(soonest) = soonest::<N>(action, snapshot.skipped) else {
                return false;
            };
            let ours = action.key().saga() == snapshot.id
                && attempts.owns(action.attempt().raw())
                && sequence >= soonest;
            if !ours || !coherent::<N>(settled, snapshot.skipped) {
                return false;
            }
            let Some(after) = advanced::<N>(settled, snapshot.skipped) else {
                return false;
            };
            (after, sequence)
        }
    };
    if snapshot.progress == after {
        return issuing(after) || snapshot.issued == settled_at;
    }
    // Reached by a cancel after the settlement. A cancel is honoured only
    // while going forward, so attempts may have been issued, and lost to a
    // restart, before it, even when it aborts the saga outright.
    match after {
        Progress::Forward(step) => snapshot.progress == unwound(step, snapshot.skipped),
        Progress::Compensating(_) | Progress::Complete | Progress::Aborted | Progress::Stuck(_) => {
            false
        }
    }
}
