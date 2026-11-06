#![no_std]
#![doc = include_str!("../README.md")]
use core::time::Duration;
use dedup_machines::{Claim, Dedup};

/// Stable operation identity across retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionKey {
    saga: u64,
    step: usize,
    kind: Kind,
}
/// Direction of a saga action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    /// Perform a forward step.
    Forward,
    /// Undo a known completed step.
    Compensate,
}
/// One issued attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    key: ActionKey,
    attempt: u64,
}
impl Action {
    /// Stable idempotency identity.
    #[must_use]
    pub const fn key(self) -> ActionKey {
        self.key
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
    /// Claim an external exact ledger with a retry-stable key.
    #[must_use]
    pub fn claim<const N: usize>(
        self,
        ledger: &mut Dedup<ActionKey, N>,
        now: Duration,
        ttl: Duration,
    ) -> Claim {
        ledger.claim(self.key, now, ttl)
    }
}
/// Known external effect outcome, never an uncertain timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// The requested effect is complete.
    Success,
    /// The requested effect did not occur.
    Failure,
}
/// Workflow progress, excluding any unresolved issued attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
/// Fixed-step sequential workflow.
#[derive(Debug)]
pub struct Saga<const N: usize> {
    id: u64,
    progress: Progress,
    attempt: u64,
    pending: Option<Action>,
}
impl<const N: usize> Saga<N> {
    /// Start a fresh workflow identity, unique in the dedup namespace.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self {
            id,
            progress: if N == 0 {
                Progress::Complete
            } else {
                Progress::Forward(0)
            },
            attempt: 0,
            pending: None,
        }
    }
    /// Issue the next action, or repeat the same unresolved attempt.
    pub fn next_action(&mut self) -> Option<Action> {
        if let Some(action) = self.pending {
            return Some(action);
        }
        let (step, kind) = match self.progress {
            Progress::Forward(step) => (step, Kind::Forward),
            Progress::Compensating(step) => (step, Kind::Compensate),
            _ => return None,
        };
        let attempt = self.attempt.checked_add(1)?;
        let action = Action {
            key: ActionKey {
                saga: self.id,
                step,
                kind,
            },
            attempt,
        };
        self.attempt = attempt;
        self.pending = Some(action);
        Some(action)
    }
    /// Advance only for the exact current attempt and a known effect outcome.
    #[must_use]
    pub fn resolve(&mut self, action: Action, outcome: Outcome) -> bool {
        if self.pending != Some(action) {
            return false;
        }
        self.pending = None;
        self.progress = match (action.kind(), outcome) {
            (Kind::Forward, Outcome::Success) if action.step() + 1 == N => Progress::Complete,
            (Kind::Forward, Outcome::Success) => Progress::Forward(action.step() + 1),
            (Kind::Forward, Outcome::Failure) | (Kind::Compensate, Outcome::Success) => {
                if action.step() == 0 {
                    Progress::Aborted
                } else {
                    Progress::Compensating(action.step() - 1)
                }
            }
            (Kind::Compensate, Outcome::Failure) => Progress::Compensating(action.step()),
        };
        true
    }
    /// Cancel only between forward actions; never forget unresolved work.
    #[must_use]
    pub const fn cancel(&mut self) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let Progress::Forward(step) = self.progress else {
            return false;
        };
        self.progress = if step == 0 {
            Progress::Aborted
        } else {
            Progress::Compensating(step - 1)
        };
        true
    }
    /// Current workflow progress.
    #[must_use]
    pub const fn progress(&self) -> Progress {
        self.progress
    }
}
#[cfg(test)]
mod tests;
