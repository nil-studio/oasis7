//! The original seven-field authorization hash projection, without cloning
//! the revocation graph merely to exclude correlation results. DATA only.
//! Current publishers must independently bind and validate every source.
use serde::{
    Serialize, Serializer,
    ser::{Error as _, Impossible, SerializeStruct},
};

#[derive(Serialize)]
#[serde(rename = "CapabilityAuthorizationRootBody")]
pub(in crate::world) struct AuthorizationRootProjectionV1<
    'a,
    G: ?Sized,
    R: ?Sized,
    I: ?Sized,
    B: ?Sized,
    N: ?Sized,
    A: ?Sized,
    L: ?Sized,
> {
    pub(in crate::world) grants: &'a G,
    pub(in crate::world) revocation: &'a R,
    pub(in crate::world) invocation_contexts: &'a I,
    pub(in crate::world) budget_accounts: &'a B,
    pub(in crate::world) nonce_records: &'a N,
    pub(in crate::world) receipts: &'a A,
    pub(in crate::world) effect_receipt_links: &'a L,
}

/// Exactly the original `clone(); world_service_results.clear()` Serialize
/// projection. The original derived implementation still selects all fields,
/// names, defaults, option shapes and skip predicates. Empty correlation maps
/// are OMITTED by that implementation, never replaced by a new empty-map wire.
pub(in crate::world) struct AuthorityRevocationViewV1<'a>(
    pub(in crate::world) &'a oasis7_world_state::CapabilityRevocationState,
);
impl Serialize for AuthorityRevocationViewV1<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(AuthoritySerializer {
            inner: serializer,
            omit: !self.0.world_service_results.is_empty(),
        })
    }
}
struct AuthoritySerializer<S> {
    inner: S,
    omit: bool,
}
struct AuthorityStruct<S> {
    inner: S,
    omit: bool,
    omitted: bool,
}
impl<S: SerializeStruct> SerializeStruct for AuthorityStruct<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), S::Error> {
        if key == "world_service_results" {
            if !self.omit || self.omitted {
                return Err(S::Error::custom("authorization correlation projection"));
            }
            self.omitted = true;
            Ok(())
        } else {
            self.inner.serialize_field(key, value)
        }
    }
    fn skip_field(&mut self, key: &'static str) -> Result<(), S::Error> {
        self.inner.skip_field(key)
    }
    fn end(self) -> Result<S::Ok, S::Error> {
        if self.omit != self.omitted {
            return Err(S::Error::custom("authorization correlation projection"));
        }
        self.inner.end()
    }
}
macro_rules! reject_scalar {
    ($($method:ident: $ty:ty),+ $(,)?) => {$(
        fn $method(self, _: $ty) -> Result<Self::Ok, Self::Error> { Err(Self::Error::custom("authorization revocation struct")) }
    )+};
}
impl<S: Serializer> Serializer for AuthoritySerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Impossible<S::Ok, S::Error>;
    type SerializeTuple = Impossible<S::Ok, S::Error>;
    type SerializeTupleStruct = Impossible<S::Ok, S::Error>;
    type SerializeTupleVariant = Impossible<S::Ok, S::Error>;
    type SerializeMap = Impossible<S::Ok, S::Error>;
    type SerializeStruct = AuthorityStruct<S::SerializeStruct>;
    type SerializeStructVariant = Impossible<S::Ok, S::Error>;
    reject_scalar! { serialize_bool: bool, serialize_i8: i8, serialize_i16: i16, serialize_i32: i32, serialize_i64: i64,
    serialize_u8: u8, serialize_u16: u16, serialize_u32: u32, serialize_u64: u64,
    serialize_f32: f32, serialize_f64: f64, serialize_char: char, serialize_str: &str, serialize_bytes: &[u8] }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        if name != "CapabilityRevocationState" {
            return Err(Self::Error::custom("authorization revocation struct"));
        }
        let len = len
            .checked_sub(usize::from(self.omit))
            .ok_or_else(|| Self::Error::custom("authorization revocation fields"))?;
        Ok(AuthorityStruct {
            inner: self.inner.serialize_struct(name, len)?,
            omit: self.omit,
            omitted: false,
        })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn collect_str<T: std::fmt::Display + ?Sized>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        Err(Self::Error::custom("authorization revocation struct"))
    }
    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

#[cfg(test)]
#[path = "incremental_authorization_root_projection_tests.rs"]
mod tests;
