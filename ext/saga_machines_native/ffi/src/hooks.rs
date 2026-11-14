//! The private hooks the Data values validate through at construction, and
//! the `Action` `next_action` builds its answer as.
use magnus::{Error, Ruby, Value, prelude::*};
use matryona_machines::ruby::integer;
use saga_machines::{Action, AttemptId};

use crate::{
    classes::{Classes, classes},
    names,
    read::{self, SnapshotFields},
};

/// `SagaId.new`: an Integer in `0..2**64-1`, any of which is an identity.
pub fn saga_id(raw: Value) -> Result<u64, Error> {
    integer(raw, "raw")
}

/// `AttemptId.new` and `AttemptId#scope`: the scope of a buildable attempt.
pub fn attempt_scope(ruby: &Ruby, raw: Value) -> Result<u16, Error> {
    names::attempt(ruby, raw).map(AttemptId::scope)
}

/// `Forward.new`, `Compensating.new`: a step.
pub fn step(step: Value) -> Result<usize, Error> {
    names::step(step)
}

/// `ActionKey.new`: exactly a `SagaId`, a step, `:forward` or `:compensate`.
pub fn check_key(ruby: &Ruby, saga: Value, step: Value, kind: Value) -> Result<(), Error> {
    read::key_from(ruby, classes(ruby)?, [saga, step, kind]).map(drop)
}

/// `Action.new`: exactly an `ActionKey` and an `AttemptId`, each checked again.
pub fn check_action(ruby: &Ruby, key: Value, attempt: Value) -> Result<(), Error> {
    read::action_from(ruby, classes(ruby)?, [key, attempt]).map(drop)
}

/// `Settlement.new`: exactly an `Action`, and `:success` or `:failure`.
pub fn check_settlement(ruby: &Ruby, action: Value, outcome: Value) -> Result<(), Error> {
    read::settlement_from(ruby, classes(ruby)?, [action, outcome]).map(drop)
}

/// `Snapshot.new`: `RangeError` for a value `Snapshot::new` refuses.
pub fn check_snapshot(
    ruby: &Ruby,
    id: Value,
    scope: Value,
    progress: Value,
    issued: Value,
    settled: Value,
) -> Result<(), Error> {
    let fields: SnapshotFields = [id, scope, progress, issued, settled];
    read::snapshot_from(ruby, classes(ruby)?, fields).map(drop)
}

/// The Ruby `Action` for a core action the saga answered.
pub fn action(ruby: &Ruby, classes: &Classes, action: Action) -> Result<Value, Error> {
    let (saga, step, kind, attempt) = names::action_row(ruby, action);
    let saga: Value = classes.saga_id.get(ruby).funcall("new", (saga,))?;
    let attempt: Value = classes.attempt_id.get(ruby).funcall("new", (attempt,))?;
    let key: Value = classes
        .action_key
        .get(ruby)
        .funcall("new", (saga, step, kind))?;
    classes.action.get(ruby).funcall("new", (key, attempt))
}
