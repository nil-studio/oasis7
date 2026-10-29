//! Typed successor-window materials. Shape validation is never a trust root.
//! Provider state, driver replay and protected installation remain consumers.
use super::binding::{
    BftSignerBindingV3, BftSignerErrorV3, BftSigningKeyHandleV3, decode_key, invalid,
};
use super::crypto::{
    decode_bounded, digest_hex, encode_bounded, framed_cbor, sign_preimage, verify_signature,
};
use super::state::VerifiedBftSignerStateV3;
use oasis7_formal_protocol::bft::BftHeaderHeightScheduleV3;
use oasis7_proto::distributed_bft::v2::BftScopeV2;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1: usize = 1024 * 1024;
pub const BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1: u64 = 64;
pub const BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1: u64 = 4 * 1024 * 1024 * 1024;
pub const BFT_SUCCESSOR_WINDOW_MAX_REFS_V1: usize = 4096;
pub const BFT_SUCCESSOR_WINDOW_SOFT_HEIGHTS_V1: u64 = 32;
pub const BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1: &str = "oasis7.bft.successor-window.v1.origin";
pub const BFT_SUCCESSOR_WINDOW_CHECKPOINT_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.checkpoint";
pub const BFT_SUCCESSOR_WINDOW_INTENT_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.install-intent";
pub const BFT_SUCCESSOR_WINDOW_RECEIPT_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.install-receipt";
pub const BFT_SUCCESSOR_WINDOW_ARCHIVE_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.archive-manifest";
pub const BFT_SUCCESSOR_WINDOW_QUERY_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.archive-query";
pub const BFT_SUCCESSOR_WINDOW_REPLY_DOMAIN_V1: &str =
    "oasis7.bft.successor-window.v1.archive-reply";
const IDENTITY_DOMAIN: &str = "oasis7.bft.successor-window.v1.archive-identity";

