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
#[test]
fn restored_progress_continues_forward_and_rejects_pre_restart_replies() {
    let mut saga = Saga::<3>::new(SagaId::from_bytes(b"manifest-1").raw());
    let first = saga.next_action().expect("step");
    assert!(saga.resolve(first, Outcome::Success));
    let lost = saga.next_action().expect("step");
    let snapshot = saga.snapshot();
    assert_eq!(snapshot.progress, Progress::Forward(1));
    let mut restored = Saga::<3>::restore(snapshot).expect("restore");
    assert!(!restored.resolve(lost, Outcome::Success));
    let reissued = restored.next_action().expect("step");
    assert_eq!(reissued.key(), lost.key());
    assert_ne!(reissued, lost);
    assert!(!restored.resolve(lost, Outcome::Success));
    assert!(restored.resolve(reissued, Outcome::Success));
    let third = restored.next_action().expect("step");
    assert!(restored.resolve(third, Outcome::Success));
    assert_eq!(restored.progress(), Progress::Complete);
    assert_eq!(restored.snapshot().attempt, 4);
}
#[test]
fn restored_progress_compensates_in_reverse() {
    let mut saga = Saga::<3>::new(5);
    for _ in 0..2 {
        let action = saga.next_action().expect("step");
        assert!(saga.resolve(action, Outcome::Success));
    }
    let failed = saga.next_action().expect("step");
    assert!(saga.resolve(failed, Outcome::Failure));
    let mut restored = Saga::<3>::restore(saga.snapshot()).expect("restore");
    assert_eq!(restored.progress(), Progress::Compensating(1));
    assert!(!restored.cancel());
    let undo = restored.next_action().expect("compensation");
    assert_eq!((undo.step(), undo.kind()), (1, Kind::Compensate));
    assert!(restored.resolve(undo, Outcome::Success));
    let undo_first = restored.next_action().expect("compensation");
    assert_eq!(
        (undo_first.step(), undo_first.kind()),
        (0, Kind::Compensate)
    );
    assert!(restored.resolve(undo_first, Outcome::Success));
    assert_eq!(restored.progress(), Progress::Aborted);
    assert_eq!(
        Saga::<3>::restore(restored.snapshot()).map(|s| s.progress()),
        Some(Progress::Aborted)
    );
}
#[test]
fn restore_rejects_progress_outside_the_workflow() {
    let snapshot = |progress, attempt| Snapshot {
        id: 1,
        progress,
        attempt,
    };
    assert!(Saga::<2>::restore(snapshot(Progress::Forward(2), 2)).is_none());
    assert!(Saga::<2>::restore(snapshot(Progress::Compensating(1), 2)).is_none());
    assert!(Saga::<2>::restore(snapshot(Progress::Forward(1), 0)).is_none());
    assert!(Saga::<2>::restore(snapshot(Progress::Compensating(0), 0)).is_none());
    assert!(Saga::<2>::restore(snapshot(Progress::Complete, 1)).is_none());
    assert!(Saga::<0>::restore(snapshot(Progress::Forward(0), 0)).is_none());
    assert!(Saga::<0>::restore(snapshot(Progress::Complete, 0)).is_some());
    assert!(Saga::<2>::restore(snapshot(Progress::Compensating(0), 1)).is_some());
    assert!(Saga::<2>::restore(snapshot(Progress::Aborted, 0)).is_some());
}
#[test]
fn string_identities_hash_deterministically_and_keep_action_keys_stable() {
    let id = SagaId::from_bytes(b"manifest-42");
    assert_eq!(id, SagaId::from_bytes(b"manifest-42"));
    assert_ne!(id, SagaId::from_bytes(b"manifest-43"));
    assert_eq!(SagaId::from_raw(id.raw()), id);
    assert_eq!(u64::from(id), id.raw());
    let mut before = Saga::<1>::new(id.raw());
    let mut after = Saga::<1>::restore(before.snapshot()).expect("restore");
    let key = before.next_action().expect("step").key();
    assert_eq!(after.next_action().expect("step").key(), key);
    let mut ledger = Dedup::<ActionKey, 1>::new();
    let Claim::Acquired(claim) = ledger.claim(key, Duration::ZERO, Duration::from_secs(1)) else {
        panic!("claim");
    };
    assert!(ledger.complete(claim, Duration::ZERO));
    assert_eq!(
        after.next_action().expect("step").claim(
            &mut ledger,
            Duration::ZERO,
            Duration::from_secs(1)
        ),
        Claim::Completed
    );
}
