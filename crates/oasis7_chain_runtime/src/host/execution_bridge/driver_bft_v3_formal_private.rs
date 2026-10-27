//! Explicit private import material. Neither type implements Serialize or Debug;
//! callers must deliberately request private bytes or a public-only projection.
use super::*;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, SeqAccess, Visitor},
};
use std::fmt;

pub(crate) const PRIVATE_HEADER_PREFIX_MAX_BYTES_V3: usize = 32 * 1024 * 1024;
const MAX_HEIGHTS: usize = 64;
const MAX_ACTION_BYTES: usize = 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrivateHeaderEntryV3 {
    pub(crate) certificate: FormalBftCertifiedHeaderV3,
    #[serde(deserialize_with = "deserialize_actions")]
    pub(crate) actions_cbor: Vec<u8>,
}
impl PrivateHeaderEntryV3 {
    pub(crate) fn validate_binding(&self) -> Result<(), String> {
        let bound = self.certificate.validate_binding()?;
        ValidatedBftPrivatePayloadV3::decode(bound, &self.actions_cbor)?;
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PrivateHeaderPrefixV3 {
    entries: Vec<PrivateHeaderEntryV3>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema_version: u32,
    #[serde(deserialize_with = "deserialize_entries")]
    entries: Vec<PrivateHeaderEntryV3>,
}
#[derive(Serialize)]
struct EntryView<'a> {
    certificate: &'a FormalBftCertifiedHeaderV3,
    actions_cbor: &'a [u8],
}
#[derive(Serialize)]
struct PrefixView<'a> {
    schema_version: u32,
    entries: Vec<EntryView<'a>>,
}

impl PrivateHeaderPrefixV3 {
    pub(crate) fn from_entries(entries: Vec<PrivateHeaderEntryV3>) -> Result<Self, String> {
        if entries.len() > MAX_HEIGHTS {
            return Err("private BFT header prefix height capacity".into());
        }
        for entry in &entries {
            if entry.actions_cbor.len() > MAX_ACTION_BYTES {
                return Err("private BFT header action byte capacity".into());
            }
            entry.validate_binding()?;
        }
        let value = Self { entries };
        value.canonical_private_bytes()?;
        Ok(value)
    }

    pub(crate) fn entries(&self) -> &[PrivateHeaderEntryV3] {
        &self.entries
    }

    pub(crate) fn decode_private_canonical(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PRIVATE_HEADER_PREFIX_MAX_BYTES_V3 {
            return Err("private BFT header prefix byte capacity".into());
        }
        let input: Input = serde_cbor::from_slice(bytes)
            .map_err(|_| "invalid bounded private BFT header prefix".to_string())?;
        if input.schema_version != 1 {
            return Err("unsupported private BFT header prefix schema".into());
        }
        let value = Self::from_entries(input.entries)?;
        if value.canonical_private_bytes()? != bytes {
            return Err("noncanonical private BFT header prefix".into());
        }
        Ok(value)
    }

    /// The only serialization entry point is deliberately labeled private.
    /// This preserves the existing offline-preview history schema byte-for-byte.
    pub(crate) fn canonical_private_bytes(&self) -> Result<Vec<u8>, String> {
        if self.entries.len() > MAX_HEIGHTS {
            return Err("private BFT header prefix height capacity".into());
        }
        let view = PrefixView {
            schema_version: 1,
            entries: self
                .entries
                .iter()
                .map(|entry| EntryView {
                    certificate: &entry.certificate,
                    actions_cbor: &entry.actions_cbor,
                })
                .collect(),
        };
        let lower_bound = serde_cbor::to_vec(&view)
            .map_err(|_| "measure private BFT header prefix".to_string())?;
        if lower_bound.len() > PRIVATE_HEADER_PREFIX_MAX_BYTES_V3 {
            return Err("private BFT header prefix byte capacity".into());
        }
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&view)
            .map_err(|_| "encode private BFT header prefix".to_string())?;
        if bytes.len() > PRIVATE_HEADER_PREFIX_MAX_BYTES_V3 {
            return Err("private BFT header prefix byte capacity".into());
        }
        Ok(bytes)
    }

    /// This projection has no route by which raw private actions can escape.
    pub(crate) fn public_prefix(
        &self,
    ) -> Result<oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3, String> {
        let value = oasis7_formal_protocol::bft::FormalBftCertifiedHeaderPrefixV3 {
            schema_version: 1,
            entries: self
                .entries
                .iter()
                .map(|entry| entry.certificate.clone())
                .collect(),
        };
        value.canonical_bytes()?;
        Ok(value)
    }
}

fn bounded_sequence<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
    maximum: usize,
) -> Result<Vec<T>, D::Error> {
    struct Bounded<T> {
        maximum: usize,
        marker: std::marker::PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a bounded private BFT sequence")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > self.maximum) {
                return Err(de::Error::custom(
                    "private BFT sequence count exceeds bound",
                ));
            }
            let mut values = Vec::new();
            while values.len() < self.maximum {
                match seq.next_element()? {
                    Some(value) => values.push(value),
                    None => return Ok(values),
                }
            }
            if seq.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom(
                    "private BFT sequence count exceeds bound",
                ));
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Bounded {
        maximum,
        marker: std::marker::PhantomData,
    })
}
fn deserialize_entries<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Vec<PrivateHeaderEntryV3>, D::Error> {
    bounded_sequence(d, MAX_HEIGHTS)
}

pub(super) fn deserialize_actions<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    bounded_sequence(d, MAX_ACTION_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_private_history_and_action_counts_are_rejected_before_content() {
        #[derive(Deserialize)]
        struct Entries(
            #[serde(deserialize_with = "deserialize_entries")] Vec<PrivateHeaderEntryV3>,
        );
        assert!(
            serde_cbor::from_slice::<Entries>(&[0x98, 65])
                .err()
                .unwrap()
                .to_string()
                .contains("count exceeds bound")
        );
        assert!(
            serde_cbor::from_slice::<Entries>(&[0x80])
                .unwrap()
                .0
                .is_empty()
        );
        #[derive(Deserialize)]
        struct Actions(#[serde(deserialize_with = "deserialize_actions")] Vec<u8>);
        let _empty = serde_cbor::from_slice::<Actions>(&[0x80]).unwrap().0;
        assert!(
            serde_cbor::from_slice::<Actions>(&[0x9a, 0x00, 0x10, 0x00, 0x01])
                .err()
                .unwrap()
                .to_string()
                .contains("count exceeds bound")
        );
    }
}
