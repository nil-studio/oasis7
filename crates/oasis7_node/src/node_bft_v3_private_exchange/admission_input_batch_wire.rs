//! Transport-v2 wire/subject component only. A verified subject PoP grants no
//! issuer permission, current view, source ownership or material authority.
use super::super::wire::{
    CHUNK, MAX_FRAME, decode_frame, digest, encode, encode_frame, hash, verify_signature,
};
use super::*;
use serde::de::{self, SeqAccess, Visitor};

pub(super) const PROTOCOL_V2: &str = "/oasis7/bft/admission-input/2.0.0";
pub(super) const DOMAIN_V2: &str = "oasis7.bft.admission-input.exchange.v2";
pub(super) const MAX_PARTS_V2: u8 = 3;
const SUBJECT_V2: &[u8] = b"oasis7.bft.admission-input.subject-batch.v2\0";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum BatchOperationV2 {
    Manifest,
    Batch { offset_bytes: u64, part_count: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchReadV2 {
    pub reference: InputRef,
    pub requester_peer_id: String,
    pub serving_peer_id: String,
    pub nonce: String,
    pub operation: BatchOperationV2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PartRangeV2 {
    pub index: u32,
    pub offset: u64,
    pub length: u32,
}
impl BatchReadV2 {
    pub(super) fn ranges(&self) -> Result<Vec<PartRangeV2>, String> {
        let BatchOperationV2::Batch {
            offset_bytes,
            part_count,
        } = self.operation
        else {
            return Ok(Vec::new());
        };
        if part_count == 0
            || part_count > MAX_PARTS_V2
            || !offset_bytes.is_multiple_of(u64::from(CHUNK))
            || offset_bytes >= self.reference.object.bytes
        {
            return Err("admission batch offset/count bounds".into());
        }
        let first = u32::try_from(offset_bytes / u64::from(CHUNK))
            .map_err(|_| "admission batch index conversion")?;
        let layout = super::input_wire::package_operation(&self.reference)?;
        let mut result = Vec::with_capacity(usize::from(part_count));
        for delta in 0..u32::from(part_count) {
            let index = first
                .checked_add(delta)
                .ok_or("admission batch index overflow")?;
            let (offset, length) = layout.chunk_range(index)?;
            result.push(PartRangeV2 {
                index,
                offset,
                length,
            });
        }
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.reference
            .validate()
            .map_err(|_| "admission batch reference rejected")?;
        hash(&self.nonce)?;
        for peer in [&self.requester_peer_id, &self.serving_peer_id] {
            if peer.len() > 128 {
                return Err("admission batch peer bound".into());
            }
            peer.parse::<libp2p::PeerId>()
                .map_err(|_| "admission batch Noise peer identity")?;
        }
        if self.requester_peer_id == self.serving_peer_id {
            return Err("admission batch endpoints equal".into());
        }
        self.ranges()?;
        Ok(())
    }
    fn bind_grant(&self, grant: &BftPrivateRetentionAdmissionReadGrantV1) -> Result<(), String> {
        self.validate()?;
        grant
            .body
            .validate()
            .map_err(|_| "admission batch grant shape")?;
        hash(&grant.issuer_public_key)?;
        signature_shape(&grant.signature_hex)?;
        if grant.body.reference != self.reference
            || grant.body.requester_peer_id != self.requester_peer_id
            || grant.body.serving_peer_id != self.serving_peer_id
            || grant.body.request_nonce != self.nonce
        {
            return Err("admission batch original grant target differs".into());
        }
        Ok(())
    }
    pub fn signing_bytes(
        &self,
        grant: &BftPrivateRetentionAdmissionReadGrantV1,
    ) -> Result<Vec<u8>, String> {
        self.bind_grant(grant)?;
        let encoded = encode(&(self, grant))?;
        if encoded.len() > MAX_FRAME - SUBJECT_V2.len() {
            return Err("admission batch subject capacity".into());
        }
        let mut result = SUBJECT_V2.to_vec();
        result.extend(encoded);
        Ok(result)
    }
}
fn signature_shape(value: &str) -> Result<(), String> {
    if value.len() != 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("admission batch signature encoding".into());
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchAuthorizationV2 {
    pub grant: BftPrivateRetentionAdmissionReadGrantV1,
    pub subject_public_key: String,
    pub signature_hex: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchRequestV2 {
    pub domain: String,
    pub read: BatchReadV2,
    pub authorization: BatchAuthorizationV2,
}
/// Not deserializable. This proves only the exact v2 subject signature. A real
/// consumer must separately check its pinned issuer and fresh authorization.
#[derive(Clone)]
pub struct VerifiedBatchSubjectV2 {
    read: BatchReadV2,
    grant: BftPrivateRetentionAdmissionReadGrantV1,
    subject: String,
}
impl VerifiedBatchSubjectV2 {
    pub fn read(&self) -> &BatchReadV2 {
        &self.read
    }
    pub fn grant(&self) -> &BftPrivateRetentionAdmissionReadGrantV1 {
        &self.grant
    }
    pub fn subject_public_key(&self) -> &str {
        &self.subject
    }
    pub fn authorizes_part(&self, index: u32) -> bool {
        self.read
            .ranges()
            .is_ok_and(|ranges| ranges.iter().any(|range| range.index == index))
    }
}
impl BatchRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        if self.domain != DOMAIN_V2 {
            return Err("admission batch domain".into());
        }
        self.read.bind_grant(&self.authorization.grant)?;
        hash(&self.authorization.subject_public_key)?;
        signature_shape(&self.authorization.signature_hex)?;
        if self.authorization.subject_public_key != self.authorization.grant.body.subject_public_key
        {
            return Err("admission batch grant subject differs".into());
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        encode_frame(self)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let request: Self = decode_frame(bytes)?;
        request.validate()?;
        Ok(request)
    }
    pub fn base(&self) -> Result<String, String> {
        Ok(digest(
            b"oasis7.bft.admission-input.original.v2",
            &encode(&(
                &self.read.reference,
                &self.read.requester_peer_id,
                &self.read.serving_peer_id,
                &self.read.nonce,
            ))?,
        ))
    }
    pub fn digest(&self) -> Result<String, String> {
        Ok(digest(
            b"oasis7.bft.admission-input.request.v2",
            &self.encode()?,
        ))
    }
    pub fn verify_subject(&self) -> Result<VerifiedBatchSubjectV2, String> {
        self.validate()?;
        verify_signature(
            &self.authorization.subject_public_key,
            &self.read.signing_bytes(&self.authorization.grant)?,
            &self.authorization.signature_hex,
        )?;
        Ok(VerifiedBatchSubjectV2 {
            read: self.read.clone(),
            grant: self.authorization.grant.clone(),
            subject: self.authorization.subject_public_key.clone(),
        })
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct PartBytesV2(Vec<u8>);
impl std::fmt::Debug for PartBytesV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfidentialBatchPart")
            .field("bytes", &self.0.len())
            .finish()
    }
}
impl PartBytesV2 {
    pub fn new(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > CHUNK as usize {
            return Err("admission batch part byte bound".into());
        }
        Ok(Self(bytes))
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}
impl Serialize for PartBytesV2 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}
impl<'de> Deserialize<'de> for PartBytesV2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;
        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = PartBytesV2;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a bounded nonempty binary part")
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                if bytes.is_empty() || bytes.len() > CHUNK as usize {
                    return Err(E::custom("admission batch part byte bound"));
                }
                let mut owned = Vec::new();
                owned
                    .try_reserve_exact(bytes.len())
                    .map_err(|_| E::custom("admission batch part allocation"))?;
                owned.extend_from_slice(bytes);
                Ok(PartBytesV2(owned))
            }
            fn visit_borrowed_bytes<E: de::Error>(
                self,
                bytes: &'de [u8],
            ) -> Result<Self::Value, E> {
                self.visit_bytes(bytes)
            }
        }
        deserializer.deserialize_bytes(BytesVisitor)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchPartV2 {
    pub index: u32,
    pub bytes: PartBytesV2,
}
fn parts<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<BatchPartV2>, D::Error> {
    struct PartsVisitor;
    impl<'de> Visitor<'de> for PartsVisitor {
        type Value = Vec<BatchPartV2>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most three binary parts")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            if sequence
                .size_hint()
                .is_some_and(|count| count > usize::from(MAX_PARTS_V2))
            {
                return Err(de::Error::custom("admission batch part count"));
            }
            let mut result = Vec::new();
            result
                .try_reserve_exact(usize::from(MAX_PARTS_V2))
                .map_err(|_| <A::Error as de::Error>::custom("admission batch part allocation"))?;
            while result.len() < usize::from(MAX_PARTS_V2) {
                let Some(part) = sequence.next_element::<BatchPartV2>()? else {
                    return Ok(result);
                };
                result.push(part);
            }
            if sequence.next_element::<de::IgnoredAny>()?.is_some() {
                return Err(de::Error::custom("admission batch part count"));
            }
            Ok(result)
        }
    }
    deserializer.deserialize_seq(PartsVisitor)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BatchResponseV2 {
    pub domain: String,
    pub request_digest: String,
    pub reference: InputRef,
    pub offset_bytes: u64,
    #[serde(deserialize_with = "parts")]
    pub parts: Vec<BatchPartV2>,
}
impl BatchResponseV2 {
    pub fn validate_for(&self, request: &BatchRequestV2) -> Result<(), String> {
        if self.domain != DOMAIN_V2
            || self.request_digest != request.digest()?
            || self.reference != request.read.reference
            || self.parts.len() > usize::from(MAX_PARTS_V2)
        {
            return Err("admission batch response binding differs".into());
        }
        let ranges = request.read.ranges()?;
        let offset = match request.read.operation {
            BatchOperationV2::Manifest => 0,
            BatchOperationV2::Batch { offset_bytes, .. } => offset_bytes,
        };
        if self.offset_bytes != offset || self.parts.len() != ranges.len() {
            return Err("admission batch response offset/count differs".into());
        }
        for (part, range) in self.parts.iter().zip(ranges) {
            if part.index != range.index || part.bytes.as_slice().len() != range.length as usize {
                return Err("admission batch response contiguous part/length differs".into());
            }
        }
        Ok(())
    }
    pub fn encode_for(&self, request: &BatchRequestV2) -> Result<Vec<u8>, String> {
        self.validate_for(request)?;
        encode_frame(self)
    }
    pub fn decode_for(bytes: &[u8], request: &BatchRequestV2) -> Result<Self, String> {
        let response: Self = decode_frame(bytes)?;
        response.validate_for(request)?;
        Ok(response)
    }
}
#[cfg(test)]
#[path = "admission_input_batch_codec_tests.rs"]
mod tests;
