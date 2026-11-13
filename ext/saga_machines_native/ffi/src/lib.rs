//! Thin binding to sequential forward and compensating action policies.
mod init;
mod names;

use std::cell::RefCell;

use magnus::{Error, IntoValue, RString, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use saga_machines::{
    Action, AttemptId, Cancellation, Outcome, Refusal, Resolution, Saga, SagaId, Snapshot,
};

use names::{ActionRow, Number, ProgressRow, Scope, SettledRow};

/// The step counts compiled in: the core takes the count as a const generic,
/// so each is its own exact instantiation. No padding.
const STEPS: [u64; 7] = [0, 1, 2, 3, 4, 8, 16];

// Type erasure dispatches only to exact core const instantiations.
trait Policy: Send {
    fn next_action(&mut self) -> Result<Option<Action>, Refusal>;
    fn resolve(&mut self, action: Action, outcome: Outcome) -> Result<Resolution, Refusal>;
    fn cancel(&mut self) -> Result<Cancellation, Refusal>;
    fn snapshot(&self) -> Snapshot;
    fn pending(&self) -> Option<Action>;
    fn steps(&self) -> usize;
}

impl<const N: usize> Policy for Saga<N> {
    fn next_action(&mut self) -> Result<Option<Action>, Refusal> {
        self.next_action()
    }
    fn resolve(&mut self, action: Action, outcome: Outcome) -> Result<Resolution, Refusal> {
        self.resolve(action, outcome)
    }
    fn cancel(&mut self) -> Result<Cancellation, Refusal> {
        self.cancel()
    }
    fn snapshot(&self) -> Snapshot {
        self.snapshot()
    }
    fn pending(&self) -> Option<Action> {
        self.pending()
    }
    fn steps(&self) -> usize {
        N
    }
}

/// What a saga is built from: a fresh identity, or saved state.
#[derive(Clone, Copy)]
enum Seed {
    Fresh(SagaId, u16),
    Saved(Snapshot, u16),
}

/// `None` where the core refuses the saved state.
fn sized<const N: usize>(seed: Seed) -> Option<Box<dyn Policy>> {
    let saga = match seed {
        Seed::Fresh(id, scope) => Saga::<N>::with_scope(id, scope),
        Seed::Saved(snapshot, scope) => Saga::<N>::restored(snapshot, scope)?,
    };
    Some(Box::new(saga))
}

/// The scope an attempt id carries: `AttemptId#scope`.
const fn scope_of(raw: Number) -> u16 {
    AttemptId::from_raw(raw.0).scope()
}

/// `SagaId::from_bytes` over the bytes of a String, in any encoding.
fn from_bytes(name: RString) -> Result<u64, Error> {
    let bytes: Vec<u8> = name.funcall("bytes", ())?;
    Ok(SagaId::from_bytes(&bytes).raw())
}

fn symbol(ruby: &Ruby, name: &str) -> Value {
    ruby.to_symbol(name).as_value()
}

#[magnus::wrap(class = "SagaMachines::Saga", free_immediately, size)]
struct Wrapper {
    core: RefCell<Box<dyn Policy>>,
}

// Refusals are answers (symbols), not exceptions. Only malformed input raises.
impl Wrapper {
    /// A restoring constructor has no refusal symbol to answer with: saved
    /// state the core refuses raises `RangeError`, like an unsupported count.
    fn build(ruby: &Ruby, steps: Number, seed: Seed) -> Result<Self, Error> {
        let core = match steps.0 {
            0 => sized::<0>(seed),
            1 => sized::<1>(seed),
            2 => sized::<2>(seed),
            3 => sized::<3>(seed),
            4 => sized::<4>(seed),
            8 => sized::<8>(seed),
            16 => sized::<16>(seed),
            _ => {
                return Err(Error::new(
                    ruby.exception_range_error(),
                    "steps must be one of 0, 1, 2, 3, 4, 8, 16",
                ));
            }
        };
        let core = core.ok_or_else(|| {
            Error::new(
                ruby.exception_range_error(),
                "snapshot is not the saved state of a saga of these steps and this scope",
            )
        })?;
        Ok(Self {
            core: RefCell::new(core),
        })
    }

    fn with_scope(ruby: &Ruby, steps: Number, id: Number, scope: Scope) -> Result<Self, Error> {
        Self::build(ruby, steps, Seed::Fresh(SagaId::from_raw(id.0), scope.0))
    }

    fn restored(
        ruby: &Ruby,
        steps: Number,
        saved: (Number, Scope, ProgressRow, Number, Option<SettledRow>),
        scope: Scope,
    ) -> Result<Self, Error> {
        let (id, saved_scope, progress, issued, settled) = saved;
        let settled = match settled {
            Some(row) => Some(names::settlement(ruby, row)?),
            None => None,
        };
        let snapshot = Snapshot {
            id: SagaId::from_raw(id.0),
            scope: saved_scope.0,
            progress: names::progress(ruby, progress)?,
            issued: issued.0,
            settled,
        };
        Self::build(ruby, steps, Seed::Saved(snapshot, scope.0))
    }

    /// An action row, `nil` when nothing is left, or a refusal symbol.
    fn next_action(ruby: &Ruby, this: Obj<Self>) -> Result<Value, Error> {
        this.check_frozen()?;
        let answer = this.core.borrow_mut().next_action();
        Ok(match answer {
            Ok(Some(action)) => names::action_row(ruby, action).into_value_with(ruby),
            Ok(None) => ruby.qnil().as_value(),
            Err(why) => symbol(ruby, names::refusal(why)),
        })
    }

    fn resolve(
        ruby: &Ruby,
        this: Obj<Self>,
        action: ActionRow,
        outcome: Symbol,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let action = names::action(ruby, action)?;
        let outcome = names::outcome(ruby, outcome)?;
        let answer = this.core.borrow_mut().resolve(action, outcome);
        Ok(match answer {
            Ok(resolution) => symbol(ruby, names::resolution(resolution)),
            Err(why) => symbol(ruby, names::refusal(why)),
        })
    }

    fn cancel(ruby: &Ruby, this: Obj<Self>) -> Result<Value, Error> {
        this.check_frozen()?;
        let answer = this.core.borrow_mut().cancel();
        Ok(match answer {
            Ok(cancellation) => symbol(ruby, names::cancellation(cancellation)),
            Err(why) => symbol(ruby, names::refusal(why)),
        })
    }

    /// The core `Snapshot`, field by field in its order.
    fn snapshot(ruby: &Ruby, this: &Self) -> Value {
        let snapshot = this.core.borrow().snapshot();
        let settled = snapshot
            .settled
            .map(|settled| names::settled_row(ruby, settled));
        let progress = names::progress_row(ruby, snapshot.progress);
        let fields = (
            snapshot.id.raw(),
            snapshot.scope,
            progress,
            snapshot.issued,
            settled,
        );
        fields.into_value_with(ruby)
    }

    fn pending(ruby: &Ruby, this: &Self) -> Value {
        let pending = this.core.borrow().pending();
        pending
            .map(|action| names::action_row(ruby, action))
            .into_value_with(ruby)
    }

    fn steps(&self) -> usize {
        self.core.borrow().steps()
    }
}
