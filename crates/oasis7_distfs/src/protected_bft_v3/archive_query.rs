//! Archive-only authenticated read DTOs. These never carry current signing authority.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::crypto::BFT_SIGNER_MAX_FRAME_BYTES_V3;
use super::state::BftSignerStateV3;
use super::successor_public_archive::{
    BftSuccessorWindowOriginalSignedResultV1, resolve_whole_state_signed_result_v1,
};
use super::successor_window::*;
use serde::{Deserialize, Serialize};

pub const BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1: usize = 1024 * 1024;
pub const BFT_SUCCESSOR_ARCHIVE_MAX_CHUNKS_V1: usize = 4096;
const DOMAIN: &str = "oasis7.bft.successor-window.v1.archive-chunk";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", deny_unknown_fields)]
pub enum BftSuccessorArchiveReadTargetV1 {
    GetOriginalState {
        request_id: String,
        checkpoint_digest: String,
        segment_root: String,
        minimum_lower_bound: Box<BftSuccessorWindowMinimumV1>,
    },
    SigningIdentity {
        query: Box<BftSuccessorWindowArchiveQueryV1>,
    },
}
impl BftSuccessorArchiveReadTargetV1 {
    pub(crate) fn coordinates(&self) -> (&str, &str, &str, &BftSuccessorWindowMinimumV1) {
        match self {
            Self::GetOriginalState {
                request_id,
                checkpoint_digest,
                segment_root,
                minimum_lower_bound,
            } => (
                request_id,
                checkpoint_digest,
                segment_root,
                minimum_lower_bound,
            ),
            Self::SigningIdentity { query } => (
                &query.request_id,
                &query.checkpoint_digest,
                &query.segment_root,
                &query.minimum_lower_bound,
            ),
        }
    }
    pub(crate) fn validate(
        &self,
        binding: &BftSignerBindingV3,
        generation: u64,
        token_digest: &str,
    ) -> Result<(), BftSignerErrorV3> {
        if matches!(self, Self::SigningIdentity { query } if matches!(&query.identity, BftSuccessorWindowArchiveIdentityV1::CanonicalRequest { .. }))
        {
            return Err(invalid(
                "public archive cannot resolve private canonical requests",
            ));
        }
        let (id, checkpoint, segment, minimum) = self.coordinates();
        if id.is_empty() || id.len() > 256 || id.trim() != id {
            return Err(invalid("archive request identity bounds"));
        }
        super::state::validate_digest(checkpoint)?;
        super::state::validate_digest(segment)?;
        minimum.validate()?;
        minimum.source.validate_for_binding(binding)?;
        if let Self::SigningIdentity { query } = self {
            query.validate()?;
            if query.lease_generation != generation || query.lease_token_digest != token_digest {
                return Err(invalid("archive identity query lease binding"));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorArchiveReadV1 {
    pub target: BftSuccessorArchiveReadTargetV1,
    pub object: BftSuccessorWindowArchiveRefV1,
    pub offset: u64,
    pub length: u32,
}
impl BftSuccessorArchiveReadV1 {
    pub(crate) fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.object.validate()?;
        if self.object.bytes == 0
            || self.object.bytes > BFT_SIGNER_MAX_FRAME_BYTES_V3 as u64
            || self.length == 0
            || self.length as usize > BFT_SUCCESSOR_ARCHIVE_CHUNK_MAX_BYTES_V1
            || self.offset >= self.object.bytes
            || self
                .offset
                .checked_add(self.length as u64)
                .is_none_or(|end| end > self.object.bytes)
        {
            return Err(invalid("archive chunk range or total bounds"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorArchiveChunkBodyV1 {
    pub schema_version: u32,
    pub binding_digest: String,
    pub provider_public_key: String,
    pub request_digest: String,
    pub challenge: String,
    pub lease_generation: u64,
    pub lease_token_digest: String,
    pub actual_revision: u64,
    pub actual_state_digest: String,
    pub minimum: BftSuccessorWindowMinimumV1,
    pub read: BftSuccessorArchiveReadV1,
    pub reserved_response_bytes: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorArchiveChunkV1 {
    pub body: BftSuccessorArchiveChunkBodyV1,
    pub provider_signature_hex: String,
}
impl BftSuccessorArchiveChunkV1 {
    pub(crate) fn sign(
        body: BftSuccessorArchiveChunkBodyV1,
        key: &dyn super::binding::BftSigningKeyHandleV3,
    ) -> Result<Self, BftSignerErrorV3> {
        let signature = super::crypto::sign_preimage(key, DOMAIN, &body)?;
        Ok(Self {
            body,
            provider_signature_hex: signature,
        })
    }
    pub(crate) fn verify(&self, provider: &str) -> Result<(), BftSignerErrorV3> {
        self.body.read.validate()?;
        super::state::validate_digest(&self.body.actual_state_digest)?;
        self.body.minimum.validate()?;
        super::archive_transfer::BftSuccessorArchiveTransferLedgerV1 {
            schema_version: 1,
            reserved_response_bytes: self.body.reserved_response_bytes,
        }
        .validate()?;
        if self.body.schema_version != 1
            || self.body.provider_public_key != provider
            || self.body.bytes.len() != self.body.read.length as usize
        {
            return Err(invalid("archive chunk envelope shape"));
        }
        super::crypto::verify_signature(DOMAIN, &self.body, &self.provider_signature_hex, provider)
    }
}
/// Fully authenticated original public State, deliberately not a current checkpoint.
#[derive(Debug, Clone)]
pub struct ValidatedBftSuccessorOriginalStateV1 {
    pub(crate) state: BftSignerStateV3,
    pub(crate) provider_public_key: String,
    pub(crate) active_context:
        Option<std::sync::Arc<super::active_window::VerifiedBftActiveWindowContextV1>>,
}
impl ValidatedBftSuccessorOriginalStateV1 {
    pub fn provider_public_key(&self) -> &str {
        &self.provider_public_key
    }
    pub fn binding_digest(&self) -> &str {
        &self.state.binding_digest
    }

    pub fn reconstruct_installed_pending(
        &self,
        binding: &super::binding::BftSignerBindingV3,
        provider_public_key: &str,
        original: &super::successor_state::BftSuccessorWindowStoredRequestV1,
    ) -> Result<
        super::active_window_reconstruction::ValidatedBftSuccessorInstalledStateV1,
        BftSignerErrorV3,
    > {
        super::active_window_reconstruction::reconstruct(
            self,
            binding,
            provider_public_key,
            original,
        )
    }

    pub fn original_state(&self) -> &BftSignerStateV3 {
        &self.state
    }
    /// Missing only describes this immutable segment; it grants no new signing permission.
    pub fn resolve(
        &self,
        identity: &BftSuccessorWindowArchiveIdentityV1,
    ) -> Result<Option<BftSuccessorWindowOriginalSignedResultV1>, BftSignerErrorV3> {
        if matches!(
            identity,
            BftSuccessorWindowArchiveIdentityV1::CanonicalRequest { .. }
        ) {
            return Err(invalid(
                "public archive cannot resolve private canonical requests",
            ));
        }
        resolve_whole_state_signed_result_v1(&self.state, identity)
    }
}
