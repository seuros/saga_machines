//! Restarts: the sequence resumes, the scope is checked, unsound state is refused.
use super::*;

#[test]
fn a_restored_saga_equals_the_one_it_was_saved_from() {
    let mut saga = Saga::<3>::with_scope(ID, 4);
    assert_eq!(Saga::<3>::restored(saga.snapshot(), 4), Some(saga.clone()));
    let first = run(&mut saga, Outcome::Success);
    let snapshot = saga.snapshot();
    assert_eq!((snapshot.id, snapshot.scope, snapshot.issued), (ID, 4, 1));
    assert_eq!(snapshot.progress, Progress::Forward(1));
    let outcome = Outcome::Success;
    assert_eq!(
        snapshot.settled,
        Some(Settlement {
            action: first,
            outcome
        })
    );
    let mut restored = Saga::<3>::restored(snapshot, 4).expect("sound");
    assert_eq!(restored, saga);
    // The last settlement survived, so its repeat is still provable.
    let again = |saga: &mut Saga<3>| saga.resolve(first, Outcome::Success);
    repeated(&mut restored, &Resolution::AlreadyResolved, again);
    refused(&mut restored, Refusal::Mismatch, |saga| {
        saga.resolve(first, Outcome::Failure)
    });
}

#[test]
fn a_pending_attempt_is_not_carried_over_and_is_never_reissued() {
    let mut saga = Saga::<3>::with_scope(ID, 4);
    run(&mut saga, Outcome::Success);
    let lost = next(&mut saga);
    let mut restored = Saga::<3>::restored(saga.snapshot(), 4).expect("sound");
    assert_eq!((restored.pending(), restored.issued()), (None, 2));
    refused(&mut restored, Refusal::Stale, |saga| {
        saga.resolve(lost, Outcome::Success)
    });
    let reissued = next(&mut restored);
    assert_eq!(reissued.key(), lost.key());
    assert_eq!(reissued.attempt().raw(), (4 << 48) | 3);
    refused(&mut restored, Refusal::Stale, |saga| {
        saga.resolve(lost, Outcome::Success)
    });
    assert_eq!(
        restored.resolve(reissued, Outcome::Success),
        Ok(Resolution::Resolved)
    );
    run(&mut restored, Outcome::Success);
    assert_eq!(restored.progress(), Progress::Complete);
    assert_eq!(restored.snapshot().issued, 4);
}

#[test]
fn restored_progress_compensates_in_reverse() {
    let mut saga = Saga::<3>::new(ID);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Success);
    run(&mut saga, Outcome::Failure);
    let mut restored = Saga::<3>::restored(saga.snapshot(), 0).expect("sound");
    assert_eq!(restored.progress(), Progress::Compensating(1));
    assert_eq!(restored.cancel(), Ok(Cancellation::AlreadyUnwinding));
    let undo = run(&mut restored, Outcome::Success);
    assert_eq!((undo.step(), undo.kind()), (1, Kind::Compensate));
    run(&mut restored, Outcome::Success);
    assert_eq!(restored.progress(), Progress::Aborted);
    let aborted = Saga::<3>::restored(restored.snapshot(), 0).expect("sound");
    assert_eq!(aborted, restored);
}

#[test]
fn a_snapshot_of_another_scope_is_refused() {
    let mut saga = Saga::<2>::with_scope(ID, 4);
    let fresh = saga.snapshot();
    run(&mut saga, Outcome::Success);
    let snapshot = saga.snapshot();
    for saved in [fresh, snapshot] {
        assert!(Saga::<2>::restored(saved, 4).is_some());
        assert_eq!(Saga::<2>::restored(saved, 5), None);
        assert_eq!(Saga::<2>::restored(saved, 0), None);
        // Relabelling the snapshot does not help once an attempt is recorded.
        let relabelled = Saga::<2>::restored(Snapshot { scope: 5, ..saved }, 5);
        assert_eq!(relabelled.is_some(), saved.settled.is_none());
    }
}

