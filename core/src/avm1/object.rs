//! Object trait to expose objects to AVM

use crate::avm1::error::Error;
use crate::avm1::function::{Executable, ExecutionReason, FunctionObject};
use crate::avm1::object::shared_object::SharedObject;
use crate::avm1::object::super_object::SuperObject;
use crate::avm1::object::value_object::ValueObject;
use crate::avm1::property::Attribute;

use crate::avm1::activation::Activation;
use crate::avm1::object::array_object::ArrayObject;
use crate::avm1::object::bevel_filter::BevelFilterObject;
use crate::avm1::object::bitmap_data::BitmapDataObject;
use crate::avm1::object::blur_filter::BlurFilterObject;
use crate::avm1::object::color_matrix_filter::ColorMatrixFilterObject;
use crate::avm1::object::color_transform_object::ColorTransformObject;
use crate::avm1::object::convolution_filter::ConvolutionFilterObject;
use crate::avm1::object::date_object::DateObject;
use crate::avm1::object::displacement_map_filter::DisplacementMapFilterObject;
use crate::avm1::object::drop_shadow_filter::DropShadowFilterObject;
use crate::avm1::object::glow_filter::GlowFilterObject;
use crate::avm1::object::gradient_bevel_filter::GradientBevelFilterObject;
use crate::avm1::object::gradient_glow_filter::GradientGlowFilterObject;
use crate::avm1::object::transform_object::TransformObject;
use crate::avm1::object::xml_attributes_object::XmlAttributesObject;
use crate::avm1::object::xml_idmap_object::XmlIdMapObject;
use crate::avm1::object::xml_object::XmlObject;
use crate::avm1::{ScriptObject, SoundObject, StageObject, Value};
use crate::avm_warn;
use crate::display_object::DisplayObject;
use crate::xml::XmlNode;
use gc_arena::{Collect, MutationContext};
use ruffle_macros::enum_trait_object;
use std::borrow::Cow;
use std::fmt::Debug;

pub mod array_object;
pub mod bevel_filter;
pub mod bitmap_data;
pub mod blur_filter;
pub mod color_matrix_filter;
pub mod color_transform_object;
pub mod convolution_filter;
mod custom_object;
pub mod date_object;
pub mod displacement_map_filter;
pub mod drop_shadow_filter;
pub mod glow_filter;
pub mod gradient_bevel_filter;
pub mod gradient_glow_filter;
pub mod script_object;
pub mod shared_object;
pub mod sound_object;
pub mod stage_object;
pub mod super_object;
pub mod transform_object;
pub mod value_object;
pub mod xml_attributes_object;
pub mod xml_idmap_object;
pub mod xml_object;

