//! Thin binding to sequential forward and compensating action policies.
mod classes;
mod hooks;
mod init;
mod names;
mod read;

use std::cell::RefCell;

use magnus::{Error, IntoValue, RString, Ruby, Symbol, Value, prelude::*, typed_data::Obj};
use saga_machines::{Action, Cancellation, Outcome, Refusal, Resolution, Saga, SagaId, Snapshot};

use classes::{classes, keep};
use names::{Number, Scope};

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
    fn duplicate(&self) -> Box<dyn Policy>;
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
    fn duplicate(&self) -> Box<dyn Policy> {
        Box::new(self.clone())
    }
}

/// The erased saga as a value: it clones, and two are equal when they hold
/// the same state, which is exactly what the snapshot and the pending action
/// say (identity, scope, issued, progress, settlement, pending).
struct Core(Box<dyn Policy>);

impl Clone for Core {
    fn clone(&self) -> Self {
        Self(self.0.duplicate())
    }
}

impl PartialEq for Core {
    fn eq(&self, other: &Self) -> bool {
        let state = |core: &Self| (core.0.steps(), core.0.snapshot(), core.0.pending());
        state(self) == state(other)
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
    core: RefCell<Core>,
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
            core: RefCell::new(Core(core)),
        })
    }

    /// `id` must be exactly a `SagaId`.
    fn with_scope(ruby: &Ruby, steps: Number, id: Value, scope: Scope) -> Result<Self, Error> {
        let id = read::saga_id(ruby, classes(ruby)?, id)?;
        Self::build(ruby, steps, Seed::Fresh(id, scope.0))
    }

    /// `saved` must be exactly a `Snapshot`, read natively and checked again.
    fn restored(ruby: &Ruby, steps: Number, saved: Value, scope: Scope) -> Result<Self, Error> {
        let snapshot = read::snapshot(ruby, classes(ruby)?, saved)?;
        Self::build(ruby, steps, Seed::Saved(snapshot, scope.0))
    }

    /// An `Action`, `nil` when nothing is left, or a refusal symbol. Decided
    /// on a copy, kept once the `Action` is built and the saga is still
    /// unfrozen and unchanged: an attempt the caller never saw is never
    /// issued.
    fn next_action(ruby: &Ruby, this: Obj<Self>) -> Result<Value, Error> {
        let classes = classes(ruby)?;
        keep(ruby, this.as_value(), &this.core, |core| {
            match core.0.next_action() {
                Ok(Some(action)) => hooks::action(ruby, classes, action),
                Ok(None) => Ok(ruby.qnil().as_value()),
                Err(why) => Ok(symbol(ruby, names::refusal(why))),
            }
        })
    }

    /// `action` must be exactly an `Action`, read natively and checked again.
    fn resolve(
        ruby: &Ruby,
        this: Obj<Self>,
        action: Value,
        outcome: Symbol,
    ) -> Result<Value, Error> {
        this.check_frozen()?;
        let action = read::action(ruby, classes(ruby)?, action)?;
        let outcome = names::outcome(ruby, outcome)?;
        let answer = this.core.borrow_mut().0.resolve(action, outcome);
        Ok(match answer {
            Ok(resolution) => symbol(ruby, names::resolution(resolution)),
            Err(why) => symbol(ruby, names::refusal(why)),
        })
    }

    fn cancel(ruby: &Ruby, this: Obj<Self>) -> Result<Value, Error> {
        this.check_frozen()?;
        let answer = this.core.borrow_mut().0.cancel();
        Ok(match answer {
            Ok(cancellation) => symbol(ruby, names::cancellation(cancellation)),
            Err(why) => symbol(ruby, names::refusal(why)),
        })
    }

    /// The core `Snapshot`, field by field in its order.
    fn snapshot(ruby: &Ruby, this: &Self) -> Value {
        let snapshot = this.core.borrow().0.snapshot();
        let settled = snapshot
            .settled()
            .map(|settled| names::settled_row(ruby, settled));
        let progress = names::progress_row(ruby, snapshot.progress());
        let fields = (
            snapshot.id().raw(),
            snapshot.scope(),
            progress,
            snapshot.issued(),
            settled,
        );
        fields.into_value_with(ruby)
    }

    fn pending(ruby: &Ruby, this: &Self) -> Value {
        let pending = this.core.borrow().0.pending();
        pending
            .map(|action| names::action_row(ruby, action))
            .into_value_with(ruby)
    }

    fn steps(&self) -> usize {
        self.core.borrow().0.steps()
    }
}
