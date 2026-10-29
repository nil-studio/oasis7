//! Public peer data only. Decoding grants no schedule or execution authority.
use super::{BftPublicProposalV3, FormalBftCertifiedHeaderV3};
use oasis7_formal_protocol::bft::VerifiedBftHandoffTargetV2;
use oasis7_proto::distributed_bft::{v2::BftScopeV2, v3::BftVoteV3};
use serde::{Deserialize, Serialize};

pub const BFT_PUBLIC_PEER_DOMAIN_V3: &str = "oasis7.bft.public-peer.v3";
pub const BFT_PUBLIC_PEER_MAX_HISTORY_BYTES_V3: usize = 16 * 1024 * 1024;
pub const BFT_PUBLIC_PEER_MAX_BYTES_V3: usize = BFT_PUBLIC_PEER_MAX_HISTORY_BYTES_V3 + 64 * 1024;
pub const BFT_PUBLIC_PEER_MAX_PROPOSAL_BYTES_V3: usize = 3 * 1024 * 1024;
pub const BFT_PUBLIC_PEER_MAX_CERTIFICATE_BYTES_V3: usize = 3 * 1024 * 1024;
pub const BFT_PUBLIC_PEER_MAX_HISTORY_V3: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPeerContextV3 {
    pub activation_digest: String,
    pub target_digest: String,
    pub scope: BftScopeV2,
}
impl BftPeerContextV3 {
    pub fn from_handoff(h: &VerifiedBftHandoffTargetV2) -> Self {
        Self {
            activation_digest: h.activation_digest().into(),
            target_digest: h.target_digest().into(),
            scope: h.target().scope.clone(),
        }
    }
    fn validate(&self) -> Result<(), String> {
        digest(&self.activation_digest)?;
        digest(&self.target_digest)?;
        self.scope.validate()
    }
}
/// The anchor is an already verified header (or the initial schedule parent).
/// Replies are data: callers must verify every successor against their schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPeerHistoryRequestV3 {
    pub nonce: String,
    pub after_height: u64,
    pub after_header_id: String,
    pub max_entries: u16,
}
impl BftPeerHistoryRequestV3 {
    fn validate(&self) -> Result<(), String> {
        digest(&self.nonce)?;
        digest(&self.after_header_id)?;
        if self.max_entries == 0 || usize::from(self.max_entries) > BFT_PUBLIC_PEER_MAX_HISTORY_V3 {
            return Err("BFT V3 history request count exceeds bound".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftPeerHistoryReplyV3 {
    pub request: BftPeerHistoryRequestV3,
    #[serde(deserialize_with = "history")]
    pub certificates: Vec<FormalBftCertifiedHeaderV3>,
}
impl BftPeerHistoryReplyV3 {
    /// Bind a response to the original outstanding request, not a peer's echo alone.
    pub fn validate_for_request(&self, request: &BftPeerHistoryRequestV3) -> Result<(), String> {
        if &self.request != request {
            return Err("BFT V3 history response request mismatch".into());
        }
        request.validate()?;
        if canonical(&self.certificates)?.len() > BFT_PUBLIC_PEER_MAX_HISTORY_BYTES_V3 {
            return Err("BFT V3 history byte capacity".into());
        }
        if self.certificates.len() > usize::from(request.max_entries) {
            return Err("BFT V3 history response count exceeds request".into());
        }
        let mut height = request.after_height;
        let mut parent = request.after_header_id.clone();
        for certificate in &self.certificates {
            height = height
                .checked_add(1)
                .ok_or("BFT V3 history height overflow")?;
            if certificate.execution_commitment.height != height
                || certificate.execution_commitment.parent_commit_ref != parent
            {
                return Err("BFT V3 history response is not an exact prefix extension".into());
            }
            parent = certificate.header.block_id()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum BftPublicPeerMessageV3 {
    Proposal(Box<BftPublicProposalV3>),
    Vote(Box<BftVoteV3>),
    Commit(Box<FormalBftCertifiedHeaderV3>),
    HistoryRequest(BftPeerHistoryRequestV3),
    HistoryReply(BftPeerHistoryReplyV3),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    domain: String,
    schema_version: u32,
    context: BftPeerContextV3,
    message: BftPublicPeerMessageV3,
}
fn digest(s: &str) -> Result<(), String> {
    if s.len() != 64
        || !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("BFT V3 peer digest is not canonical".into());
    }
    Ok(())
}
fn canonical<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    oasis7_cas::to_canonical_cbor(v).map_err(|_| "BFT V3 peer canonical encoding rejected".into())
}
fn certificate(c: &FormalBftCertifiedHeaderV3, context: &BftPeerContextV3) -> Result<(), String> {
    c.validate_binding()?;
    if c.execution_commitment.scope != context.scope
        || c.execution_commitment.activation_digest != context.activation_digest
    {
        return Err("BFT V3 peer certificate context mismatch".into());
    }
    if canonical(c)?.len() > BFT_PUBLIC_PEER_MAX_CERTIFICATE_BYTES_V3 {
        return Err("BFT V3 peer certificate exceeds bound".into());
    }
    Ok(())
}
fn validate(context: &BftPeerContextV3, message: &BftPublicPeerMessageV3) -> Result<(), String> {
    context.validate()?;
    match message {
        BftPublicPeerMessageV3::Proposal(p) => {
            p.validate_binding()?;
            p.header.validate()?;
            if p.header.scope != context.scope
                || p.block.execution_commitment.activation_digest != context.activation_digest
            {
                return Err("BFT V3 peer proposal context mismatch".into());
            }
            if canonical(p)?.len() > BFT_PUBLIC_PEER_MAX_PROPOSAL_BYTES_V3 {
                return Err("BFT V3 peer proposal exceeds bound".into());
            }
        }
        BftPublicPeerMessageV3::Vote(v) => {
            v.validate()?;
            if v.scope != context.scope {
                return Err("BFT V3 peer vote context mismatch".into());
            }
        }
        BftPublicPeerMessageV3::Commit(c) => certificate(c, context)?,
        BftPublicPeerMessageV3::HistoryRequest(r) => r.validate()?,
        BftPublicPeerMessageV3::HistoryReply(r) => {
            r.validate_for_request(&r.request)?;
            for c in &r.certificates {
                certificate(c, context)?;
            }
        }
    }
    Ok(())
}
pub fn encode_bft_public_peer_message_v3(
    context: &BftPeerContextV3,
    message: &BftPublicPeerMessageV3,
) -> Result<Vec<u8>, String> {
    validate(context, message)?;
    let bytes = canonical(&Envelope {
        domain: BFT_PUBLIC_PEER_DOMAIN_V3.into(),
        schema_version: 3,
        context: context.clone(),
        message: message.clone(),
    })?;
    scan(&bytes)?;
    Ok(bytes)
}
pub fn decode_bft_public_peer_message_v3(
    bytes: &[u8],
    expected: &BftPeerContextV3,
) -> Result<BftPublicPeerMessageV3, String> {
    scan(bytes)?;
    let e: Envelope = serde_cbor::from_slice(bytes).map_err(|e| e.to_string())?;
    if e.domain != BFT_PUBLIC_PEER_DOMAIN_V3 || e.schema_version != 3 || &e.context != expected {
        return Err("BFT V3 peer envelope domain/version/context mismatch".into());
    }
    if canonical(&e)? != bytes {
        return Err("BFT V3 peer envelope is not canonical".into());
    }
    validate(&e.context, &e.message)?;
    Ok(e.message)
}
// Zero-allocation preflight precedes all nested derived Deserialize calls. It
// rejects attacker-declared lengths before serde can reserve any collection.
fn scan(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > BFT_PUBLIC_PEER_MAX_BYTES_V3 {
        return Err("BFT V3 peer frame exceeds bound".into());
    }
    let mut cursor = 0;
    let mut tokens = 0;
    item(bytes, &mut cursor, 0, &mut tokens)?;
    if cursor != bytes.len() {
        return Err("BFT V3 peer trailing bytes".into());
    }
    Ok(())
}
fn item(b: &[u8], p: &mut usize, depth: usize, tokens: &mut usize) -> Result<(), String> {
    *tokens += 1;
    if depth > 48 || *tokens > BFT_PUBLIC_PEER_MAX_BYTES_V3 {
        return Err("BFT V3 peer structural capacity".into());
    }
    let head = *b.get(*p).ok_or("BFT V3 peer truncated CBOR")?;
    *p += 1;
    let major = head >> 5;
    let low = head & 31;
    let n = match low {
        0..=23 => u64::from(low),
        24..=27 => {
            let width = 1usize << (low - 24);
            let end = p.checked_add(width).ok_or("BFT V3 peer CBOR overflow")?;
            let bytes = b.get(*p..end).ok_or("BFT V3 peer truncated CBOR length")?;
            *p = end;
            bytes.iter().fold(0u64, |v, x| (v << 8) | u64::from(*x))
        }
        _ => return Err("BFT V3 peer indefinite/reserved CBOR".into()),
    };
    match major {
        0 | 1 => {}
        2 | 3 => {
            if n > 1024 {
                return Err("BFT V3 peer string exceeds bound".into());
            }
            let end = p
                .checked_add(n as usize)
                .ok_or("BFT V3 peer string overflow")?;
            if end > b.len() {
                return Err("BFT V3 peer truncated string".into());
            }
            *p = end;
        }
        4 | 5 => {
            let limit = if major == 4 { 256 } else { 64 };
            if n > limit {
                return Err("BFT V3 peer declared collection exceeds bound".into());
            }
            let count = if major == 5 { n * 2 } else { n };
            if count > (b.len() - *p) as u64 {
                return Err("BFT V3 peer impossible collection count".into());
            }
            if major == 4 {
                for _ in 0..count {
                    let start = *p;
                    item(b, p, depth + 1, tokens)?;
                    if *p - start > BFT_PUBLIC_PEER_MAX_CERTIFICATE_BYTES_V3 {
                        return Err("BFT V3 array item byte capacity".into());
                    }
                }
            } else {
                for _ in 0..n {
                    let key_start = *p;
                    item(b, p, depth + 1, tokens)?;
                    let key = &b[key_start..*p];
                    let start = *p;
                    item(b, p, depth + 1, tokens)?;
                    // Externally tagged variants bind these individual bodies.
                    if (key == b"\x68Proposal" || key == b"\x66Commit")
                        && *p - start > BFT_PUBLIC_PEER_MAX_CERTIFICATE_BYTES_V3
                    {
                        return Err("BFT V3 peer body byte capacity".into());
                    }
                    if key == b"\x6ccertificates"
                        && *p - start > BFT_PUBLIC_PEER_MAX_HISTORY_BYTES_V3
                    {
                        return Err("BFT V3 history byte capacity".into());
                    }
                }
            }
        }
        6 if n == 55799 && depth == 0 => item(b, p, depth + 1, tokens)?,
        7 if matches!(low, 20..=22) => {}
        _ => return Err("BFT V3 peer unsupported CBOR type/tag".into()),
    }
    Ok(())
}
fn history<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<FormalBftCertifiedHeaderV3>, D::Error> {
    struct Bound;
    impl<'de> serde::de::Visitor<'de> for Bound {
        type Value = Vec<FormalBftCertifiedHeaderV3>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 64 V3 header certificates")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq
                .size_hint()
                .is_some_and(|n| n > BFT_PUBLIC_PEER_MAX_HISTORY_V3)
            {
                return Err(serde::de::Error::custom(
                    "BFT V3 history declared count exceeds bound",
                ));
            }
            let mut values = Vec::new();
            while let Some(value) = seq.next_element()? {
                if values.len() == BFT_PUBLIC_PEER_MAX_HISTORY_V3 {
                    return Err(serde::de::Error::custom(
                        "BFT V3 history count exceeds bound",
                    ));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    d.deserialize_seq(Bound)
}
#[cfg(test)]
#[path = "peer_message_tests.rs"]
mod tests;
