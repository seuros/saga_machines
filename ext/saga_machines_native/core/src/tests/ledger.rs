//! The seam to the sibling dedup ledger: the stable key, the ledger's answers.
use dedup_machines::Completion;

use super::*;

const TTL: Duration = Duration::from_secs(1);

#[test]
fn a_retry_and_a_restart_claim_the_same_key() {
    let mut saga = Saga::<2>::new(ID);
    let mut ledger = Dedup::<ActionKey, 2>::new();
    let first = next(&mut saga);
    let before = saga.clone();
    let Ok(Claim::Acquired(claim)) = first.claim_in(&mut ledger, Duration::ZERO, TTL) else {
        panic!("the key is free");
    };
    assert_eq!(
        first.claim_in(&mut ledger, Duration::ZERO, TTL),
        Ok(Claim::InProgress)
    );
    // The restarted saga issues the same key under a fresh attempt, and the
    // ledger still knows the effect is in flight, then done.
    let mut restarted = Saga::<2>::restored(saga.snapshot(), 0).expect("sound");
    let reissued = next(&mut restarted);
    assert_eq!(reissued.key(), first.key());
    assert_ne!(reissued.attempt(), first.attempt());
    assert_eq!(
        reissued.claim_in(&mut ledger, Duration::ZERO, TTL),
        Ok(Claim::InProgress)
    );
    assert_eq!(
        ledger.complete(claim, Duration::ZERO),
        Ok(Completion::Completed)
    );
    assert_eq!(
        reissued.claim_in(&mut ledger, Duration::ZERO, TTL),
        Ok(Claim::Completed(&()))
    );
    // Claiming is the ledger's business: the saga never changed.
    assert_eq!(saga, before);
}

#[test]
fn forward_and_compensating_work_on_one_step_are_different_keys() {
    let mut saga = Saga::<2>::new(ID);
    let mut ledger = Dedup::<ActionKey, 4, u8>::new();
    let forward = run(&mut saga, Outcome::Success);
    assert_eq!(saga.cancel(), Ok(Cancellation::Cancelled));
    let undo = next(&mut saga);
    assert_eq!((forward.step(), undo.step()), (0, 0));
    let claims =
        [forward, undo].map(
            |action| match action.claim_in(&mut ledger, Duration::ZERO, TTL) {
                Ok(Claim::Acquired(claim)) => claim,
                other => panic!("the key is free: {other:?}"),
            },
        );
    assert_ne!(claims[0], claims[1]);
    assert_eq!(
        ledger.complete_with(claims[1], 9, Duration::ZERO),
        Ok(Completion::Completed)
    );
    assert_eq!(
        undo.claim_in(&mut ledger, Duration::ZERO, TTL),
        Ok(Claim::Completed(&9))
    );
}

#[test]
fn the_ledgers_refusals_come_back_unchanged() {
    let mut saga = Saga::<3>::new(ID);
    let mut ledger = Dedup::<ActionKey, 1>::new();
    let first = run(&mut saga, Outcome::Success);
    let second = next(&mut saga);
    let late = Duration::from_secs(5);
    assert!(first.claim_in(&mut ledger, late, TTL).is_ok());
    let claim = |action: Action, now, ttl| {
        move |ledger: &mut Dedup<ActionKey, 1>| action.claim_in(ledger, now, ttl).map(drop)
    };
    refuses(&mut ledger, LedgerRefusal::Full, claim(second, late, TTL));
    let regressed = claim(first, Duration::ZERO, TTL);
    refuses(&mut ledger, LedgerRefusal::TimeRegression, regressed);
    let dead = claim(first, late, Duration::ZERO);
    refuses(&mut ledger, LedgerRefusal::InvalidDuration, dead);
    let overflow = claim(first, late, Duration::MAX);
    refuses(&mut ledger, LedgerRefusal::Overflow, overflow);
}
