use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::object::value_object::ValueObject;
use crate::avm1::{AvmString, Object, TObject};
use crate::ecma_conversions::{
    f64_to_string, f64_to_wrapping_i16, f64_to_wrapping_i32, f64_to_wrapping_u16,
    f64_to_wrapping_u32,
};
use gc_arena::Collect;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, Collect)]
#[collect(no_drop)]
#[allow(dead_code)]
pub enum Value<'gc> {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(AvmString<'gc>),
    Object(Object<'gc>),
}

impl<'gc> From<AvmString<'gc>> for Value<'gc> {
    fn from(string: AvmString<'gc>) -> Self {
        Value::String(string)
    }
}

impl<'gc> From<&'static str> for Value<'gc> {
    fn from(string: &'static str) -> Self {
        Value::String(string.into())
    }
}

impl<'gc> From<bool> for Value<'gc> {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl<'gc, T> From<T> for Value<'gc>
where
    Object<'gc>: From<T>,
{
    fn from(value: T) -> Self {
        Value::Object(Object::from(value))
    }
}

impl<'gc> From<f64> for Value<'gc> {
    fn from(value: f64) -> Self {
        Value::Number(value)
    }
}

impl<'gc> From<f32> for Value<'gc> {
    fn from(value: f32) -> Self {
        Value::Number(f64::from(value))
    }
}

impl<'gc> From<u8> for Value<'gc> {
    fn from(value: u8) -> Self {
        Value::Number(f64::from(value))
    }
}

impl<'gc> From<i16> for Value<'gc> {
    fn from(value: i16) -> Self {
        Value::Number(f64::from(value))
    }
}

impl<'gc> From<u16> for Value<'gc> {
    fn from(value: u16) -> Self {
        Value::Number(f64::from(value))
    }
}

impl<'gc> From<i32> for Value<'gc> {
    fn from(value: i32) -> Self {
        Value::Number(f64::from(value))
    }
}

impl<'gc> From<u32> for Value<'gc> {
    fn from(value: u32) -> Self {
        Value::Number(f64::from(value))
    }
}

impl<'gc> From<usize> for Value<'gc> {
    fn from(value: usize) -> Self {
        Value::Number(value as f64)
    }
}

impl PartialEq for Value<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) => true,
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => (a == b) || (a.is_nan() && b.is_nan()),
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => Object::ptr_eq(*a, *b),
            _ => false,
        }
    }
}

impl<'gc> Value<'gc> {
    /// Yields `true` if the given value is a primitive value.
    ///
    /// Note: Boxed primitive values are not considered primitive - it is
    /// expected that their `toString`/`valueOf` handlers have already had a
    /// chance to unbox the primitive contained within.
    pub fn is_primitive(&self) -> bool {
        !matches!(self, Value::Object(_))
    }

