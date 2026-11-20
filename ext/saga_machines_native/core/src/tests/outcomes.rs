//! `Skipped` and `Stuck`: a skipped step is done without effect and never
//! compensated; a stuck compensation parks the saga until `unstick`.
use super::*;

/// The steps a saga issues as compensations until it stops, each resolved
/// `Success`.
fn compensated<const N: usize>(saga: &mut Saga<N>) -> std::vec::Vec<usize> {
    let mut steps = std::vec::Vec::new();
    while let Ok(Some(action)) = saga.next_action() {
        assert_eq!(action.kind(), Kind::Compensate);
        steps.push(action.step());
        assert_eq!(
            saga.resolve(action, Outcome::Success),
            Ok(Resolution::Resolved)
        );
    }
    steps
}

#[test]
fn a_skipped_forward_step_advances_like_success() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    let skipped = run(&mut saga, Outcome::Skipped);
    assert_eq!((skipped.step(), skipped.kind()), (1, Kind::Forward));
    assert_eq!(saga.progress(), Progress::Forward(2));
    assert_eq!(saga.skipped(), 0b010);
    assert_eq!(saga.snapshot().skipped(), 0b010);
    let outcome = Outcome::Skipped;
    assert_eq!(
        saga.settled(),
        Some(Settlement {
            action: skipped,
            outcome
        })
    );
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.progress(), Progress::Complete);
    assert_eq!(saga.next_action(), Ok(None));
    assert_eq!(saga.issued(), 3);
}

#[test]
fn skipping_the_last_step_completes() {
    let mut saga = Saga::<1>::new(ID);
    run(&mut saga, Outcome::Skipped);
    assert_eq!((saga.progress(), saga.skipped()), (Progress::Complete, 1));
    assert_eq!(saga.cancel(), Err(Refusal::Concluded));
}

#[test]
fn a_skipped_step_is_not_compensated_after_a_failure() {
    let mut saga = Saga::<5>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Skipped);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Skipped);
    run(&mut saga, Outcome::Failure);
    assert_eq!(saga.progress(), Progress::Compensating(2));
    assert_eq!(compensated(&mut saga), [2, 0]);
    assert_eq!(saga.progress(), Progress::Aborted);
    // The unwind does not forget which steps never ran.
    assert_eq!(saga.skipped(), 0b01010);
}

#[test]
fn a_skipped_step_is_not_compensated_after_a_cancel() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Skipped);
    assert_eq!(saga.cancel(), Ok(Cancellation::Cancelled));
    assert_eq!(saga.progress(), Progress::Compensating(0));
    assert_eq!(compensated(&mut saga), [0]);
}

#[test]
fn unwinding_over_nothing_but_skips_aborts_at_once() {
    let mut failing = Saga::<3>::new(ID);
    run(&mut failing, Outcome::Skipped);
    run(&mut failing, Outcome::Skipped);
    run(&mut failing, Outcome::Failure);
    assert_eq!(failing.progress(), Progress::Aborted);
    assert_eq!(failing.next_action(), Ok(None));

    let mut cancelled = Saga::<2>::new(ID);
    run(&mut cancelled, Outcome::Skipped);
    assert_eq!(cancelled.cancel(), Ok(Cancellation::Cancelled));
    assert_eq!(cancelled.progress(), Progress::Aborted);
}

#[test]
fn a_failed_compensation_retries_around_skipped_steps() {
    let mut saga = Saga::<4>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Skipped);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    run(&mut saga, Outcome::Failure);
    assert_eq!(saga.progress(), Progress::Compensating(2));
    assert_eq!(compensated(&mut saga), [2, 0]);
}