/// A snapshot of a two-step saga in scope zero, settled by attempt `attempt`.
fn saved(
    progress: Progress,
    issued: u64,
    settled: Option<(usize, Kind, Outcome, u64)>,
) -> Snapshot {
    let settled = settled.map(|(step, kind, outcome, attempt)| Settlement {
        action: Action::new(ActionKey::new(ID, step, kind), AttemptId::from_raw(attempt)),
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

#[test]
fn sound_snapshots_are_accepted() {
    use Kind::{Compensate, Forward as Fwd};
    use Outcome::{Failure, Success};
    let sound = [
        saved(Progress::Forward(0), 0, None),
        saved(Progress::Forward(0), 9, None),
        saved(Progress::Aborted, 0, None),
        saved(Progress::Forward(1), 1, Some((0, Fwd, Success, 1))),
        saved(Progress::Compensating(0), 1, Some((0, Fwd, Success, 1))),
        saved(Progress::Aborted, 1, Some((0, Fwd, Failure, 1))),
        saved(Progress::Compensating(0), 2, Some((1, Fwd, Failure, 2))),
        saved(
            Progress::Compensating(0),
            3,
            Some((0, Compensate, Failure, 3)),
        ),
        saved(Progress::Aborted, 3, Some((0, Compensate, Success, 3))),
        saved(Progress::Complete, 2, Some((1, Fwd, Success, 2))),
    ];
    for snapshot in sound {
        let saga = Saga::<2>::restored(snapshot, 0).expect("sound");
        assert_eq!(saga.snapshot(), snapshot);
    }
    let empty = Snapshot {
        progress: Progress::Complete,
        ..saved(Progress::Complete, 0, None)
    };
    assert!(Saga::<0>::restored(empty, 0).is_some());
}

#[test]
fn unsound_snapshots_are_refused() {
    use Kind::{Compensate, Forward as Fwd};
    use Outcome::{Failure, Success};
    let unsound = [
        // Progress outside a two-step workflow.
        saved(Progress::Forward(2), 2, Some((1, Fwd, Success, 2))),
        saved(Progress::Compensating(1), 2, Some((1, Fwd, Success, 2))),
        saved(Progress::Compensating(usize::MAX), 2, None),
        // Progress nothing recorded leads to.
        saved(Progress::Forward(1), 1, None),
        saved(Progress::Compensating(0), 1, None),
        saved(Progress::Complete, 2, None),
        saved(Progress::Forward(0), 1, Some((0, Fwd, Success, 1))),
        saved(Progress::Complete, 1, Some((0, Fwd, Success, 1))),
        saved(Progress::Aborted, 2, Some((1, Fwd, Failure, 2))),
        saved(Progress::Forward(1), 3, Some((0, Compensate, Success, 3))),
        // A settlement outside the workflow.
        saved(Progress::Compensating(0), 3, Some((2, Fwd, Failure, 3))),
        saved(
            Progress::Compensating(0),
            3,
            Some((1, Compensate, Success, 3)),
        ),
        saved(
            Progress::Aborted,
            3,
            Some((usize::MAX, Compensate, Failure, 3)),
        ),
        // An attempt this scope did not issue within the count.
        saved(Progress::Forward(1), 1, Some((0, Fwd, Success, 2))),
        saved(Progress::Forward(1), 1, Some((0, Fwd, Success, 0))),
        saved(
            Progress::Forward(1),
            1,
            Some((0, Fwd, Success, (1 << 48) | 1)),
        ),
        // Too few attempts to have got there.
        saved(Progress::Complete, 2, Some((1, Fwd, Success, 1))),
        saved(Progress::Compensating(0), 2, Some((1, Fwd, Failure, 0))),
        // A count beyond a scope's sequence.
        saved(Progress::Forward(0), 1 << 48, None),
    ];
    for snapshot in unsound {
        assert_eq!(Saga::<2>::restored(snapshot, 0), None, "{snapshot:?}");
    }
    // Another saga's settlement.
    let mut other = saved(Progress::Forward(1), 1, Some((0, Fwd, Success, 1)));
    other.id = SagaId::from_raw(8);
    assert_eq!(Saga::<2>::restored(other, 0), None);
    // Nothing fits a workflow without steps but being complete.
    assert_eq!(
        Saga::<0>::restored(saved(Progress::Forward(0), 0, None), 0),
        None
    );
    assert_eq!(
        Saga::<0>::restored(saved(Progress::Aborted, 0, None), 0),
        None
    );
}