/// Represents an object that can be directly interacted with by the AVM
/// runtime.
#[enum_trait_object(
    #[allow(clippy::enum_variant_names)]
    #[derive(Clone, Collect, Debug, Copy)]
    #[collect(no_drop)]
    pub enum Object<'gc> {
        ScriptObject(ScriptObject<'gc>),
        ArrayObject(ArrayObject<'gc>),
        SoundObject(SoundObject<'gc>),
        StageObject(StageObject<'gc>),
        SuperObject(SuperObject<'gc>),
        XmlObject(XmlObject<'gc>),
        XmlAttributesObject(XmlAttributesObject<'gc>),
        XmlIdMapObject(XmlIdMapObject<'gc>),
        ValueObject(ValueObject<'gc>),
        FunctionObject(FunctionObject<'gc>),
        SharedObject(SharedObject<'gc>),
        ColorTransformObject(ColorTransformObject<'gc>),
        TransformObject(TransformObject<'gc>),
        BlurFilterObject(BlurFilterObject<'gc>),
        BevelFilterObject(BevelFilterObject<'gc>),
        GlowFilterObject(GlowFilterObject<'gc>),
        DropShadowFilterObject(DropShadowFilterObject<'gc>),
        ColorMatrixFilterObject(ColorMatrixFilterObject<'gc>),
        DisplacementMapFilterObject(DisplacementMapFilterObject<'gc>),
        ConvolutionFilterObject(ConvolutionFilterObject<'gc>),
        GradientBevelFilterObject(GradientBevelFilterObject<'gc>),
        GradientGlowFilterObject(GradientGlowFilterObject<'gc>),
        DateObject(DateObject<'gc>),
        BitmapData(BitmapDataObject<'gc>),
    }
)]
pub trait TObject<'gc>: 'gc + Collect + Debug + Into<Object<'gc>> + Clone + Copy {
    /// Retrieve a named property from this object exclusively.
    ///
    /// This function takes a redundant `this` parameter which should be
    /// the object's own `GcCell`, so that it can pass it to user-defined
    /// overrides that may need to interact with the underlying object.
    ///
    /// This function should not inspect prototype chains. Instead, use `get`
    /// to do ordinary property look-up and resolution.
    fn get_local(
        &self,
        name: &str,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        depth: u8,
    ) -> Option<Result<Value<'gc>, Error<'gc>>>;

    /// Retrieve a named property from the object, or its prototype.
    fn get(
        &self,
        name: &str,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error<'gc>> {
        if name == "__proto__" {
            return Ok(self.proto());
        }

        let this = (*self).into();
        Ok(search_prototype(Value::Object(this), name, activation, this)?.0)
    }

    fn set_local(
        &self,
        name: &str,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        depth: u8,
    ) -> Result<(), Error<'gc>>;

    /// Set a named property on this object, or its prototype.
    fn set(
        &self,
        name: &str,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<(), Error<'gc>> {
        if name.is_empty() {
            return Ok(());
        }

        if name == "__proto__" {
            self.set_proto(activation.context.gc_context, value);
            return Ok(());
        }

        let this = (*self).into();
        if !self.has_own_property(activation, name) {
            // Before actually inserting a new property, we need to crawl the
            // prototype chain for virtual setters.
            let mut depth = 0;
            let mut proto = Value::Object(this);
            while let Value::Object(this_proto) = proto {
                if this_proto.has_own_virtual(activation, name) {
                    if let Some(setter) = this_proto.call_setter(name, value, activation) {
                        if let Some(exec) = setter.as_executable() {
                            let _ = exec.exec(
                                "[Setter]",
                                activation,
                                this,
                                depth,
                                &[value],
                                ExecutionReason::Special,
                                setter,
                            );
                        }
                    }
                    return Ok(());
                }

                proto = this_proto.proto();

                depth += 1;
                // TODO: max depth
            }
        }

        self.set_local(name, value, activation, this, 0)
    }

    /// Call the underlying object.
    ///
    /// This function takes a  `this` parameter which generally
    /// refers to the object which has this property, although
    /// it can be changed by `Function.apply`/`Function.call`.
    fn call(
        &self,
        name: &str,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        depth: u8,
        args: &[Value<'gc>],
    ) -> Result<Value<'gc>, Error<'gc>>;

    /// Construct the underlying object, if this is a valid constructor, and returns the result.
    /// Calling this on something other than a constructor will return a new Undefined object.
    fn construct(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        _args: &[Value<'gc>],
    ) -> Result<Value<'gc>, Error<'gc>> {
        Ok(Value::Undefined)
    }

    /// Takes an already existing object and performs this constructor (if valid) on it.
    fn construct_on_existing(
        &self,
        _activation: &mut Activation<'_, 'gc, '_>,
        mut _this: Object<'gc>,
        _args: &[Value<'gc>],
    ) -> Result<(), Error<'gc>> {
        Ok(())
    }

    /// Call a method on the object.
    ///
    /// It is highly recommended to use this convenience method to perform
    /// method calls. It is morally equivalent to an AVM1 `ActionCallMethod`
    /// opcode. It will take care of retrieving the method, calculating its
    /// base prototype for `super` calls, and providing it with the correct
    /// `this` parameter.
    fn call_method(
        &self,
        name: &str,
        depth: u8,
        args: &[Value<'gc>],
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error<'gc>> {
        let this = (*self).into();
        let (method, d) = search_prototype(Value::Object(this), name, activation, this)?;

        if method.is_primitive() {
            avm_warn!(activation, "Object method {} is not callable", name);
        }

        method.call(name, activation, this, depth + d.max(1), args)
    }

    /// Call a setter defined in this object.
    ///
    /// This function may return a `Executable` of the function to call; it
    /// should be resolved and discarded. Attempts to call a non-virtual setter
    /// or non-existent setter fail silently.
    ///
    /// The setter will be invoked with the provided `this`.
    fn call_setter(
        &self,
        name: &str,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Option<Object<'gc>>;

    /// Construct a host object of some kind and return its cell.
    ///
    /// As the first step in object construction, the `new` method is called on
    /// the prototype to initialize an object. The prototype may construct any
    /// object implementation it wants, with itself as the new object's proto.
    /// Then, the constructor is `call`ed with the new object as `this` to
    /// initialize the object.
    fn create_bare_object(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
    ) -> Result<Object<'gc>, Error<'gc>>;

    /// Delete a named property from the object.
    ///
    /// Returns false if the property cannot be deleted.
    fn delete(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool;

    /// Retrieve the `__proto__` of a given object.
    ///
    /// The proto is another object used to resolve methods across a class of
    /// multiple objects. It should also be accessible as `__proto__` from
    /// `get`.
    fn proto(&self) -> Value<'gc>;

    /// Sets the `__proto__` of a given object.
    ///
    /// The proto is another object used to resolve methods across a class of
    /// multiple objects. It should also be accessible as `__proto__` in
    /// `set`.
    fn set_proto(&self, gc_context: MutationContext<'gc, '_>, prototype: Value<'gc>);

    /// Define a value on an object.
    ///
    /// Unlike setting a value, this function is intended to replace any
    /// existing virtual or built-in properties already installed on a given
    /// object. As such, this should not run any setters; the resulting name
    /// slot should either be completely replaced with the value or completely
    /// untouched.
    ///
    /// It is not guaranteed that all objects accept value definitions,
    /// especially if a property name conflicts with a built-in property, such
    /// as `__proto__`.
    fn define_value(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: &str,
        value: Value<'gc>,
        attributes: Attribute,
    );

    /// Set the attributes of a given property.
    ///
    /// Leaving `name` unspecified allows setting all properties on a given
    /// object to the same set of properties.
    ///
    /// Attributes can be set, cleared, or left as-is using the pairs of `set_`
    /// and `clear_attributes` parameters.
    fn set_attributes(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: Option<&str>,
        set_attributes: Attribute,
        clear_attributes: Attribute,
    );

    /// Define a virtual property onto a given object.
    ///
    /// A virtual property is a set of get/set functions that are called when a
    /// given named property is retrieved or stored on an object. These
    /// functions are then responsible for providing or accepting the value
    /// that is given to or taken from the AVM.
    ///
    /// It is not guaranteed that all objects accept virtual properties,
    /// especially if a property name conflicts with a built-in property, such
    /// as `__proto__`.
    fn add_property(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: &str,
        get: Object<'gc>,
        set: Option<Object<'gc>>,
        attributes: Attribute,
    );

    /// Define a virtual property onto a given object.
    ///
    /// A virtual property is a set of get/set functions that are called when a
    /// given named property is retrieved or stored on an object. These
    /// functions are then responsible for providing or accepting the value
    /// that is given to or taken from the AVM.
    ///
    /// It is not guaranteed that all objects accept virtual properties,
    /// especially if a property name conflicts with a built-in property, such
    /// as `__proto__`.
    fn add_property_with_case(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        name: &str,
        get: Object<'gc>,
        set: Option<Object<'gc>>,
        attributes: Attribute,
    );

    /// Set the 'watcher' of a given property.
    ///
    /// The property does not need to exist at the time of this being called.
    fn set_watcher(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        name: Cow<str>,
        callback: Object<'gc>,
        user_data: Value<'gc>,
    );

    /// Removed any assigned 'watcher' from the given property.
    ///
    /// The return value will indicate if there was a watcher present before this method was
    /// called.
    fn remove_watcher(&self, activation: &mut Activation<'_, 'gc, '_>, name: Cow<str>) -> bool;

    /// Checks if the object has a given named property.
    fn has_property(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool;

    /// Checks if the object has a given named property on itself (and not,
    /// say, the object's prototype or superclass)
    fn has_own_property(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool;

    /// Checks if the object has a given named property on itself that is
    /// virtual.
    fn has_own_virtual(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool;

    /// Checks if a named property appears when enumerating the object.
    fn is_property_enumerable(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool;

    /// Enumerate the object.
    fn get_keys(&self, activation: &mut Activation<'_, 'gc, '_>) -> Vec<String>;

    /// Get the object's type string.
    fn type_of(&self) -> &'static str;

    /// Enumerate all interfaces implemented by this object.
    fn interfaces(&self) -> Vec<Object<'gc>>;

    /// Set the interface list for this object. (Only useful for prototypes.)
    fn set_interfaces(&self, gc_context: MutationContext<'gc, '_>, iface_list: Vec<Object<'gc>>);

    /// Determine if this object is an instance of a class.
    ///
    /// The class is provided in the form of its constructor function and the
    /// explicit prototype of that constructor function. It is assumed that
    /// they are already linked.
    ///
    /// Because ActionScript 2.0 added interfaces, this function cannot simply
    /// check the prototype chain and call it a day. Each interface represents
    /// a new, parallel prototype chain which also needs to be checked. You
    /// can't implement interfaces within interfaces (fortunately), but if you
    /// somehow could this would support that, too.
    fn is_instance_of(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        constructor: Object<'gc>,
        prototype: Object<'gc>,
    ) -> Result<bool, Error<'gc>> {
        let mut proto_stack = vec![];
        if let Value::Object(p) = self.proto() {
            proto_stack.push(p);
        }

        while let Some(this_proto) = proto_stack.pop() {
            if Object::ptr_eq(this_proto, prototype) {
                return Ok(true);
            }

            if let Value::Object(p) = this_proto.proto() {
                proto_stack.push(p);
            }

            if activation.swf_version() >= 7 {
                for interface in this_proto.interfaces() {
                    if Object::ptr_eq(interface, constructor) {
                        return Ok(true);
                    }

                    if let Value::Object(o) = interface.get("prototype", activation)? {
                        proto_stack.push(o);
                    }
                }
            }
        }

        Ok(false)
    }

    /// Get the underlying script object, if it exists.
    fn as_script_object(&self) -> Option<ScriptObject<'gc>>;

    /// Get the underlying array object, if it exists.
    fn as_array_object(&self) -> Option<ArrayObject<'gc>> {
        None
    }

    /// Get the underlying sound object, if it exists.
    fn as_sound_object(&self) -> Option<SoundObject<'gc>> {
        None
    }

    /// Get the underlying stage object, if it exists.
    fn as_stage_object(&self) -> Option<StageObject<'gc>> {
        None
    }

    /// Get the underlying super object, if it exists.
    fn as_super_object(&self) -> Option<SuperObject<'gc>> {
        None
    }

    /// Get the underlying display node for this object, if it exists.
    fn as_display_object(&self) -> Option<DisplayObject<'gc>> {
        None
    }

    /// Get the underlying executable for this object, if it exists.
    fn as_executable(&self) -> Option<Executable<'gc>> {
        None
    }

    /// Get the underlying XML node for this object, if it exists.
    fn as_xml_node(&self) -> Option<XmlNode<'gc>> {
        None
    }

    /// Get the underlying `ValueObject`, if it exists.
    fn as_value_object(&self) -> Option<ValueObject<'gc>> {
        None
    }

    /// Get the underlying `SharedObject`, if it exists
    fn as_shared_object(&self) -> Option<SharedObject<'gc>> {
        None
    }

    /// Get the underlying `DateObject`, if it exists
    fn as_date_object(&self) -> Option<DateObject<'gc>> {
        None
    }

    /// Get the underlying `ColorTransformObject`, if it exists
    fn as_color_transform_object(&self) -> Option<ColorTransformObject<'gc>> {
        None
    }

    /// Get the underlying `TransformObject`, if it exists
    fn as_transform_object(&self) -> Option<TransformObject<'gc>> {
        None
    }

    /// Get the underlying `BlurFilterObject`, if it exists
    fn as_blur_filter_object(&self) -> Option<BlurFilterObject<'gc>> {
        None
    }

    /// Get the underlying `BevelFilterObject`, if it exists
    fn as_bevel_filter_object(&self) -> Option<BevelFilterObject<'gc>> {
        None
    }

    /// Get the underlying `GlowFilterObject`, if it exists
    fn as_glow_filter_object(&self) -> Option<GlowFilterObject<'gc>> {
        None
    }

    /// Get the underlying `DropShadowFilterObject`, if it exists
    fn as_drop_shadow_filter_object(&self) -> Option<DropShadowFilterObject<'gc>> {
        None
    }

    /// Get the underlying `ColorMatrixFilterObject`, if it exists
    fn as_color_matrix_filter_object(&self) -> Option<ColorMatrixFilterObject<'gc>> {
        None
    }

    /// Get the underlying `DisplacementMapFilterObject`, if it exists
    fn as_displacement_map_filter_object(&self) -> Option<DisplacementMapFilterObject<'gc>> {
        None
    }

    /// Get the underlying `ConvolutionFilterObject`, if it exists
    fn as_convolution_filter_object(&self) -> Option<ConvolutionFilterObject<'gc>> {
        None
    }

    /// Get the underlying `GradientBevelFilterObject`, if it exists
    fn as_gradient_bevel_filter_object(&self) -> Option<GradientBevelFilterObject<'gc>> {
        None
    }

    /// Get the underlying `GradientGlowFilterObject`, if it exists
    fn as_gradient_glow_filter_object(&self) -> Option<GradientGlowFilterObject<'gc>> {
        None
    }

    /// Get the underlying `BitmapDataObject`, if it exists
    fn as_bitmap_data_object(&self) -> Option<BitmapDataObject<'gc>> {
        None
    }

    fn as_ptr(&self) -> *const ObjectPtr;

    /// Check if this object is in the prototype chain of the specified test object.
    fn is_prototype_of(&self, other: Object<'gc>) -> bool {
        let mut proto = other.proto();

        while let Value::Object(proto_ob) = proto {
            if self.as_ptr() == proto_ob.as_ptr() {
                return true;
            }

            proto = proto_ob.proto();
        }

        false
    }

    /// Gets the length of this object, as if it were an array.
    fn length(&self, activation: &mut Activation<'_, 'gc, '_>) -> Result<i32, Error<'gc>>;

    /// Sets the length of this object, as if it were an array.
    fn set_length(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        length: i32,
    ) -> Result<(), Error<'gc>>;

    /// Checks if this object has an element.
    fn has_element(&self, activation: &mut Activation<'_, 'gc, '_>, index: i32) -> bool;

    /// Gets a property of this object, as if it were an array.
    fn get_element(&self, activation: &mut Activation<'_, 'gc, '_>, index: i32) -> Value<'gc>;

    /// Sets a property of this object, as if it were an array.
    fn set_element(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        index: i32,
        value: Value<'gc>,
    ) -> Result<(), Error<'gc>>;

    /// Deletes a property of this object as if it were an array.
    fn delete_element(&self, activation: &mut Activation<'_, 'gc, '_>, index: i32) -> bool;
}

pub enum ObjectPtr {}

impl<'gc> Object<'gc> {
    pub fn ptr_eq(a: Object<'gc>, b: Object<'gc>) -> bool {
        a.as_ptr() == b.as_ptr()
    }
}

/// Perform a prototype lookup of a given object.
///
/// This function returns both the `ReturnValue` and the prototype that
/// generated the value. If the property did not resolve, then it returns
/// `undefined` and `None` for the prototype.
///
/// The second return value can and should be used to populate the `depth`
/// property necessary to make `super` work.
pub fn search_prototype<'gc>(
    mut proto: Value<'gc>,
    name: &str,
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
) -> Result<(Value<'gc>, u8), Error<'gc>> {
    let mut depth = 0;
    while let Value::Object(p) = proto {
        if let Some(value) = p.get_local(name, activation, this, depth) {
            return Ok((value?, depth));
        }

        proto = p.proto();

        depth += 1;
        if depth == u8::MAX {
            return Err(Error::PrototypeRecursionLimit);
        }
    }

    Ok((Value::Undefined, 0))
}