#[test]
fn an_outcome_answers_only_the_kind_it_fits() {
    let mut saga = Saga::<2>::new(ID);
    let forward = next(&mut saga);
    refuses(&mut saga, Refusal::Inapplicable, |saga| {
        saga.resolve(forward, Outcome::Stuck)
    });
    assert_eq!(saga.pending(), Some(forward));
    run_resolved(&mut saga, forward, Outcome::Success);
    let failed = run(&mut saga, Outcome::Failure);
    assert_eq!(failed.step(), 1);
    let undo = next(&mut saga);
    refuses(&mut saga, Refusal::Inapplicable, |saga| {
        saga.resolve(undo, Outcome::Skipped)
    });
    assert_eq!(saga.pending(), Some(undo));
    for (kind, outcome, fits) in [
        (Kind::Forward, Outcome::Success, true),
        (Kind::Forward, Outcome::Failure, true),
        (Kind::Forward, Outcome::Skipped, true),
        (Kind::Forward, Outcome::Stuck, false),
        (Kind::Compensate, Outcome::Success, true),
        (Kind::Compensate, Outcome::Failure, true),
        (Kind::Compensate, Outcome::Skipped, false),
        (Kind::Compensate, Outcome::Stuck, true),
    ] {
        assert_eq!(outcome.applies_to(kind), fits, "{kind:?} {outcome:?}");
    }
}

fn run_resolved<const N: usize>(saga: &mut Saga<N>, action: Action, outcome: Outcome) {
    assert_eq!(saga.resolve(action, outcome), Ok(Resolution::Resolved));
}

#[test]
fn a_skipped_reply_is_protected_like_any_other() {
    let mut saga = Saga::<3>::new(ID);
    let first = run(&mut saga, Outcome::Success);
    let skipped = run(&mut saga, Outcome::Skipped);
    // The repeat is honoured; the other outcome contradicts the record.
    repeated(&mut saga, &Resolution::AlreadyResolved, |saga| {
        saga.resolve(skipped, Outcome::Skipped)
    });
    refuses(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(skipped, Outcome::Success)
    });
    // An earlier attempt is no longer provable.
    refuses(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(first, Outcome::Success)
    });
    // Once a newer action settles, the skipped one is stale too.
    run(&mut saga, Outcome::Success);
    refuses(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(skipped, Outcome::Skipped)
    });
    // A saga of another scope never issued it.
    let mut other = Saga::<3>::with_scope(ID, 9);
    refuses(&mut other, Refusal::Stale, |saga| {
        saga.resolve(skipped, Outcome::Skipped)
    });
}

#[test]
fn a_skipped_pending_attempt_is_lost_to_a_restart() {
    let mut saga = Saga::<2>::new(ID);
    let pending = next(&mut saga);
    let mut restored = Saga::<2>::restored(saga.snapshot(), 0).expect("sound");
    refuses(&mut restored, Refusal::Stale, |saga| {
        saga.resolve(pending, Outcome::Skipped)
    });
    let again = next(&mut restored);
    assert_eq!(again.key(), pending.key());
    assert_ne!(again.attempt(), pending.attempt());
    run_resolved(&mut restored, again, Outcome::Skipped);
}

#[test]
fn a_stuck_compensation_parks_the_saga() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    assert_eq!(saga.progress(), Progress::Compensating(1));
    let retry = run(&mut saga, Outcome::Failure);
    let undo = next(&mut saga);
    assert_eq!(undo.key(), retry.key());
    run_resolved(&mut saga, undo, Outcome::Stuck);
    assert_eq!(saga.progress(), Progress::Stuck(1));
    assert_eq!(saga.pending(), None);
    let outcome = Outcome::Stuck;
    assert_eq!(
        saga.settled(),
        Some(Settlement {
            action: undo,
            outcome
        })
    );
    // Nothing more is issued, however often asked.
    repeated(&mut saga, &None, Saga::next_action);
    assert_eq!(saga.issued(), undo.attempt().raw());
    refuses(&mut saga, Refusal::Concluded, Saga::cancel);
    // Only the repeat of the stuck reply is honoured; the rest is refused.
    repeated(&mut saga, &Resolution::AlreadyResolved, |saga| {
        saga.resolve(undo, Outcome::Stuck)
    });
    refuses(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(undo, Outcome::Success)
    });
    refuses(&mut saga, Refusal::Mismatch, |saga| {
        saga.resolve(undo, Outcome::Failure)
    });
    refuses(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(retry, Outcome::Failure)
    });
    // An action built by hand under a fresh attempt number was never issued.
    let forged = Action::new(undo.key(), attempt(undo.attempt().raw() + 1));
    refuses(&mut saga, Refusal::Stale, |saga| {
        saga.resolve(forged, Outcome::Success)
    });
}

