//! Compact deterministic index entries: a 32-byte digest and two unsigned
//! coordinates. Never expand digest bytes into CBOR integer arrays or hex maps.
use super::*;
use serde::{
    de::{self, SeqAccess, Visitor},
    ser::SerializeTuple,
};
use std::fmt;
struct Digest(String);
impl Serialize for Digest {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let bytes = hex::decode(&self.0).map_err(serde::ser::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::ser::Error::custom("physical index digest length"));
        }
        s.serialize_bytes(&bytes)
    }
}
impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Bytes;
        impl Visitor<'_> for Bytes {
            type Value = Digest;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("32 digest bytes")
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Digest, E> {
                if bytes.len() != 32 {
                    return Err(E::custom("physical index digest length"));
                }
                Ok(Digest(hex::encode(bytes)))
            }
        }
        d.deserialize_bytes(Bytes)
    }
}
impl Serialize for Span {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut tuple = s.serialize_tuple(3)?;
        tuple.serialize_element(&Digest(self.id.clone()))?;
        tuple.serialize_element(&self.start)?;
        tuple.serialize_element(&self.length)?;
        tuple.end()
    }
}
impl<'de> Deserialize<'de> for Span {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Index;
        impl<'de> Visitor<'de> for Index {
            type Value = Span;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("digest/start/length tuple")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Span, A::Error> {
                if seq.size_hint().is_some_and(|n| n != 3) {
                    return Err(de::Error::custom("physical index tuple length"));
                }
                let Digest(id) = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::custom("physical index digest missing"))?;
                let start = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::custom("physical index start missing"))?;
                let length = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::custom("physical index length missing"))?;
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("physical index tuple extras"));
                }
                Ok(Span { id, start, length })
            }
        }
        d.deserialize_tuple(3, Index)
    }
}
pub(super) fn serialize_body<S: serde::Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_bytes(bytes)
}
pub(super) fn deserialize_body<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<u8>, D::Error> {
    struct Bytes;
    impl Visitor<'_> for Bytes {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bounded physical recipe bytes")
        }
        fn visit_bytes<E: de::Error>(self, b: &[u8]) -> Result<Vec<u8>, E> {
            if b.len() > META_LIMIT {
                return Err(E::custom("physical recipe metadata budget"));
            }
            Ok(b.to_vec())
        }
    }
    d.deserialize_bytes(Bytes)
}
fn bounded<'de, D, T>(d: D, limit: usize) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Values<T>(usize, std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Values<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("bounded physical index")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            if seq.size_hint().is_some_and(|n| n > self.0) {
                return Err(de::Error::custom("physical index declared count budget"));
            }
            let mut result = Vec::new();
            while let Some(value) = seq.next_element()? {
                if result.len() == self.0 {
                    return Err(de::Error::custom("physical index count budget"));
                }
                result.push(value);
            }
            Ok(result)
        }
    }
    d.deserialize_seq(Values(limit, std::marker::PhantomData))
}
pub(super) fn objects<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Span>, D::Error> {
    bounded(d, MAX_OBJECTS)
}
pub(super) fn frames<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Frame>, D::Error> {
    bounded(d, LIMIT / 4096 + 1)
}
