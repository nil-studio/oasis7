use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::binding::{BftSignerError, invalid};

pub(crate) const BFT_BINDING_DOMAIN_V1: &str = "oasis7.bft.protected-signer.v1.binding";
pub(crate) const BFT_REQUEST_DOMAIN_V1: &str = "oasis7.bft.protected-signer.v1.request";
pub(crate) const BFT_REPLY_DOMAIN_V1: &str = "oasis7.bft.protected-signer.v1.reply";
pub(crate) const BFT_REQUEST_DIGEST_DOMAIN_V1: &str =
    "oasis7.bft.protected-signer.v1.request-digest";
pub(crate) const BFT_CHECKPOINT_DOMAIN_V1: &str = "oasis7.bft.protected-signer.v1.checkpoint";
pub(crate) const BFT_SIGNER_MAX_SIGNING_BYTES_V1: usize = 560 * 1024 * 1024;
pub(crate) const MAX_FRAME_BYTES: usize = super::validation::BFT_SIGNER_MAX_FRAME_BYTES_V1;

pub(crate) fn framed_cbor<T: Serialize>(
    domain: &str,
    value: &T,
    max_len: usize,
) -> Result<Vec<u8>, BftSignerError> {
    let payload = serde_cbor::to_vec(value)
        .map_err(|error| invalid(format!("encode BFT signer frame: {error}")))?;
    let total_len = 8usize
        .checked_add(domain.len())
        .and_then(|len| len.checked_add(payload.len()))
        .ok_or_else(|| invalid("BFT signer frame length overflow"))?;
    if total_len > max_len {
        return Err(invalid("BFT signer frame exceeds configured bound"));
    }
    let mut framed = Vec::with_capacity(total_len);
    framed.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    framed.extend_from_slice(domain.as_bytes());
    framed.extend_from_slice(&payload);
    Ok(framed)
}

