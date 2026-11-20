//! `unstick`: a parked compensation is retried under a fresh attempt, also
//! from a snapshot that sat in storage.
use super::*;

/// Three steps done, step 2 failed, compensation of step 1 stuck.
fn parked() -> (Saga<3>, Action) {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    let undo = run(&mut saga, Outcome::Stuck);
    assert_eq!(saga.progress(), Progress::Stuck(1));
    (saga, undo)
}

#[test]
fn unstick_resumes_the_compensation_of_the_stuck_step() {
    let (mut saga, undo) = parked();
    let issued = saga.issued();
    assert_eq!(saga.unstick(), Ok(()));
    assert_eq!(saga.progress(), Progress::Compensating(1));
    // Nothing is issued by the call itself, and the settlement stays.
    assert_eq!((saga.issued(), saga.pending()), (issued, None));
    let outcome = Outcome::Stuck;
    let settled = Some(Settlement {
        action: undo,
        outcome,
    });
    assert_eq!(saga.settled(), settled);
    let retry = next(&mut saga);
    assert_eq!(retry.key(), undo.key());
    assert_ne!(retry.attempt(), undo.attempt());
    assert_eq!(retry.attempt().raw(), issued + 1);
    assert_eq!(
        saga.resolve(retry, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    assert_eq!(saga.progress(), Progress::Compensating(0));
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.progress(), Progress::Aborted);
}

#[test]
fn a_retried_compensation_can_fail_or_get_stuck_again() {
    let (mut saga, _) = parked();
    for _ in 0..3 {
        assert_eq!(saga.unstick(), Ok(()));
        run(&mut saga, Outcome::Failure);
        assert_eq!(saga.progress(), Progress::Compensating(1));
        run(&mut saga, Outcome::Stuck);
        assert_eq!(saga.progress(), Progress::Stuck(1));
    }
    assert_eq!(saga.unstick(), Ok(()));
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.progress(), Progress::Compensating(0));
}

#[test]
fn a_saga_that_is_not_stuck_refuses_unstick() {
    let mut fresh = Saga::<2>::new(ID);
    refuses(&mut fresh, Refusal::NotStuck, Saga::unstick);
    next(&mut fresh);
    refuses(&mut fresh, Refusal::NotStuck, Saga::unstick);
    run(&mut fresh, Outcome::Success);
    run(&mut fresh, Outcome::Success);
    assert_eq!(fresh.progress(), Progress::Complete);
    refuses(&mut fresh, Refusal::NotStuck, Saga::unstick);

    let mut unwinding = Saga::<2>::new(ID);
    run(&mut unwinding, Outcome::Success);
    assert_eq!(unwinding.cancel(), Ok(Cancellation::Cancelled));
    refuses(&mut unwinding, Refusal::NotStuck, Saga::unstick);
    run(&mut unwinding, Outcome::Success);
    assert_eq!(unwinding.progress(), Progress::Aborted);
    refuses(&mut unwinding, Refusal::NotStuck, Saga::unstick);

    // A saga already resumed is not stuck either.
    let (mut saga, _) = parked();
    assert_eq!(saga.unstick(), Ok(()));
    refuses(&mut saga, Refusal::NotStuck, Saga::unstick);
}

#[test]
fn replies_for_the_stuck_attempt_never_resolve_the_retry() {
    let (mut saga, undo) = parked();
    assert_eq!(saga.unstick(), Ok(()));
    let retry = next(&mut saga);
    // The old attempt is not the pending one: it is only the last settlement,
    // so its repeat is idempotent and anything else is refused.
    repeated(&mut saga, &Resolution::AlreadyResolved, |saga| {
        saga.resolve(undo, Outcome::Stuck)
    });
    refuses(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(undo, Outcome::Success)
    });
    assert_eq!(saga.pending(), Some(retry));
    assert_eq!(saga.progress(), Progress::Compensating(1));
    // Once the retry settles, the old attempt is stale for good.
    assert_eq!(
        saga.resolve(retry, Outcome::Failure),
        Ok(Resolution::Resolved)
    );
    refuses(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(undo, Outcome::Stuck)
    });
    refuses(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(undo, Outcome::Success)
    });
}

#[test]
fn a_resumed_saga_unwinds_on_cancel_as_already_unwinding() {
    let (mut saga, _) = parked();
    refuses(&mut saga, Refusal::Concluded, Saga::cancel);
    assert_eq!(saga.unstick(), Ok(()));
    repeated(&mut saga, &Cancellation::AlreadyUnwinding, Saga::cancel);
}

#[test]
fn a_skipped_step_is_still_passed_over_after_unstick() {
    let mut saga = Saga::<4>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Skipped);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    assert_eq!(saga.progress(), Progress::Compensating(2));
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Stuck);
    assert_eq!(saga.progress(), Progress::Stuck(0));
    assert_eq!(saga.unstick(), Ok(()));
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.progress(), Progress::Aborted);
    assert_eq!(saga.skipped(), 0b0010);
}

#[test]
fn a_snapshot_with_a_stuck_progress_restores_and_unsticks() {
    let (saga, undo) = parked();
    let snapshot = saga.snapshot();
    let mut restored = Saga::<3>::restored(snapshot, 0).expect("sound");
    assert_eq!(restored, saga);
    assert_eq!(restored.unstick(), Ok(()));
    // The resumed state is itself saved and restored, before any attempt.
    let saved = restored.snapshot();
    assert_eq!(saved.progress(), Progress::Compensating(1));
    let mut again = Saga::<3>::restored(saved, 0).expect("sound");
    assert_eq!(again, restored);
    // And after an attempt was issued and lost.
    next(&mut again);
    let lost = Saga::<3>::restored(again.snapshot(), 0).expect("sound");
    assert_eq!(lost.issued(), undo.attempt().raw() + 1);
    let mut lost = lost;
    let retry = next(&mut lost);
    assert_eq!(retry.attempt().raw(), undo.attempt().raw() + 2);
    refuses(&mut lost, Refusal::NotStuck, Saga::unstick);
}

#[test]
fn restoring_refuses_a_resumed_state_no_saga_reaches() {
    use Kind::{Compensate, Forward as Fwd};
    use Outcome::{Failure, Stuck, Success};
    let stuck = Some((1, Compensate, Stuck, 4));
    let comp = |step| Progress::Compensating(step);
    // Compensating the stuck step, with or without attempts lost since.
    for issued in [4, 5, 9] {
        assert!(Saga::<3>::restored(saved(comp(1), issued, stuck), 0).is_some());
    }
    // Resumed at another step, or from another outcome, is not unstick.
    assert_eq!(Saga::<3>::restored(saved(comp(0), 4, stuck), 0), None);
    let failed = Some((1, Compensate, Failure, 4));
    assert!(Saga::<3>::restored(saved(comp(1), 4, failed), 0).is_some());
    let forward = Some((0, Fwd, Success, 1));
    assert_eq!(Saga::<3>::restored(saved(comp(1), 4, forward), 0), None);
    // A stuck state still cannot have issued anything after the settlement.
    assert_eq!(
        Saga::<3>::restored(saved(Progress::Stuck(1), 5, stuck), 0),
        None
    );
}
