//! Bounded, H-anchored protocol evidence. This never certifies World replay,
//! current serving authority, signer installation, or checkpoint freshness.
use super::*;
use crate::bft::{VerifiedBftCommitCertificateV2, verify_commit_certificate_token_v2};
use oasis7_proto::distributed_bft::v2::BftCommitCertificateV2;

pub const BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1: usize = 64;
pub const BFT_FORMAL_PREFIX_MAX_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftCertifiedEntryV1 {
    pub certificate: BftCommitCertificateV2,
    pub commitment: FormalBftExecutionCommitmentV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormalBftCertifiedPrefixV1 {
    pub schema_version: u32,
    pub entries: Vec<FormalBftCertifiedEntryV1>,
}

impl FormalBftCertifiedPrefixV1 {
    fn validate_bounds(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.entries.is_empty()
            || self.entries.len() > BFT_FORMAL_PREFIX_MAX_HEIGHTS_V1
        {
            return Err("formal BFT prefix schema or height capacity".into());
        }
        let mut entry_bytes = 0usize;
        for entry in &self.entries {
            entry.certificate.validate_wire_bounds()?;
            entry.commitment.validate()?;
            // Map ordering does not change encoded size. Each entry's
            // ordinary encoding is a lower bound on the complete canonical
            // envelope, so reject cumulative excess before constructing its
            // much larger intermediate canonical Value tree.
            let encoded = serde_cbor::to_vec(entry)
                .map_err(|error| format!("measure formal BFT prefix entry: {error}"))?;
            entry_bytes = entry_bytes
                .checked_add(encoded.len())
                .ok_or("formal BFT prefix cumulative byte count overflow")?;
            if entry_bytes > BFT_FORMAL_PREFIX_MAX_BYTES_V1 {
                return Err("formal BFT prefix exceeds cumulative byte bound".into());
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate_bounds()?;
        canonical_bytes(self, BFT_FORMAL_PREFIX_MAX_BYTES_V1, "formal BFT prefix")
    }

    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, String> {
        let value: Self =
            decode_canonical(bytes, BFT_FORMAL_PREFIX_MAX_BYTES_V1, "formal BFT prefix")?;
        value.validate_bounds()?;
        Ok(value)
    }

    /// Verify every height from the externally authenticated H. The caller
    /// must additionally match the resulting tip to its independently pinned
    /// exact target, and a publishing driver must replay all World transitions.
    pub fn verify(
        &self,
        handoff: &VerifiedBftHandoffTargetV1,
    ) -> Result<VerifiedFormalBftPrefixV1, String> {
        let encoded_bytes = self.canonical_bytes()?.len();
        let mut schedule = None;
        let mut tip = None;
        for entry in &self.entries {
            let block = &entry.certificate.proposal.block;
            entry
                .commitment
                .verify_block(block, handoff.activation_digest())?;
            let current = match schedule.take() {
                Some(schedule) => schedule,
                None => handoff.initial_schedule_for_commitment(&entry.commitment, block)?,
            };
            if entry.commitment.parent_tick() != current.parent_tick()
                || entry.commitment.parent_semantic_state_root() != current.parent_state_root()
            {
                return Err("formal BFT prefix execution parent differs from its schedule".into());
            }
            let verified = verify_commit_certificate_token_v2(&current, &entry.certificate)?;
            schedule = Some(verified.next_height_schedule().clone());
            tip = Some((verified, entry.commitment.clone()));
        }
        let (certificate, commitment) = tip.ok_or("formal BFT prefix is empty")?;
        Ok(VerifiedFormalBftPrefixV1 {
            handoff: handoff.clone(),
            certificate,
            commitment,
            heights: self.entries.len(),
            encoded_bytes,
        })
    }
}

/// Opaque proof of the complete bounded protocol prefix from one qualified H.
/// Deliberately has no deserializer or constructor from an arbitrary V2 token.
#[derive(Debug, Clone)]
pub struct VerifiedFormalBftPrefixV1 {
    handoff: VerifiedBftHandoffTargetV1,
    certificate: VerifiedBftCommitCertificateV2,
    commitment: FormalBftExecutionCommitmentV1,
    heights: usize,
    encoded_bytes: usize,
}

impl VerifiedFormalBftPrefixV1 {
    pub fn handoff(&self) -> &VerifiedBftHandoffTargetV1 {
        &self.handoff
    }

    pub fn certificate(&self) -> &VerifiedBftCommitCertificateV2 {
        &self.certificate
    }

    pub fn commitment(&self) -> &FormalBftExecutionCommitmentV1 {
        &self.commitment
    }

    pub fn next_height_schedule(&self) -> &BftHeightProposerScheduleV2 {
        self.certificate.next_height_schedule()
    }

    pub fn heights(&self) -> usize {
        self.heights
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }
}
