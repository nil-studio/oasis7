use super::*;
use serde::ser::{self, SerializeSeq, SerializeTuple, SerializeTupleStruct, SerializeTupleVariant};
pub(super) struct Node<'a, 'e, 'v, T: ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    pub engine: &'e mut Engine<'a, A, C>,
    pub out: &'e mut dyn Write,
    pub root: &'v T,
}
pub(super) struct Seq<'a, 'e, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    engine: &'e mut Engine<'a, A, C>,
    out: &'e mut dyn Write,
    count: u64,
    expected: u64,
    tag: bool,
}
impl<'a, 'e, 'v, T: Serialize + ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool>
    Node<'a, 'e, 'v, T, A, C>
{
    fn sequence(
        self,
        variant: Option<(&'static str, &'static str)>,
    ) -> Result<Seq<'a, 'e, A, C>, Error> {
        let tag = variant == Some(("@@TAG@@", "@@TAGGED@@"));
        let n = self.root.serialize(super::replay::Probe {
            engine: self.engine,
            tag: false,
        })?;
        if tag && n != 2 {
            return Err(Error::Invalid("tag tuple must contain tag/value"));
        }
        if !tag {
            if let Some((_, variant)) = variant {
                self.engine.head(self.out, 5, 1)?;
                self.engine.text(self.out, variant)?;
            }
            self.engine.head(self.out, 4, n)?;
        }
        Ok(Seq {
            engine: self.engine,
            out: self.out,
            count: 0,
            expected: n,
            tag,
        })
    }
}
impl<'a, 'e, 'v, T: Serialize + ?Sized, A: CanonicalCborAllocator, C: FnMut() -> bool>
    ser::Serializer for Node<'a, 'e, 'v, T, A, C>
{
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Seq<'a, 'e, A, C>;
    type SerializeTuple = Self::SerializeSeq;
    type SerializeTupleStruct = Self::SerializeSeq;
    type SerializeTupleVariant = Self::SerializeSeq;
    type SerializeMap = super::keys::Map<'a, 'e, 'v, T, A, C>;
    type SerializeStruct = Self::SerializeMap;
    type SerializeStructVariant = Self::SerializeMap;
    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_i128(self, v: i128) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_u128(self, v: u128) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        self.engine.primitive(&v, self.out)
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        let mut b = [0u8; 4];
        self.engine.text(self.out, v.encode_utf8(&mut b))
    }
    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.engine.text(self.out, v)
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        self.engine.head(self.out, 2, v.len() as u64)?;
        self.engine.emit(self.out, v)
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.engine.primitive(&(), self.out)
    }
    fn serialize_some<V: Serialize + ?Sized>(self, v: &V) -> Result<(), Error> {
        self.engine.encode(v, self.out)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.serialize_none()
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        self.serialize_none()
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, v: &'static str) -> Result<(), Error> {
        self.engine.text(self.out, v)
    }
    fn serialize_newtype_struct<V: Serialize + ?Sized>(
        self,
        _: &'static str,
        v: &V,
    ) -> Result<(), Error> {
        self.engine.encode(v, self.out)
    }
    fn serialize_newtype_variant<V: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        v: &V,
    ) -> Result<(), Error> {
        if (name, variant) != ("@@TAG@@", "@@UNTAGGED@@") {
            self.engine.head(self.out, 5, 1)?;
            self.engine.text(self.out, variant)?;
        }
        self.engine.encode(v, self.out)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        self.sequence(None)
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        self.sequence(None)
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        self.sequence(None)
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        self.sequence(Some((name, variant)))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        super::keys::Map::new(self.engine, self.out, self.root, super::keys::Kind::Map)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        super::keys::Map::new(self.engine, self.out, self.root, super::keys::Kind::Struct)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        self.engine.head(self.out, 5, 1)?;
        self.engine.text(self.out, variant)?;
        super::keys::Map::new(
            self.engine,
            self.out,
            self.root,
            super::keys::Kind::Variant(variant),
        )
    }
    fn is_human_readable(&self) -> bool {
        false
    }
    fn collect_str<V: fmt::Display + ?Sized>(self, value: &V) -> Result<(), Error> {
        self.engine.display(value, self.out)
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> Seq<'_, '_, A, C> {
    fn element<V: Serialize + ?Sized>(&mut self, v: &V) -> Result<(), Error> {
        self.engine.step()?;
        if self.count >= self.expected {
            return Err(Error::Invalid("sequence changed during replay"));
        }
        if self.tag && self.count == 0 {
            let tag = v.serialize(super::replay::Probe {
                engine: self.engine,
                tag: true,
            })?;
            self.engine.head(self.out, 6, tag)?;
        } else {
            self.engine.encode(v, self.out)?;
        }
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Result<(), Error> {
        if self.count != self.expected {
            return Err(Error::Invalid("sequence changed during replay"));
        }
        self.engine.step()
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeSeq for Seq<'_, '_, A, C> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<V: Serialize + ?Sized>(&mut self, v: &V) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeTuple for Seq<'_, '_, A, C> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<V: Serialize + ?Sized>(&mut self, v: &V) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeTupleStruct for Seq<'_, '_, A, C> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(&mut self, v: &V) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeTupleVariant for Seq<'_, '_, A, C> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(&mut self, v: &V) -> Result<(), Error> {
        self.element(v)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
