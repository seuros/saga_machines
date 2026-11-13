//! `restored` accepts exactly the snapshots a saga can produce: every state
//! reachable by issuing, resolving, cancelling and restarting is walked, and
//! every candidate snapshot over a small range is checked against that set.
use std::collections::BTreeSet;
use std::format;
use std::string::String;
use std::vec::Vec;

use periscope_machines::scoped;

use super::*;

const SCOPE: u16 = 4;
const OTHER_SCOPE: u16 = 5;
const OTHER_ID: SagaId = SagaId::from_raw(8);

/// Attempts a walk may issue: enough for the longest path through three
/// steps (three forward, a failure, two compensations) with retries to spare.
const ISSUED_MAX: u64 = 9;

fn name<T: core::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

/// Every saga reachable from a fresh one within `ISSUED_MAX` attempts.
fn walk<const N: usize>() -> Vec<Saga<N>> {
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    let mut queue = Vec::from([Saga::<N>::with_scope(ID, SCOPE)]);
    while let Some(saga) = queue.pop() {
        if saga.issued() > ISSUED_MAX || !seen.insert(name(&saga)) {
            continue;
        }
        let mut issuing = saga.clone();
        issuing.next_action().expect("not exhausted");
        queue.push(issuing);
        for outcome in [Outcome::Success, Outcome::Failure] {
            let mut resolving = saga.clone();
            if let Some(action) = saga.pending() {
                assert_eq!(resolving.resolve(action, outcome), Ok(Resolution::Resolved));
                queue.push(resolving);
            }
        }
        let mut cancelling = saga.clone();
        if cancelling.cancel().is_ok() {
            queue.push(cancelling);
        }
        // A restart: every state a saga saves is one it can be restored from.
        let restarted = Saga::<N>::restored(saga.snapshot(), SCOPE);
        queue.push(restarted.unwrap_or_else(|| panic!("false reject: {saga:?}")));
        found.push(saga);
    }
    found
}

/// Every settlement over steps `0..=N`, sequences `0..=ISSUED_MAX + 1`, both
/// kinds and outcomes, this saga's key or another's, this scope or another.
fn settlements<const N: usize>() -> Vec<Option<Settlement>> {
    let mut all = Vec::from([None]);
    for step in 0..=N {
        for sequence in 0..=ISSUED_MAX + 1 {
            for kind in [Kind::Forward, Kind::Compensate] {
                for outcome in [Outcome::Success, Outcome::Failure] {
                    for (id, scope) in [(ID, SCOPE), (OTHER_ID, SCOPE), (ID, OTHER_SCOPE)] {
                        let attempt = AttemptId::from_raw(scoped(scope, sequence));
                        let action = Action::new(ActionKey::new(id, step, kind), attempt);
                        all.push(Some(Settlement { action, outcome }));
                    }
                }
            }
        }
    }
    all
}

fn progresses<const N: usize>() -> Vec<Progress> {
    let mut all = Vec::from([Progress::Complete, Progress::Aborted]);
    for step in 0..=N {
        all.push(Progress::Forward(step));
        all.push(Progress::Compensating(step));
    }
    all
}

/// Returns how many snapshots are reachable and how many candidates are not.
fn exact<const N: usize>() -> (usize, usize) {
    let reachable: BTreeSet<String> = walk::<N>()
        .iter()
        .map(|saga| name(&saga.snapshot()))
        .collect();
    let (mut accepted, mut refused) = (0, 0);
    for progress in progresses::<N>() {
        for issued in 0..=ISSUED_MAX {
            for settled in settlements::<N>() {
                let snapshot = Snapshot {
                    id: ID,
                    scope: SCOPE,
                    progress,
                    issued,
                    settled,
                };
                let restored = Saga::<N>::restored(snapshot, SCOPE);
                let expected = reachable.contains(&name(&snapshot));
                assert_eq!(restored.is_some(), expected, "N = {N}: {snapshot:?}");
                if let Some(saga) = restored {
                    assert_eq!((saga.snapshot(), saga.pending()), (snapshot, None));
                    accepted += 1;
                } else {
                    refused += 1;
                }
            }
        }
    }
    assert_eq!(
        accepted,
        reachable.len(),
        "N = {N}: a reachable snapshot was not a candidate"
    );
    (accepted, refused)
}

#[test]
fn restored_accepts_exactly_the_reachable_snapshots() {
    // A saga without steps is complete, and that is all it ever is.
    assert_eq!(exact::<0>().0, 1);
    let one = exact::<1>();
    let two = exact::<2>();
    let three = exact::<3>();
    assert!(
        one.0 < two.0 && two.0 < three.0,
        "{one:?} {two:?} {three:?}"
    );
    assert!(three.1 > 10_000, "{three:?}");
}

/// The false accepts the walk found before the floor followed the settlement.
#[test]
fn a_settlement_needs_the_attempts_that_lead_to_it() {
    use Kind::{Compensate, Forward as Fwd};
    use Outcome::Success;
    let saved = |progress, issued, settled: Option<(usize, Kind, u64)>| Snapshot {
        id: ID,
        scope: 0,
        progress,
        issued,
        settled: settled.map(|(step, kind, attempt)| Settlement {
            action: Action::new(ActionKey::new(ID, step, kind), AttemptId::from_raw(attempt)),
            outcome: Success,
        }),
    };
    // Attempt 1 is the forward try of step 0, never its compensation.
    let early = saved(Progress::Aborted, 1, Some((0, Compensate, 1)));
    assert_eq!(Saga::<2>::restored(early, 0), None);
    let soonest = saved(Progress::Aborted, 2, Some((0, Compensate, 2)));
    assert!(Saga::<2>::restored(soonest, 0).is_some());
    // No attempt is issued after a settlement that finished the saga.
    let extra = saved(Progress::Complete, 2, Some((0, Fwd, 1)));
    assert_eq!(Saga::<1>::restored(extra, 0), None);
    assert_eq!(
        Saga::<0>::restored(saved(Progress::Complete, 1, None), 0),
        None
    );
    let extra = saved(Progress::Aborted, 3, Some((0, Compensate, 2)));
    assert_eq!(Saga::<2>::restored(extra, 0), None);
    // A cancel settles nothing, so attempts lost to restarts may precede it.
    let cancelled = saved(Progress::Aborted, 5, None);
    assert!(Saga::<2>::restored(cancelled, 0).is_some());
}