#[test]
fn a_stuck_saga_survives_a_restart_and_stays_stuck() {
    let mut saga = Saga::<2>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    let undo = run(&mut saga, Outcome::Stuck);
    let snapshot = saga.snapshot();
    assert_eq!(snapshot.progress(), Progress::Stuck(0));
    let mut restored = Saga::<2>::restored(snapshot, 0).expect("sound");
    assert_eq!(restored, saga);
    repeated(&mut restored, &None, Saga::next_action);
    repeated(&mut restored, &Resolution::AlreadyResolved, |saga| {
        saga.resolve(undo, Outcome::Stuck)
    });
    refuses(&mut restored, Refusal::Concluded, Saga::cancel);
}

#[test]
fn restoring_refuses_skipped_and_stuck_states_no_saga_reaches() {
    use Kind::{Compensate, Forward as Fwd};
    use Outcome::{Failure, Skipped, Stuck, Success};
    let with = |snapshot: Snapshot, skipped: u64| Snapshot {
        skipped,
        ..snapshot
    };
    let skipping = saved(Progress::Forward(2), 2, Some((1, Fwd, Skipped, 2)));
    assert!(Saga::<3>::restored(with(skipping, 0b10), 0).is_some());
    assert!(Saga::<3>::restored(with(skipping, 0b11), 0).is_some());
    for wrong in [0, 0b100, 0b1010] {
        assert_eq!(
            Saga::<3>::restored(with(skipping, wrong), 0),
            None,
            "{wrong:b}"
        );
    }
    // A success leaves its own step unskipped; a skip needs its bit.
    let succeeding = saved(Progress::Forward(2), 2, Some((1, Fwd, Success, 2)));
    assert!(Saga::<3>::restored(with(succeeding, 0b01), 0).is_some());
    assert_eq!(Saga::<3>::restored(with(succeeding, 0b11), 0), None);
    // Nothing settled, nothing skipped.
    assert_eq!(
        Saga::<3>::restored(with(saved(Progress::Forward(0), 0, None), 1), 0),
        None
    );
    // A compensation never targets a skipped step, and the saga never
    // compensates the last one, so it was never skipped before a failure.
    let undo = saved(Progress::Aborted, 3, Some((0, Compensate, Success, 3)));
    assert!(Saga::<3>::restored(with(undo, 0b010), 0).is_some());
    assert_eq!(Saga::<3>::restored(with(undo, 0b001), 0), None);
    assert_eq!(Saga::<3>::restored(with(undo, 0b100), 0), None);
    // Skipping step 1 took an attempt: the compensation of 0 is at least 3rd.
    let early = saved(Progress::Aborted, 2, Some((0, Compensate, Success, 2)));
    assert!(Saga::<3>::restored(early, 0).is_some());
    assert_eq!(Saga::<3>::restored(with(early, 0b010), 0), None);
    // The unwind steps over a skipped step.
    let over = saved(Progress::Compensating(0), 3, Some((2, Fwd, Failure, 3)));
    assert!(Saga::<3>::restored(with(over, 0b010), 0).is_some());
    assert_eq!(Saga::<3>::restored(over, 0), None);
    // Stuck comes only from a stuck compensation, and issues nothing after.
    // Only `unstick` leaves it: that state is covered in `unstick.rs`.
    let stuck = saved(Progress::Stuck(0), 3, Some((0, Compensate, Stuck, 3)));
    assert!(Saga::<3>::restored(stuck, 0).is_some());
    for unsound in [
        saved(Progress::Stuck(0), 4, Some((0, Compensate, Stuck, 3))),
        saved(Progress::Stuck(0), 3, Some((0, Compensate, Failure, 3))),
        saved(Progress::Stuck(1), 3, Some((0, Compensate, Stuck, 3))),
        saved(Progress::Stuck(0), 3, None),
        saved(Progress::Aborted, 3, Some((0, Compensate, Stuck, 3))),
        saved(Progress::Forward(1), 1, Some((0, Fwd, Stuck, 1))),
        saved(Progress::Forward(1), 1, Some((0, Compensate, Skipped, 1))),
    ] {
        assert_eq!(Saga::<3>::restored(unsound, 0), None, "{unsound:?}");
    }
}
