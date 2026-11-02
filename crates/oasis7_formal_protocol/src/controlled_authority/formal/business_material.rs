//! Shared bounded business-result wire codec; this grants no qualification.
use super::business_index::BusinessIndexDeltaV1;
use super::{crypto::invalid, *};
use serde::{Deserialize, Serialize};
pub const FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1: &str = "oasis7.formal.business-result-envelope.v1";
pub const FORMAL_BUSINESS_ENVELOPE_LIMIT: usize = 64 * 1024 * 1024;
const RESULT_BYTES_LIMIT: usize = 8 * 1024 * 1024;
const DESCRIPTOR_BYTES_LIMIT: usize = 64 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBusinessResultEnvelopeV1 {
    pub schema_version: u32,
    pub domain: String,
    #[serde(serialize_with = "serialize_bytes", deserialize_with = "decode_result")]
    pub execution_result_cbor: Vec<u8>,
    #[serde(
        serialize_with = "serialize_bytes",
        deserialize_with = "decode_descriptor"
    )]
    pub tree_descriptor_cbor: Vec<u8>,
    pub delta: BusinessIndexDeltaV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_history_delta: Option<BusinessIndexDeltaV1>,
}
fn serialize_bytes<S: serde::Serializer>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(value)
}
fn decode_result<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    decode_bounded_bytes::<D, RESULT_BYTES_LIMIT>(decoder)
}
fn decode_descriptor<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Vec<u8>, D::Error> {
    decode_bounded_bytes::<D, DESCRIPTOR_BYTES_LIMIT>(decoder)
}
fn decode_bounded_bytes<'de, D: serde::Deserializer<'de>, const LIMIT: usize>(
    decoder: D,
) -> Result<Vec<u8>, D::Error> {
    struct Visitor<const LIMIT: usize>;
    impl<'de, const LIMIT: usize> serde::de::Visitor<'de> for Visitor<LIMIT> {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded opaque canonical CBOR bytes")
        }
        fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Vec<u8>, E> {
            if bytes.len() > LIMIT {
                return Err(E::custom("opaque CBOR byte bound"));
            }
            Ok(bytes.to_vec())
        }
        fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Vec<u8>, E> {
            if bytes.len() > LIMIT {
                return Err(E::custom("opaque CBOR byte bound"));
            }
            Ok(bytes)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Vec<u8>, A::Error> {
            if sequence.size_hint().is_some_and(|len| len > LIMIT) {
                return Err(serde::de::Error::custom("opaque CBOR byte bound"));
            }
            let mut bytes = Vec::new();
            while let Some(byte) = sequence.next_element()? {
                if bytes.len() == LIMIT {
                    return Err(serde::de::Error::custom("opaque CBOR byte bound"));
                }
                bytes.push(byte);
            }
            Ok(bytes)
        }
    }
    decoder.deserialize_byte_buf(Visitor::<LIMIT>)
}
impl FormalBusinessResultEnvelopeV1 {
    pub fn encode(&self) -> Result<Vec<u8>, FormalError> {
        self.validate_shape()?;
        let bytes = serde_cbor::to_vec(self).map_err(|error| invalid(error.to_string()))?;
        if bytes.len() > FORMAL_BUSINESS_ENVELOPE_LIMIT {
            return Err(invalid("formal business envelope byte bound"));
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, FormalError> {
        if bytes.len() > FORMAL_BUSINESS_ENVELOPE_LIMIT {
            return Err(invalid("formal business envelope byte bound"));
        }
        let envelope: Self =
            serde_cbor::from_slice(bytes).map_err(|error| invalid(error.to_string()))?;
        envelope.validate_shape()?;
        Ok(envelope)
    }
    fn validate_shape(&self) -> Result<(), FormalError> {
        if self.schema_version != 1
            || self.domain != FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1
            || self.execution_result_cbor.is_empty()
            || self.execution_result_cbor.len() > RESULT_BYTES_LIMIT
            || self.tree_descriptor_cbor.is_empty()
            || self.tree_descriptor_cbor.len() > DESCRIPTOR_BYTES_LIMIT
        {
            return Err(invalid(
                "formal business envelope schema/domain/opaque bytes",
            ));
        }
        Ok(())
    }
}