fn hash(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "successor window requires a lowercase 32-byte hash",
        ));
    }
    Ok(())
}
fn identity(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.is_empty() || value.len() > 256 || value.trim() != value {
        return Err(invalid("successor window identity bounds"));
    }
    Ok(())
}
fn metadata<T: Serialize>(value: &T) -> Result<(), BftSignerErrorV3> {
    encode_bounded(value, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1).map(|_| ())
}
fn refs(values: &[BftSuccessorWindowArchiveRefV1]) -> Result<(), BftSignerErrorV3> {
    if values.len() > BFT_SUCCESSOR_WINDOW_MAX_REFS_V1 {
        return Err(invalid("successor reference count"));
    }
    let mut seen = BTreeSet::new();
    for value in values {
        value.validate()?;
        if !seen.insert(&value.digest) {
            return Err(invalid("duplicate successor reference"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowSourceV1 {
    pub scope: BftScopeV2,
    pub binding_digest: String,
    pub handoff_position: u64,
    pub handoff_record_id: String,
    pub handoff_snapshot_root: String,
    pub target_digest: String,
    pub activation_digest: String,
    pub material_identity: String,
}
impl BftSuccessorWindowSourceV1 {
    pub fn from_binding(binding: &BftSignerBindingV3) -> Result<Self, BftSignerErrorV3> {
        binding.validate()?;
        Ok(Self {
            scope: binding.scope.clone(),
            binding_digest: binding.digest()?,
            handoff_position: binding.handoff_position,
            handoff_record_id: binding.handoff_record_id.clone(),
            handoff_snapshot_root: binding.handoff().raw_snapshot_root().into(),
            target_digest: binding.handoff().target_digest().into(),
            activation_digest: binding.activation_digest.clone(),
            material_identity: binding.material_identity.clone(),
        })
    }
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        identity(&self.scope.world_id)?;
        identity(&self.scope.chain_id)?;
        for value in [
            &self.scope.genesis_hash,
            &self.binding_digest,
            &self.handoff_record_id,
            &self.handoff_snapshot_root,
            &self.target_digest,
            &self.activation_digest,
            &self.material_identity,
        ] {
            hash(value)?;
        }
        if self.handoff_position == 0 || self.handoff_position.checked_add(1).is_none() {
            return Err(invalid("successor H position"));
        }
        Ok(())
    }
    pub fn validate_for_binding(
        &self,
        binding: &BftSignerBindingV3,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        if self != &Self::from_binding(binding)? {
            return Err(invalid(
                "successor source differs from original qualified H/binding",
            ));
        }
        Ok(())
    }
    pub fn origin_digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1, self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveRefV1 {
    pub digest: String,
    pub bytes: u64,
}
impl BftSuccessorWindowArchiveRefV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        hash(&self.digest)?;
        if self.bytes == 0 || self.bytes > 512 * 1024 * 1024 {
            return Err(invalid("successor archive object bound"));
        }
        Ok(())
    }
    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        if u64::try_from(bytes.len()).ok() != Some(self.bytes)
            || blake3::hash(bytes).to_hex().as_str() != self.digest
        {
            return Err(invalid("successor archive object length/hash mismatch"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveUsageV1 {
    pub segments: u64,
    pub objects: u64,
    pub total_bytes: u64,
}
impl BftSuccessorWindowArchiveUsageV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.segments > BFT_SUCCESSOR_WINDOW_MAX_SEGMENTS_V1
            || self.total_bytes > BFT_SUCCESSOR_WINDOW_MAX_ARCHIVE_BYTES_V1
            || self.objects > self.total_bytes
        {
            return Err(invalid("successor cumulative archive capacity"));
        }
        Ok(())
    }
    pub fn checked_add(self, delta: Self) -> Result<Self, BftSignerErrorV3> {
        self.validate()?;
        delta.validate()?;
        let next = Self {
            segments: self
                .segments
                .checked_add(delta.segments)
                .ok_or_else(|| invalid("successor segment overflow"))?,
            objects: self
                .objects
                .checked_add(delta.objects)
                .ok_or_else(|| invalid("successor object overflow"))?,
            total_bytes: self
                .total_bytes
                .checked_add(delta.total_bytes)
                .ok_or_else(|| invalid("successor byte overflow"))?,
        };
        next.validate()?;
        Ok(next)
    }
    pub fn validate_after(&self, previous: &Self) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        previous.validate()?;
        if self.segments <= previous.segments
            || self.objects < previous.objects
            || self.total_bytes < previous.total_bytes
        {
            return Err(invalid("successor archive usage regression"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftSuccessorWindowArchiveKindV1 {
    Public,
    Execution,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveEntryV1 {
    pub identity_digest: String,
    pub object: BftSuccessorWindowArchiveRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveManifestV1 {
    pub schema_version: u32,
    pub source: BftSuccessorWindowSourceV1,
    pub kind: BftSuccessorWindowArchiveKindV1,
    pub segment_sequence: u64,
    pub predecessor_root: Option<String>,
    pub first_height: u64,
    pub last_height: u64,
    pub entries: Vec<BftSuccessorWindowArchiveEntryV1>,
    pub usage: BftSuccessorWindowArchiveUsageV1,
}
impl BftSuccessorWindowArchiveManifestV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        self.usage.validate()?;
        if self.schema_version != 1
            || self.segment_sequence == 0
            || self.segment_sequence > self.usage.segments
            || self.first_height <= self.source.handoff_position
            || self.last_height < self.first_height
            || self
                .last_height
                .checked_sub(self.first_height)
                .is_none_or(|n| n >= 64)
            || self.entries.is_empty()
            || self.entries.len() > BFT_SUCCESSOR_WINDOW_MAX_REFS_V1
            || (self.segment_sequence == 1) != self.predecessor_root.is_none()
        {
            return Err(invalid("successor archive manifest version/range/count"));
        }
        if let Some(root) = &self.predecessor_root {
            hash(root)?;
        }
        let mut previous = None;
        let mut bytes = 0_u64;
        for entry in &self.entries {
            hash(&entry.identity_digest)?;
            entry.object.validate()?;
            if previous.is_some_and(|id: &str| id >= entry.identity_digest.as_str()) {
                return Err(invalid(
                    "successor archive identities must be unique and sorted",
                ));
            }
            previous = Some(entry.identity_digest.as_str());
            bytes = bytes
                .checked_add(entry.object.bytes)
                .ok_or_else(|| invalid("successor manifest bytes overflow"))?;
        }
        if bytes > self.usage.total_bytes
            || u64::try_from(self.entries.len()).map_err(|_| invalid("successor manifest count"))?
                > self.usage.objects
        {
            return Err(invalid("successor manifest exceeds cumulative usage"));
        }
        metadata(self)
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SUCCESSOR_WINDOW_ARCHIVE_DOMAIN_V1, self)
    }
    pub fn validate_for_checkpoint(
        &self,
        checkpoint: &BftSuccessorWindowCheckpointBodyV1,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        checkpoint.validate()?;
        let root = match self.kind {
            BftSuccessorWindowArchiveKindV1::Public => &checkpoint.public_archive_root,
            BftSuccessorWindowArchiveKindV1::Execution => &checkpoint.execution_archive_root,
        };
        if self.source != checkpoint.source
            || self.last_height != checkpoint.cut.height
            || self.usage != checkpoint.usage
            || self.digest()? != *root
        {
            return Err(invalid(
                "successor archive manifest differs from original checkpoint cut",
            ));
        }
        Ok(())
    }
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        let value: Self = decode_bounded(bytes, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?;
        value.validate()?;
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowCutV1 {
    pub height: u64,
    pub header_id: String,
    pub certificate_digest: String,
    pub execution_commitment_digest: String,
    pub snapshot_ref: BftSuccessorWindowArchiveRefV1,
    pub journal_ref: BftSuccessorWindowArchiveRefV1,
    pub semantic_state_root: String,
    pub post_manifest_hash: String,
    pub business_result_root: String,
    pub driver_record_digest: String,
    pub driver_install_digest: String,
}
impl BftSuccessorWindowCutV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        if self.height == 0 || self.height.checked_add(1).is_none() {
            return Err(invalid("successor cut height"));
        }
        for value in [
            &self.header_id,
            &self.certificate_digest,
            &self.execution_commitment_digest,
            &self.semantic_state_root,
            &self.post_manifest_hash,
            &self.business_result_root,
            &self.driver_record_digest,
            &self.driver_install_digest,
        ] {
            hash(value)?;
        }
        self.snapshot_ref.validate()?;
        self.journal_ref.validate()?;
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowCheckpointBodyV1 {
    pub schema_version: u32,
    pub source: BftSuccessorWindowSourceV1,
    pub request_id: String,
    pub sequence: u64,
    pub predecessor_checkpoint_digest: String,
    pub cut: BftSuccessorWindowCutV1,
    /// Canonical opaque schedule bytes are compared with a genuinely validated
    /// schedule. They are never deserialized into an authority capability.
    pub successor_schedule_cbor: Vec<u8>,
    pub successor_schedule_digest: String,
    pub current_journal_ref: Option<BftSuccessorWindowArchiveRefV1>,
    pub current_journal_digest: Option<String>,
    pub public_archive_root: String,
    pub execution_archive_root: String,
    /// Raw CAS location of the canonical execution manifest. Its domain digest
    /// must separately equal execution_archive_root; a raw hash is no authority.
    pub execution_manifest_ref: BftSuccessorWindowArchiveRefV1,
    pub evidence_refs: Vec<BftSuccessorWindowArchiveRefV1>,
    pub usage: BftSuccessorWindowArchiveUsageV1,
}
impl BftSuccessorWindowCheckpointBodyV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        self.cut.validate()?;
        identity(&self.request_id)?;
        self.usage.validate()?;
        self.execution_manifest_ref.validate()?;
        if self.execution_manifest_ref.bytes > BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1 as u64 {
            return Err(invalid("successor execution manifest reference capacity"));
        }
        for value in [
            &self.predecessor_checkpoint_digest,
            &self.successor_schedule_digest,
            &self.public_archive_root,
            &self.execution_archive_root,
        ] {
            hash(value)?;
        }
        if self.schema_version != 1
            || self.sequence == 0
            || self.sequence > self.usage.segments
            || self.cut.height <= self.source.handoff_position
            || self.successor_schedule_cbor.is_empty()
            || self.successor_schedule_cbor.len() > BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1
            || self.usage.segments == 0
            || self.current_journal_ref.is_some() != self.current_journal_digest.is_some()
        {
            return Err(invalid("successor checkpoint version/cut/journal shape"));
        }
        if let (Some(reference), Some(digest)) =
            (&self.current_journal_ref, &self.current_journal_digest)
        {
            reference.validate()?;
            if reference.bytes > super::crypto::BFT_SIGNER_MAX_JOURNAL_BYTES_V3 as u64 {
                return Err(invalid("successor journal reference capacity"));
            }
            hash(digest)?;
        }
        if self.sequence == 1
            && self.predecessor_checkpoint_digest != self.source.origin_digest()?
        {
            return Err(invalid(
                "successor first predecessor differs from original H",
            ));
        }
        refs(&self.evidence_refs)?;
        metadata(self)
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SUCCESSOR_WINDOW_CHECKPOINT_DOMAIN_V1, self)
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>, BftSignerErrorV3> {
        self.validate()?;
        framed_cbor(
            BFT_SUCCESSOR_WINDOW_CHECKPOINT_DOMAIN_V1,
            self,
            BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1,
        )
    }
    pub fn decode_canonical(bytes: &[u8]) -> Result<Self, BftSignerErrorV3> {
        let value: Self = decode_bounded(bytes, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?;
        value.validate()?;
        Ok(value)
    }
    pub fn minimum(&self) -> Result<BftSuccessorWindowMinimumV1, BftSignerErrorV3> {
        Ok(BftSuccessorWindowMinimumV1 {
            source: self.source.clone(),
            sequence: self.sequence,
            height: self.cut.height,
            header_id: self.cut.header_id.clone(),
            checkpoint_digest: self.digest()?,
        })
    }
    /// Local binding check only: the caller must already hold verified provider
    /// state and a protected predecessor. Driver same-cut/private archive and
    /// backend anti-rollback checks are additional mandatory service consumers.
    pub(crate) fn bind_verified_state(
        &self,
        binding: &BftSignerBindingV3,
        state: &VerifiedBftSignerStateV3,
        predecessor: Option<&VerifiedBftSuccessorWindowV1>,
    ) -> Result<VerifiedBftSuccessorWindowV1, BftSignerErrorV3> {
        self.validate()?;
        self.source.validate_for_binding(binding)?;
        if let Some(previous) = predecessor {
            if self.source != previous.body.source
                || self.sequence
                    != previous
                        .body
                        .sequence
                        .checked_add(1)
                        .ok_or_else(|| invalid("successor sequence overflow"))?
                || self.predecessor_checkpoint_digest != previous.digest
                || self.cut.height <= previous.body.cut.height
            {
                return Err(invalid("successor protected predecessor discontinuity"));
            }
            self.usage.validate_after(&previous.body.usage)?;
        } else if self.sequence != 1 {
            return Err(invalid("successor missing protected predecessor"));
        }
        let current = &state.state;
        let certificate = current
            .history
            .last()
            .ok_or_else(|| invalid("successor lacks actual certified cut"))?;
        let commitment = &certificate.execution_commitment;
        if current.binding_digest != self.source.binding_digest
            || current.pending_driver_apply.is_some()
            || current
                .journal
                .as_ref()
                .is_some_and(|j| j.pending_commit().is_some())
            || commitment.height != self.cut.height
            || certificate.header.block_id().map_err(invalid)? != self.cut.header_id
            || blake3::hash(&certificate.canonical_bytes().map_err(invalid)?)
                .to_hex()
                .as_str()
                != self.cut.certificate_digest
            || commitment.digest().map_err(invalid)? != self.cut.execution_commitment_digest
            || commitment.execution_state_root != self.cut.snapshot_ref.digest
            || commitment.state_root != self.cut.semantic_state_root
            || commitment.post_manifest_hash != self.cut.post_manifest_hash
            || commitment.result_root != self.cut.business_result_root
            || state.schedule.height()
                != self
                    .cut
                    .height
                    .checked_add(1)
                    .ok_or_else(|| invalid("successor height overflow"))?
            || state.schedule.parent_commit_ref() != self.cut.header_id
            || state.schedule.digest().map_err(invalid)? != self.successor_schedule_digest
            || encode_bounded(&state.schedule, BFT_SUCCESSOR_WINDOW_METADATA_MAX_BYTES_V1)?
                != self.successor_schedule_cbor
            || !current.applied_installs.iter().any(|r| {
                r.height == self.cut.height
                    && r.header_id == self.cut.header_id
                    && r.driver_install_digest == self.cut.driver_install_digest
            })
        {
            return Err(invalid(
                "successor checkpoint differs from actual applied provider cut/schedule",
            ));
        }
        match (
            &current.journal,
            &self.current_journal_ref,
            &self.current_journal_digest,
        ) {
            (None, None, None) => {}
            (Some(journal), Some(reference), Some(digest)) => {
                let bytes =
                    encode_bounded(journal, super::crypto::BFT_SIGNER_MAX_JOURNAL_BYTES_V3)?;
                reference.validate_bytes(&bytes)?;
                if journal.digest().map_err(invalid)? != *digest
                    || journal.height() != state.schedule.height()
                    || journal.parent_commit_ref() != self.cut.header_id
                {
                    return Err(invalid("successor changed original T+1 journal"));
                }
            }
            _ => {
                return Err(invalid(
                    "successor cannot synthesize or remove original journal",
                ));
            }
        }
        Ok(VerifiedBftSuccessorWindowV1 {
            body: self.clone(),
            digest: self.digest()?,
            schedule: state.schedule.clone(),
        })
    }
}
/// No Deserialize or public DTO constructor. This token is a local checked
/// binding, not another peer's independently pinned checkpoint authority.
#[derive(Debug, Clone)]
pub(crate) struct VerifiedBftSuccessorWindowV1 {
    body: BftSuccessorWindowCheckpointBodyV1,
    digest: String,
    schedule: BftHeaderHeightScheduleV3,
}
impl VerifiedBftSuccessorWindowV1 {
    pub(crate) fn body(&self) -> &BftSuccessorWindowCheckpointBodyV1 {
        &self.body
    }
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }
    pub(crate) fn schedule(&self) -> &BftHeaderHeightScheduleV3 {
        &self.schedule
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowMinimumV1 {
    pub source: BftSuccessorWindowSourceV1,
    pub sequence: u64,
    pub height: u64,
    pub header_id: String,
    pub checkpoint_digest: String,
}
impl BftSuccessorWindowMinimumV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        hash(&self.header_id)?;
        hash(&self.checkpoint_digest)?;
        if self.height < self.source.handoff_position
            || (self.sequence > 0 && self.height == self.source.handoff_position)
            || (self.sequence == 0
                && (self.height != self.source.handoff_position
                    || self.header_id != self.source.handoff_record_id
                    || self.checkpoint_digest != self.source.origin_digest()?))
        {
            return Err(invalid("successor minimum original H binding"));
        }
        Ok(())
    }
    pub fn validate_after(&self, previous: &Self) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        previous.validate()?;
        if self == previous {
            return Ok(());
        }
        if self.source != previous.source
            || self.height <= previous.height
            || self.sequence
                != previous
                    .sequence
                    .checked_add(1)
                    .ok_or_else(|| invalid("successor minimum sequence overflow"))?
        {
            return Err(invalid("successor minimum rollback, fork or sequence gap"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowInstallIntentBodyV1 {
    pub schema_version: u32,
    pub checkpoint: BftSuccessorWindowCheckpointBodyV1,
    pub expected_lease_generation: u64,
    pub expected_revision: u64,
    pub expected_state_digest: String,
    pub driver_evidence_digest: String,
}
impl BftSuccessorWindowInstallIntentBodyV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.checkpoint.validate()?;
        hash(&self.expected_state_digest)?;
        hash(&self.driver_evidence_digest)?;
        if self.schema_version != 1
            || self.expected_lease_generation == 0
            || self.expected_revision.checked_add(2).is_none()
        {
            return Err(invalid("successor intent version/generation/revision"));
        }
        metadata(self)
    }
    /// Lower bound only: reservation/material CAS steps may place the actual
    /// Prepared state later. StoredRequest commits that actual revision.
    pub fn minimum_prepared_revision(&self) -> Result<u64, BftSignerErrorV3> {
        self.validate()?;
        self.expected_revision
            .checked_add(1)
            .ok_or_else(|| invalid("successor prepared revision overflow"))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowInstallIntentV1 {
    pub body: BftSuccessorWindowInstallIntentBodyV1,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}
impl BftSuccessorWindowInstallIntentV1 {
    pub(crate) fn sign(
        body: BftSuccessorWindowInstallIntentBodyV1,
        key: &dyn BftSigningKeyHandleV3,
    ) -> Result<Self, BftSignerErrorV3> {
        body.validate()?;
        let public = key.public_key_hex();
        decode_key(&public)?;
        let signature = sign_preimage(key, BFT_SUCCESSOR_WINDOW_INTENT_DOMAIN_V1, &body)?;
        Ok(Self {
            body,
            provider_public_key: public,
            provider_signature_hex: signature,
        })
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.body.validate()?;
        metadata(self)?;
        digest_hex(BFT_SUCCESSOR_WINDOW_INTENT_DOMAIN_V1, self)
    }
    pub fn verify_for(
        &self,
        binding: &BftSignerBindingV3,
        provider_public_key: &str,
    ) -> Result<(), BftSignerErrorV3> {
        self.body.validate()?;
        self.body.checkpoint.source.validate_for_binding(binding)?;
        self.verify_signature(provider_public_key)
    }
    fn verify_signature(&self, public: &str) -> Result<(), BftSignerErrorV3> {
        if self.provider_public_key != public {
            return Err(invalid("successor intent provider pin"));
        }
        metadata(self)?;
        verify_signature(
            BFT_SUCCESSOR_WINDOW_INTENT_DOMAIN_V1,
            &self.body,
            &self.provider_signature_hex,
            public,
        )
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowInstallReceiptBodyV1 {
    pub schema_version: u32,
    pub source: BftSuccessorWindowSourceV1,
    pub request_id: String,
    pub checkpoint_digest: String,
    pub intent_digest: String,
    pub minimum: BftSuccessorWindowMinimumV1,
    /// Historical committing generation; never rewritten for a new querier.
    pub lease_generation: u64,
    pub committed_revision: u64,
    pub driver_install_digest: String,
}
impl BftSuccessorWindowInstallReceiptBodyV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        identity(&self.request_id)?;
        self.minimum.validate()?;
        for value in [
            &self.checkpoint_digest,
            &self.intent_digest,
            &self.driver_install_digest,
        ] {
            hash(value)?;
        }
        if self.schema_version != 1
            || self.lease_generation == 0
            || self.committed_revision == 0
            || self.source != self.minimum.source
            || self.checkpoint_digest != self.minimum.checkpoint_digest
        {
            return Err(invalid("successor receipt version/source/minimum"));
        }
        metadata(self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowInstallReceiptV1 {
    pub body: BftSuccessorWindowInstallReceiptBodyV1,
    pub provider_public_key: String,
    pub provider_signature_hex: String,
}
impl BftSuccessorWindowInstallReceiptV1 {
    pub(crate) fn sign(
        body: BftSuccessorWindowInstallReceiptBodyV1,
        key: &dyn BftSigningKeyHandleV3,
    ) -> Result<Self, BftSignerErrorV3> {
        body.validate()?;
        let public = key.public_key_hex();
        decode_key(&public)?;
        let signature = sign_preimage(key, BFT_SUCCESSOR_WINDOW_RECEIPT_DOMAIN_V1, &body)?;
        Ok(Self {
            body,
            provider_public_key: public,
            provider_signature_hex: signature,
        })
    }
    pub fn verify_for(
        &self,
        intent: &BftSuccessorWindowInstallIntentV1,
        public: &str,
    ) -> Result<(), BftSignerErrorV3> {
        self.body.validate()?;
        intent.body.validate()?;
        intent.verify_signature(public)?;
        let expected_minimum = intent.body.checkpoint.minimum()?;
        if self.provider_public_key != public
            || self.body.source != intent.body.checkpoint.source
            || self.body.request_id != intent.body.checkpoint.request_id
            || self.body.intent_digest != intent.digest()?
            || self.body.minimum != expected_minimum
            || self.body.driver_install_digest != intent.body.checkpoint.cut.driver_install_digest
            || self.body.lease_generation < intent.body.expected_lease_generation
            || self.body.committed_revision <= intent.body.minimum_prepared_revision()?
        {
            return Err(invalid(
                "successor original receipt differs from intent/committed CAS",
            ));
        }
        metadata(self)?;
        verify_signature(
            BFT_SUCCESSOR_WINDOW_RECEIPT_DOMAIN_V1,
            &self.body,
            &self.provider_signature_hex,
            public,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BftSuccessorWindowStatusV1 {
    Prepared,
    InstalledPending,
    Adopted,
    Superseded,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowAdoptedEvidenceV1 {
    pub checkpoint_digest: String,
    pub driver_install_digest: String,
    pub schedule_digest: String,
    pub journal_digest: Option<String>,
    pub readback_state_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowPersistedRequestV1 {
    pub intent: BftSuccessorWindowInstallIntentV1,
    pub status: BftSuccessorWindowStatusV1,
    pub receipt: Option<BftSuccessorWindowInstallReceiptV1>,
    pub adopted_evidence: Option<BftSuccessorWindowAdoptedEvidenceV1>,
    pub superseded_reason: Option<String>,
}
impl BftSuccessorWindowPersistedRequestV1 {
    pub fn validate(&self, provider_public_key: &str) -> Result<(), BftSignerErrorV3> {
        self.intent.body.validate()?;
        self.intent.verify_signature(provider_public_key)?;
        if let Some(receipt) = &self.receipt {
            receipt.verify_for(&self.intent, provider_public_key)?;
        }
        let valid = match self.status {
            BftSuccessorWindowStatusV1::Prepared => {
                self.receipt.is_none()
                    && self.adopted_evidence.is_none()
                    && self.superseded_reason.is_none()
            }
            BftSuccessorWindowStatusV1::InstalledPending => {
                self.receipt.is_some()
                    && self.adopted_evidence.is_none()
                    && self.superseded_reason.is_none()
            }
            BftSuccessorWindowStatusV1::Adopted => {
                self.receipt.is_some()
                    && self.adopted_evidence.is_some()
                    && self.superseded_reason.is_none()
            }
            BftSuccessorWindowStatusV1::Superseded => {
                self.receipt.is_none()
                    && self.adopted_evidence.is_none()
                    && self.superseded_reason.is_some()
            }
        };
        if !valid {
            return Err(invalid("successor persisted status/result shape"));
        }
        if let Some(reason) = &self.superseded_reason {
            identity(reason)?;
        }
        if let Some(evidence) = &self.adopted_evidence {
            let body = &self.intent.body.checkpoint;
            for value in [
                &evidence.checkpoint_digest,
                &evidence.driver_install_digest,
                &evidence.schedule_digest,
                &evidence.readback_state_digest,
            ] {
                hash(value)?;
            }
            if evidence.checkpoint_digest != body.digest()?
                || evidence.driver_install_digest != body.cut.driver_install_digest
                || evidence.schedule_digest != body.successor_schedule_digest
                || evidence.journal_digest != body.current_journal_digest
            {
                return Err(invalid(
                    "successor adopted evidence differs from original installed cut",
                ));
            }
        }
        metadata(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BftSuccessorWindowArchiveIdentityV1 {
    SigningSlot {
        public_key_hex: String,
        height: u64,
        round: u64,
        phase: String,
        preimage_digest: String,
    },
    FinalityProof {
        body_digest: String,
        role: String,
    },
    CanonicalRequest {
        action_id: u64,
        payload_digest: String,
        header_id: String,
    },
}
impl BftSuccessorWindowArchiveIdentityV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        match self {
            Self::SigningSlot {
                public_key_hex,
                height,
                round,
                phase,
                preimage_digest,
            } => {
                hash(public_key_hex)?;
                decode_key(public_key_hex)?;
                hash(preimage_digest)?;
                if *height == 0
                    || *round > 4096
                    || !["proposal", "prevote", "precommit"].contains(&phase.as_str())
                {
                    return Err(invalid("successor signing slot bounds"));
                }
            }
            Self::FinalityProof { body_digest, role } => {
                hash(body_digest)?;
                if ![
                    "admitted_old",
                    "admitted_new",
                    "bootstrap_old",
                    "bootstrap_new",
                ]
                .contains(&role.as_str())
                {
                    return Err(invalid("successor typed proof role"));
                }
            }
            Self::CanonicalRequest {
                payload_digest,
                header_id,
                ..
            } => {
                hash(payload_digest)?;
                hash(header_id)?;
            }
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(IDENTITY_DOMAIN, self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveQueryV1 {
    pub schema_version: u32,
    pub source: BftSuccessorWindowSourceV1,
    pub request_id: String,
    pub checkpoint_digest: String,
    pub segment_root: String,
    pub identity: BftSuccessorWindowArchiveIdentityV1,
    pub challenge: String,
    pub lease_generation: u64,
    pub lease_token_digest: String,
    pub minimum_lower_bound: BftSuccessorWindowMinimumV1,
}
impl BftSuccessorWindowArchiveQueryV1 {
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        self.source.validate()?;
        identity(&self.request_id)?;
        self.identity.validate()?;
        self.minimum_lower_bound.validate()?;
        for value in [
            &self.checkpoint_digest,
            &self.segment_root,
            &self.challenge,
            &self.lease_token_digest,
        ] {
            hash(value)?;
        }
        if self.schema_version != 1
            || self.lease_generation == 0
            || self.source != self.minimum_lower_bound.source
        {
            return Err(invalid("successor archive query source/lease"));
        }
        metadata(self)
    }
    pub fn digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SUCCESSOR_WINDOW_QUERY_DOMAIN_V1, self)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowArchiveReplyV1 {
    pub schema_version: u32,
    pub request_digest: String,
    pub challenge: String,
    pub actual_revision: u64,
    pub actual_state_digest: String,
    pub minimum: BftSuccessorWindowMinimumV1,
    pub manifest: BftSuccessorWindowArchiveManifestV1,
    pub object: BftSuccessorWindowArchiveRefV1,
    pub original_signature_hex: Option<String>,
}
impl BftSuccessorWindowArchiveReplyV1 {
    /// The transport additionally authenticates the provider pin, current lease
    /// and this exact reply, and bind its archive root/minimum to the verified
    /// protected checkpoint chain. This local check cannot create that trust.
    /// Referenced object chunks must pass validate_bytes.
    pub fn validate_for(
        &self,
        query: &BftSuccessorWindowArchiveQueryV1,
    ) -> Result<(), BftSignerErrorV3> {
        query.validate()?;
        self.minimum.validate()?;
        self.manifest.validate()?;
        self.object.validate()?;
        hash(&self.actual_state_digest)?;
        let identity_digest = query.identity.digest()?;
        if let Some(signature) = &self.original_signature_hex {
            if signature.len() != 128
                || !signature
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("successor archive original signature encoding"));
            }
        }
        if self.schema_version != 1
            || self.request_digest != query.digest()?
            || self.challenge != query.challenge
            || self.minimum.source != query.source
            || self.manifest.source != query.source
            || self.manifest.digest()? != query.segment_root
            || self.minimum.height < query.minimum_lower_bound.height
            || self.minimum.sequence < query.minimum_lower_bound.sequence
            || (self.minimum.height == query.minimum_lower_bound.height
                && self.minimum != query.minimum_lower_bound)
            || !self.manifest.entries.iter().any(|entry| {
                entry.identity_digest == identity_digest && entry.object == self.object
            })
        {
            return Err(invalid(
                "successor archive reply freshness/membership/minimum binding",
            ));
        }
        metadata(self)
    }
}
#[cfg(test)]
#[path = "successor_window_tests.rs"]
mod tests;
