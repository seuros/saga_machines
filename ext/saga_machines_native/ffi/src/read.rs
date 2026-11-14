//! Values read back from Ruby. Each must be exactly its class (`TypeError`
//! otherwise, a subclass included); its fields are read natively and checked
//! again by the core's constructors (`RangeError`), so a forged value never
//! reaches the core and never earns a refusal symbol.
use magnus::{Error, Ruby, Symbol, Value};
use matryona_machines::ruby::{Symbols, exact_fields, integer, optional_fields, which};
use saga_machines::{Action, ActionKey, AttemptId, Progress, SagaId, Settlement, Snapshot};

use crate::{
    classes::Classes,
    names::{self, Kinds, Outcomes},
};

/// `Snapshot`'s five fields, in the core's order.
pub type SnapshotFields = [Value; 5];

pub fn saga_id(ruby: &Ruby, classes: &Classes, value: Value) -> Result<SagaId, Error> {
    let message = "expected a SagaMachines::SagaId";
    let [raw] = exact_fields::<1>(ruby, value, classes.saga_id.get(ruby), message)?;
    integer(raw, "raw").map(SagaId::from_raw)
}

pub fn attempt_id(ruby: &Ruby, classes: &Classes, value: Value) -> Result<AttemptId, Error> {
    let message = "expected a SagaMachines::AttemptId";
    let [raw] = exact_fields::<1>(ruby, value, classes.attempt_id.get(ruby), message)?;
    names::attempt(ruby, raw)
}

/// An `ActionKey` from its three fields: what `ActionKey.new` checks.
pub fn key_from(
    ruby: &Ruby,
    classes: &Classes,
    [saga, at, kind]: [Value; 3],
) -> Result<ActionKey, Error> {
    let saga = saga_id(ruby, classes, saga)?;
    let kind = Kinds::parse(ruby, kind)?;
    Ok(ActionKey::new(saga, names::step(at)?, kind))
}

fn key(ruby: &Ruby, classes: &Classes, value: Value) -> Result<ActionKey, Error> {
    let message = "expected a SagaMachines::ActionKey";
    let fields = exact_fields::<3>(ruby, value, classes.action_key.get(ruby), message)?;
    key_from(ruby, classes, fields)
}

/// An `Action` from its two fields: what `Action.new` checks.
pub fn action_from(
    ruby: &Ruby,
    classes: &Classes,
    [action_key, attempt]: [Value; 2],
) -> Result<Action, Error> {
    let action_key = key(ruby, classes, action_key)?;
    Ok(Action::new(action_key, attempt_id(ruby, classes, attempt)?))
}

pub fn action(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Action, Error> {
    let message = "expected a SagaMachines::Action";
    let fields = exact_fields::<2>(ruby, value, classes.action.get(ruby), message)?;
    action_from(ruby, classes, fields)
}

/// A `Settlement` from its two fields: what `Settlement.new` checks.
pub fn settlement_from(
    ruby: &Ruby,
    classes: &Classes,
    [settled, outcome]: [Value; 2],
) -> Result<Settlement, Error> {
    Ok(Settlement {
        action: action(ruby, classes, settled)?,
        outcome: Outcomes::parse(ruby, outcome)?,
    })
}

/// A Forward, a Compensating, `:complete` or `:aborted`.
fn progress(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Progress, Error> {
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
    let message = "expected a Forward, a Compensating, :complete or :aborted";
    let (forward, compensating) = (classes.forward.get(ruby), classes.compensating.get(ruby));
    let (make, class): (fn(usize) -> Progress, _) = match which(value, &[forward, compensating]) {
        Some(0) => (Progress::Forward, forward),
        Some(_) => (Progress::Compensating, compensating),
        None => return Err(Error::new(ruby.exception_type_error(), message)),
    };
    let [step] = exact_fields::<1>(ruby, value, class, message)?;
    Ok(make(names::step(step)?))
}

/// A `Snapshot` from its five fields, through `Snapshot::new`: what
/// `Snapshot.new` checks.
pub fn snapshot_from(
    ruby: &Ruby,
    classes: &Classes,
    [id, scope, at, issued, settled]: SnapshotFields,
) -> Result<Snapshot, Error> {
    let id = saga_id(ruby, classes, id)?;
    let scope = integer(scope, "scope")?;
    let at = progress(ruby, classes, at)?;
    let issued = integer(issued, "issued")?;
    let message = "expected a SagaMachines::Settlement or nil";
    let class = classes.settlement.get(ruby);
    let settled = optional_fields::<2>(ruby, settled, class, message)?
        .map(|fields| settlement_from(ruby, classes, fields))
        .transpose()?;
    Snapshot::new(id, scope, at, issued, settled).ok_or_else(|| {
        Error::new(
            ruby.exception_range_error(),
            "issued must be in 0..2**48-1 and the settlement this saga's, of its scope, \
             within issued",
        )
    })
}

pub fn snapshot(ruby: &Ruby, classes: &Classes, value: Value) -> Result<Snapshot, Error> {
    let message = "expected a SagaMachines::Snapshot";
    let fields = exact_fields::<5>(ruby, value, classes.snapshot.get(ruby), message)?;
    snapshot_from(ruby, classes, fields)
}
