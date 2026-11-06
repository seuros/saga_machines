#![allow(clippy::expect_used)]
use super::*;
#[test]
fn failed_forward_steps_compensate_in_reverse_and_retries_reject_stale_replies() {
    let mut saga = Saga::<3>::new(7);
    let first = saga.next_action().expect("step");
    assert!(!saga.cancel());
    assert!(saga.resolve(first, Outcome::Success));
    let second = saga.next_action().expect("step");
    assert!(saga.resolve(second, Outcome::Success));
    let third = saga.next_action().expect("step");
    assert!(saga.resolve(third, Outcome::Failure));
    let undo = saga.next_action().expect("compensation");
    assert_eq!((undo.step(), undo.kind()), (1, Kind::Compensate));
    assert!(saga.resolve(undo, Outcome::Failure));
    let retry = saga.next_action().expect("retry");
    assert_eq!(retry.key(), undo.key());
    assert_ne!(retry, undo);
    assert!(!saga.resolve(undo, Outcome::Success));
    assert!(saga.resolve(retry, Outcome::Success));
    let undo_first = saga.next_action().expect("compensation");
    assert_eq!(undo_first.step(), 0);
    assert!(saga.resolve(undo_first, Outcome::Success));
    assert_eq!(saga.progress(), Progress::Aborted);
    assert_eq!(saga.next_action(), None);
}
#[test]
fn cancellation_and_idempotency_have_explicit_boundaries() {
    let mut saga = Saga::<2>::new(8);
    let action = saga.next_action().expect("step");
    let mut ledger = Dedup::<ActionKey, 2>::new();
    let Claim::Acquired(claim) = action.claim(&mut ledger, Duration::ZERO, Duration::from_secs(1))
    else {
        panic!("claim");
    };
    assert_eq!(
        action.claim(&mut ledger, Duration::ZERO, Duration::from_secs(1)),
        Claim::InProgress
    );
    assert!(ledger.complete(claim, Duration::ZERO));
    assert!(saga.resolve(action, Outcome::Success));
    assert!(!saga.resolve(action, Outcome::Success));
    assert!(saga.cancel());
    assert_eq!(saga.progress(), Progress::Compensating(0));
    assert_eq!(Saga::<0>::new(0).progress(), Progress::Complete);
}
