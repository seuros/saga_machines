//! Every refusal, each proved to change nothing.
use periscope_machines::SEQUENCE_MAX;

use super::*;

#[test]
fn exhaustion_is_a_refusal_and_not_the_terminal_answer() {
    let snapshot = Snapshot {
        id: ID,
        scope: 3,
        progress: Progress::Forward(0),
        issued: SEQUENCE_MAX - 1,
        settled: None,
    };
    let mut saga = Saga::<1>::restored(snapshot, 3).expect("sound");
    let last = next(&mut saga);
    assert_eq!(last.attempt().raw(), (3 << 48) | SEQUENCE_MAX);
    // The pending action is still answered: the repeat issues nothing.
    repeated(&mut saga, &Some(last), Saga::next_action);
    assert_eq!(
        saga.resolve(last, Outcome::Failure),
        Ok(Resolution::Resolved)
    );
    assert_eq!(saga.next_action(), Ok(None));

    let mut stuck = Saga::<2>::restored(snapshot, 3).expect("sound");
    run(&mut stuck, Outcome::Success);
    assert_eq!(stuck.progress(), Progress::Forward(1));
    refused(&mut stuck, Refusal::Exhausted, Saga::next_action);
    // Cancelling is still possible; the compensation then cannot be issued.
    assert_eq!(stuck.cancel(), Ok(Cancellation::Cancelled));
    refused(&mut stuck, Refusal::Exhausted, Saga::next_action);
}

#[test]
fn an_attempt_the_saga_never_issued_is_stale() {
    let mut near = Saga::<2>::with_scope(ID, 1);
    let mut far = Saga::<2>::with_scope(ID, 2);
    let from_near = next(&mut near);
    let from_far = next(&mut far);
    // Same saga id, same key, same sequence: only the scope tells them apart.
    assert_eq!(from_near.key(), from_far.key());
    refused(&mut far, Refusal::Stale, |saga| {
        saga.resolve(from_near, Outcome::Success)
    });
    refused(&mut near, Refusal::Stale, |saga| {
        saga.resolve(from_far, Outcome::Success)
    });
    for raw in [0, (1 << 48) | 2, 1 << 48, u64::MAX] {
        let forged = Action::new(from_near.key(), AttemptId::from_raw(raw));
        refused(&mut near, Refusal::Stale, |saga| {
            saga.resolve(forged, Outcome::Success)
        });
    }
    assert_eq!(
        near.resolve(from_near, Outcome::Success),
        Ok(Resolution::Resolved)
    );
}

#[test]
fn an_attempt_that_is_neither_pending_nor_last_settled_is_stale() {
    let mut saga = Saga::<3>::new(ID);
    let first = run(&mut saga, Outcome::Success);
    let second = run(&mut saga, Outcome::Failure);
    for outcome in [Outcome::Success, Outcome::Failure] {
        refused(&mut saga, Refusal::Stale, |saga| {
            saga.resolve(first, outcome)
        });
    }
    let undo = run(&mut saga, Outcome::Failure);
    let retry = next(&mut saga);
    assert_eq!(retry.key(), undo.key());
    refused(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(second, Outcome::Failure)
    });
    // The failed compensation is the last settlement, its retry the pending one.
    let again = |saga: &mut Saga<3>| saga.resolve(undo, Outcome::Failure);
    repeated(&mut saga, &Resolution::AlreadyResolved, again);
    refused(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(undo, Outcome::Success)
    });
}

#[test]
fn a_contradicting_report_is_a_mismatch() {
    let mut saga = Saga::<2>::new(ID);
    let pending = next(&mut saga);
    // The pending attempt under a key it was not issued for.
    let keys = [
        ActionKey::new(ID, 1, Kind::Forward),
        ActionKey::new(ID, 0, Kind::Compensate),
        ActionKey::new(SagaId::from_raw(8), 0, Kind::Forward),
    ];
    for key in keys {
        let forged = Action::new(key, pending.attempt());
        refused(&mut saga, Refusal::Mismatch, |saga| {
            saga.resolve(forged, Outcome::Success)
        });
    }
    assert_eq!(
        saga.resolve(pending, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    // The last settlement with another outcome, or under another key.
    refused(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(pending, Outcome::Failure)
    });
    let forged = Action::new(keys[0], pending.attempt());
    refused(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(forged, Outcome::Success)
    });
    assert_eq!(saga.progress(), Progress::Forward(1));
}

#[test]
fn cancel_is_refused_while_a_forward_action_is_pending_and_once_complete() {
    let mut saga = Saga::<1>::new(ID);
    let action = next(&mut saga);
    refused(&mut saga, Refusal::Busy, Saga::cancel);
    assert_eq!(
        saga.resolve(action, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    refused(&mut saga, Refusal::Concluded, Saga::cancel);
    refused(&mut Saga::<0>::new(ID), Refusal::Concluded, Saga::cancel);
}
