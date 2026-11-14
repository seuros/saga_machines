//! The value classes, resolved once at init and pinned. The exact-class
//! reads and clone-then-keep are `matryona_machines::ruby`'s.
use magnus::{Error, RModule, Ruby};
use matryona_machines::ruby::{Class, Registry};

/// The `Data` classes the binding builds and reads.
pub struct Classes {
    pub saga_id: Class,
    pub attempt_id: Class,
    pub action_key: Class,
    pub action: Class,
    pub forward: Class,
    pub compensating: Class,
    pub settlement: Class,
    pub snapshot: Class,
}

static CLASSES: Registry<Classes> = Registry::new("saga_machines");

/// Resolves the classes `lib/saga_machines.rb` defined before it loaded the
/// extension. Called once, from init; never by name again.
pub fn resolve(ruby: &Ruby, module: RModule) -> Result<(), Error> {
    let class = |name| Class::resolve(ruby, module, name);
    let classes = Classes {
        saga_id: class("SagaId")?,
        attempt_id: class("AttemptId")?,
        action_key: class("ActionKey")?,
        action: class("Action")?,
        forward: class("Forward")?,
        compensating: class("Compensating")?,
        settlement: class("Settlement")?,
        snapshot: class("Snapshot")?,
    };
    CLASSES.set(ruby, classes).map(drop)
}

/// The classes resolved at init.
pub fn classes(ruby: &Ruby) -> Result<&'static Classes, Error> {
    CLASSES.get(ruby)
}
