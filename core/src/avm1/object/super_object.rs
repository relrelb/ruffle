//! Special object that implements `super`

use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::function::Executable;
use crate::avm1::object::script_object::TYPE_OF_OBJECT;
use crate::avm1::object::search_prototype;
use crate::avm1::property::Attribute;
use crate::avm1::{Object, ObjectPtr, ScriptObject, TObject, Value};
use crate::avm_warn;
use crate::display_object::DisplayObject;
use gc_arena::{Collect, GcCell, MutationContext};
use std::borrow::Cow;

/// Implementation of the `super` object in AS2.
///
/// A `SuperObject` references all data from another object, but with one layer
/// of prototyping removed. It's as if the given object had been constructed
/// with its parent class.
#[derive(Copy, Clone, Collect, Debug)]
#[collect(no_drop)]
pub struct SuperObject<'gc>(GcCell<'gc, SuperObjectData<'gc>>);

#[derive(Clone, Collect, Debug)]
#[collect(no_drop)]
pub struct SuperObjectData<'gc> {
    /// The object present as `this` throughout the superchain.
    this: Object<'gc>,

    /// The `proto` that the currently-executing method was pulled from.
    base_proto: Object<'gc>,
}

impl<'gc> SuperObject<'gc> {
    /// Construct a `super` for an incoming stack frame.
    ///
    /// `this` and `base_proto` must be the values provided to
    /// `Executable.exec`.
    ///
    /// NOTE: This function must not borrow any `GcCell` data as it is
    /// sometimes called while mutable borrows are held on cells. Specifically,
    /// `Object.call_setter` will panic if this function attempts to borrow
    /// *any* objects.
    pub fn new(
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        base_proto: Object<'gc>,
    ) -> Self {
        Self(GcCell::allocate(
            activation.context.gc_context,
            SuperObjectData {
                this,
                base_proto,
            },
        ))
    }

    /// Retrieve the prototype that `super` should be pulling from.
    fn super_proto(self) -> Value<'gc> {
        self.0.read().base_proto.proto()
    }
}

