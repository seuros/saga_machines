use core::time::Duration;

use super::*;

mod ledger;
mod model;
mod reachable;
mod refusals;
mod restore;

const ID: SagaId = SagaId::from_raw(7);

/// An attempt id from its integer form; panics on a sequence of zero.
fn attempt(raw: u64) -> AttemptId {
    AttemptId::from_raw(raw).expect("a sequence of at least one")
}

/// A snapshot of a saga in scope zero, settled by `(step, kind, outcome,
/// attempt)`, built field by field: unsound ones included.
fn saved(
    progress: Progress,
    issued: u64,
    settled: Option<(usize, Kind, Outcome, u64)>,
) -> Snapshot {
    let settled = settled.map(|(step, kind, outcome, raw)| Settlement {
        action: Action::new(ActionKey::new(ID, step, kind), attempt(raw)),
        outcome,
    });
    Snapshot {
        id: ID,
        scope: 0,
        progress,
        issued,
        settled,
    }
}

/// Issue the next action; panics when the saga is finished or exhausted.
fn next<const N: usize>(saga: &mut Saga<N>) -> Action {
    saga.next_action()
        .expect("not exhausted")
        .expect("work remains")
}

/// Issue the next action and resolve it with `outcome`.
fn run<const N: usize>(saga: &mut Saga<N>, outcome: Outcome) -> Action {
    let action = next(saga);
    assert_eq!(saga.resolve(action, outcome), Ok(Resolution::Resolved));
    action
}

/// The call is refused with `refusal` and leaves the saga as it was.
fn refused<const N: usize, T: core::fmt::Debug + PartialEq>(
    saga: &mut Saga<N>,
    refusal: Refusal,
    call: impl FnOnce(&mut Saga<N>) -> Result<T, Refusal>,
) {
    let before = saga.clone();
    assert_eq!(call(saga), Err(refusal));
    assert_eq!(*saga, before);
}

/// The call is honoured with `answer` and leaves the saga as it was.
fn repeated<const N: usize, T: core::fmt::Debug + PartialEq>(
    saga: &mut Saga<N>,
    answer: &T,
    call: impl FnOnce(&mut Saga<N>) -> Result<T, Refusal>,
) {
    let before = saga.clone();
    assert_eq!(call(saga).as_ref(), Ok(answer));
    assert_eq!(*saga, before);
}

#[test]
fn forward_steps_complete_in_order() {
    let mut saga = Saga::<3>::new(ID);
    assert_eq!((saga.id(), saga.scope(), saga.issued()), (ID, 0, 0));
    assert_eq!((saga.pending(), saga.settled()), (None, None));
    for step in 0..3 {
        assert_eq!(saga.progress(), Progress::Forward(step));
        let action = next(&mut saga);
        assert_eq!(action.key(), ActionKey::new(ID, step, Kind::Forward));
        assert_eq!((action.step(), action.kind()), (step, Kind::Forward));
        assert_eq!(action.attempt().raw(), step as u64 + 1);
        assert_eq!(saga.pending(), Some(action));
        assert_eq!(
            saga.resolve(action, Outcome::Success),
            Ok(Resolution::Resolved)
        );
        let outcome = Outcome::Success;
        assert_eq!(saga.settled(), Some(Settlement { action, outcome }));
        assert_eq!(saga.pending(), None);
    }
    assert_eq!(saga.progress(), Progress::Complete);
    assert_eq!(saga.next_action(), Ok(None));
    assert_eq!(saga.issued(), 3);
    assert_eq!(Saga::<0>::new(ID).progress(), Progress::Complete);
    assert_eq!(Saga::<0>::new(ID).next_action(), Ok(None));
}

