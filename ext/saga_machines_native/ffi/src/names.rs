//! Ruby value conversion and the symbol names of the core's enums. Every
//! match on a core enum lists its variants: a new one fails to compile here.
use magnus::{Error, Integer, Ruby, Symbol, TryConvert, Value};
use saga_machines::{
    Action, ActionKey, AttemptId, Cancellation, Kind, Outcome, Progress, Refusal, Resolution,
    SagaId, Settlement,
};

#[derive(Clone, Copy)]
pub struct Number(pub u64);

impl TryConvert for Number {
    fn try_convert(value: Value) -> Result<Self, Error> {
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                Ruby::get_with(value).exception_type_error(),
                "expected an Integer (unsigned 64-bit)",
            ));
        }
        u64::try_convert(value).map(Self)
    }
}

/// A scope: an Integer in `0..=65535`.
#[derive(Clone, Copy)]
pub struct Scope(pub u16);

impl TryConvert for Scope {
    fn try_convert(value: Value) -> Result<Self, Error> {
        let ruby = Ruby::get_with(value);
        if Integer::from_value(value).is_none() {
            return Err(Error::new(
                ruby.exception_type_error(),
                "scope must be an Integer",
            ));
        }
        u16::try_convert(value)
            .map(Self)
            .map_err(|_| Error::new(ruby.exception_range_error(), "scope must be in 0..=65535"))
    }
}

/// An action as Ruby passes it: saga id, step, kind, attempt.
pub type ActionRow = (Number, Number, Symbol, Number);
/// A settlement as Ruby passes it: the action's four fields, then the outcome.
pub type SettledRow = (Number, Number, Symbol, Number, Symbol);
/// A progress as Ruby passes it: the variant's name and its step, if any.
pub type ProgressRow = (Symbol, Option<Number>);

fn step(ruby: &Ruby, step: Number) -> Result<usize, Error> {
    usize::try_from(step.0).map_err(|_| Error::new(ruby.exception_range_error(), "step too large"))
}

pub const fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Forward => "forward",
        Kind::Compensate => "compensate",
    }
}

pub fn kind(ruby: &Ruby, name: Symbol) -> Result<Kind, Error> {
    match name.name()?.as_ref() {
        "forward" => Ok(Kind::Forward),
        "compensate" => Ok(Kind::Compensate),
        _ => Err(Error::new(
            ruby.exception_arg_error(),
            "kind must be :forward or :compensate",
        )),
    }
}

pub const fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Success => "success",
        Outcome::Failure => "failure",
    }
}

pub fn outcome(ruby: &Ruby, name: Symbol) -> Result<Outcome, Error> {
    match name.name()?.as_ref() {
        "success" => Ok(Outcome::Success),
        "failure" => Ok(Outcome::Failure),
        _ => Err(Error::new(
            ruby.exception_arg_error(),
            "outcome must be :success or :failure (known effects only)",
        )),
    }
}

pub fn action(ruby: &Ruby, (saga, at, name, attempt): ActionRow) -> Result<Action, Error> {
    let key = ActionKey::new(SagaId::from_raw(saga.0), step(ruby, at)?, kind(ruby, name)?);
    Ok(Action::new(key, AttemptId::from_raw(attempt.0)))
}

pub fn action_row(ruby: &Ruby, action: Action) -> (u64, usize, Symbol, u64) {
    let key = action.key();
    let kind = ruby.to_symbol(kind_name(key.kind()));
    (key.saga().raw(), key.step(), kind, action.attempt().raw())
}

pub fn settlement(ruby: &Ruby, row: SettledRow) -> Result<Settlement, Error> {
    let (saga, at, kind, attempt, name) = row;
    Ok(Settlement {
        action: action(ruby, (saga, at, kind, attempt))?,
        outcome: outcome(ruby, name)?,
    })
}

pub fn settled_row(ruby: &Ruby, settled: Settlement) -> (u64, usize, Symbol, u64, Symbol) {
    let (saga, step, kind, attempt) = action_row(ruby, settled.action);
    let outcome = ruby.to_symbol(outcome_name(settled.outcome));
    (saga, step, kind, attempt, outcome)
}

pub fn progress(ruby: &Ruby, (name, at): ProgressRow) -> Result<Progress, Error> {
    let missing = || {
        Error::new(
            ruby.exception_type_error(),
            "a progress step must be an Integer",
        )
    };
    let stepped = || step(ruby, at.ok_or_else(missing)?);
    match name.name()?.as_ref() {
        "forward" => Ok(Progress::Forward(stepped()?)),
        "compensating" => Ok(Progress::Compensating(stepped()?)),
        "complete" => Ok(Progress::Complete),
        "aborted" => Ok(Progress::Aborted),
        _ => Err(Error::new(
            ruby.exception_arg_error(),
            "progress must be a Forward, a Compensating, :complete or :aborted",
        )),
    }
}

pub fn progress_row(ruby: &Ruby, progress: Progress) -> (Symbol, Option<usize>) {
    let (name, step) = match progress {
        Progress::Forward(step) => ("forward", Some(step)),
        Progress::Compensating(step) => ("compensating", Some(step)),
        Progress::Complete => ("complete", None),
        Progress::Aborted => ("aborted", None),
    };
    (ruby.to_symbol(name), step)
}

pub const fn resolution(resolution: Resolution) -> &'static str {
    match resolution {
        Resolution::Resolved => "resolved",
        Resolution::AlreadyResolved => "already_resolved",
    }
}

pub const fn cancellation(cancellation: Cancellation) -> &'static str {
    match cancellation {
        Cancellation::Cancelled => "cancelled",
        Cancellation::AlreadyUnwinding => "already_unwinding",
    }
}

/// Ruby symbol name of a refusal: the snake case of the core variant.
pub const fn refusal(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Exhausted => "exhausted",
        Refusal::Stale => "stale",
        Refusal::Mismatch => "mismatch",
        Refusal::Busy => "busy",
        Refusal::Concluded => "concluded",
    }
}
