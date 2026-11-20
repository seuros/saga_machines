//! `reconcile`: work that was in flight when the process died is settled
//! from what the caller found out, without a fresh action to issue and resolve.
use periscope_machines::scoped;

use super::*;

/// The action `next_action` and `resolve` would settle, on a clone.
fn by_hand<const N: usize>(saga: &Saga<N>, outcome: Outcome) -> (Saga<N>, Action) {
    let mut hand = saga.clone();
    let action = next(&mut hand);
    assert_eq!(hand.resolve(action, outcome), Ok(Resolution::Resolved));
    (hand, action)
}

#[test]
fn reconcile_settles_the_step_the_saga_is_on() {
    let mut saga = Saga::<3>::with_scope(ID, 4);
    let first = saga
        .reconcile(0, Kind::Forward, Outcome::Success)
        .expect("on step 0");
    assert_eq!(first.key(), ActionKey::new(ID, 0, Kind::Forward));
    assert_eq!(first.attempt().raw(), scoped(4, 1));
    assert_eq!(saga.progress(), Progress::Forward(1));
    assert_eq!((saga.pending(), saga.issued()), (None, 1));
    let outcome = Outcome::Success;
    assert_eq!(
        saga.settled(),
        Some(Settlement {
            action: first,
            outcome
        })
    );
    // The settled action is the last one: its repeat is honoured.
    repeated(&mut saga, &Resolution::AlreadyResolved, |saga| {
        saga.resolve(first, Outcome::Success)
    });
    saga.reconcile(1, Kind::Forward, Outcome::Skipped)
        .expect("on step 1");
    assert_eq!(
        (saga.progress(), saga.skipped()),
        (Progress::Forward(2), 0b010)
    );
    saga.reconcile(2, Kind::Forward, Outcome::Failure)
        .expect("on step 2");
    assert_eq!(saga.progress(), Progress::Compensating(0));
    saga.reconcile(0, Kind::Compensate, Outcome::Failure)
        .expect("compensating 0");
    assert_eq!(saga.progress(), Progress::Compensating(0));
    saga.reconcile(0, Kind::Compensate, Outcome::Stuck)
        .expect("compensating 0");
    assert_eq!(saga.progress(), Progress::Stuck(0));
    assert_eq!(saga.issued(), 5);
}

#[test]
fn reconcile_is_next_action_then_resolve_in_one_call() {
    for saga in super::reachable::walk::<3>() {
        let (kind, step) = match saga.progress() {
            Progress::Forward(step) => (Kind::Forward, step),
            Progress::Compensating(step) => (Kind::Compensate, step),
            Progress::Complete | Progress::Aborted | Progress::Stuck(_) => continue,
        };
        for outcome in [
            Outcome::Success,
            Outcome::Failure,
            Outcome::Skipped,
            Outcome::Stuck,
        ] {
            if !outcome.applies_to(kind) {
                continue;
            }
            let (expected, action) = by_hand(&saga, outcome);
            let mut reconciled = saga.clone();
            assert_eq!(reconciled.reconcile(step, kind, outcome), Ok(action));
            assert_eq!(reconciled, expected, "{saga:?} {outcome:?}");
        }
    }
}

#[test]
fn a_pending_action_is_the_one_settled() {
    let mut saga = Saga::<2>::new(ID);
    let pending = next(&mut saga);
    let issued = saga.issued();
    assert_eq!(
        saga.reconcile(0, Kind::Forward, Outcome::Success),
        Ok(pending)
    );
    assert_eq!((saga.issued(), saga.pending()), (issued, None));
    assert_eq!(saga.progress(), Progress::Forward(1));
}

#[test]
fn the_in_flight_reply_of_a_lost_process_is_stale_after_reconcile() {
    let mut saga = Saga::<2>::new(ID);
    let lost = next(&mut saga);
    let mut restored = Saga::<2>::restored(saga.snapshot(), 0).expect("sound");
    let settled = restored
        .reconcile(0, Kind::Forward, Outcome::Success)
        .expect("on step 0");
    assert_ne!(settled.attempt(), lost.attempt());
    refuses(&mut restored, Refusal::Stale, |saga| {
        saga.resolve(lost, Outcome::Success)
    });
    refuses(&mut restored, Refusal::Stale, |saga| {
        saga.resolve(lost, Outcome::Failure)
    });
}

