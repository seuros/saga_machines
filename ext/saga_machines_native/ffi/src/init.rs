//! Private native entry point.
use crate::{STEPS, Wrapper, from_bytes, scope_of};
use magnus::{Error, Ruby, function, method, prelude::*};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("SagaMachines")?;
    let steps = ruby.ary_from_iter(STEPS);
    steps.freeze();
    module.const_set("STEPS", steps)?;
    let class = module.define_class("Saga", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("native_with_scope", function!(Wrapper::with_scope, 3))?;
    class.define_singleton_method("native_restored", function!(Wrapper::restored, 3))?;
    class.define_singleton_method("native_scope_of", function!(scope_of, 1))?;
    class.define_singleton_method("native_from_bytes", function!(from_bytes, 1))?;
    class.define_private_method("native_next_action", method!(Wrapper::next_action, 0))?;
    class.define_private_method("native_resolve", method!(Wrapper::resolve, 2))?;
    class.define_private_method("native_snapshot", method!(Wrapper::snapshot, 0))?;
    class.define_private_method("native_pending", method!(Wrapper::pending, 0))?;
    class.define_method("cancel", method!(Wrapper::cancel, 0))?;
    class.define_method("steps", method!(Wrapper::steps, 0))?;
    Ok(())
}
