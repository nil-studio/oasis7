//! A work-only walk of the actual typed fields. It encodes no wire format,
//! allocates no graph and cannot supply a validator or acceptance verdict.
use oasis7_wasm_abi::{CanonicalCborStreamError, CapabilityCanonicalWorkV1};
use serde::{
    Serialize,
    ser::{
        self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
        SerializeTupleStruct, SerializeTupleVariant,
    },
};
#[derive(Debug)]
pub(super) struct Error(CanonicalCborStreamError);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self(CanonicalCborStreamError::Invalid(
            "typed correlation validation field walk",
        ))
    }
}
struct Trace<'a, A> {
    source: &'a A,
    metrics: Option<&'a std::cell::Cell<(usize, usize)>>,
}
impl<A: CapabilityCanonicalWorkV1> Trace<'_, A> {
    fn add(&self, n: usize, b: usize) -> Result<(), Error> {
        if let Some(metrics) = self.metrics {
            let (oldn, oldb) = metrics.get();
            metrics.set((
                oldn.checked_add(n)
                    .ok_or(Error(CanonicalCborStreamError::ResourceLimited(
                        "validation field count",
                    )))?,
                oldb.checked_add(b)
                    .ok_or(Error(CanonicalCborStreamError::ResourceLimited(
                        "validation field bytes",
                    )))?,
            ));
        }
        Ok(())
    }
    fn text(&self, v: &str) -> Result<(), Error> {
        self.add(1, v.len())?;
        for part in v.as_bytes().chunks(8192) {
            self.source
                .charge_work(1 + (part.len() as u64).saturating_mul(4), part.len() as u64)
                .map_err(Error)?;
        }
        Ok(())
    }
    fn scalar(&self, n: u64) -> Result<(), Error> {
        self.add(1, n as usize)?;
        self.source.charge_work(1, n).map_err(Error)
    }
}
macro_rules! scalar{($($name:ident:$t:ty),*)=>{$(fn $name(self,_:$t)->Result<(),Error>{self.scalar(std::mem::size_of::<$t>()as u64)})*};}
impl<'a, A: CapabilityCanonicalWorkV1> ser::Serializer for Trace<'a, A> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;
    scalar!(serialize_bool:bool,serialize_i8:i8,serialize_i16:i16,serialize_i32:i32,serialize_i64:i64,serialize_i128:i128,serialize_u8:u8,serialize_u16:u16,serialize_u32:u32,serialize_u64:u64,serialize_u128:u128,serialize_f32:f32,serialize_f64:f64,serialize_char:char);
    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.text(v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        self.add(1, v.len())?;
        for part in v.chunks(8192) {
            self.source
                .charge_work(1 + part.len() as u64, part.len() as u64)
                .map_err(Error)?;
        }
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.scalar(0)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<(), Error> {
        self.scalar(0)?;
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.scalar(0)
    }
    fn serialize_unit_struct(self, n: &'static str) -> Result<(), Error> {
        self.text(n)
    }
    fn serialize_unit_variant(self, n: &'static str, _: u32, v: &'static str) -> Result<(), Error> {
        self.text(n)?;
        self.text(v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.text(n)?;
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        _: u32,
        k: &'static str,
        v: &T,
    ) -> Result<(), Error> {
        self.text(n)?;
        self.text(k)?;
        v.serialize(self)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self, Error> {
        self.scalar(0)?;
        Ok(self)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self, Error> {
        self.scalar(0)?;
        Ok(self)
    }
    fn serialize_tuple_struct(self, n: &'static str, _: usize) -> Result<Self, Error> {
        self.text(n)?;
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        n: &'static str,
        _: u32,
        k: &'static str,
        _: usize,
    ) -> Result<Self, Error> {
        self.text(n)?;
        self.text(k)?;
        Ok(self)
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self, Error> {
        self.scalar(0)?;
        Ok(self)
    }
    fn serialize_struct(self, n: &'static str, _: usize) -> Result<Self, Error> {
        self.text(n)?;
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        n: &'static str,
        _: u32,
        k: &'static str,
        _: usize,
    ) -> Result<Self, Error> {
        self.text(n)?;
        self.text(k)?;
        Ok(self)
    }
    fn collect_str<T: std::fmt::Display + ?Sized>(self, v: &T) -> Result<(), Error> {
        struct Output<'a, A> {
            source: &'a A,
            metrics: Option<&'a std::cell::Cell<(usize, usize)>>,
            first: Option<CanonicalCborStreamError>,
        }
        impl<A: CapabilityCanonicalWorkV1> std::fmt::Write for Output<'_, A> {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                if let Err(Error(e)) = (Trace {
                    source: self.source,
                    metrics: self.metrics,
                })
                .text(text)
                {
                    self.first = Some(e);
                    return Err(std::fmt::Error);
                }
                Ok(())
            }
        }
        let mut out = Output {
            source: self.source,
            metrics: self.metrics,
            first: None,
        };
        let result = std::fmt::write(&mut out, format_args!("{v}"));
        if let Some(e) = out.first {
            return Err(Error(e));
        }
        result.map_err(|_| Error(CanonicalCborStreamError::Invalid("typed field Display")))
    }
}
macro_rules! compound {
    ($trait:ident,$method:ident) => {
        impl<A: CapabilityCanonicalWorkV1> $trait for Trace<'_, A> {
            type Ok = ();
            type Error = Error;
            fn $method<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
                v.serialize(Trace {
                    source: self.source,
                    metrics: self.metrics,
                })
            }
            fn end(self) -> Result<(), Error> {
                self.scalar(0)
            }
        }
    };
}
compound!(SerializeSeq, serialize_element);
compound!(SerializeTuple, serialize_element);
compound!(SerializeTupleStruct, serialize_field);
compound!(SerializeTupleVariant, serialize_field);
impl<A: CapabilityCanonicalWorkV1> SerializeMap for Trace<'_, A> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        v.serialize(Trace {
            source: self.source,
            metrics: self.metrics,
        })
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, v: &T) -> Result<(), Error> {
        v.serialize(Trace {
            source: self.source,
            metrics: self.metrics,
        })
    }
    fn end(self) -> Result<(), Error> {
        self.scalar(0)
    }
}
macro_rules! fields {
    ($trait:ident) => {
        impl<A: CapabilityCanonicalWorkV1> $trait for Trace<'_, A> {
            type Ok = ();
            type Error = Error;
            fn serialize_field<T: Serialize + ?Sized>(
                &mut self,
                k: &'static str,
                v: &T,
            ) -> Result<(), Error> {
                self.text(k)?;
                v.serialize(Trace {
                    source: self.source,
                    metrics: self.metrics,
                })
            }
            fn end(self) -> Result<(), Error> {
                self.scalar(0)
            }
        }
    };
}
fields!(SerializeStruct);
fields!(SerializeStructVariant);
pub(super) fn trace<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
) -> Result<(), CanonicalCborStreamError> {
    value
        .serialize(Trace {
            source,
            metrics: None,
        })
        .map_err(|e| e.0)
}

pub(super) fn trace_size<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
) -> Result<(usize, usize), CanonicalCborStreamError> {
    let metrics = std::cell::Cell::new((0usize, 0usize));
    value
        .serialize(Trace {
            source,
            metrics: Some(&metrics),
        })
        .map_err(|e| e.0)?;
    Ok(metrics.get())
}
