//! Values read back from Ruby. Each must be exactly its class (`TypeError`
//! otherwise, a subclass included); its fields are read natively and checked
//! again by the core's constructors (`RangeError`), so a forged value never
//! reaches the core and never earns a refusal symbol.
use magnus::{Error, RStruct, Ruby, Symbol, TryConvert, Value, prelude::*};
use saga_machines::{Action, ActionKey, AttemptId, Progress, SagaId, Settlement, Snapshot};

use crate::{
    classes::{Classes, exact, is_instance_of},
    names::{self, Number, Scope, wrong_type},
};

/// `Snapshot`'s five fields, in the core's order.
pub type SnapshotFields = (Value, Value, Value, Value, Value);

fn symbol(ruby: &Ruby, value: Value) -> Result<Symbol, Error> {
    Symbol::from_value(value).ok_or_else(|| wrong_type(ruby, "expected a Symbol"))
}

fn step(ruby: &Ruby, value: Value) -> Result<usize, Error> {
    names::step(ruby, Number::try_convert(value)?)
}

pub fn saga_id(ruby: &Ruby, classes: Classes, value: Value) -> Result<SagaId, Error> {
    let message = "expected a SagaMachines::SagaId";
    let fields = exact(ruby, value, classes.saga_id, message)?;
    Ok(SagaId::from_raw(fields.aref::<_, Number>(0)?.0))
}

pub fn attempt_id(ruby: &Ruby, classes: Classes, value: Value) -> Result<AttemptId, Error> {
    let message = "expected a SagaMachines::AttemptId";
    let fields = exact(ruby, value, classes.attempt_id, message)?;
    names::attempt(ruby, fields.aref(0)?)
}

/// An `ActionKey` from its three fields: what `ActionKey.new` checks.
pub fn key_from(
    ruby: &Ruby,
    classes: Classes,
    (saga, at, kind): (Value, Value, Value),
) -> Result<ActionKey, Error> {
    let saga = saga_id(ruby, classes, saga)?;
    let kind = names::kind(ruby, symbol(ruby, kind)?)?;
    Ok(ActionKey::new(saga, step(ruby, at)?, kind))
}

fn key(ruby: &Ruby, classes: Classes, value: Value) -> Result<ActionKey, Error> {
    let message = "expected a SagaMachines::ActionKey";
    let fields = exact(ruby, value, classes.action_key, message)?;
    key_from(
        ruby,
        classes,
        (fields.aref(0)?, fields.aref(1)?, fields.aref(2)?),
    )
}

/// An `Action` from its two fields: what `Action.new` checks.
pub fn action_from(
    ruby: &Ruby,
    classes: Classes,
    (action_key, attempt): (Value, Value),
) -> Result<Action, Error> {
    let action_key = key(ruby, classes, action_key)?;
    Ok(Action::new(action_key, attempt_id(ruby, classes, attempt)?))
}

pub fn action(ruby: &Ruby, classes: Classes, value: Value) -> Result<Action, Error> {
    let fields = exact(
        ruby,
        value,
        classes.action,
        "expected a SagaMachines::Action",
    )?;
    action_from(ruby, classes, (fields.aref(0)?, fields.aref(1)?))
}

/// A `Settlement` from its two fields: what `Settlement.new` checks.
pub fn settlement_from(
    ruby: &Ruby,
    classes: Classes,
    (settled, outcome): (Value, Value),
) -> Result<Settlement, Error> {
    Ok(Settlement {
        action: action(ruby, classes, settled)?,
        outcome: names::outcome(ruby, symbol(ruby, outcome)?)?,
    })
}

fn settlement(ruby: &Ruby, classes: Classes, value: Value) -> Result<Settlement, Error> {
    let message = "expected a SagaMachines::Settlement";
    let fields = exact(ruby, value, classes.settlement, message)?;
    settlement_from(ruby, classes, (fields.aref(0)?, fields.aref(1)?))
}

/// A Forward, a Compensating, `:complete` or `:aborted`.
fn progress(ruby: &Ruby, classes: Classes, value: Value) -> Result<Progress, Error> {
    if let Some(name) = Symbol::from_value(value) {
        return match name.name()?.as_ref() {
            "complete" => Ok(Progress::Complete),
            "aborted" => Ok(Progress::Aborted),
            _ => Err(Error::new(
                ruby.exception_range_error(),
                "progress must be a Forward, a Compensating, :complete or :aborted",
            )),
        };
    }
    let stepped = |make: fn(usize) -> Progress| {
        let fields = RStruct::from_value(value)
            .ok_or_else(|| wrong_type(ruby, "expected a Forward or a Compensating"))?;
        Ok(make(step(ruby, fields.aref(0)?)?))
    };
    if is_instance_of(value, classes.forward) {
        stepped(Progress::Forward)
    } else if is_instance_of(value, classes.compensating) {
        stepped(Progress::Compensating)
    } else {
        Err(wrong_type(
            ruby,
            "expected a Forward, a Compensating, :complete or :aborted",
        ))
    }
}

/// A `Snapshot` from its five fields, through `Snapshot::new`: what
/// `Snapshot.new` checks.
pub fn snapshot_from(
    ruby: &Ruby,
    classes: Classes,
    (id, scope, at, issued, settled): SnapshotFields,
) -> Result<Snapshot, Error> {
    let id = saga_id(ruby, classes, id)?;
    let scope = Scope::try_convert(scope)?;
    let at = progress(ruby, classes, at)?;
    let issued = Number::try_convert(issued)?;
    let settled = if settled.is_nil() {
        None
    } else {
        Some(settlement(ruby, classes, settled)?)
    };
    Snapshot::new(id, scope.0, at, issued.0, settled).ok_or_else(|| {
        Error::new(
            ruby.exception_range_error(),
            "issued must be in 0..2**48-1 and the settlement this saga's, of its scope, \
             within issued",
        )
    })
}

pub fn snapshot(ruby: &Ruby, classes: Classes, value: Value) -> Result<Snapshot, Error> {
    let message = "expected a SagaMachines::Snapshot";
    let fields = exact(ruby, value, classes.snapshot, message)?;
    let read = |index: usize| fields.aref::<_, Value>(index);
    snapshot_from(
        ruby,
        classes,
        (read(0)?, read(1)?, read(2)?, read(3)?, read(4)?),
    )
}
