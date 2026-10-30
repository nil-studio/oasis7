//! Immutable future-publication claims. Only validated original candidate
//! retention can return a claim accepted by the ledger's budget-only transfer.
use super::super::super::super::storage as common_storage;
use super::super::formal_ledger::{
    LedgerPublicationAccountingV1, PublicationRecordBindingV1, PublicationRecordIdentityV1,
    RetainedPublicationClaimV1,
};
use super::*;
use serde::{Deserialize, Serialize};
const DOMAIN: &str = "oasis7.formal.bft.node_publication_reservation.v1";
pub(super) const MAX_PUBLICATION_METADATA: usize =
    super::super::formal_ledger::MAX_PUBLICATION_METADATA_BYTES;
pub(super) struct PublicationCandidateIdentityV1 {
    pub(super) origin: String,
    pub(super) material: String,
    pub(super) instance: String,
    pub(super) height: u64,
    pub(super) header: String,
    pub(super) actions: String,
    pub(super) reservation_ref: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicationReservationV1 {
    schema_version: u32,
    domain: String,
    ledger_origin_digest: String,
    material_identity: String,
    instance_id: String,
    height: u64,
    header_id: String,
    actions_digest: String,
    snapshot_digest: String,
    snapshot_bytes: u64,
    journal_digest: String,
    journal_bytes: u64,
    derived_nodes_digest: String,
    publication_bytes: u64,
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
impl PublicationReservationV1 {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.domain != DOMAIN
            || self.height == 0
            || [
                &self.ledger_origin_digest,
                &self.material_identity,
                &self.instance_id,
                &self.header_id,
                &self.actions_digest,
                &self.snapshot_digest,
                &self.journal_digest,
                &self.derived_nodes_digest,
            ]
            .into_iter()
            .any(|value| !hash(value))
            || self.publication_bytes == 0
            || self.publication_bytes > 512 * 1024 * 1024
            || self
                .snapshot_bytes
                .checked_add(self.journal_bytes)
                .is_none_or(|n| n > self.publication_bytes)
        {
            return Err("publication reservation identity or capacity rejected".into());
        }
        Ok(())
    }
    pub(super) fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_cbor::to_vec(self).map_err(|_| "encode publication reservation")?;
        if bytes.len() > MAX_PUBLICATION_METADATA {
            return Err("publication metadata capacity".into());
        }
        Ok(bytes)
    }
    pub(super) fn digest(&self) -> Result<String, String> {
        Ok(blake3_hex(&self.bytes()?))
    }
    pub(super) fn validate_identity(
        &self,
        identity: &PublicationCandidateIdentityV1,
    ) -> Result<(), String> {
        self.validate()?;
        if identity.origin != self.ledger_origin_digest
            || identity.material != self.material_identity
            || identity.instance != self.instance_id
            || identity.height != self.height
            || identity.header != self.header_id
            || identity.actions != self.actions_digest
            || identity.reservation_ref.as_ref() != Some(&self.digest()?)
        {
            return Err("publication reservation differs from original candidate".into());
        }
        Ok(())
    }
    pub(super) fn validate_owner(&self, material: &str, instance: &str) -> Result<(), String> {
        if self.material_identity != material || self.instance_id != instance {
            return Err("publication reservation archive owner differs".into());
        }
        Ok(())
    }
    pub(super) fn validate_execution(
        &self,
        origin: &str,
        height: u64,
        actions: &[u8],
        executed: &ExecutedFormalHeaderV3,
    ) -> Result<(), String> {
        self.validate()?;
        if self.ledger_origin_digest != origin
            || self.height != height
            || self.actions_digest != blake3_hex(actions)
            || self.header_id != executed.candidate.header.block_id()?
            || self.snapshot_digest != blake3_hex(&executed.snapshot_bytes)
            || self.snapshot_bytes != executed.snapshot_bytes.len() as u64
            || self.journal_digest != blake3_hex(&executed.journal_bytes)
            || self.journal_bytes != executed.journal_bytes.len() as u64
            || self.derived_nodes_digest
                != blake3_hex(
                    &serde_cbor::to_vec(&executed.result_delta.node_writes)
                        .map_err(|_| "encode publication executed delta")?,
                )
        {
            return Err("publication reservation differs from actual execution".into());
        }
        Ok(())
    }
    pub(super) fn promised_bytes(&self) -> Result<usize, String> {
        usize::try_from(self.publication_bytes)
            .map_err(|_| "publication promise byte overflow".into())
    }
    pub(super) fn accounted_bytes(&self) -> Result<usize, String> {
        self.promised_bytes()?
            .checked_add(self.bytes()?.len())
            .ok_or("publication usage overflow".into())
    }
    pub(super) fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PUBLICATION_METADATA {
            return Err("publication metadata capacity".into());
        }
        let value: Self =
            serde_cbor::from_slice(bytes).map_err(|_| "decode publication reservation")?;
        if value.bytes()? != bytes {
            return Err("publication reservation is not canonical".into());
        }
        Ok(value)
    }
    pub(super) fn read(store: &LocalCasStore, reference: &str) -> Result<Self, String> {
        Self::decode(&common_storage::read_cas_bounded(
            store,
            reference,
            MAX_PUBLICATION_METADATA,
        )?)
    }
    pub(super) fn from_plan(
        identity: &PublicationCandidateIdentityV1,
        plan: &super::publication_plan::PublicationPlanV1,
    ) -> Result<Self, String> {
        if identity.header != plan.candidate.header.block_id()?
            || identity.height != plan.candidate.commitment.height
        {
            return Err("publication plan differs from actual candidate".into());
        }
        let value = Self {
            schema_version: 1,
            domain: DOMAIN.into(),
            ledger_origin_digest: identity.origin.clone(),
            material_identity: identity.material.clone(),
            instance_id: identity.instance.clone(),
            height: identity.height,
            header_id: identity.header.clone(),
            actions_digest: identity.actions.clone(),
            snapshot_digest: plan.snapshot_digest.clone(),
            snapshot_bytes: plan.snapshot_bytes,
            journal_digest: plan.journal_digest.clone(),
            journal_bytes: plan.journal_bytes,
            derived_nodes_digest: plan.derived_nodes_digest.clone(),
            publication_bytes: plan.publication_bytes as u64,
        };
        value.validate()?;
        Ok(value)
    }
    pub(super) fn promise_for_identity(
        &self,
        identity: &PublicationCandidateIdentityV1,
    ) -> Result<super::super::formal_ledger::PublicationPromiseV1, String> {
        self.validate_identity(identity)?;
        self.validate_owner(&identity.material, &identity.instance)?;
        Ok(super::super::formal_ledger::PublicationPromiseV1 {
            binding: PublicationRecordBindingV1 {
                reservation_ref: self.digest()?,
                header_id: self.header_id.clone(),
                actions_digest: self.actions_digest.clone(),
            },
            origin: self.ledger_origin_digest.clone(),
            height: self.height,
            snapshot: self.snapshot_digest.clone(),
            journal: self.journal_digest.clone(),
            snapshot_bytes: usize::try_from(self.snapshot_bytes)
                .map_err(|_| "publication snapshot overflow")?,
            journal_bytes: usize::try_from(self.journal_bytes)
                .map_err(|_| "publication journal overflow")?,
            promised: self.promised_bytes()?,
        })
    }
    pub(super) fn claim_for_actual(
        &self,
        identity: &PublicationCandidateIdentityV1,
        actions: &[u8],
        executed: &ExecutedFormalHeaderV3,
        accounting: &LedgerPublicationAccountingV1,
    ) -> Result<Box<dyn RetainedPublicationClaimV1>, String> {
        self.validate_identity(identity)?;
        self.validate_execution(&identity.origin, identity.height, actions, executed)?;
        let binding = PublicationRecordBindingV1 {
            reservation_ref: self.digest()?,
            header_id: self.header_id.clone(),
            actions_digest: self.actions_digest.clone(),
        };
        let consumed = accounting.consumed_for(
            &identity.origin,
            identity.height,
            &binding,
            &self.snapshot_digest,
            &self.journal_digest,
        )?;
        let remaining = self
            .promised_bytes()?
            .checked_sub(consumed)
            .ok_or("actual storage exceeds publication promise")?;
        Ok(Box::new(RetainedClaim {
            metadata: self.clone(),
            remaining,
        }))
    }
}
struct RetainedClaim {
    metadata: PublicationReservationV1,
    remaining: usize,
}
impl RetainedPublicationClaimV1 for RetainedClaim {
    fn held_bytes(&self) -> usize {
        self.remaining
    }
    fn validate_record(&self, actual: PublicationRecordIdentityV1<'_>) -> Result<(), String> {
        let PublicationRecordIdentityV1 {
            binding,
            origin,
            height,
            snapshot,
            journal,
            snapshot_bytes,
            journal_bytes,
        } = actual;
        let metadata = &self.metadata;
        if binding.reservation_ref != metadata.digest()?
            || binding.header_id != metadata.header_id
            || binding.actions_digest != metadata.actions_digest
            || origin != metadata.ledger_origin_digest
            || height != metadata.height
            || snapshot != metadata.snapshot_digest
            || journal != metadata.journal_digest
            || snapshot_bytes as u64 != metadata.snapshot_bytes
            || journal_bytes as u64 != metadata.journal_bytes
        {
            return Err("publication transfer changed original claim or QC".into());
        }
        Ok(())
    }
}
