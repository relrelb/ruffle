//! AVM1 LoadVars object
//! TODO: bytesLoaded, bytesTotal, contentType, addRequestHeader

use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::property::Attribute;
use crate::avm1::{AvmString, Object, ScriptObject, TObject, Value};
use crate::avm_warn;
use crate::backend::navigator::{NavigationMethod, Request};
use gc_arena::MutationContext;

/// Implements `LoadVars`
pub fn constructor<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // No-op constructor
    Ok(this.into())
}

pub fn create_proto<'gc>(
    gc_context: MutationContext<'gc, '_>,
    proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    let mut object = ScriptObject::object(gc_context, Some(proto));

    object.force_set_function(
        "load",
        load,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "send",
        send,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "sendAndLoad",
        send_and_load,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "decode",
        decode,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "getBytesLoaded",
        get_bytes_loaded,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "getBytesTotal",
        get_bytes_total,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "toString",
        to_string,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.define_value(
        gc_context,
        "contentType",
        "application/x-www-form-url-encoded".into(),
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
    );

    object.force_set_function(
        "onLoad",
        on_load,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "onData",
        on_data,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.force_set_function(
        "addRequestHeader",
        add_request_header,
        gc_context,
        Attribute::DONT_DELETE | Attribute::READ_ONLY | Attribute::DONT_ENUM,
        Some(fn_proto),
    );

    object.into()
}

fn add_request_header<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    avm_warn!(activation, "LoadVars.addRequestHeader: Unimplemented");
    Ok(Value::Undefined)
}

fn decode<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Spec says added in SWF 7, but not version gated.
    // Decode the query string into properties on this object.
    if let Some(data) = args.get(0) {
        let data = data.coerce_to_string(activation)?;
        for (k, v) in url::form_urlencoded::parse(data.as_bytes()) {
            this.set(
                &k,
                crate::avm1::AvmString::new(activation.context.gc_context, v.into_owned()).into(),
                activation,
            )?;
        }
    }

    Ok(Value::Undefined)
}

fn get_bytes_loaded<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Forwards to undocumented property on the object.
    this.get("_bytesLoaded", activation)
}

fn get_bytes_total<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Forwards to undocumented property on the object.
    this.get("_bytesTotal", activation)
}

fn load<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let url = match args.get(0) {
        Some(val) => val.coerce_to_string(activation)?,
        None => return Ok(false.into()),
    };

    spawn_load_var_fetch(activation, this, &url, None)?;

    Ok(true.into())
}

fn on_data<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Default implementation forwards to decode and onLoad.
    let success = match args.get(0) {
        None | Some(Value::Undefined) | Some(Value::Null) => false,
        Some(val) => {
            this.call_method(&"decode", &[*val], activation)?;
            this.set("loaded", true.into(), activation)?;
            true
        }
    };

    this.call_method(&"onLoad", &[success.into()], activation)?;

    Ok(Value::Undefined)
}

fn on_load<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Default implementation: no-op?
    Ok(Value::Undefined)
}

fn send<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // `send` navigates the browser to a URL with the given query parameter.
    let url = match args.get(0) {
        Some(url) => url.coerce_to_string(activation)?,
        None => return Ok(false.into()),
    };

    let window = match args.get(1) {
        Some(v) => Some(v.coerce_to_string(activation)?),
        None => None,
    };

    let method_name = args
        .get(1)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;
    let method = NavigationMethod::from_method_str(&method_name).unwrap_or(NavigationMethod::Post);

    use indexmap::IndexMap;

    let mut form_values = IndexMap::new();
    let keys = this.get_keys(activation);

    for k in keys {
        let v = this.get(&k, activation);

        form_values.insert(
            k,
            v.ok()
                .unwrap_or(Value::Undefined)
                .coerce_to_string(activation)
                .unwrap_or_else(|_| "undefined".into())
                .to_string(),
        );
    }

    if let Some(window) = window {
        activation.context.navigator.navigate_to_url(
            url.to_string(),
            Some(window.to_string()),
            Some((method, form_values)),
        );
    }

    Ok(true.into())
}

fn send_and_load<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let url_val = args.get(0).cloned().unwrap_or(Value::Undefined);
    let url = url_val.coerce_to_string(activation)?;
    let target = match args.get(1) {
        Some(&Value::Object(o)) => o,
        _ => return Ok(false.into()),
    };

    let method_name = args
        .get(2)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;
    let method = NavigationMethod::from_method_str(&method_name).unwrap_or(NavigationMethod::Post);

    spawn_load_var_fetch(activation, target, &url, Some((this, method)))?;

    Ok(true.into())
}

fn to_string<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    use indexmap::IndexMap;

    let mut form_values = IndexMap::new();
    let keys = this.get_keys(activation);

    for k in keys {
        let v = this.get(&k, activation);

        //TODO: What happens if an error occurs inside a virtual property?
        form_values.insert(
            k,
            v.ok()
                .unwrap_or(Value::Undefined)
                .coerce_to_string(activation)
                .unwrap_or_else(|_| "undefined".into())
                .to_string(),
        );
    }

    let query_string = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(form_values.iter())
        .finish();

    Ok(crate::avm1::AvmString::new(activation.context.gc_context, query_string).into())
}

fn spawn_load_var_fetch<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    loader_object: Object<'gc>,
    url: &AvmString,
    send_object: Option<(Object<'gc>, NavigationMethod)>,
) -> Result<Value<'gc>, Error<'gc>> {
    let request = if let Some((send_object, method)) = send_object {
        // Send properties from `send_object`.
        activation.object_into_request(send_object, &url, Some(method))
    } else {
        // Not sending any parameters.
        Request::get(url.as_str())
    };

    let fetch = activation.context.navigator.fetch(request);
    let process = activation.context.load_manager.load_form_into_load_vars(
        activation.context.player.clone().unwrap(),
        loader_object,
        fetch,
    );

    // Create hidden properties on object.
    if !loader_object.has_property(activation, "_bytesLoaded") {
        loader_object.define_value(
            activation.context.gc_context,
            "_bytesLoaded",
            0.into(),
            Attribute::DONT_DELETE | Attribute::DONT_ENUM,
        );
    } else {
        loader_object.set("_bytesLoaded", 0.into(), activation)?;
    }

    if !loader_object.has_property(activation, "loaded") {
        loader_object.define_value(
            activation.context.gc_context,
            "loaded",
            false.into(),
            Attribute::DONT_DELETE | Attribute::DONT_ENUM,
        );
    } else {
        loader_object.set("loaded", false.into(), activation)?;
    }

    activation.context.navigator.spawn_future(process);

    Ok(true.into())
}
