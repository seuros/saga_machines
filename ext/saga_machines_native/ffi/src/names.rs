//! Ruby value conversion and the symbol names of the core's enums. Every
//! match on a core enum lists its variants: a new one fails to compile here.
use magnus::{Error, Ruby, Symbol, Value};
use matryona_machines::ruby::{SymbolNames, Symbols, integer, symbol};
use saga_machines::{
    Action, AttemptId, Cancellation, Kind, Outcome, Progress, Refusal, Resolution, Settlement,
};

/// A step: an Integer a `usize` holds.
pub fn step(value: Value) -> Result<usize, Error> {
    integer(value, "step")
}

matryona_machines::symbols! {
    /// The core's action kinds as Symbols.
    pub Kinds for Kind as "kind" {
        Forward => "forward",
        Compensate => "compensate",
    }
}

matryona_machines::symbols! {
    /// The core's outcomes as Symbols: known effects only.
    pub Outcomes for Outcome as "outcome" {
        Success => "success",
        Failure => "failure",
    }
}

matryona_machines::symbols! {
    /// The core's progress as Symbols: a step's direction, or the end.
    /// `:complete` and `:aborted` read back; a step needs its value.
    pub Progresses for Progress as "progress" => names_only {
        Forward(..) => "forward",
        Compensating(..) => "compensating",
        Complete => "complete",
        Aborted => "aborted",
    }
}

matryona_machines::symbols! {
    /// The core's refusals as Symbols: the snake case of each variant.
    pub Refusals for Refusal as "a refusal" {
        Exhausted => "exhausted",
        Stale => "stale",
        Mismatch => "mismatch",
        Busy => "busy",
        Concluded => "concluded",
    }
}

/// The core id an attempt's Integer form names; `RangeError` for a
/// sequence of zero, which no saga issues.
pub fn attempt(ruby: &Ruby, raw: Value) -> Result<AttemptId, Error> {
    AttemptId::from_raw(integer(raw, "raw")?).ok_or_else(|| {
        Error::new(
            ruby.exception_range_error(),
            "an attempt's sequence (the low 48 bits) is at least 1",
        )
    })
}

pub fn action_row(ruby: &Ruby, action: Action) -> (u64, usize, Symbol, u64) {
    let key = action.key();
    let kind = Kinds::symbol(ruby, key.kind());
    (key.saga().raw(), key.step(), kind, action.attempt().raw())
}

pub fn settled_row(ruby: &Ruby, settled: Settlement) -> (u64, usize, Symbol, u64, Symbol) {
    let (saga, step, kind, attempt) = action_row(ruby, settled.action);
    let outcome = Outcomes::symbol(ruby, settled.outcome);
    (saga, step, kind, attempt, outcome)
}

pub fn progress_row(ruby: &Ruby, progress: Progress) -> (Symbol, Option<usize>) {
    let step = match progress {
        Progress::Forward(step) | Progress::Compensating(step) => Some(step),
        Progress::Complete | Progress::Aborted => None,
    };
    (symbol(ruby, Progresses::name(&progress)), step)
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
