//! Bounded *binary* OriginalAdmissionInput V2 response frames (Dark codec).
//! These are neither grants nor material-release decisions. The caller must
//! separately check fresh issuer/view, revocation, Noise peers and all budgets.
use super::admission_input_request_v2::{AdmissionBatchOperationV2, AdmissionBatchRequestV2};
use super::wire::{CHUNK, decode_frame, encode_frame};
use oasis7_distfs::protected_bft_v3::{
    BftAdmissionInputV2StreamStage, BftPrivateRetentionAdmissionInputRefV1,
};
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::io::Write;

pub(super) const RESPONSE_DOMAIN: &str = "oasis7.bft.admission-input.response.v2";
const MAX_PARTS: usize = 3;

/// Raw confidential bytes never enter Debug, hex, base64, or JSON arrays.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct OriginalAdmissionPartBytesV2(Vec<u8>);

impl std::fmt::Debug for OriginalAdmissionPartBytesV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrivateAdmissionPart")
            .field("len", &self.0.len())
            .finish()
    }
}
impl OriginalAdmissionPartBytesV2 {
    pub(super) fn new(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > CHUNK as usize {
            return Err("original admission response part byte bound".into());
        }
        Ok(Self(bytes))
    }
    pub(super) fn as_slice(&self) -> &[u8] {
        &self.0
    }
}
impl Serialize for OriginalAdmissionPartBytesV2 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}
impl<'de> Deserialize<'de> for OriginalAdmissionPartBytesV2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BoundedPart;
        impl<'de> Visitor<'de> for BoundedPart {
            type Value = OriginalAdmissionPartBytesV2;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a bounded nonempty original binary part")
            }
            fn visit_bytes<E: de::Error>(self, raw: &[u8]) -> Result<Self::Value, E> {
                if raw.is_empty() || raw.len() > CHUNK as usize {
                    return Err(E::custom("original admission response part byte bound"));
                }
                Ok(OriginalAdmissionPartBytesV2(raw.to_vec()))
            }
            fn visit_borrowed_bytes<E: de::Error>(self, raw: &'de [u8]) -> Result<Self::Value, E> {
                self.visit_bytes(raw)
            }
        }
        deserializer.deserialize_bytes(BoundedPart)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OriginalAdmissionPartV2 {
    pub index: u32,
    pub bytes: OriginalAdmissionPartBytesV2,
}

/// Allocate at most three typed parts even for a malicious indefinite CBOR
/// sequence. The shared canonical scanner also bounds each individual item.
fn bounded_parts<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<OriginalAdmissionPartV2>, D::Error> {
    struct ThreeParts;
    impl<'de> Visitor<'de> for ThreeParts {
        type Value = Vec<OriginalAdmissionPartV2>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("no more than three original binary parts")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Self::Value, A::Error> {
            if items.size_hint().is_some_and(|count| count > MAX_PARTS) {
                return Err(de::Error::custom("original admission response part count"));
            }
            let mut parts = Vec::with_capacity(MAX_PARTS);
            for _ in 0..MAX_PARTS {
                match items.next_element::<OriginalAdmissionPartV2>()? {
                    Some(part) => parts.push(part),
                    None => return Ok(parts),
                }
            }
            if items.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("original admission response part count"));
            }
            Ok(parts)
        }
    }
    deserializer.deserialize_seq(ThreeParts)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OriginalAdmissionBatchResponseV2 {
    pub domain: String,
    pub request_digest: String,
    pub reference: BftPrivateRetentionAdmissionInputRefV1,
    pub offset_bytes: u64,
    #[serde(deserialize_with = "bounded_parts")]
    pub parts: Vec<OriginalAdmissionPartV2>,
}

impl OriginalAdmissionBatchResponseV2 {
    /// This checks the *exact signed request shape* and binary part geometry,
    /// NOT fresh issuance, material ownership, transport or budget authority.
    pub(super) fn validate_for(&self, request: &AdmissionBatchRequestV2) -> Result<(), String> {
        request.validate()?;
        let ranges = request.read.ranges()?;
        let offset = match request.read.operation {
            AdmissionBatchOperationV2::Manifest => 0,
            AdmissionBatchOperationV2::Batch { offset_bytes, .. } => offset_bytes,
        };
        if self.domain != RESPONSE_DOMAIN
            || self.request_digest != request.request_digest()?
            || self.reference != request.read.reference
            || self.offset_bytes != offset
            || self.parts.len() != ranges.len()
            || self.parts.len() > MAX_PARTS
        {
            return Err("original admission response not bound to exact request".into());
        }
        for (part, range) in self.parts.iter().zip(ranges.iter()) {
            if part.index != range.index || part.bytes.as_slice().len() != range.length as usize {
                return Err(
                    "original admission response has noncontiguous or wrong-size part".into(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn encode_for(&self, request: &AdmissionBatchRequestV2) -> Result<Vec<u8>, String> {
        self.validate_for(request)?;
        encode_frame(self)
    }

    #[allow(dead_code)] // V2 client decode has no registered runtime consumer yet.
    pub(super) fn decode_for(
        bytes: &[u8],
        request: &AdmissionBatchRequestV2,
    ) -> Result<Self, String> {
        let response: Self = decode_frame(bytes)?;
        response.validate_for(request)?;
        Ok(response)
    }

    /// Consume the complete encoded application frame through the *existing*
    /// original staging primitive. The frame count is measured, never guessed
    /// from raw parts. The caller still owns admission, deadline, transfer CAS,
    /// original object readback and irreversible attempt-error handling.
    #[allow(dead_code)] // Retain pre-existing Dark staging until the client Cutover.
    pub(super) fn decode_and_stage<W: Write>(
        bytes: &[u8],
        request: &AdmissionBatchRequestV2,
        staging: &mut BftAdmissionInputV2StreamStage<W>,
    ) -> Result<Self, String> {
        let response = Self::decode_for(bytes, request)?;
        let ranges = request.read.ranges()?;
        let first = ranges
            .first()
            .ok_or("original admission manifest does not stage private bytes")?;
        let message_index = first.index / MAX_PARTS as u32;
        let parts: Vec<_> = response
            .parts
            .iter()
            .map(|part| part.bytes.as_slice())
            .collect();
        staging
            .append_decoded_message(message_index, bytes.len(), &parts)
            .map_err(|error| error.to_string())?;
        Ok(response)
    }
}

#[cfg(test)]
#[path = "admission_input_response_v2_tests.rs"]
mod tests;