impl<'gc> TObject<'gc> for SuperObject<'gc> {
    fn get_local(
        &self,
        _name: &str,
        _activation: &mut Activation<'_, 'gc, '_>,
        _this: Object<'gc>,
    ) -> Option<Result<Value<'gc>, Error<'gc>>> {
        Some(Ok(Value::Undefined))
    }

    fn set_local(
        &self,
        _name: &str,
        _value: Value<'gc>,
        _activation: &mut Activation<'_, 'gc, '_>,
        _this: Object<'gc>,
        _base_proto: Option<Object<'gc>>,
    ) -> Result<(), Error<'gc>> {
        //TODO: What happens if you set `super.__proto__`?
        Ok(())
    }

    fn call(
        &self,
        name: &str,
        activation: &mut Activation<'_, 'gc, '_>,
        _this: Object<'gc>,
        _base_proto: Option<Object<'gc>>,
        args: &[Value<'gc>],
    ) -> Result<Value<'gc>, Error<'gc>> {
        if let Value::Object(super_proto) = self.super_proto() {
            let constructor = super_proto
                .get("__constructor__", activation)?
                .coerce_to_object(activation);
            let this = self.0.read().this;
            constructor.call(name, activation, this, Some(super_proto), args)
        } else {
            Ok(Value::Undefined)
        }
    }

    fn call_method(
        &self,
        name: &str,
        args: &[Value<'gc>],
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error<'gc>> {
        let this = self.0.read().this;
        let (method, base_proto) = search_prototype(self.super_proto(), name, activation, this)?;

        if method.is_primitive() {
            avm_warn!(activation, "Super method {} is not callable", name);
        }

        method.call(name, activation, this, base_proto, args)
    }

    fn call_setter(
        &self,
        name: &str,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Option<Object<'gc>> {
        self.0.read().this.call_setter(name, value, activation)
    }

    fn create_bare_object(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
    ) -> Result<Object<'gc>, Error<'gc>> {
        if let Value::Object(proto) = self.proto() {
            proto.create_bare_object(activation, this)
        } else {
            // TODO: What happens when you `new super` but there's no
            // super? Is this code even reachable?!
            self.0.read().this.create_bare_object(activation, this)
        }
    }

    fn delete(&self, _activation: &mut Activation<'_, 'gc, '_>, _name: &str) -> bool {
        //`super` cannot have properties deleted from it
        false
    }

    fn proto(&self) -> Value<'gc> {
        self.super_proto()
    }

    fn set_proto(&self, gc_context: MutationContext<'gc, '_>, prototype: Value<'gc>) {
        if let Value::Object(prototype) = prototype {
            self.0.write(gc_context).base_proto = prototype;
        }
    }

    fn define_value(
        &self,
        _gc_context: MutationContext<'gc, '_>,
        _name: &str,
        _value: Value<'gc>,
        _attributes: Attribute,
    ) {
        //`super` cannot have values defined on it
    }

    fn set_attributes(
        &self,
        _gc_context: MutationContext<'gc, '_>,
        _name: Option<&str>,
        _set_attributes: Attribute,
        _clear_attributes: Attribute,
    ) {
        //TODO: Does ASSetPropFlags work on `super`? What would it even work on?
    }

    fn add_property(
        &self,
        _gc_context: MutationContext<'gc, '_>,
        _name: &str,
        _get: Object<'gc>,
        _set: Option<Object<'gc>>,
        _attributes: Attribute,
    ) {
        //`super` cannot have properties defined on it
    }

    fn add_property_with_case(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _name: &str,
        _get: Object<'gc>,
        _set: Option<Object<'gc>>,
        _attributes: Attribute,
    ) {
        //`super` cannot have properties defined on it
    }

    fn set_watcher(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _name: Cow<str>,
        _callback: Object<'gc>,
        _user_data: Value<'gc>,
    ) {
        //`super` cannot have properties defined on it
    }

    fn remove_watcher(&self, _activation: &mut Activation<'_, 'gc, '_>, _name: Cow<str>) -> bool {
        //`super` cannot have properties defined on it
        false
    }

    fn has_property(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        self.0.read().this.has_property(activation, name)
    }

    fn has_own_property(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        self.0.read().this.has_own_property(activation, name)
    }

    fn has_own_virtual(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        self.0.read().this.has_own_virtual(activation, name)
    }

    fn is_property_enumerable(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        self.0.read().this.is_property_enumerable(activation, name)
    }

    fn get_keys(&self, _activation: &mut Activation<'_, 'gc, '_>) -> Vec<String> {
        vec![]
    }

    fn type_of(&self) -> &'static str {
        TYPE_OF_OBJECT
    }

    fn length(&self, _activation: &mut Activation<'_, 'gc, '_>) -> Result<i32, Error<'gc>> {
        Ok(0)
    }

    fn set_length(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _length: i32,
    ) -> Result<(), Error<'gc>> {
        Ok(())
    }

    fn has_element(&self, _activation: &mut Activation<'_, 'gc, '_>, _index: i32) -> bool {
        false
    }

    fn get_element(&self, _activation: &mut Activation<'_, 'gc, '_>, _index: i32) -> Value<'gc> {
        Value::Undefined
    }

    fn set_element(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _index: i32,
        _value: Value<'gc>,
    ) -> Result<(), Error<'gc>> {
        Ok(())
    }

    fn delete_element(&self, _activation: &mut Activation<'_, 'gc, '_>, _index: i32) -> bool {
        false
    }

    fn interfaces(&self) -> Vec<Object<'gc>> {
        //`super` does not implement interfaces
        vec![]
    }

    fn set_interfaces(&self, _gc_context: MutationContext<'gc, '_>, _iface_list: Vec<Object<'gc>>) {
        //`super` probably cannot have interfaces set on it
    }

    fn as_script_object(&self) -> Option<ScriptObject<'gc>> {
        None
    }

    fn as_super_object(&self) -> Option<SuperObject<'gc>> {
        Some(*self)
    }

    fn as_display_object(&self) -> Option<DisplayObject<'gc>> {
        //`super` actually can be used to invoke MovieClip methods
        self.0.read().this.as_display_object()
    }

    fn as_executable(&self) -> Option<Executable<'gc>> {
        //well, `super` *can* be called...
        //...but `super_constr` needs an avm and context in order to get called.
        //ergo, we can't downcast.
        None
    }

    fn as_ptr(&self) -> *const ObjectPtr {
        self.0.as_ptr() as *const ObjectPtr
    }
}
