// Test-only recorder for legacy adjacent-tag wire compatibility.
// It records the public Serde calls made by adjacent-tagged event/action enums;
// it intentionally does not inspect private Rust implementation type names.

use serde::Serialize;
use serde::ser::{self, Impossible, SerializeStruct, Serializer};
use serde_json::Value;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TraceEvent {
    StructStart {
        name: String,
        len: usize,
    },
    Field {
        name: String,
        value: Value,
    },
    SkipField {
        name: String,
    },
    UnitVariant {
        enum_name: String,
        variant_index: u32,
        variant_name: String,
    },
    StructEnd,
}

#[derive(Debug, Clone, Copy)]
enum TraceMode {
    Outer,
    Tag,
    Content,
}

#[derive(Debug)]
pub struct TraceError(String);

impl fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TraceError {}

impl ser::Error for TraceError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self(message.to_string())
    }
}

impl From<serde_json::Error> for TraceError {
    fn from(error: serde_json::Error) -> Self {
        Self(format!("nested JSON value capture failed: {error}"))
    }
}

struct TraceSerializer<'a> {
    events: &'a mut Vec<TraceEvent>,
    mode: TraceMode,
}

struct TraceStruct<'a> {
    events: &'a mut Vec<TraceEvent>,
    mode: TraceMode,
}

fn unsupported<T>(operation: &str) -> Result<T, TraceError> {
    Err(TraceError(format!(
        "unexpected Serde operation in scoped Action/DomainEvent trace: {operation}"
    )))
}

/// Record the outer adjacent-tag struct, its unit-variant tag, and content
/// struct. Payload values are captured as JSON at each public field callback.
pub fn record_public_serde_calls<T: Serialize>(value: &T) -> Result<Vec<TraceEvent>, TraceError> {
    let mut events = Vec::new();
    value.serialize(TraceSerializer {
        events: &mut events,
        mode: TraceMode::Outer,
    })?;
    Ok(events)
}

impl SerializeStruct for TraceStruct<'_> {
    type Ok = ();
    type Error = TraceError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), TraceError> {
        let json_value = serde_json::to_value(value)?;
        self.events.push(TraceEvent::Field {
            name: key.to_owned(),
            value: json_value,
        });

        match self.mode {
            TraceMode::Outer => {
                let nested_mode = match key {
                    "type" => TraceMode::Tag,
                    "data" => TraceMode::Content,
                    _ => return unsupported("outer field other than `type` or `data`"),
                };
                value.serialize(TraceSerializer {
                    events: self.events,
                    mode: nested_mode,
                })
            }
            TraceMode::Content => Ok(()),
            TraceMode::Tag => unsupported("struct field while serializing tag"),
        }
    }

    fn skip_field(&mut self, key: &'static str) -> Result<(), TraceError> {
        match self.mode {
            TraceMode::Content => {
                self.events.push(TraceEvent::SkipField {
                    name: key.to_owned(),
                });
                Ok(())
            }
            TraceMode::Outer => unsupported("skipped outer field"),
            TraceMode::Tag => unsupported("skipped field while serializing tag"),
        }
    }

    fn end(self) -> Result<(), TraceError> {
        self.events.push(TraceEvent::StructEnd);
        Ok(())
    }
}

impl<'a> Serializer for TraceSerializer<'a> {
    type Ok = ();
    type Error = TraceError;
    type SerializeSeq = Impossible<(), TraceError>;
    type SerializeTuple = Impossible<(), TraceError>;
    type SerializeTupleStruct = Impossible<(), TraceError>;
    type SerializeTupleVariant = Impossible<(), TraceError>;
    type SerializeMap = Impossible<(), TraceError>;
    type SerializeStruct = TraceStruct<'a>;
    type SerializeStructVariant = Impossible<(), TraceError>;

    fn serialize_bool(self, _value: bool) -> Result<(), TraceError> {
        unsupported("bool")
    }

    fn serialize_i8(self, _value: i8) -> Result<(), TraceError> {
        unsupported("i8")
    }

    fn serialize_i16(self, _value: i16) -> Result<(), TraceError> {
        unsupported("i16")
    }

    fn serialize_i32(self, _value: i32) -> Result<(), TraceError> {
        unsupported("i32")
    }

    fn serialize_i64(self, _value: i64) -> Result<(), TraceError> {
        unsupported("i64")
    }

    fn serialize_u8(self, _value: u8) -> Result<(), TraceError> {
        unsupported("u8")
    }

    fn serialize_u16(self, _value: u16) -> Result<(), TraceError> {
        unsupported("u16")
    }

    fn serialize_u32(self, _value: u32) -> Result<(), TraceError> {
        unsupported("u32")
    }

    fn serialize_u64(self, _value: u64) -> Result<(), TraceError> {
        unsupported("u64")
    }

    fn serialize_f32(self, _value: f32) -> Result<(), TraceError> {
        unsupported("f32")
    }

    fn serialize_f64(self, _value: f64) -> Result<(), TraceError> {
        unsupported("f64")
    }

    fn serialize_char(self, _value: char) -> Result<(), TraceError> {
        unsupported("char")
    }

    fn serialize_str(self, _value: &str) -> Result<(), TraceError> {
        unsupported("str")
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<(), TraceError> {
        unsupported("bytes")
    }

    fn serialize_none(self) -> Result<(), TraceError> {
        unsupported("none")
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<(), TraceError> {
        unsupported("some")
    }

    fn serialize_unit(self) -> Result<(), TraceError> {
        unsupported("unit")
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), TraceError> {
        unsupported("unit struct")
    }

    fn serialize_unit_variant(
        self,
        enum_name: &'static str,
        variant_index: u32,
        variant_name: &'static str,
    ) -> Result<(), TraceError> {
        match self.mode {
            TraceMode::Tag => {
                self.events.push(TraceEvent::UnitVariant {
                    enum_name: enum_name.to_owned(),
                    variant_index,
                    variant_name: variant_name.to_owned(),
                });
                Ok(())
            }
            _ => unsupported("unit variant outside tag field"),
        }
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<(), TraceError> {
        unsupported("newtype struct")
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _enum_name: &'static str,
        _variant_index: u32,
        _variant_name: &'static str,
        _value: &T,
    ) -> Result<(), TraceError> {
        unsupported("newtype variant")
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, TraceError> {
        unsupported("sequence")
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, TraceError> {
        unsupported("tuple")
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, TraceError> {
        unsupported("tuple struct")
    }

    fn serialize_tuple_variant(
        self,
        _enum_name: &'static str,
        _variant_index: u32,
        _variant_name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, TraceError> {
        unsupported("tuple variant")
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, TraceError> {
        unsupported("map")
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, TraceError> {
        match self.mode {
            TraceMode::Outer | TraceMode::Content => {
                self.events.push(TraceEvent::StructStart {
                    name: name.to_owned(),
                    len,
                });
                Ok(TraceStruct {
                    events: self.events,
                    mode: self.mode,
                })
            }
            TraceMode::Tag => unsupported("struct while serializing tag"),
        }
    }

    fn serialize_struct_variant(
        self,
        _enum_name: &'static str,
        _variant_index: u32,
        _variant_name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, TraceError> {
        unsupported("struct variant")
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}
