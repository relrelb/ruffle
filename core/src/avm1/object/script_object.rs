use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::function::ExecutionReason;
use crate::avm1::property::{Attribute, Property};
use crate::avm1::property_map::{Entry, PropertyMap};
use crate::avm1::{AvmString, Object, ObjectPtr, TObject, Value};
use core::fmt;
use gc_arena::{Collect, GcCell, MutationContext};
use std::borrow::Cow;

pub const TYPE_OF_OBJECT: &str = "object";

#[derive(Debug, Clone, Collect)]
#[collect(no_drop)]
pub struct Watcher<'gc> {
    callback: Object<'gc>,
    user_data: Value<'gc>,
}

impl<'gc> Watcher<'gc> {
    pub fn new(callback: Object<'gc>, user_data: Value<'gc>) -> Self {
        Self {
            callback,
            user_data,
        }
    }

    pub fn call(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        name: &str,
        old_value: Value<'gc>,
        new_value: Value<'gc>,
        this: Object<'gc>,
        depth: u8,
    ) -> Result<Value<'gc>, Error<'gc>> {
        let args = [
            Value::String(AvmString::new(
                activation.context.gc_context,
                name.to_string(),
            )),
            old_value,
            new_value,
            self.user_data,
        ];
        if let Some(executable) = self.callback.as_executable() {
            executable.exec(
                name,
                activation,
                this,
                depth,
                &args,
                ExecutionReason::Special,
                self.callback,
            )
        } else {
            Ok(Value::Undefined)
        }
    }
}

#[derive(Debug, Copy, Clone, Collect)]
#[collect(no_drop)]
pub struct ScriptObject<'gc>(GcCell<'gc, ScriptObjectData<'gc>>);

#[derive(Collect)]
#[collect(no_drop)]
pub struct ScriptObjectData<'gc> {
    prototype: Value<'gc>,
    values: PropertyMap<Property<'gc>>,
    interfaces: Vec<Object<'gc>>,
    type_of: &'static str,
    watchers: PropertyMap<Watcher<'gc>>,
}

impl fmt::Debug for ScriptObjectData<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Object")
            .field("prototype", &self.prototype)
            .field("values", &self.values)
            .field("watchers", &self.watchers)
            .finish()
    }
}

impl<'gc> ScriptObject<'gc> {
    pub fn object(gc_context: MutationContext<'gc, '_>, proto: Option<Object<'gc>>) -> Self {
        Self(GcCell::allocate(
            gc_context,
            ScriptObjectData {
                prototype: proto.map_or(Value::Undefined, Value::Object),
                type_of: TYPE_OF_OBJECT,
                values: PropertyMap::new(),
                interfaces: vec![],
                watchers: PropertyMap::new(),
            },
        ))
    }

    /// Constructs and allocates an empty but normal object in one go.
    pub fn object_cell(
        gc_context: MutationContext<'gc, '_>,
        proto: Option<Object<'gc>>,
    ) -> Object<'gc> {
        Self::object(gc_context, proto).into()
    }

    /// Constructs an object with no values, not even builtins.
    ///
    /// Intended for constructing scope chains, since they exclusively use the
    /// object values, but can't just have a hashmap because of `with` and
    /// friends.
    pub fn bare_object(gc_context: MutationContext<'gc, '_>) -> Self {
        Self::object(gc_context, None)
    }

    pub fn set_type_of(&mut self, gc_context: MutationContext<'gc, '_>, type_of: &'static str) {
        self.0.write(gc_context).type_of = type_of;
    }

    /// Gets the value of a data property on this object.
    ///
    /// Doesn't look up the prototype chain and ignores virtual properties, thus cannot cause
    /// any side-effects.
    pub fn get_data(&self, name: &str, activation: &mut Activation<'_, 'gc, '_>) -> Value<'gc> {
        if let Some(Property::Stored { value, .. }) = self
            .0
            .read()
            .values
            .get(name, activation.is_case_sensitive())
        {
            value.to_owned()
        } else {
            Value::Undefined
        }
    }

    /// Sets a data property on this object.
    ///
    /// Doesn't look up the prototype chain and ignores virtual properties, but still might
    /// call to watchers.
    pub fn set_data(
        &self,
        name: &str,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<(), Error<'gc>> {
        // TODO: Call watchers.
        match self
            .0
            .write(activation.context.gc_context)
            .values
            .entry(name, activation.is_case_sensitive())
        {
            Entry::Occupied(mut entry) => {
                entry.get_mut().set(value);
            }
            Entry::Vacant(entry) => {
                entry.insert(Property::Stored {
                    value,
                    attributes: Attribute::empty(),
                });
            }
        }
        Ok(())
    }
}

