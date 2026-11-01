use super::*;
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
pub(super) struct Probe<'a, 'e, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    pub engine: &'e mut Engine<'a, A, C>,
    pub tag: bool,
}
pub(super) struct Count<'a, 'e, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    engine: &'e mut Engine<'a, A, C>,
    count: u64,
}
impl<'a, 'e, A: CanonicalCborAllocator, C: FnMut() -> bool> Probe<'a, 'e, A, C> {
    fn sequence(self) -> Result<Count<'a, 'e, A, C>, Error> {
        self.engine.step()?;
        if self.tag {
            return Err(Error::Invalid("tag is not unsigned primitive"));
        }
        Ok(Count {
            engine: self.engine,
            count: 0,
        })
    }
}
impl<'a, 'e, A: CanonicalCborAllocator, C: FnMut() -> bool> ser::Serializer
    for Probe<'a, 'e, A, C>
{
    type Ok = u64;
    type Error = Error;
    type SerializeSeq = Count<'a, 'e, A, C>;
    type SerializeTuple = Self::SerializeSeq;
    type SerializeTupleStruct = Self::SerializeSeq;
    type SerializeTupleVariant = Self::SerializeSeq;
    type SerializeMap = ser::Impossible<u64, Error>;
    type SerializeStruct = Self::SerializeMap;
    type SerializeStructVariant = Self::SerializeMap;
    fn serialize_bool(self, _: bool) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_i8(self, _: i8) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_i16(self, _: i16) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_i32(self, _: i32) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_i64(self, _: i64) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_i128(self, _: i128) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_u8(self, v: u8) -> Result<u64, Error> {
        self.engine.step()?;
        if self.tag {
            Ok(v as u64)
        } else {
            Err(Error::Invalid("sequence probe shape"))
        }
    }
    fn serialize_u16(self, v: u16) -> Result<u64, Error> {
        self.engine.step()?;
        if self.tag {
            Ok(v as u64)
        } else {
            Err(Error::Invalid("sequence probe shape"))
        }
    }
    fn serialize_u32(self, v: u32) -> Result<u64, Error> {
        self.engine.step()?;
        if self.tag {
            Ok(v as u64)
        } else {
            Err(Error::Invalid("sequence probe shape"))
        }
    }
    fn serialize_u64(self, v: u64) -> Result<u64, Error> {
        self.engine.step()?;
        if self.tag {
            Ok(v)
        } else {
            Err(Error::Invalid("sequence probe shape"))
        }
    }
    fn serialize_u128(self, _: u128) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_f32(self, _: f32) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_f64(self, _: f64) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_char(self, _: char) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_str(self, _: &str) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<u64, Error> {
        Err(Error::Invalid("sequence/tag probe shape"))
    }
    fn serialize_none(self) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_some<V: Serialize + ?Sized>(self, _: &V) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_unit(self) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_newtype_struct<V: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &V,
    ) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_newtype_variant<V: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &V,
    ) -> Result<u64, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        self.sequence()
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        self.sequence()
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        self.sequence()
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        self.sequence()
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        Err(Error::Invalid("sequence probe shape"))
    }
    fn collect_str<V: fmt::Display + ?Sized>(self, _: &V) -> Result<Self::Ok, Error> {
        Err(Error::Invalid("probe/replay expected collection"))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeSeq for Count<'_, '_, A, C> {
    type Ok = u64;
    type Error = Error;
    fn serialize_element<V: Serialize + ?Sized>(&mut self, _: &V) -> Result<(), Error> {
        self.engine.step()?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(Error::ResourceLimited("sequence length"))?;
        Ok(())
    }
    fn end(self) -> Result<u64, Error> {
        self.engine.step()?;
        Ok(self.count)
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeTuple for Count<'_, '_, A, C> {
    type Ok = u64;
    type Error = Error;
    fn serialize_element<V: Serialize + ?Sized>(&mut self, _: &V) -> Result<(), Error> {
        self.engine.step()?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(Error::ResourceLimited("sequence length"))?;
        Ok(())
    }
    fn end(self) -> Result<u64, Error> {
        self.engine.step()?;
        Ok(self.count)
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeTupleStruct for Count<'_, '_, A, C> {
    type Ok = u64;
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(&mut self, _: &V) -> Result<(), Error> {
        self.engine.step()?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(Error::ResourceLimited("sequence length"))?;
        Ok(())
    }
    fn end(self) -> Result<u64, Error> {
        self.engine.step()?;
        Ok(self.count)
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeTupleVariant for Count<'_, '_, A, C> {
    type Ok = u64;
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(&mut self, _: &V) -> Result<(), Error> {
        self.engine.step()?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(Error::ResourceLimited("sequence length"))?;
        Ok(())
    }
    fn end(self) -> Result<u64, Error> {
        self.engine.step()?;
        Ok(self.count)
    }
}
pub(super) struct Select<'a, 'e, 'k, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    pub engine: &'e mut Engine<'a, A, C>,
    pub out: &'e mut dyn Write,
    pub index: usize,
    pub expected: &'k [u8],
    pub total: usize,
    pub kind: super::keys::Kind,
}
pub(super) struct SelectedMap<'a, 'e, 'k, A: CanonicalCborAllocator, C: FnMut() -> bool> {
    select: Select<'a, 'e, 'k, A, C>,
    count: usize,
    pending: bool,
    found: bool,
}
impl<'a, 'e, 'k, A: CanonicalCborAllocator, C: FnMut() -> bool> Select<'a, 'e, 'k, A, C> {
    fn map(self, kind: super::keys::Kind) -> Result<SelectedMap<'a, 'e, 'k, A, C>, Error> {
        self.engine.step()?;
        if self.kind != kind {
            return Err(Error::Invalid("map shape changed during replay"));
        }
        Ok(SelectedMap {
            select: self,
            count: 0,
            pending: false,
            found: false,
        })
    }
}
impl<'a, 'e, 'k, A: CanonicalCborAllocator, C: FnMut() -> bool> ser::Serializer
    for Select<'a, 'e, 'k, A, C>
{
    type Ok = ();
    type Error = Error;
    type SerializeSeq = ser::Impossible<(), Error>;
    type SerializeTuple = Self::SerializeSeq;
    type SerializeTupleStruct = Self::SerializeSeq;
    type SerializeTupleVariant = Self::SerializeSeq;
    type SerializeMap = SelectedMap<'a, 'e, 'k, A, C>;
    type SerializeStruct = Self::SerializeMap;
    type SerializeStructVariant = Self::SerializeMap;
    fn serialize_bool(self, _: bool) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_i8(self, _: i8) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_i16(self, _: i16) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_i32(self, _: i32) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_i64(self, _: i64) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_i128(self, _: i128) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_u8(self, _: u8) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_u16(self, _: u16) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_u32(self, _: u32) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_u64(self, _: u64) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_u128(self, _: u128) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_f32(self, _: f32) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_f64(self, _: f64) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_char(self, _: char) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_str(self, _: &str) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_none(self) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_some<V: Serialize + ?Sized>(self, _: &V) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_unit(self) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_newtype_struct<V: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &V,
    ) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_newtype_variant<V: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &V,
    ) -> Result<(), Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        Err(Error::Invalid("map replay shape"))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Error> {
        self.map(super::keys::Kind::Map)
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Self::SerializeStruct, Error> {
        self.map(super::keys::Kind::Struct)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        self.map(super::keys::Kind::Variant(variant))
    }
    fn collect_str<V: fmt::Display + ?Sized>(self, _: &V) -> Result<Self::Ok, Error> {
        Err(Error::Invalid("probe/replay expected collection"))
    }
    fn is_human_readable(&self) -> bool {
        false
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SelectedMap<'_, '_, '_, A, C> {
    fn key<K: Serialize + ?Sized>(&mut self, key: &K) -> Result<(), Error> {
        self.select.engine.step()?;
        if self.pending || self.count >= self.select.total {
            return Err(Error::Invalid("map changed during replay"));
        }
        if self.count == self.select.index {
            let actual = self.select.engine.key(key)?;
            if actual.bytes != self.select.expected {
                return Err(Error::Invalid("map key changed during replay"));
            }
        }
        self.pending = true;
        Ok(())
    }
    fn value<V: Serialize + ?Sized>(&mut self, value: &V) -> Result<(), Error> {
        self.select.engine.step()?;
        if !self.pending {
            return Err(Error::Invalid("map replay missing key"));
        }
        if self.count == self.select.index {
            self.select.engine.encode(value, self.select.out)?;
            self.found = true;
        }
        self.pending = false;
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Result<(), Error> {
        self.select.engine.step()?;
        if self.pending || !self.found || self.count != self.select.total {
            Err(Error::Invalid("map changed during replay"))
        } else {
            Ok(())
        }
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeMap for SelectedMap<'_, '_, '_, A, C> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<K: Serialize + ?Sized>(&mut self, key: &K) -> Result<(), Error> {
        self.key(key)
    }
    fn serialize_value<V: Serialize + ?Sized>(&mut self, value: &V) -> Result<(), Error> {
        self.value(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeStruct
    for SelectedMap<'_, '_, '_, A, C>
{
    type Ok = ();
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &V,
    ) -> Result<(), Error> {
        self.key(key)?;
        self.value(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
impl<A: CanonicalCborAllocator, C: FnMut() -> bool> SerializeStructVariant
    for SelectedMap<'_, '_, '_, A, C>
{
    type Ok = ();
    type Error = Error;
    fn serialize_field<V: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &V,
    ) -> Result<(), Error> {
        self.key(key)?;
        self.value(value)
    }
    fn end(self) -> Result<(), Error> {
        self.finish()
    }
}
