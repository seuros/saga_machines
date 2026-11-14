//! Private native entry point.
use crate::{STEPS, Wrapper, from_bytes, hooks};
use magnus::{Error, Ruby, function, method, prelude::*};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("SagaMachines")?;
    crate::classes::resolve(ruby, module)?;
    let steps = ruby.ary_from_iter(STEPS);
    steps.freeze();
    module.const_set("STEPS", steps)?;
    let class = module.define_class("Saga", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("native_with_scope", function!(Wrapper::with_scope, 3))?;
    class.define_singleton_method("native_restored", function!(Wrapper::restored, 3))?;
    class.define_singleton_method("native_scope_of", function!(hooks::attempt_scope, 1))?;
    class.define_singleton_method("native_saga_id", function!(hooks::saga_id, 1))?;
    class.define_singleton_method("native_step", function!(hooks::step, 1))?;
    class.define_singleton_method("native_key", function!(hooks::check_key, 3))?;
    class.define_singleton_method("native_action", function!(hooks::check_action, 2))?;
    class.define_singleton_method("native_settlement", function!(hooks::check_settlement, 2))?;
    class.define_singleton_method("native_snapshot", function!(hooks::check_snapshot, 5))?;
    class.define_singleton_method("native_from_bytes", function!(from_bytes, 1))?;
    class.define_method("next_action", method!(Wrapper::next_action, 0))?;
    class.define_private_method("native_resolve", method!(Wrapper::resolve, 2))?;
    class.define_private_method("native_snapshot", method!(Wrapper::snapshot, 0))?;
    class.define_private_method("native_pending", method!(Wrapper::pending, 0))?;
    class.define_method("cancel", method!(Wrapper::cancel, 0))?;
    class.define_method("steps", method!(Wrapper::steps, 0))?;
    Ok(())
}
