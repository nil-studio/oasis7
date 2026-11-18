//! Measure the actual original Serde call depth. CBOR wire depth alone omits
//! Option and newtype frames used by the maintained canonical encoder.
use super::*;
use serde::{Serialize, Serializer, ser};

#[derive(Debug)]
struct Error(BodyMutationError);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(paid(<serde_cbor::Error as ser::Error>::custom(message)))
    }
}
struct Count<'a, 'c> {
    common: &'a mut FormalBoundedContextV1<'c>,
    maximum: usize,
}
impl Count<'_, '_> {
    fn bytes(&mut self, bytes: usize) -> Result<(), Error> {
        self.common
            .charge(1, bytes as u64)
            .map_err(|e| Error(paid(e)))
    }
    fn visit<T: Serialize + ?Sized>(&mut self, value: &T, depth: usize) -> Result<(), Error> {
        self.bytes(size_of::<Node<'_, '_, '_>>())?;
        let _frame = self
            .common
            .memory()
            .reserve(size_of::<Node<'_, '_, '_>>(), MemoryClass::Ordinary)
            .map_err(|e| Error(ValueTreeError::Memory(e).into()))?;
        self.maximum = self.maximum.max(depth);
        value.serialize(Node { count: self, depth })
    }
}
struct Node<'a, 'b, 'c> {
    count: &'a mut Count<'b, 'c>,
    depth: usize,
}
impl<'a, 'b, 'c> Node<'a, 'b, 'c> {
    fn child<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        let depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| Error(invalid("GEN authorization Serde depth")))?;
        self.count.visit(value, depth)
    }
    fn variant(mut self, name: &str) -> Result<Self, Error> {
        self.count.bytes(name.len())?;
        self.depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| Error(invalid("GEN authorization Serde variant depth")))?;
        self.count.maximum = self.count.maximum.max(self.depth);
        Ok(self)
    }
}
macro_rules! scalar {
    ($($name:ident($ty:ty)),* $(,)?) => {$ (
        fn $name(self, _: $ty) -> Result<(), Error> {
            self.count.bytes(size_of::<$ty>())
        }
    )* };
}
impl<'a, 'b, 'c> Serializer for Node<'a, 'b, 'c> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;
    scalar!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char)
    );
    fn serialize_str(self, value: &str) -> Result<(), Error> {
        self.count.bytes(value.len())
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), Error> {
        self.count.bytes(value.len())
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.count.bytes(1)
    }
    fn serialize_some<T: Serialize + ?Sized>(mut self, value: &T) -> Result<(), Error> {
        self.child(value)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.count.bytes(1)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        self.count.bytes(1)
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
    ) -> Result<(), Error> {
        self.serialize_str(name)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        mut self,
        _: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.child(value)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        mut self,
        _: &'static str,
        _: u32,
        name: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.count.bytes(name.len())?;
        self.child(value)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self, Error> {
        Ok(self)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self, Error> {
        Ok(self)
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Self, Error> {
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Self, Error> {
        self.variant(name)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self, Error> {
        Ok(self)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self, Error> {
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        name: &'static str,
        _: usize,
    ) -> Result<Self, Error> {
        self.variant(name)
    }
    fn collect_str<T: std::fmt::Display + ?Sized>(self, value: &T) -> Result<(), Error> {
        struct Output<'a, 'b, 'c> {
            count: &'a mut Count<'b, 'c>,
            first: Option<Error>,
        }
        impl std::fmt::Write for Output<'_, '_, '_> {
            fn write_str(&mut self, value: &str) -> std::fmt::Result {
                if self.first.is_some() {
                    return Err(std::fmt::Error);
                }
                if let Err(error) = self.count.bytes(value.len()) {
                    self.first = Some(error);
                    return Err(std::fmt::Error);
                }
                Ok(())
            }
        }
        let mut out = Output {
            count: self.count,
            first: None,
        };
        let result = std::fmt::write(&mut out, format_args!("{value}"));
        if let Some(error) = out.first {
            return Err(error);
        }
        result.map_err(|e| Error(paid(e)))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}
macro_rules! sequence {
    ($trait:ident, $method:ident) => {
        impl ser::$trait for Node<'_, '_, '_> {
            type Ok = ();
            type Error = Error;
            fn $method<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
                self.child(value)
            }
            fn end(self) -> Result<(), Error> {
                Ok(())
            }
        }
    };
}
sequence!(SerializeSeq, serialize_element);
sequence!(SerializeTuple, serialize_element);
sequence!(SerializeTupleStruct, serialize_field);
sequence!(SerializeTupleVariant, serialize_field);
impl ser::SerializeMap for Node<'_, '_, '_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.child(value)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        self.child(value)
    }
    fn end(self) -> Result<(), Error> {
        Ok(())
    }
}
macro_rules! structure {
    ($trait:ident) => {
        impl ser::$trait for Node<'_, '_, '_> {
            type Ok = ();
            type Error = Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                key: &'static str,
                value: &T,
            ) -> Result<(), Error> {
                self.child(key)?;
                self.child(value)
            }
            fn end(self) -> Result<(), Error> {
                Ok(())
            }
        }
    };
}
structure!(SerializeStruct);
structure!(SerializeStructVariant);
pub(super) fn measure<T: Serialize + ?Sized>(
    source: &T,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    let mut count = Count { common, maximum: 0 };
    count.visit(source, 1).map_err(|e| e.0)?;
    Ok(count.maximum)
}