#[test]
fn a_forward_failure_compensates_in_reverse_and_a_failed_compensation_is_retried() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Success);
    let failed = run(&mut saga, Outcome::Failure);
    assert_eq!((failed.step(), failed.kind()), (2, Kind::Forward));
    assert_eq!(saga.progress(), Progress::Compensating(1));
    let undo = run(&mut saga, Outcome::Failure);
    assert_eq!((undo.step(), undo.kind()), (1, Kind::Compensate));
    assert_eq!(saga.progress(), Progress::Compensating(1));
    let retry = next(&mut saga);
    assert_eq!(retry.key(), undo.key());
    assert_ne!(retry.attempt(), undo.attempt());
    assert_eq!(
        saga.resolve(retry, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    let last = run(&mut saga, Outcome::Success);
    assert_eq!((last.step(), last.kind()), (0, Kind::Compensate));
    assert_eq!(saga.progress(), Progress::Aborted);
    assert_eq!(saga.next_action(), Ok(None));
}

#[test]
fn next_action_repeats_the_pending_action_without_issuing() {
    let mut saga = Saga::<2>::new(ID);
    let action = next(&mut saga);
    repeated(&mut saga, &Some(action), Saga::next_action);
    assert_eq!(saga.issued(), 1);
}

#[test]
fn cancel_unwinds_between_forward_actions() {
    let mut fresh = Saga::<1>::new(ID);
    assert_eq!(fresh.cancel(), Ok(Cancellation::Cancelled));
    assert_eq!(fresh.progress(), Progress::Aborted);
    repeated(&mut fresh, &Cancellation::AlreadyUnwinding, Saga::cancel);

    let mut saga = Saga::<2>::new(ID);
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.cancel(), Ok(Cancellation::Cancelled));
    assert_eq!(saga.progress(), Progress::Compensating(0));
    repeated(&mut saga, &Cancellation::AlreadyUnwinding, Saga::cancel);
    // A pending compensation does not make the repeat a lie.
    let undo = next(&mut saga);
    repeated(&mut saga, &Cancellation::AlreadyUnwinding, Saga::cancel);
    assert_eq!(
        saga.resolve(undo, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    assert_eq!(saga.progress(), Progress::Aborted);
}

#[test]
fn a_repeated_resolve_of_the_last_settlement_is_honoured() {
    let mut saga = Saga::<3>::new(ID);
    let first = run(&mut saga, Outcome::Success);
    let again = |saga: &mut Saga<3>| saga.resolve(first, Outcome::Success);
    repeated(&mut saga, &Resolution::AlreadyResolved, again);
    // Still the last settlement while a newer action is pending.
    let second = next(&mut saga);
    repeated(&mut saga, &Resolution::AlreadyResolved, again);
    // And after a cancel.
    assert_eq!(
        saga.resolve(second, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    assert_eq!(saga.cancel(), Ok(Cancellation::Cancelled));
    let again = |saga: &mut Saga<3>| saga.resolve(second, Outcome::Success);
    repeated(&mut saga, &Resolution::AlreadyResolved, again);
    // `first` is no longer the last settlement: no history, so not provable.
    refused(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(first, Outcome::Success)
    });
}

#[test]
fn identities_hash_deterministically_and_rebuild_from_parts() {
    let id = SagaId::from_bytes(b"manifest-42");
    assert_eq!(id, SagaId::from_bytes(b"manifest-42"));
    assert_ne!(id, SagaId::from_bytes(b"manifest-43"));
    assert_eq!(SagaId::from_raw(id.raw()), id);
    let mut saga = Saga::<1>::with_scope(id, 5);
    let action = next(&mut saga);
    assert_eq!(action.key().saga(), id);
    assert_eq!(
        (action.attempt().scope(), action.attempt().raw()),
        (5, (5 << 48) | 1)
    );
    // An action crosses a queue as plain data and is rebuilt on the way back.
    let key = ActionKey::new(SagaId::from_raw(id.raw()), action.step(), action.kind());
    let rebuilt = Action::new(key, attempt(action.attempt().raw()));
    assert_eq!(
        saga.resolve(rebuilt, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    // No saga issues a sequence of zero, in any scope, so no id carries one.
    for raw in [0, 1 << 48, u64::from(u16::MAX) << 48] {
        assert_eq!(AttemptId::from_raw(raw), None);
    }
    for raw in [1, (7 << 48) | 1, u64::MAX] {
        assert_eq!(AttemptId::from_raw(raw).map(AttemptId::raw), Some(raw));
    }
}

#[test]
fn refusals_format_and_are_errors() {
    use std::string::ToString;
    let all = [
        Refusal::Exhausted,
        Refusal::Stale,
        Refusal::Mismatch,
        Refusal::Busy,
        Refusal::Concluded,
    ];
    for (index, refusal) in all.iter().enumerate() {
        let error: &dyn core::error::Error = refusal;
        assert_ne!(error.to_string(), "");
        for other in &all[index + 1..] {
            assert_ne!(refusal.to_string(), other.to_string());
        }
    }
}