impl<'gc> TObject<'gc> for ScriptObject<'gc> {
    /// Get the value of a particular property on this object.
    fn get_local(
        &self,
        name: &str,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        depth: u8,
    ) -> Option<Result<Value<'gc>, Error<'gc>>> {
        let getter = match self
            .0
            .read()
            .values
            .get(name, activation.is_case_sensitive())
        {
            Some(Property::Virtual { get, .. }) => get.to_owned(),
            Some(Property::Stored { value, .. }) => return Some(Ok(value.to_owned())),
            None => return None,
        };

        if let Some(exec) = getter.as_executable() {
            let result = exec.exec(
                "[Getter]",
                activation,
                this,
                depth,
                &[],
                ExecutionReason::Special,
                getter,
            );
            Some(match result {
                Ok(v) => Ok(v),
                Err(Error::ThrownValue(e)) => Err(Error::ThrownValue(e)),
                Err(_) => Ok(Value::Undefined),
            })
        } else {
            None
        }
    }

    /// Set a named property on the object.
    ///
    /// This function takes a redundant `this` parameter which should be
    /// the object's own `GcCell`, so that it can pass it to user-defined
    /// overrides that may need to interact with the underlying object.
    fn set_local(
        &self,
        name: &str,
        mut value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        depth: u8,
    ) -> Result<(), Error<'gc>> {
        let watcher = self
            .0
            .read()
            .watchers
            .get(name, activation.is_case_sensitive())
            .cloned();
        let mut result = Ok(());
        if let Some(watcher) = watcher {
            let old_value = self.get(name, activation)?;
            match watcher.call(activation, name, old_value, value, this, depth) {
                Ok(v) => value = v,
                Err(Error::ThrownValue(e)) => {
                    value = Value::Undefined;
                    result = Err(Error::ThrownValue(e));
                }
                Err(_) => value = Value::Undefined,
            };
        }

        let setter = match self
            .0
            .write(activation.context.gc_context)
            .values
            .entry(name, activation.is_case_sensitive())
        {
            Entry::Occupied(mut entry) => entry.get_mut().set(value),
            Entry::Vacant(entry) => {
                entry.insert(Property::Stored {
                    value,
                    attributes: Attribute::empty(),
                });
                None
            }
        };

        if let Some(setter) = setter {
            if let Some(exec) = setter.as_executable() {
                if let Err(Error::ThrownValue(e)) = exec.exec(
                    "[Setter]",
                    activation,
                    this,
                    depth,
                    &[value],
                    ExecutionReason::Special,
                    setter,
                ) {
                    return Err(Error::ThrownValue(e));
                }
            }
        }

        result
    }

    /// Call the underlying object.
    ///
    /// This function takes a redundant `this` parameter which should be
    /// the object's own `GcCell`, so that it can pass it to user-defined
    /// overrides that may need to interact with the underlying object.
    fn call(
        &self,
        _name: &str,
        _activation: &mut Activation<'_, 'gc, '_>,
        _this: Object<'gc>,
        _depth: u8,
        _args: &[Value<'gc>],
    ) -> Result<Value<'gc>, Error<'gc>> {
        Ok(Value::Undefined)
    }

    fn call_setter(
        &self,
        name: &str,
        value: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Option<Object<'gc>> {
        match self
            .0
            .write(activation.context.gc_context)
            .values
            .get_mut(name, activation.is_case_sensitive())
        {
            Some(propref) if propref.is_virtual() => propref.set(value),
            _ => None,
        }
    }

    fn create_bare_object(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
    ) -> Result<Object<'gc>, Error<'gc>> {
        Ok(ScriptObject::object(activation.context.gc_context, Some(this)).into())
    }

    /// Delete a named property from the object.
    ///
    /// Returns false if the property cannot be deleted.
    fn delete(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        if let Entry::Occupied(mut entry) = self
            .0
            .write(activation.context.gc_context)
            .values
            .entry(name, activation.is_case_sensitive())
        {
            if entry.get().can_delete() {
                entry.remove_entry();
                return true;
            }
        }
        false
    }

    fn add_property(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: &str,
        get: Object<'gc>,
        set: Option<Object<'gc>>,
        attributes: Attribute,
    ) {
        self.0.write(gc_context).values.insert(
            name,
            Property::Virtual {
                get,
                set,
                attributes,
            },
            false,
        );
    }

    fn add_property_with_case(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        name: &str,
        get: Object<'gc>,
        set: Option<Object<'gc>>,
        attributes: Attribute,
    ) {
        self.0.write(activation.context.gc_context).values.insert(
            name,
            Property::Virtual {
                get,
                set,
                attributes,
            },
            activation.is_case_sensitive(),
        );
    }

    fn set_watcher(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        name: Cow<str>,
        callback: Object<'gc>,
        user_data: Value<'gc>,
    ) {
        self.0.write(activation.context.gc_context).watchers.insert(
            &name,
            Watcher::new(callback, user_data),
            activation.is_case_sensitive(),
        );
    }

    fn remove_watcher(&self, activation: &mut Activation<'_, 'gc, '_>, name: Cow<str>) -> bool {
        let old = self
            .0
            .write(activation.context.gc_context)
            .watchers
            .remove(name.as_ref(), activation.is_case_sensitive());
        old.is_some()
    }

    fn define_value(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: &str,
        value: Value<'gc>,
        attributes: Attribute,
    ) {
        self.0
            .write(gc_context)
            .values
            .insert(name, Property::Stored { value, attributes }, true);
    }

    fn set_attributes(
        &self,
        gc_context: MutationContext<'gc, '_>,
        name: Option<&str>,
        set_attributes: Attribute,
        clear_attributes: Attribute,
    ) {
        match name {
            None => {
                // Change *all* attributes.
                for (_name, prop) in self.0.write(gc_context).values.iter_mut() {
                    let new_atts = (prop.attributes() - clear_attributes) | set_attributes;
                    prop.set_attributes(new_atts);
                }
            }
            Some(name) => {
                if let Some(prop) = self.0.write(gc_context).values.get_mut(name, false) {
                    let new_atts = (prop.attributes() - clear_attributes) | set_attributes;
                    prop.set_attributes(new_atts);
                }
            }
        }
    }

    fn proto(&self) -> Value<'gc> {
        self.0.read().prototype
    }

    fn set_proto(&self, gc_context: MutationContext<'gc, '_>, prototype: Value<'gc>) {
        self.0.write(gc_context).prototype = prototype;
    }

    /// Checks if the object has a given named property.
    fn has_property(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        self.has_own_property(activation, name)
            || if let Value::Object(proto) = self.proto() {
                proto.has_property(activation, name)
            } else {
                false
            }
    }

    /// Checks if the object has a given named property on itself (and not,
    /// say, the object's prototype or superclass)
    fn has_own_property(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        if name == "__proto__" {
            return true;
        }
        self.0
            .read()
            .values
            .contains_key(name, activation.is_case_sensitive())
    }

    fn has_own_virtual(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        if let Some(slot) = self
            .0
            .read()
            .values
            .get(name, activation.is_case_sensitive())
        {
            slot.is_virtual()
        } else {
            false
        }
    }

    /// Checks if a named property appears when enumerating the object.
    fn is_property_enumerable(&self, activation: &mut Activation<'_, 'gc, '_>, name: &str) -> bool {
        if let Some(prop) = self
            .0
            .read()
            .values
            .get(name, activation.is_case_sensitive())
        {
            prop.is_enumerable()
        } else {
            false
        }
    }

    /// Enumerate the object.
    fn get_keys(&self, activation: &mut Activation<'_, 'gc, '_>) -> Vec<String> {
        let proto_keys = if let Value::Object(proto) = self.proto() {
            proto.get_keys(activation)
        } else {
            Vec::new()
        };
        let mut out_keys = vec![];
        let object = self.0.read();

        // Prototype keys come first.
        out_keys.extend(proto_keys.into_iter().filter(|k| {
            !object
                .values
                .contains_key(k, activation.is_case_sensitive())
        }));

        // Then our own keys.
        out_keys.extend(self.0.read().values.iter().filter_map(move |(k, p)| {
            if p.is_enumerable() {
                Some(k.to_string())
            } else {
                None
            }
        }));

        out_keys
    }

    fn type_of(&self) -> &'static str {
        self.0.read().type_of
    }

    fn interfaces(&self) -> Vec<Object<'gc>> {
        self.0.read().interfaces.clone()
    }

    fn set_interfaces(&self, gc_context: MutationContext<'gc, '_>, iface_list: Vec<Object<'gc>>) {
        self.0.write(gc_context).interfaces = iface_list;
    }

    fn as_script_object(&self) -> Option<ScriptObject<'gc>> {
        Some(*self)
    }

    fn as_ptr(&self) -> *const ObjectPtr {
        self.0.as_ptr() as *const ObjectPtr
    }

    fn length(&self, activation: &mut Activation<'_, 'gc, '_>) -> Result<i32, Error<'gc>> {
        self.get_data("length", activation)
            .coerce_to_i32(activation)
    }

    fn set_length(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        new_length: i32,
    ) -> Result<(), Error<'gc>> {
        self.set_data("length", new_length.into(), activation)
    }

    fn has_element(&self, activation: &mut Activation<'_, 'gc, '_>, index: i32) -> bool {
        self.has_own_property(activation, &index.to_string())
    }

    fn get_element(&self, activation: &mut Activation<'_, 'gc, '_>, index: i32) -> Value<'gc> {
        self.get_data(&index.to_string(), activation)
    }

    fn set_element(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
        index: i32,
        value: Value<'gc>,
    ) -> Result<(), Error<'gc>> {
        self.set_data(&index.to_string(), value, activation)
    }

    fn delete_element(&self, activation: &mut Activation<'_, 'gc, '_>, index: i32) -> bool {
        self.delete(activation, &index.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::avm1::function::Executable;
    use crate::avm1::globals::system::SystemProperties;
    use crate::avm1::property::Attribute;
    use crate::avm1::{activation::ActivationIdentifier, function::FunctionObject};
    use crate::avm1::{Avm1, Timers};
    use crate::avm2::Avm2;
    use crate::backend::audio::{AudioManager, NullAudioBackend};
    use crate::backend::locale::NullLocaleBackend;
    use crate::backend::log::NullLogBackend;
    use crate::backend::navigator::NullNavigatorBackend;
    use crate::backend::render::NullRenderer;
    use crate::backend::storage::MemoryStorageBackend;
    use crate::backend::ui::NullUiBackend;
    use crate::backend::video::NullVideoBackend;
    use crate::context::UpdateContext;
    use crate::display_object::{MovieClip, Stage};
    use crate::focus_tracker::FocusTracker;
    use crate::library::Library;
    use crate::loader::LoadManager;
    use crate::prelude::*;
    use crate::tag_utils::{SwfMovie, SwfSlice};
    use crate::vminterface::Instantiator;
    use gc_arena::rootless_arena;
    use instant::Instant;
    use rand::{rngs::SmallRng, SeedableRng};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    fn with_object<F, R>(swf_version: u8, test: F) -> R
    where
        F: for<'a, 'gc> FnOnce(&mut Activation<'_, 'gc, '_>, Object<'gc>) -> R,
    {
        rootless_arena(|gc_context| {
            let mut avm1 = Avm1::new(gc_context, swf_version);
            let mut avm2 = Avm2::new(gc_context);
            let swf = Arc::new(SwfMovie::empty(swf_version));
            let root: DisplayObject<'_> =
                MovieClip::new(SwfSlice::empty(swf.clone()), gc_context).into();
            root.set_depth(gc_context, 0);

            let stage = Stage::empty(gc_context, 550, 400);
            let mut frame_rate = 12.0;

            let object = ScriptObject::object(gc_context, Some(avm1.prototypes().object)).into();
            let globals = avm1.global_object_cell();

            let mut context = UpdateContext {
                gc_context,
                player_version: 32,
                swf: &swf,
                stage,
                rng: &mut SmallRng::from_seed([0u8; 32]),
                action_queue: &mut crate::context::ActionQueue::new(),
                audio: &mut NullAudioBackend::new(),
                audio_manager: &mut AudioManager::new(),
                ui: &mut NullUiBackend::new(),
                library: &mut Library::empty(gc_context),
                navigator: &mut NullNavigatorBackend::new(),
                renderer: &mut NullRenderer::new(),
                locale: &mut NullLocaleBackend::new(),
                log: &mut NullLogBackend::new(),
                video: &mut NullVideoBackend::new(),
                mouse_over_object: None,
                mouse_down_object: None,
                mouse_position: &(Twips::ZERO, Twips::ZERO),
                drag_object: &mut None,
                player: None,
                load_manager: &mut LoadManager::new(),
                system: &mut SystemProperties::default(),
                instance_counter: &mut 0,
                storage: &mut MemoryStorageBackend::default(),
                shared_objects: &mut HashMap::new(),
                unbound_text_fields: &mut Vec::new(),
                timers: &mut Timers::new(),
                current_context_menu: &mut None,
                needs_render: &mut false,
                avm1: &mut avm1,
                avm2: &mut avm2,
                external_interface: &mut Default::default(),
                update_start: Instant::now(),
                max_execution_duration: Duration::from_secs(15),
                focus_tracker: FocusTracker::new(gc_context),
                times_get_time_called: 0,
                time_offset: &mut 0,
                frame_rate: &mut frame_rate,
            };
            context.stage.replace_at_depth(&mut context, root, 0);

            root.post_instantiation(&mut context, root, None, Instantiator::Movie, false);
            root.set_name(context.gc_context, "");

            let swf_version = context.swf.version();
            let mut activation = Activation::from_nothing(
                context,
                ActivationIdentifier::root("[Test]"),
                swf_version,
                globals,
                root,
            );

            test(&mut activation, object)
        })
    }

    #[test]
    fn test_get_undefined() {
        with_object(0, |activation, object| {
            assert_eq!(
                object.get("not_defined", activation).unwrap(),
                Value::Undefined
            );
        })
    }

    #[test]
    fn test_set_get() {
        with_object(0, |activation, object| {
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "forced",
                "forced".into(),
                Attribute::empty(),
            );
            object.set("natural", "natural".into(), activation).unwrap();

            assert_eq!(object.get("forced", activation).unwrap(), "forced".into());
            assert_eq!(object.get("natural", activation).unwrap(), "natural".into());
        })
    }

    #[test]
    fn test_set_readonly() {
        with_object(0, |activation, object| {
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "normal",
                "initial".into(),
                Attribute::empty(),
            );
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "readonly",
                "initial".into(),
                Attribute::READ_ONLY,
            );

            object.set("normal", "replaced".into(), activation).unwrap();
            object
                .set("readonly", "replaced".into(), activation)
                .unwrap();

            assert_eq!(object.get("normal", activation).unwrap(), "replaced".into());
            assert_eq!(
                object.get("readonly", activation).unwrap(),
                "initial".into()
            );
        })
    }

    #[test]
    fn test_deletable_not_readonly() {
        with_object(0, |activation, object| {
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "test",
                "initial".into(),
                Attribute::DONT_DELETE,
            );

            assert!(!object.delete(activation, "test"));
            assert_eq!(object.get("test", activation).unwrap(), "initial".into());

            object
                .as_script_object()
                .unwrap()
                .set("test", "replaced".into(), activation)
                .unwrap();

            assert!(!object.delete(activation, "test"));
            assert_eq!(object.get("test", activation).unwrap(), "replaced".into());
        })
    }

    #[test]
    fn test_virtual_get() {
        with_object(0, |activation, object| {
            let getter = FunctionObject::function(
                activation.context.gc_context,
                Executable::Native(|_avm, _this, _args| Ok("Virtual!".into())),
                None,
                activation.context.avm1.prototypes.function,
            );

            object.as_script_object().unwrap().add_property(
                activation.context.gc_context,
                "test",
                getter,
                None,
                Attribute::empty(),
            );

            assert_eq!(object.get("test", activation).unwrap(), "Virtual!".into());

            // This set should do nothing
            object.set("test", "Ignored!".into(), activation).unwrap();
            assert_eq!(object.get("test", activation).unwrap(), "Virtual!".into());
        })
    }

    #[test]
    fn test_delete() {
        with_object(0, |activation, object| {
            let getter = FunctionObject::function(
                activation.context.gc_context,
                Executable::Native(|_avm, _this, _args| Ok("Virtual!".into())),
                None,
                activation.context.avm1.prototypes.function,
            );

            object.as_script_object().unwrap().add_property(
                activation.context.gc_context,
                "virtual",
                getter,
                None,
                Attribute::empty(),
            );
            object.as_script_object().unwrap().add_property(
                activation.context.gc_context,
                "virtual_un",
                getter,
                None,
                Attribute::DONT_DELETE,
            );
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "stored",
                "Stored!".into(),
                Attribute::empty(),
            );
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "stored_un",
                "Stored!".into(),
                Attribute::DONT_DELETE,
            );

            assert!(object.delete(activation, "virtual"));
            assert!(!object.delete(activation, "virtual_un"));
            assert!(object.delete(activation, "stored"));
            assert!(!object.delete(activation, "stored_un"));
            assert!(!object.delete(activation, "non_existent"));

            assert_eq!(object.get("virtual", activation).unwrap(), Value::Undefined);
            assert_eq!(
                object.get("virtual_un", activation).unwrap(),
                "Virtual!".into()
            );
            assert_eq!(object.get("stored", activation).unwrap(), Value::Undefined);
            assert_eq!(
                object.get("stored_un", activation).unwrap(),
                "Stored!".into()
            );
        })
    }

    #[test]
    fn test_iter_values() {
        with_object(0, |activation, object| {
            let getter = FunctionObject::function(
                activation.context.gc_context,
                Executable::Native(|_avm, _this, _args| Ok(Value::Null)),
                None,
                activation.context.avm1.prototypes.function,
            );

            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "stored",
                Value::Null,
                Attribute::empty(),
            );
            object.as_script_object().unwrap().define_value(
                activation.context.gc_context,
                "stored_hidden",
                Value::Null,
                Attribute::DONT_ENUM,
            );
            object.as_script_object().unwrap().add_property(
                activation.context.gc_context,
                "virtual",
                getter,
                None,
                Attribute::empty(),
            );
            object.as_script_object().unwrap().add_property(
                activation.context.gc_context,
                "virtual_hidden",
                getter,
                None,
                Attribute::DONT_ENUM,
            );

            let keys: Vec<_> = object.get_keys(activation);
            assert_eq!(keys.len(), 2);
            assert!(keys.contains(&"stored".to_string()));
            assert!(!keys.contains(&"stored_hidden".to_string()));
            assert!(keys.contains(&"virtual".to_string()));
            assert!(!keys.contains(&"virtual_hidden".to_string()));
        })
    }
}
