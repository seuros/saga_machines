//! Private native entry point.
use crate::{Handle, Wrapper};
use magnus::{Error, Ruby, function, method, prelude::*};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("SagaMachines")?;
    let action = module.define_class("Action", ruby.class_object())?;
    action.undef_default_alloc_func();
    action.define_method("step", method!(Handle::step, 0))?;
    action.define_method("kind", method!(Handle::kind, 0))?;
    action.define_method("same_key?", method!(Handle::same_key, 1))?;
    let class = module.define_class("Saga", ruby.class_object())?;
    class.undef_default_alloc_func();
    class.define_singleton_method("new", function!(Wrapper::new, -1))?;
    class.define_method("next_action", method!(Wrapper::next_action, 0))?;
    class.define_method("resolve", method!(Wrapper::resolve, 2))?;
    class.define_method("cancel", method!(Wrapper::cancel, 0))?;
    class.define_method("progress", method!(Wrapper::progress, 0))?;
    class.define_method("steps", method!(Wrapper::steps, 0))?;
    Ok(())
}