#[test]
fn reconcile_refuses_what_the_saga_cannot_be_waiting_on() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    // Another step, or the other kind of work.
    for (step, kind) in [
        (0, Kind::Forward),
        (2, Kind::Forward),
        (1, Kind::Compensate),
        (0, Kind::Compensate),
        (7, Kind::Forward),
    ] {
        refuses(&mut saga, Refusal::Mismatch, |saga| {
            saga.reconcile(step, kind, Outcome::Success)
        });
    }
    // An outcome that cannot answer the kind, whatever the step.
    for (step, kind, outcome) in [
        (1, Kind::Forward, Outcome::Stuck),
        (1, Kind::Compensate, Outcome::Skipped),
        (5, Kind::Forward, Outcome::Stuck),
    ] {
        refuses(&mut saga, Refusal::Inapplicable, |saga| {
            saga.reconcile(step, kind, outcome)
        });
    }
    // Nothing is in flight on a finished saga.
    let mut complete = Saga::<1>::new(ID);
    run(&mut complete, Outcome::Success);
    let mut aborted = Saga::<1>::new(ID);
    assert_eq!(aborted.cancel(), Ok(Cancellation::Cancelled));
    let (mut stuck, _) = stuck_saga();
    for done in [&mut complete, &mut aborted] {
        refuses(done, Refusal::Concluded, |saga| {
            saga.reconcile(0, Kind::Forward, Outcome::Success)
        });
    }
    refuses(&mut stuck, Refusal::Concluded, |saga| {
        saga.reconcile(0, Kind::Compensate, Outcome::Success)
    });
    // The answer to a stuck saga is unstick, then reconcile.
    assert_eq!(stuck.unstick(), Ok(()));
    stuck
        .reconcile(0, Kind::Compensate, Outcome::Success)
        .expect("resumed");
    assert_eq!(stuck.progress(), Progress::Aborted);
}

fn stuck_saga() -> (Saga<2>, Action) {
    let mut saga = Saga::<2>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    let undo = run(&mut saga, Outcome::Stuck);
    (saga, undo)
}

#[test]
fn reconcile_reports_exhaustion_and_changes_nothing() {
    let snapshot = Snapshot {
        scope: 3,
        ..saved(Progress::Forward(0), periscope_machines::SEQUENCE_MAX, None)
    };
    let mut saga = Saga::<2>::restored(snapshot, 3).expect("sound");
    refuses(&mut saga, Refusal::Exhausted, |saga| {
        saga.reconcile(0, Kind::Forward, Outcome::Success)
    });
}

#[test]
fn a_kill_mid_step_is_reconciled_and_the_saga_completes() {
    let mut saga = Saga::<3>::with_scope(ID, 3);
    run(&mut saga, Outcome::Success);
    let in_flight = next(&mut saga);
    // The effect is running; the process dies. Only the snapshot survives.
    let snapshot = saga.snapshot();
    let mut saga = Saga::<3>::restored(snapshot, 3).expect("restores");
    assert_eq!(saga.pending(), None);
    // The progress says what was in flight; the caller probed and found it done.
    assert_eq!(saga.progress(), Progress::Forward(in_flight.step()));
    saga.reconcile(in_flight.step(), in_flight.kind(), Outcome::Success)
        .expect("in flight");
    assert_eq!(saga.progress(), Progress::Forward(2));
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.progress(), Progress::Complete);
}

#[test]
fn a_kill_mid_step_is_reconciled_and_the_saga_compensates_fully() {
    let mut saga = Saga::<3>::with_scope(ID, 3);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Success);
    let in_flight = next(&mut saga);
    let snapshot = saga.snapshot();
    let mut saga = Saga::<3>::restored(snapshot, 3).expect("restores");
    // The probe found step 2 never took effect.
    saga.reconcile(in_flight.step(), in_flight.kind(), Outcome::Failure)
        .expect("in flight");
    assert_eq!(saga.progress(), Progress::Compensating(1));
    // Killed again, mid-compensation; this time the undo had taken effect.
    next(&mut saga);
    let snapshot = saga.snapshot();
    let mut saga = Saga::<3>::restored(snapshot, 3).expect("restores");
    saga.reconcile(1, Kind::Compensate, Outcome::Success)
        .expect("in flight");
    assert_eq!(saga.progress(), Progress::Compensating(0));
    run(&mut saga, Outcome::Success);
    assert_eq!(saga.progress(), Progress::Aborted);
    assert_eq!(saga.next_action(), Ok(None));
}

#[test]
fn a_skipped_step_reconciled_is_never_compensated() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    saga.reconcile(1, Kind::Forward, Outcome::Skipped)
        .expect("on step 1");
    saga.reconcile(2, Kind::Forward, Outcome::Failure)
        .expect("on step 2");
    // Step 1 is passed over: compensation goes straight to step 0.
    assert_eq!(saga.progress(), Progress::Compensating(0));
    assert_eq!(saga.skipped(), 0b010);
}
