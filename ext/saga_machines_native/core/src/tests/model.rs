//! Randomized run against an independent reference model. Every refusal and
//! every honoured repeat is compared against a clone taken before the call.
use std::vec::Vec;

use super::*;

const STEPS: usize = 4;
const SCOPE: u16 = 6;

/// The saga as two facts: how many steps stand done, and whether it unwinds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Model {
    done: usize,
    unwinding: bool,
    pending: Option<Action>,
    settled: Option<(Action, Outcome)>,
    issued: u64,
}

impl Model {
    const fn progress(&self) -> Progress {
        match (self.unwinding, self.done) {
            (false, STEPS) => Progress::Complete,
            (false, done) => Progress::Forward(done),
            (true, 0) => Progress::Aborted,
            (true, done) => Progress::Compensating(done - 1),
        }
    }

    fn next_action(&mut self) -> Option<Action> {
        if self.pending.is_none() {
            let (step, kind) = match self.progress() {
                Progress::Forward(step) => (step, Kind::Forward),
                Progress::Compensating(step) => (step, Kind::Compensate),
                Progress::Complete | Progress::Aborted => return None,
            };
            self.issued += 1;
            let attempt = attempt((u64::from(SCOPE) << 48) | self.issued);
            self.pending = Some(Action::new(ActionKey::new(ID, step, kind), attempt));
        }
        self.pending
    }

    fn resolve(&mut self, action: Action, outcome: Outcome) -> Result<Resolution, Refusal> {
        let sequence = action.attempt().raw() & ((1 << 48) - 1);
        if action.attempt().scope() != SCOPE || sequence == 0 || sequence > self.issued {
            return Err(Refusal::Stale);
        }
        if let Some(pending) = self.pending.filter(|p| p.attempt() == action.attempt()) {
            if pending.key() != action.key() {
                return Err(Refusal::Mismatch);
            }
            match (action.kind(), outcome) {
                (Kind::Forward, Outcome::Success) => self.done += 1,
                (Kind::Forward, Outcome::Failure) => self.unwinding = true,
                (Kind::Compensate, Outcome::Success) => self.done -= 1,
                (Kind::Compensate, Outcome::Failure) => {}
            }
            self.pending = None;
            self.settled = Some((action, outcome));
            return Ok(Resolution::Resolved);
        }
        match self.settled {
            Some(settled) if settled == (action, outcome) => Ok(Resolution::AlreadyResolved),
            Some((settled, _)) if settled.attempt() == action.attempt() => Err(Refusal::Mismatch),
            Some(_) | None => Err(Refusal::Stale),
        }
    }

    const fn cancel(&mut self) -> Result<Cancellation, Refusal> {
        if self.unwinding {
            Ok(Cancellation::AlreadyUnwinding)
        } else if self.done == STEPS {
            Err(Refusal::Concluded)
        } else if self.pending.is_some() {
            Err(Refusal::Busy)
        } else {
            self.unwinding = true;
            Ok(Cancellation::Cancelled)
        }
    }

    fn agrees(&self, saga: &Saga<STEPS>) {
        assert_eq!(saga.progress(), self.progress());
        assert_eq!(saga.pending(), self.pending);
        assert_eq!(saga.issued(), self.issued);
        let settled = saga
            .settled()
            .map(|settled| (settled.action, settled.outcome));
        assert_eq!(settled, self.settled);
    }
}

struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    const fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// An action to report: mostly one the saga issued, sometimes a forgery.
fn pick(rng: &mut Rng, saga: &Saga<STEPS>, history: &[Action]) -> Action {
    let issued = |rng: &mut Rng| {
        let index = usize::try_from(rng.below(history.len() as u64)).expect("small");
        history[index]
    };
    match (rng.below(10), saga.pending(), saga.settled()) {
        (0..=3, Some(pending), _) => pending,
        (4..=5, _, Some(settled)) => settled.action,
        (6, _, _) if !history.is_empty() => {
            // A real attempt under another issued action's key.
            Action::new(issued(rng).key(), issued(rng).attempt())
        }
        (7, _, _) => {
            let key = ActionKey::new(ID, 0, Kind::Forward);
            let scope = u64::from(SCOPE) + rng.below(2);
            Action::new(key, attempt((scope << 48) | (1 + rng.below(5))))
        }
        _ if !history.is_empty() => issued(rng),
        _ => Action::new(ActionKey::new(ID, 0, Kind::Forward), attempt(1)),
    }
}

/// One random call on both; `true` when it was honoured and changed state.
fn step(rng: &mut Rng, saga: &mut Saga<STEPS>, model: &mut Model, history: &mut Vec<Action>) {
    let before = saga.clone();
    let unchanged = match rng.below(8) {
        0..=2 => {
            let had = model.pending;
            let expected = model.next_action();
            assert_eq!(saga.next_action(), Ok(expected));
            if had.is_none()
                && let Some(action) = expected
            {
                history.push(action);
            }
            had.is_some() || expected.is_none()
        }
        3..=5 => {
            let action = pick(rng, saga, history);
            let outcome = [Outcome::Success, Outcome::Failure][usize::from(rng.below(3) == 0)];
            let expected = model.resolve(action, outcome);
            assert_eq!(
                saga.resolve(action, outcome),
                expected,
                "{action:?} {outcome:?}"
            );
            expected != Ok(Resolution::Resolved)
        }
        6 => {
            let expected = model.cancel();
            assert_eq!(saga.cancel(), expected);
            expected != Ok(Cancellation::Cancelled)
        }
        _ => {
            // A restart: the snapshot is always sound, and drops only the
            // pending action.
            *saga = Saga::restored(saga.snapshot(), SCOPE).expect("sound");
            assert_eq!(Saga::<STEPS>::restored(saga.snapshot(), SCOPE + 1), None);
            let dropped = model.pending.take().is_some();
            !dropped
        }
    };
    if unchanged {
        assert_eq!(*saga, before);
    } else {
        assert_ne!(*saga, before);
    }
    model.agrees(saga);
}

#[test]
fn random_runs_agree_with_the_model() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut complete, mut aborted) = (0, 0);
    for _ in 0..400 {
        let mut saga = Saga::<STEPS>::with_scope(ID, SCOPE);
        let mut model = Model {
            done: 0,
            unwinding: false,
            pending: None,
            settled: None,
            issued: 0,
        };
        let mut history = Vec::new();
        for _ in 0..120 {
            step(&mut rng, &mut saga, &mut model, &mut history);
        }
        complete += u32::from(saga.progress() == Progress::Complete);
        aborted += u32::from(saga.progress() == Progress::Aborted);
    }
    assert!(
        complete > 0 && aborted > 0,
        "{complete} complete, {aborted} aborted"
    );
}
