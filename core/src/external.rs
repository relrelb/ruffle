use crate::avm1::activation::{
    Activation as Avm1Activation, ActivationIdentifier as Avm1ActivationIdentifier,
};
use crate::avm1::object::TObject;
use crate::avm1::Value as Avm1Value;
use crate::avm1::{
    AvmString as Avm1String, Object as Avm1Object, ScriptObject as Avm1ScriptObject,
};
use crate::context::UpdateContext;
use gc_arena::Collect;
use std::collections::BTreeMap;

/// An intermediate format of representing shared data between ActionScript and elsewhere.
/// Regardless of the capabilities of both sides, all data will be translated to this potentially
/// lossy format. Any recursion or additional metadata in ActionScript will not be translated.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(BTreeMap<String, Value>),
    List(Vec<Value>),
}

impl From<Avm1String<'_>> for Value {
    fn from(string: Avm1String<'_>) -> Self {
        Value::String(string.to_string())
    }
}

impl From<String> for Value {
    fn from(string: String) -> Self {
        Value::String(string)
    }
}

impl From<&'static str> for Value {
    fn from(string: &'static str) -> Self {
        Value::String(string.into())
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Number(value)
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value::Number(f64::from(value))
    }
}

impl From<u8> for Value {
    fn from(value: u8) -> Self {
        Value::Number(f64::from(value))
    }
}

impl From<i16> for Value {
    fn from(value: i16) -> Self {
        Value::Number(f64::from(value))
    }
}

impl From<u16> for Value {
    fn from(value: u16) -> Self {
        Value::Number(f64::from(value))
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Number(f64::from(value))
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Number(value as f64)
    }
}

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Value::Number(f64::from(value))
    }
}

impl From<usize> for Value {
    fn from(value: usize) -> Self {
        Value::Number(value as f64)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Value::Object(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Value::List(value)
    }
}

impl Value {
    pub fn from_avm1<'gc>(
        activation: &mut Avm1Activation<'_, 'gc, '_>,
        value: Avm1Value<'gc>,
    ) -> Result<Value, crate::avm1::error::Error<'gc>> {
        Ok(match value {
            Avm1Value::Undefined | Avm1Value::Null => Value::Null,
            Avm1Value::Bool(value) => Value::Bool(value),
            Avm1Value::Number(value) => Value::Number(value),
            Avm1Value::String(value) => Value::String(value.to_string()),
            Avm1Value::Object(object) => {
                if activation
                    .context
                    .avm1
                    .prototypes()
                    .array
                    .is_prototype_of(object)
                {
                    let mut values = Vec::new();
                    for value in object.array() {
                        values.push(Value::from_avm1(activation, value)?);
                    }
                    Value::List(values)
                } else {
                    let keys = object.get_keys(activation);
                    let mut values = BTreeMap::new();
                    for key in keys {
                        let value = object.get(&key, activation)?;
                        values.insert(key, Value::from_avm1(activation, value)?);
                    }
                    Value::Object(values)
                }
            }
        })
    }

    pub fn into_avm1<'gc>(self, activation: &mut Avm1Activation<'_, 'gc, '_>) -> Avm1Value<'gc> {
        match self {
            Value::Null => Avm1Value::Null,
            Value::Bool(value) => Avm1Value::Bool(value),
            Value::Number(value) => Avm1Value::Number(value),
            Value::String(value) => {
                Avm1Value::String(Avm1String::new(activation.context.gc_context, value))
            }
            Value::Object(values) => {
                let object = Avm1ScriptObject::object(
                    activation.context.gc_context,
                    Some(activation.context.avm1.prototypes().object),
                );
                for (key, value) in values {
                    let _ = object.set(&key, value.into_avm1(activation), activation);
                }
                object.into()
            }
            Value::List(values) => {
                let array = Avm1ScriptObject::array(
                    activation.context.gc_context,
                    Some(activation.context.avm1.prototypes().array),
                );
                for value in values {
                    array.set_array_element(
                        array.length(),
                        value.into_avm1(activation),
                        activation.context.gc_context,
                    );
                }
                array.into()
            }
        }
    }
}

#[derive(Collect, Clone)]
#[collect(no_drop)]
pub enum Callback<'gc> {
    Avm1 {
        this: Avm1Value<'gc>,
        method: Avm1Object<'gc>,
    },
}

impl<'gc> Callback<'gc> {
    pub fn call(
        &self,
        context: &mut UpdateContext<'_, 'gc, '_>,
        name: &str,
        args: impl IntoIterator<Item = Value>,
    ) -> Value {
        match self {
            Callback::Avm1 { this, method } => {
                let base_clip = context.levels.get(0).unwrap();
                let swf_version = context.swf.version();
                let globals = context.avm1.global_object_cell();
                let mut activation = Avm1Activation::from_nothing(
                    context.reborrow(),
                    Avm1ActivationIdentifier::root("[ExternalInterface]"),
                    swf_version,
                    globals,
                    base_clip,
                );
                let this = this.coerce_to_object(&mut activation);
                let args: Vec<Avm1Value> = args
                    .into_iter()
                    .map(|v| v.into_avm1(&mut activation))
                    .collect();
                if let Ok(result) = method
                    .call(name, &mut activation, this, None, &args)
                    .and_then(|value| Value::from_avm1(&mut activation, value))
                {
                    result
                } else {
                    Value::Null
                }
            }
        }
    }
}

pub trait ExternalInterfaceProvider {
    fn get_method(&self, name: &str) -> Option<Box<dyn ExternalInterfaceMethod>>;

    fn on_callback_available(&self, name: &str);

    fn on_fs_command(&self, command: &str, args: &str) -> bool;
}

pub trait ExternalInterfaceMethod {
    fn call(&self, context: &mut UpdateContext<'_, '_, '_>, args: &[Value]) -> Value;
}

impl<F> ExternalInterfaceMethod for F
where
    F: Fn(&mut UpdateContext<'_, '_, '_>, &[Value]) -> Value,
{
    fn call(&self, context: &mut UpdateContext<'_, '_, '_>, args: &[Value]) -> Value {
        self(context, args)
    }
}

#[derive(Default, Collect)]
#[collect(no_drop)]
pub struct ExternalInterface<'gc> {
    #[collect(require_static)]
    providers: Vec<Box<dyn ExternalInterfaceProvider>>,
    callbacks: BTreeMap<String, Callback<'gc>>,
}

impl<'gc> ExternalInterface<'gc> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_provider(&mut self, provider: Box<dyn ExternalInterfaceProvider>) {
        self.providers.push(provider);
    }

    pub fn add_callback(&mut self, name: String, callback: Callback<'gc>) {
        self.callbacks.insert(name.clone(), callback);
        for provider in &self.providers {
            provider.on_callback_available(&name);
        }
    }

    pub fn get_callback(&self, name: &str) -> Option<Callback<'gc>> {
        self.callbacks.get(name).cloned()
    }

    pub fn get_method_for(&self, name: &str) -> Option<Box<dyn ExternalInterfaceMethod>> {
        for provider in &self.providers {
            if let Some(method) = provider.get_method(name) {
                return Some(method);
            }
        }
        None
    }

    pub fn available(&self) -> bool {
        !self.providers.is_empty()
    }

    pub fn invoke_fs_command(&self, command: &str, args: &str) -> bool {
        for provider in &self.providers {
            if provider.on_fs_command(command, args) {
                return true;
            }
        }
        false
    }
}