pub(crate) fn digest_hex<T: Serialize>(domain: &str, value: &T) -> Result<String, BftSignerError> {
    let bytes = framed_cbor(domain, value, BFT_SIGNER_MAX_SIGNING_BYTES_V1)?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerLeaseCredentialV1 {
    pub generation: u64,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub(crate) enum BftSignerCommandV1 {
    PrepareAcquire {
        binding: Box<super::binding::BftSignerBindingV1>,
    },
    Acquire {
        binding: Box<super::binding::BftSignerBindingV1>,
        expected_generation: u64,
        prepare_nonce: String,
    },
    Checkpoint,
    ProtectedTime,
    StartHeightTimer {
        expected_revision: u64,
    },
    Advance {
        expected_revision: u64,
        expected_state_digest: String,
        candidate: Box<super::validation::BftSignerCandidateV1>,
    },
    SignProposal {
        expected_revision: u64,
        intent: Box<oasis7_consensus::tendermint_v1::BftProposalIntentV1>,
    },
    SignVote {
        expected_revision: u64,
        intent: Box<oasis7_consensus::tendermint_v1::BftVoteIntentV1>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerRequestBodyV1 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub challenge: String,
    pub lease: Option<BftSignerLeaseCredentialV1>,
    pub command: BftSignerCommandV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedBftSignerRequestV1 {
    pub body: BftSignerRequestBodyV1,
    pub authentication_public_key: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub(crate) enum BftSignerReplyKindV1 {
    Prepared {
        expected_generation: u64,
        prepare_nonce: String,
    },
    AcquireConflict {
        current_generation: u64,
        lease_active: bool,
    },
    Acquired,
    Checkpoint,
    ProtectedTime {
        time_ms: i64,
    },
    HeightTimerStarted,
    Advanced,
    ProposalSigned {
        proposal: Box<oasis7_proto::distributed_bft::BftProposalV1>,
    },
    VoteSigned {
        vote: Box<oasis7_proto::distributed_bft::BftVoteV1>,
    },
    Rejected {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BftSignerReplyBodyV1 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub request_digest: String,
    pub challenge: String,
    pub lease_generation: u64,
    pub lease_token: Option<String>,
    pub state_revision: u64,
    pub state_digest: String,
    pub state: Option<Box<super::validation::BftSignerStateV1>>,
    pub result: BftSignerReplyKindV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SignedBftSignerReplyV1 {
    pub body: BftSignerReplyBodyV1,
    pub provider_signature_hex: String,
    pub checkpoint_signature_hex: Option<String>,
}

pub(crate) fn encode_bounded<T: Serialize>(value: &T) -> Result<Vec<u8>, BftSignerError> {
    let bytes = serde_cbor::to_vec(value)
        .map_err(|error| invalid(format!("encode BFT signer CBOR: {error}")))?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(invalid("BFT signer frame exceeds configured bound"));
    }
    Ok(bytes)
}

pub(crate) fn decode_bounded<T: DeserializeOwned + Serialize>(
    bytes: &[u8],
) -> Result<T, BftSignerError> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(invalid("BFT signer frame byte bound"));
    }
    scan_cbor_bounds(bytes)?;
    let decoded: T = serde_cbor::from_slice(bytes)
        .map_err(|error| invalid(format!("decode BFT signer CBOR: {error}")))?;
    if serde_cbor::to_vec(&decoded)
        .map_err(|error| invalid(format!("canonicalize BFT signer CBOR: {error}")))?
        != bytes
    {
        return Err(invalid("BFT signer frame is not canonical CBOR"));
    }
    Ok(decoded)
}

/// Rejects indefinite CBOR and oversized container declarations before serde
/// can reserve attacker-selected vector capacities from a short frame.
fn scan_cbor_bounds(bytes: &[u8]) -> Result<(), BftSignerError> {
    let mut offset = 0usize;
    scan_item(bytes, &mut offset, 0)?;
    if offset != bytes.len() {
        return Err(invalid("trailing bytes in BFT signer CBOR frame"));
    }
    Ok(())
}

fn scan_item(bytes: &[u8], offset: &mut usize, depth: usize) -> Result<(), BftSignerError> {
    if depth > 64 {
        return Err(invalid("BFT signer CBOR nesting limit"));
    }
    let initial = *bytes
        .get(*offset)
        .ok_or_else(|| invalid("truncated BFT signer CBOR"))?;
    *offset += 1;
    let major = initial >> 5;
    let additional = initial & 0x1f;
    if additional == 31 {
        return Err(invalid("indefinite BFT signer CBOR is not accepted"));
    }
    let argument = read_argument(bytes, offset, additional)?;
    match major {
        0 | 1 => Ok(()),
        2 | 3 => {
            let size = usize::try_from(argument)
                .map_err(|_| invalid("BFT signer CBOR string length overflow"))?;
            let end = offset
                .checked_add(size)
                .ok_or_else(|| invalid("BFT signer CBOR string bound overflow"))?;
            if end > bytes.len() {
                return Err(invalid("truncated BFT signer CBOR string"));
            }
            *offset = end;
            Ok(())
        }
        4 | 5 => {
            let count = usize::try_from(argument)
                .map_err(|_| invalid("BFT signer CBOR container length overflow"))?;
            let children = if major == 5 {
                count
                    .checked_mul(2)
                    .ok_or_else(|| invalid("BFT signer CBOR map size overflow"))?
            } else {
                count
            };
            let remaining = bytes.len().saturating_sub(*offset);
            if children > remaining {
                return Err(invalid(
                    "BFT signer CBOR container exceeds encoded byte bound",
                ));
            }
            for _ in 0..children {
                scan_item(bytes, offset, depth + 1)?;
            }
            Ok(())
        }
        6 => {
            if argument == 0 || argument > 64 {
                return Err(invalid("BFT signer CBOR tag is not supported"));
            }
            scan_item(bytes, offset, depth + 1)
        }
        7 if additional <= 23 => Ok(()),
        _ => Err(invalid("unsupported BFT signer CBOR primitive")),
    }
}

fn read_argument(bytes: &[u8], offset: &mut usize, additional: u8) -> Result<u64, BftSignerError> {
    let width = match additional {
        0..=23 => return Ok(u64::from(additional)),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => return Err(invalid("invalid BFT signer CBOR argument")),
    };
    let end = offset
        .checked_add(width)
        .ok_or_else(|| invalid("BFT signer CBOR argument overflow"))?;
    let slice = bytes
        .get(*offset..end)
        .ok_or_else(|| invalid("truncated BFT signer CBOR argument"))?;
    *offset = end;
    let mut value = 0u64;
    for byte in slice {
        value = (value << 8) | u64::from(*byte);
    }
    let minimum = match width {
        1 => 24,
        2 => 256,
        4 => 65_536,
        8 => 4_294_967_296,
        _ => return Err(invalid("invalid BFT signer CBOR argument width")),
    };
    if value < minimum {
        return Err(invalid("noncanonical BFT signer CBOR integer"));
    }
    Ok(value)
}
