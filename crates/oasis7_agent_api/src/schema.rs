//! Optional developer-time schemas. This module never changes protocol serialization.
use crate::game::DecimalU64;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use std::borrow::Cow;

/// Regex alternatives for canonical positive decimal strings bounded by `maximum`.
/// The caller supplies a nonzero decimal constant, never untrusted input.
pub fn bounded_decimal_pattern(maximum: &str) -> String {
    let mut alternatives = vec!["0".to_owned()];
    if maximum.len() > 1 {
        alternatives.push(format!("[1-9][0-9]{{0,{}}}", maximum.len() - 2));
    }
    for (index, digit) in maximum.bytes().enumerate() {
        let low = if index == 0 { b'1' } else { b'0' };
        if digit <= low {
            continue;
        }
        let range = if digit == low + 1 {
            char::from(low).to_string()
        } else {
            format!("[{}-{}]", char::from(low), char::from(digit - 1))
        };
        let remaining = maximum.len() - index - 1;
        alternatives.push(format!(
            "{}{}{}",
            &maximum[..index],
            range,
            if remaining == 0 {
                String::new()
            } else {
                format!("[0-9]{{{remaining}}}")
            }
        ));
    }
    alternatives.push(maximum.to_owned());
    format!("(?:{})", alternatives.join("|"))
}
impl JsonSchema for DecimalU64 {
    fn schema_name() -> Cow<'static, str> {
        "DecimalU64".into()
    }
    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({"type":"string", "minLength":1, "maxLength":20,
            "pattern":format!("^{}$",bounded_decimal_pattern("18446744073709551615"))})
    }
}