    pub fn into_number_v1(self) -> f64 {
        match self {
            Value::Bool(true) => 1.0,
            Value::Number(v) => v,
            Value::String(v) => v.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    /// ECMA-262 2nd edtion s. 9.3 ToNumber (after calling `to_primitive_num`)
    ///
    /// Flash diverges from spec in a number of ways. These ways are, as far as
    /// we are aware, version-gated:
    ///
    /// * In SWF6 and lower, `undefined` is coerced to `0.0` (like `false`)
    /// rather than `NaN` as required by spec.
    /// * In SWF5 and lower, hexadecimal is unsupported.
    fn primitive_as_number(&self, activation: &mut Activation<'_, 'gc, '_>) -> f64 {
        match self {
            Value::Undefined if activation.swf_version() < 7 => 0.0,
            Value::Null if activation.swf_version() < 7 => 0.0,
            Value::Undefined => f64::NAN,
            Value::Null => f64::NAN,
            Value::Bool(false) => 0.0,
            Value::Bool(true) => 1.0,
            Value::Number(v) => *v,
            Value::String(v) => match v.as_str() {
                v if activation.swf_version() >= 6 && v.starts_with("0x") => {
                    let mut n: u32 = 0;
                    for c in v[2..].bytes() {
                        n = n.wrapping_shl(4);
                        n |= match c {
                            b'0' => 0,
                            b'1' => 1,
                            b'2' => 2,
                            b'3' => 3,
                            b'4' => 4,
                            b'5' => 5,
                            b'6' => 6,
                            b'7' => 7,
                            b'8' => 8,
                            b'9' => 9,
                            b'a' | b'A' => 10,
                            b'b' | b'B' => 11,
                            b'c' | b'C' => 12,
                            b'd' | b'D' => 13,
                            b'e' | b'E' => 14,
                            b'f' | b'F' => 15,
                            _ => return f64::NAN,
                        }
                    }
                    f64::from(n as i32)
                }
                v if activation.swf_version() >= 6
                    && (v.starts_with('0') || v.starts_with("+0") || v.starts_with("-0"))
                    && v[1..].bytes().all(|c| c >= b'0' && c <= b'7') =>
                {
                    let trimmed = v.trim_start_matches(|c| c == '+' || c == '-');
                    let mut n: u32 = 0;
                    for c in trimmed.bytes() {
                        n = n.wrapping_shl(3);
                        n |= (c - b'0') as u32;
                    }
                    if v.starts_with('-') {
                        n = n.wrapping_neg();
                    }
                    f64::from(n as i32)
                }
                "" => f64::NAN,
                _ => {
                    // Rust parses "inf" and "+inf" into Infinity, but Flash doesn't.
                    // (as of nightly 4/13, Rust also accepts "infinity")
                    // Check if the strign starts with 'i' (ignoring any leading +/-).
                    if v.strip_prefix(['+', '-'].as_ref())
                        .unwrap_or(v)
                        .starts_with(['i', 'I'].as_ref())
                    {
                        f64::NAN
                    } else {
                        v.trim_start_matches(|c| c == '\t' || c == '\n' || c == '\r' || c == ' ')
                            .parse()
                            .unwrap_or(f64::NAN)
                    }
                }
            },
            Value::Object(_) => f64::NAN,
        }
    }

    /// ECMA-262 2nd edition s. 9.3 ToNumber
    pub fn coerce_to_f64(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<f64, Error<'gc>> {
        Ok(match self {
            Value::Object(_) => self
                .to_primitive_num(activation)?
                .primitive_as_number(activation),
            val => val.primitive_as_number(activation),
        })
    }

    /// ECMA-262 2nd edition s. 9.1 ToPrimitive (hint: Number)
    ///
    /// Flash diverges from spec in a number of ways. These ways are, as far as
    /// we are aware, version-gated:
    ///
    /// * `toString` is never called when `ToPrimitive` is invoked with number
    ///   hint.
    /// * Objects with uncallable `valueOf` implementations are coerced to
    ///   `undefined`. This is not a special-cased behavior: All values are
    ///   callable in `AVM1`. Values that are not callable objects instead
    ///   return `undefined` rather than yielding a runtime error.
    pub fn to_primitive_num(
        self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error<'gc>> {
        Ok(match self {
            Value::Object(object) => object.call_method("valueOf", 0, &[], activation)?,
            val => val.to_owned(),
        })
    }

    /// ECMA-262 2nd edition s. 11.8.5 Abstract relational comparison algorithm
    #[allow(clippy::float_cmp)]
    pub fn abstract_lt(
        &self,
        other: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<Value<'gc>, Error<'gc>> {
        let prim_self = self.to_primitive_num(activation)?;
        let prim_other = other.to_primitive_num(activation)?;

        if let (Value::String(a), Value::String(b)) = (&prim_self, &prim_other) {
            return Ok(a.to_string().bytes().lt(b.to_string().bytes()).into());
        }

        let num_self = prim_self.primitive_as_number(activation);
        let num_other = prim_other.primitive_as_number(activation);

        if num_self.is_nan() || num_other.is_nan() {
            return Ok(Value::Undefined);
        }

        if num_self == num_other
            || num_self == 0.0 && num_other == -0.0
            || num_self == -0.0 && num_other == 0.0
            || num_self.is_infinite() && num_self.is_sign_positive()
            || num_other.is_infinite() && num_other.is_sign_negative()
        {
            return Ok(false.into());
        }

        if num_self.is_infinite() && num_self.is_sign_negative()
            || num_other.is_infinite() && num_other.is_sign_positive()
        {
            return Ok(true.into());
        }

        Ok((num_self < num_other).into())
    }

    /// ECMA-262 2nd edition s. 11.9.3 Abstract equality comparison algorithm
    #[allow(clippy::unnested_or_patterns)]
    pub fn abstract_eq(
        &self,
        other: Value<'gc>,
        activation: &mut Activation<'_, 'gc, '_>,
        coerced: bool,
    ) -> Result<Value<'gc>, Error<'gc>> {
        match (self, &other) {
            (Value::Undefined, Value::Undefined) => Ok(true.into()),
            (Value::Null, Value::Null) => Ok(true.into()),
            (Value::Number(a), Value::Number(b)) => {
                if !coerced && a.is_nan() && b.is_nan() {
                    return Ok(true.into());
                }

                if a == b {
                    return Ok(true.into());
                }

                if *a == 0.0 && *b == -0.0 || *a == -0.0 && *b == 0.0 {
                    return Ok(true.into());
                }

                Ok(false.into())
            }
            (Value::String(a), Value::String(b)) => Ok((**a == **b).into()),
            (Value::Bool(a), Value::Bool(b)) => Ok((a == b).into()),
            (Value::Object(a), Value::Object(b)) => Ok(Object::ptr_eq(*a, *b).into()),
            (Value::Object(a), Value::Undefined | Value::Null) => {
                Ok(Object::ptr_eq(*a, activation.context.avm1.global_object_cell()).into())
            }
            (Value::Undefined | Value::Null, Value::Object(b)) => {
                Ok(Object::ptr_eq(*b, activation.context.avm1.global_object_cell()).into())
            }
            (Value::Undefined, Value::Null) => Ok(true.into()),
            (Value::Null, Value::Undefined) => Ok(true.into()),
            (Value::Number(_), Value::String(_)) => Ok(self.abstract_eq(
                Value::Number(other.coerce_to_f64(activation)?),
                activation,
                true,
            )?),
            (Value::String(_), Value::Number(_)) => {
                Ok(Value::Number(self.coerce_to_f64(activation)?)
                    .abstract_eq(other, activation, true)?)
            }
            (Value::Bool(_), _) => Ok(Value::Number(self.coerce_to_f64(activation)?)
                .abstract_eq(other, activation, true)?),
            (_, Value::Bool(_)) => Ok(self.abstract_eq(
                Value::Number(other.coerce_to_f64(activation)?),
                activation,
                true,
            )?),
            (Value::String(_), Value::Object(_)) => {
                let non_obj_other = other.to_primitive_num(activation)?;
                if !non_obj_other.is_primitive() {
                    return Ok(false.into());
                }

                Ok(self.abstract_eq(non_obj_other, activation, true)?)
            }
            (Value::Number(_), Value::Object(_)) => {
                let non_obj_other = other.to_primitive_num(activation)?;
                if !non_obj_other.is_primitive() {
                    return Ok(false.into());
                }

                Ok(self.abstract_eq(non_obj_other, activation, true)?)
            }
            (Value::Object(_), Value::String(_)) => {
                let non_obj_self = self.to_primitive_num(activation)?;
                if !non_obj_self.is_primitive() {
                    return Ok(false.into());
                }

                Ok(non_obj_self.abstract_eq(other, activation, true)?)
            }
            (Value::Object(_), Value::Number(_)) => {
                let non_obj_self = self.to_primitive_num(activation)?;
                if !non_obj_self.is_primitive() {
                    return Ok(false.into());
                }

                Ok(non_obj_self.abstract_eq(other, activation, true)?)
            }
            _ => Ok(false.into()),
        }
    }

    /// Converts a bool value into the appropriate value for the platform.
    /// This should be used when pushing a bool onto the stack.
    /// This handles SWFv4 pushing a Number, 0 or 1.
    pub fn from_bool(value: bool, swf_version: u8) -> Value<'gc> {
        // SWF version 4 did not have true bools and will push bools as 0 or 1.
        // e.g. SWF19 p. 72:
        // "If the numbers are equal, true is pushed to the stack for SWF 5 and later. For SWF 4, 1 is pushed to the stack."
        if swf_version >= 5 {
            value.into()
        } else {
            (value as i32).into()
        }
    }

    /// Coerce a number to an `u16` following the ECMAScript specifications for `ToUInt16`.
    /// The value will be wrapped modulo 2^16.
    /// This will call `valueOf` and do any conversions that are necessary.
    #[allow(dead_code)]
    pub fn coerce_to_u16(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<u16, Error<'gc>> {
        self.coerce_to_f64(activation).map(f64_to_wrapping_u16)
    }

    /// Coerce a number to an `i16` following the wrapping behavior ECMAScript specifications.
    /// The value will be wrapped in the range [-2^15, 2^15).
    /// This will call `valueOf` and do any conversions that are necessary.
    #[allow(dead_code)]
    pub fn coerce_to_i16(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<i16, Error<'gc>> {
        self.coerce_to_f64(activation).map(f64_to_wrapping_i16)
    }

    /// Coerce a number to an `i32` following the ECMAScript specifications for `ToInt32`.
    /// The value will be wrapped modulo 2^32.
    /// This will call `valueOf` and do any conversions that are necessary.
    /// If you are writing AVM code that accepts an integer, you probably want to use this.
    #[allow(dead_code)]
    pub fn coerce_to_i32(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<i32, Error<'gc>> {
        self.coerce_to_f64(activation).map(f64_to_wrapping_i32)
    }

    /// Coerce a number to an `u32` following the ECMAScript specifications for `ToUInt32`.
    /// The value will be wrapped in the range [-2^31, 2^31).
    /// This will call `valueOf` and do any conversions that are necessary.
    #[allow(dead_code)]
    pub fn coerce_to_u32(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<u32, Error<'gc>> {
        self.coerce_to_f64(activation).map(f64_to_wrapping_u32)
    }

    /// Coerce a value to a string.
    pub fn coerce_to_string(
        &self,
        activation: &mut Activation<'_, 'gc, '_>,
    ) -> Result<AvmString<'gc>, Error<'gc>> {
        Ok(match self {
            Value::Object(object) => match object.call_method("toString", 0, &[], activation)? {
                Value::String(s) => s,
                _ => "[type Object]".into(),
            },
            Value::Undefined => {
                if activation.swf_version() >= 7 {
                    "undefined".into()
                } else {
                    "".into()
                }
            }
            Value::Null => "null".into(),
            Value::Bool(true) => "true".into(),
            Value::Bool(false) => "false".into(),
            Value::Number(v) => match f64_to_string(*v) {
                Cow::Borrowed(s) => s.into(),
                Cow::Owned(s) => AvmString::new(activation.context.gc_context, s),
            },
            Value::String(v) => v.to_owned(),
        })
    }

    pub fn as_bool(&self, swf_version: u8) -> bool {
        match self {
            Value::Bool(v) => *v,
            Value::Number(v) => !v.is_nan() && *v != 0.0,
            Value::String(v) => {
                if swf_version >= 7 {
                    !v.is_empty()
                } else {
                    let num = v.parse().unwrap_or(0.0);
                    num != 0.0
                }
            }
            Value::Object(_) => true,
            _ => false,
        }
    }

    pub fn type_of(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Null => "null",
            Value::Number(_) => "number",
            Value::Bool(_) => "boolean",
            Value::String(_) => "string",
            Value::Object(object) => object.type_of(),
        }
    }

    pub fn coerce_to_object(&self, activation: &mut Activation<'_, 'gc, '_>) -> Object<'gc> {
        ValueObject::boxed(activation, self.to_owned())
    }

    pub fn call(
        &self,
        name: &str,
        activation: &mut Activation<'_, 'gc, '_>,
        this: Object<'gc>,
        depth: u8,
        args: &[Value<'gc>],
    ) -> Result<Value<'gc>, Error<'gc>> {
        if let Value::Object(object) = self {
            object.call(name, activation, this, depth, args)
        } else {
            Ok(Value::Undefined)
        }
    }
}

#[cfg(test)]
mod test {
    use crate::avm1::activation::Activation;
    use crate::avm1::error::Error;
    use crate::avm1::function::{Executable, FunctionObject};
    use crate::avm1::globals::create_globals;
    use crate::avm1::object::script_object::ScriptObject;
    use crate::avm1::object::{Object, TObject};
    use crate::avm1::property::Attribute;
    use crate::avm1::test_utils::with_avm;
    use crate::avm1::{AvmString, Value};

    #[test]
    fn to_primitive_num() {
        with_avm(6, |activation, _this| -> Result<(), Error> {
            let true_value = Value::Bool(true);
            let undefined = Value::Undefined;
            let false_value = Value::Bool(false);
            let null = Value::Null;

            assert_eq!(true_value.to_primitive_num(activation).unwrap(), true_value);
            assert_eq!(undefined.to_primitive_num(activation).unwrap(), undefined);
            assert_eq!(
                false_value.to_primitive_num(activation).unwrap(),
                false_value
            );
            assert_eq!(null.to_primitive_num(activation).unwrap(), null);

            let (protos, global, _) = create_globals(activation.context.gc_context);
            let vglobal = Value::Object(global);

            assert_eq!(vglobal.to_primitive_num(activation).unwrap(), undefined);

            fn value_of_impl<'gc>(
                _activation: &mut Activation<'_, 'gc, '_>,
                _: Object<'gc>,
                _: &[Value<'gc>],
            ) -> Result<Value<'gc>, Error<'gc>> {
                Ok(5.into())
            }

            let valueof = FunctionObject::function(
                activation.context.gc_context,
                Executable::Native(value_of_impl),
                Some(protos.function),
                protos.function,
            );

            let o = ScriptObject::object_cell(activation.context.gc_context, Some(protos.object));
            o.define_value(
                activation.context.gc_context,
                "valueOf",
                valueof.into(),
                Attribute::empty(),
            );

            assert_eq!(
                Value::Object(o).to_primitive_num(activation).unwrap(),
                5.into()
            );

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn to_number_swf7() {
        with_avm(7, |activation, _this| -> Result<(), Error> {
            let t = Value::Bool(true);
            let u = Value::Undefined;
            let f = Value::Bool(false);
            let n = Value::Null;

            assert_eq!(t.coerce_to_f64(activation).unwrap(), 1.0);
            assert!(u.coerce_to_f64(activation).unwrap().is_nan());
            assert_eq!(f.coerce_to_f64(activation).unwrap(), 0.0);
            assert!(n.coerce_to_f64(activation).unwrap().is_nan());

            let bo = Value::Object(ScriptObject::bare_object(activation.context.gc_context).into());

            assert!(bo.coerce_to_f64(activation).unwrap().is_nan());

            Ok(())
        });
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn to_number_swf6() {
        with_avm(6, |activation, _this| -> Result<(), Error> {
            let t = Value::Bool(true);
            let u = Value::Undefined;
            let f = Value::Bool(false);
            let n = Value::Null;

            assert_eq!(t.coerce_to_f64(activation).unwrap(), 1.0);
            assert_eq!(u.coerce_to_f64(activation).unwrap(), 0.0);
            assert_eq!(f.coerce_to_f64(activation).unwrap(), 0.0);
            assert_eq!(n.coerce_to_f64(activation).unwrap(), 0.0);

            let bo = Value::Object(ScriptObject::bare_object(activation.context.gc_context).into());

            assert_eq!(bo.coerce_to_f64(activation).unwrap(), 0.0);

            Ok(())
        });
    }

    #[test]
    fn abstract_lt_num() {
        with_avm(8, |activation, _this| -> Result<(), Error> {
            let a = Value::Number(1.0);
            let b = Value::Number(2.0);

            assert_eq!(a.abstract_lt(b, activation).unwrap(), true.into());

            let nan = Value::Number(f64::NAN);
            assert_eq!(a.abstract_lt(nan, activation).unwrap(), Value::Undefined);

            let inf = Value::Number(f64::INFINITY);
            assert_eq!(a.abstract_lt(inf, activation).unwrap(), true.into());

            let neg_inf = Value::Number(f64::NEG_INFINITY);
            assert_eq!(a.abstract_lt(neg_inf, activation).unwrap(), false.into());

            let zero = Value::Number(0.0);
            assert_eq!(a.abstract_lt(zero, activation).unwrap(), false.into());

            Ok(())
        });
    }

    #[test]
    fn abstract_gt_num() {
        with_avm(8, |activation, _this| -> Result<(), Error> {
            let a = Value::Number(1.0);
            let b = Value::Number(2.0);

            assert_eq!(b.abstract_lt(a, activation).unwrap(), false.into());

            let nan = Value::Number(f64::NAN);
            assert_eq!(nan.abstract_lt(a, activation).unwrap(), Value::Undefined);

            let inf = Value::Number(f64::INFINITY);
            assert_eq!(inf.abstract_lt(a, activation).unwrap(), false.into());

            let neg_inf = Value::Number(f64::NEG_INFINITY);
            assert_eq!(neg_inf.abstract_lt(a, activation).unwrap(), true.into());

            let zero = Value::Number(0.0);
            assert_eq!(zero.abstract_lt(a, activation).unwrap(), true.into());

            Ok(())
        });
    }

    #[test]
    fn abstract_lt_str() {
        with_avm(8, |activation, _this| -> Result<(), Error> {
            let a = Value::String(AvmString::new(
                activation.context.gc_context,
                "a".to_owned(),
            ));
            let b = Value::String(AvmString::new(
                activation.context.gc_context,
                "b".to_owned(),
            ));

            assert_eq!(a.abstract_lt(b, activation).unwrap(), true.into());

            Ok(())
        })
    }

    #[test]
    fn abstract_gt_str() {
        with_avm(8, |activation, _this| -> Result<(), Error> {
            let a = Value::String(AvmString::new(
                activation.context.gc_context,
                "a".to_owned(),
            ));
            let b = Value::String(AvmString::new(
                activation.context.gc_context,
                "b".to_owned(),
            ));

            assert_eq!(b.abstract_lt(a, activation).unwrap(), false.into());

            Ok(())
        })
    }

    #[test]
    #[allow(clippy::unreadable_literal)]

    fn wrapping_u16() {
        use super::f64_to_wrapping_u16;
        assert_eq!(f64_to_wrapping_u16(0.0), 0);
        assert_eq!(f64_to_wrapping_u16(1.0), 1);
        assert_eq!(f64_to_wrapping_u16(-1.0), 65535);
        assert_eq!(f64_to_wrapping_u16(123.1), 123);
        assert_eq!(f64_to_wrapping_u16(66535.9), 999);
        assert_eq!(f64_to_wrapping_u16(-9980.7), 55556);
        assert_eq!(f64_to_wrapping_u16(-196608.0), 0);
        assert_eq!(f64_to_wrapping_u16(f64::NAN), 0);
        assert_eq!(f64_to_wrapping_u16(f64::INFINITY), 0);
        assert_eq!(f64_to_wrapping_u16(f64::NEG_INFINITY), 0);
    }

    #[test]
    #[allow(clippy::unreadable_literal)]

    fn wrapping_i16() {
        use super::f64_to_wrapping_i16;
        assert_eq!(f64_to_wrapping_i16(0.0), 0);
        assert_eq!(f64_to_wrapping_i16(1.0), 1);
        assert_eq!(f64_to_wrapping_i16(-1.0), -1);
        assert_eq!(f64_to_wrapping_i16(123.1), 123);
        assert_eq!(f64_to_wrapping_i16(32768.9), -32768);
        assert_eq!(f64_to_wrapping_i16(-32769.9), 32767);
        assert_eq!(f64_to_wrapping_i16(-33268.1), 32268);
        assert_eq!(f64_to_wrapping_i16(-196608.0), 0);
        assert_eq!(f64_to_wrapping_i16(f64::NAN), 0);
        assert_eq!(f64_to_wrapping_i16(f64::INFINITY), 0);
        assert_eq!(f64_to_wrapping_i16(f64::NEG_INFINITY), 0);
    }

    #[test]
    #[allow(clippy::unreadable_literal)]
    fn wrapping_u32() {
        use super::f64_to_wrapping_u32;
        assert_eq!(f64_to_wrapping_u32(0.0), 0);
        assert_eq!(f64_to_wrapping_u32(1.0), 1);
        assert_eq!(f64_to_wrapping_u32(-1.0), 4294967295);
        assert_eq!(f64_to_wrapping_u32(123.1), 123);
        assert_eq!(f64_to_wrapping_u32(4294968295.9), 999);
        assert_eq!(f64_to_wrapping_u32(-4289411740.3), 5555556);
        assert_eq!(f64_to_wrapping_u32(-12884901888.0), 0);
        assert_eq!(f64_to_wrapping_u32(f64::NAN), 0);
        assert_eq!(f64_to_wrapping_u32(f64::INFINITY), 0);
        assert_eq!(f64_to_wrapping_u32(f64::NEG_INFINITY), 0);
    }

    #[test]
    #[allow(clippy::unreadable_literal)]
    fn wrapping_i32() {
        use super::f64_to_wrapping_i32;
        assert_eq!(f64_to_wrapping_i32(0.0), 0);
        assert_eq!(f64_to_wrapping_i32(1.0), 1);
        assert_eq!(f64_to_wrapping_i32(-1.0), -1);
        assert_eq!(f64_to_wrapping_i32(123.1), 123);
        assert_eq!(f64_to_wrapping_i32(4294968295.9), 999);
        assert_eq!(f64_to_wrapping_i32(2147484648.3), -2147482648);
        assert_eq!(f64_to_wrapping_i32(-8589934591.2), 1);
        assert_eq!(f64_to_wrapping_i32(4294966896.1), -400);
        assert_eq!(f64_to_wrapping_i32(f64::NAN), 0);
        assert_eq!(f64_to_wrapping_i32(f64::INFINITY), 0);
        assert_eq!(f64_to_wrapping_i32(f64::NEG_INFINITY), 0);
    }

    #[test]
    fn f64_to_string() {
        use super::f64_to_string;
        assert_eq!(f64_to_string(0.0), "0");
        assert_eq!(f64_to_string(-0.0), "0");
        assert_eq!(f64_to_string(1.0), "1");
        assert_eq!(f64_to_string(1.4), "1.4");
        assert_eq!(f64_to_string(-990.123), "-990.123");
        assert_eq!(f64_to_string(f64::NAN), "NaN");
        assert_eq!(f64_to_string(f64::INFINITY), "Infinity");
        assert_eq!(f64_to_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(f64_to_string(9.9999e14), "999990000000000");
        assert_eq!(f64_to_string(-9.9999e14), "-999990000000000");
        assert_eq!(f64_to_string(1e15), "1e+15");
        assert_eq!(f64_to_string(-1e15), "-1e+15");
        assert_eq!(f64_to_string(1e-5), "0.00001");
        assert_eq!(f64_to_string(-1e-5), "-0.00001");
        assert_eq!(f64_to_string(0.999e-5), "9.99e-6");
        assert_eq!(f64_to_string(-0.999e-5), "-9.99e-6");
    }
}
