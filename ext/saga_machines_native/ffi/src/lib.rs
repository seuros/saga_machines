//! Thin binding to sequential forward and compensating action policies.
mod init;

use magnus::scan_args::{get_kwargs, scan_args};
use magnus::{Error, Integer, RHash, Ruby, Symbol, TryConvert, Value, prelude::*, typed_data::Obj};
use saga_machines::{Action, Kind, Outcome, Progress, Saga};
use std::{cell::RefCell, sync::Arc};

// Type erasure dispatches only to exact core const instantiations. No padding.
trait Policy: Send {
    fn next_action(&mut self) -> Option<Action>;
    fn resolve(&mut self, action: Action, outcome: Outcome) -> bool;
    fn cancel(&mut self) -> bool;
    fn progress(&self) -> Progress;
    fn steps(&self) -> usize;
}

impl<const N: usize> Policy for Saga<N> {
    fn next_action(&mut self) -> Option<Action> {
        self.next_action()
    }
    fn resolve(&mut self, action: Action, outcome: Outcome) -> bool {
        self.resolve(action, outcome)
    }
    fn cancel(&mut self) -> bool {
        self.cancel()
    }
    fn progress(&self) -> Progress {
        self.progress()
    }
    fn steps(&self) -> usize {
        N
    }
}

#[derive(Clone, Copy)]
struct Number(u64);

impl TryConvert for Number {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "identity must be an unsigned Integer",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}

#[magnus::wrap(class = "SagaMachines::Action", free_immediately, size)]
struct Handle {
    owner: Arc<()>,
    action: Action,
}

impl Handle {
    const fn step(&self) -> usize {
        self.action.step()
    }
    fn kind(ruby: &Ruby, this: &Self) -> Symbol {
        ruby.to_symbol(match this.action.kind() {
            Kind::Forward => "forward",
            _ => "compensate",
        })
    }
    fn same_key(&self, other: Obj<Self>) -> bool {
        self.action.key() == other.action.key()
    }
}

#[magnus::wrap(class = "SagaMachines::Saga", free_immediately, size)]
struct Wrapper {
    owner: Arc<()>,
    core: RefCell<Box<dyn Policy>>,
}

impl Wrapper {
    fn new(ruby: &Ruby, args: &[Value]) -> Result<Self, Error> {
        let args = scan_args::<(Number,), (), (), (), RHash, ()>(args)?;
        let (id,) = args.required;
        let (steps,) = get_kwargs::<_, (Number,), (), ()>(args.keywords, &["steps"], &[])?.required;
        let core: Box<dyn Policy> = match steps.0 {
            0 => Box::new(Saga::<0>::new(id.0)),
            1 => Box::new(Saga::<1>::new(id.0)),
            2 => Box::new(Saga::<2>::new(id.0)),
            3 => Box::new(Saga::<3>::new(id.0)),
            4 => Box::new(Saga::<4>::new(id.0)),
            8 => Box::new(Saga::<8>::new(id.0)),
            16 => Box::new(Saga::<16>::new(id.0)),
            _ => {
                return Err(Error::new(
                    ruby.exception_arg_error(),
                    "supported exact steps: 0, 1, 2, 3, 4, 8, 16",
                ));
            }
        };
        Ok(Self {
            owner: Arc::new(()),
            core: RefCell::new(core),
        })
    }

    fn next_action(this: Obj<Self>) -> Result<Option<Handle>, Error> {
        this.check_frozen()?;
        let action = this.core.borrow_mut().next_action();
        Ok(action.map(|action| Handle {
            owner: Arc::clone(&this.owner),
            action,
        }))
    }

    fn resolve(
        ruby: &Ruby,
        this: Obj<Self>,
        handle: Obj<Handle>,
        outcome: Symbol,
    ) -> Result<bool, Error> {
        this.check_frozen()?;
        let outcome = match outcome.name()?.as_ref() {
            "success" => Outcome::Success,
            "failure" => Outcome::Failure,
            _ => {
                return Err(Error::new(
                    ruby.exception_arg_error(),
                    "outcome must be :success or :failure (known effects only)",
                ));
            }
        };
        if !Arc::ptr_eq(&this.owner, &handle.owner) {
            return Ok(false);
        }
        Ok(this.core.borrow_mut().resolve(handle.action, outcome))
    }

    fn cancel(this: Obj<Self>) -> Result<bool, Error> {
        this.check_frozen()?;
        Ok(this.core.borrow_mut().cancel())
    }

    fn progress(ruby: &Ruby, this: &Self) -> (Symbol, Option<usize>) {
        let progress = this.core.borrow().progress();
        match progress {
            Progress::Forward(step) => (ruby.to_symbol("forward"), Some(step)),
            Progress::Compensating(step) => (ruby.to_symbol("compensating"), Some(step)),
            Progress::Complete => (ruby.to_symbol("complete"), None),
            _ => (ruby.to_symbol("aborted"), None),
        }
    }

    fn steps(&self) -> usize {
        self.core.borrow().steps()
    }
}
