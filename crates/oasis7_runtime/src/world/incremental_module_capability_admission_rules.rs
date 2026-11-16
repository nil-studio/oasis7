//! Unique original module-capability admission, issuer, proof and parent rules.
use crate::capability_authorization::{
    CapabilityAuthorityFinalityBinding, CapabilityAuthorityFinalityProof, CapabilityAuthorityRecord,
};
use crate::governance::{GovernanceFinalityCertificate, GovernanceFinalityEpochSnapshot};
use crate::world::capability_authorization::{
    deny, validate_authority_record, validate_module_v2_grant_lifetime_for_admission,
    validate_module_v2_grant_static_body_for_admission,
};
use crate::{ProposalStatus, WorldError};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_wasm_abi::CapabilityGrantV2;
use oasis7_world_state::incremental_runtime_revocation::ModuleAdmissionRevocationValueV1;
use std::collections::BTreeSet;
pub(in crate::world) enum GrantDataV1<'a> {
    Legacy(&'a serde_json::Value),
    Current(ReservedValue<serde_json::Value>),
}
impl GrantDataV1<'_> {
    pub(in crate::world) fn value(&self) -> &serde_json::Value {
        match self {
            Self::Legacy(v) => v,
            Self::Current(v) => v,
        }
    }
}
pub(in crate::world) enum AuthorityDataV1<'a> {
    Legacy(&'a CapabilityAuthorityRecord),
    Projection(ReservedValue<CapabilityAuthorityRecord>),
    Current(ReservedValue<ModuleAdmissionRevocationValueV1>),
}
impl AuthorityDataV1<'_> {
    pub(in crate::world) fn value(&self) -> &CapabilityAuthorityRecord {
        match self {
            Self::Legacy(v) => v,
            Self::Projection(v) => v,
            Self::Current(v) => match &**v {
                ModuleAdmissionRevocationValueV1::Authority(Some(v)) => v.value(),
                _ => unreachable!("private selected Authority source"),
            },
        }
    }
}
pub(in crate::world) enum ProofDataV1<'a> {
    Legacy(&'a CapabilityAuthorityFinalityProof),
    Projection(ReservedValue<CapabilityAuthorityFinalityProof>),
    Current(ReservedValue<ModuleAdmissionRevocationValueV1>),
}
impl ProofDataV1<'_> {
    pub(in crate::world) fn value(&self) -> &CapabilityAuthorityFinalityProof {
        match self {
            Self::Legacy(v) => v,
            Self::Projection(v) => v,
            Self::Current(v) => match &**v {
                ModuleAdmissionRevocationValueV1::Proof(Some(v)) => v.value(),
                _ => unreachable!("private selected Proof source"),
            },
        }
    }
}
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait FactsV1<'a>: sealed::Sealed {
    type Error: From<WorldError>;
    fn tick(&mut self) -> Result<u64, Self::Error>;
    fn grant(&mut self, id: &str) -> Result<Option<GrantDataV1<'a>>, Self::Error>;
    fn grant_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn root(&mut self) -> Result<(), Self::Error>;
    fn legacy(&mut self, id: &str) -> Result<(), Self::Error>;
    fn authority(&mut self, id: &str) -> Result<Option<AuthorityDataV1<'a>>, Self::Error>;
    fn proof(&mut self, id: &str) -> Result<Option<ProofDataV1<'a>>, Self::Error>;
    fn epoch(&mut self) -> Result<u64, Self::Error>;
    fn revoked(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn superseded(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn node(&mut self, id: &str) -> Result<Option<String>, Self::Error>;
    fn snapshot_exists(&mut self, epoch: u64) -> Result<bool, Self::Error>;
    fn proposal_hash(&mut self, id: u64) -> Result<Option<Result<String, ()>>, Self::Error>;
    fn certificate(
        &mut self,
        certificate: &GovernanceFinalityCertificate,
    ) -> Result<(), Self::Error>;
    fn work(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn scope_contains(
        &mut self,
        parent: &oasis7_wasm_abi::CapabilityScope,
        child: &oasis7_wasm_abi::CapabilityScope,
    ) -> Result<bool, Self::Error> {
        Ok(parent.contains_subset(child))
    }
    fn validate_grant(&mut self, grant: &CapabilityGrantV2) -> Result<(), Self::Error> {
        grant
            .validate()
            .map_err(|e| deny(format!("parent grant validation: {e}")).into())
    }
    fn grant_body_matches(&mut self, grant: &CapabilityGrantV2) -> Result<bool, Self::Error> {
        grant
            .body_hash_matches()
            .map_err(|e| deny(format!("parent grant body hash: {e}")).into())
    }
    fn grant_id_matches(&mut self, grant: &CapabilityGrantV2) -> Result<bool, Self::Error> {
        grant
            .expected_grant_id()
            .map(|v| v == grant.grant_id)
            .map_err(|e| deny(format!("parent grant id hash: {e}")).into())
    }
    fn record<T: serde::Serialize>(&mut self, value: &T) -> Result<usize, Self::Error>;
}
pub(in crate::world) fn admission<'a, S: FactsV1<'a>>(
    source: &mut S,
    id: &str,
) -> Result<(), S::Error> {
    source.work(id.len().saturating_mul(8) + 4096)?;
    if source.grant_exists(id)? {
        source.root()?;
        let grant = source
            .grant(id)?
            .ok_or_else(|| deny("v2 capability is not in the durable registry"))?;
        source.record(grant.value())?;
        let grant: CapabilityGrantV2 = serde::Deserialize::deserialize(grant.value())
            .map_err(|_| deny("v2 capability grant is malformed"))?;
        source.record(&grant)?;
        validate_module_v2_grant_static_body_for_admission(&grant, id)?;
        let tick = source.tick()?;
        validate_module_v2_grant_lifetime_for_admission(&grant, tick)?;
        issuer(source, &grant)?;
        revocation(source, &grant)?;
        parents(source, &grant, tick, &mut BTreeSet::new())
    } else {
        source.legacy(id)
    }
}
pub(in crate::world) fn issuer<'a, S: FactsV1<'a>>(
    source: &mut S,
    grant: &CapabilityGrantV2,
) -> Result<(), S::Error> {
    let issuer = &grant.issuer;
    let authority = source
        .authority(&issuer.issuer_id)?
        .ok_or_else(|| deny("issuer has no finalized authority record"))?;
    let authority = authority.value();
    let n = source.record(authority)?;
    let complexity = authority
        .superseded_by
        .len()
        .checked_add(1)
        .and_then(|c| c.checked_mul(c))
        .and_then(|c| c.checked_mul(n.saturating_add(1)))
        .and_then(|c| c.checked_mul(64))
        .ok_or_else(|| deny("authority record work overflow"))?;
    source.work(complexity)?;
    validate_authority_record(authority)?;
    let proof = source
        .proof(&issuer.issuer_id)?
        .ok_or_else(|| deny("issuer has no replayable finality proof"))?;
    proof_check(source, authority, proof.value())?;
    if issuer.issuer_kind != authority.issuer_kind
        || issuer.governance_epoch != authority.governance_epoch
        || issuer.finalized_receipt_id != authority.finalized_receipt_id
        || issuer.key_id != authority.key_id
        || issuer.issuer_key_epoch != authority.issuer_key_epoch
        || issuer.authority_rotation_receipt_id != authority.authority_rotation_receipt_id
        || grant.audience.world_id != authority.world_id
        || grant.audience.branch_id != authority.branch_id
        || grant.audience.finality_epoch != authority.finality_epoch
    {
        return Err(deny("grant issuer or governance finality binding mismatch").into());
    }
    let key = hex::decode(authority.public_key_hex.trim())
        .map_err(|_| deny("issuer public key is invalid"))?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| deny("issuer public key length is invalid"))?;
    let key = VerifyingKey::from_bytes(&key).map_err(|_| deny("issuer public key is invalid"))?;
    for encoded in [&issuer.signature, &grant.issuance_signature] {
        let hex = encoded
            .strip_prefix("ed25519:")
            .ok_or_else(|| deny("issuer signature algorithm is not ed25519"))?;
        let bytes = hex::decode(hex).map_err(|_| deny("issuer signature is invalid hex"))?;
        let bytes: [u8; 64] = bytes
            .try_into()
            .map_err(|_| deny("issuer signature length is invalid"))?;
        let n = source.record(grant)?;
        source.work(n.saturating_mul(8) + 4096)?;
        key.verify(
            grant
                .canonical_body_bytes()
                .map_err(|e| deny(format!("canonical grant body: {e}")))?
                .as_slice(),
            &Signature::from_bytes(&bytes),
        )
        .map_err(|_| deny("issuer signature verification failed"))?;
    }
    Ok(())
}
pub(in crate::world) fn revocation<'a, S: FactsV1<'a>>(
    source: &mut S,
    grant: &CapabilityGrantV2,
) -> Result<(), S::Error> {
    if source.epoch()? < grant.revocation_epoch {
        return Err(deny("revocation registry is stale").into());
    }
    if source.revoked(&grant.grant_id)? || source.superseded(&grant.grant_id)? {
        return Err(deny("grant is revoked or superseded").into());
    }
    Ok(())
}
pub(in crate::world) fn authority_certificate<'a, S: FactsV1<'a>>(
    source: &mut S,
    record: &CapabilityAuthorityRecord,
    certificate: &GovernanceFinalityCertificate,
) -> Result<(), S::Error> {
    if !source.snapshot_exists(certificate.epoch_id)? {
        return Err(
            deny("authority finality requires a historical governance epoch snapshot").into(),
        );
    }
    let manifest = source
        .proposal_hash(certificate.proposal_id)?
        .ok_or_else(|| deny("authority finality proposal is not known"))?
        .map_err(|_| deny("authority finality proposal is not approved"))?;
    if manifest != certificate.manifest_hash {
        return Err(deny("authority finality proposal manifest hash mismatch").into());
    }
    source.certificate(certificate)?;
    if record.finality_epoch != certificate.epoch_id {
        return Err(deny("authority finality epoch does not match certificate").into());
    }
    if !certificate.signatures.contains_key(&record.issuer_id) {
        return Err(deny("authority issuer identity is not one of the finality signers").into());
    }
    let key = source
        .node(&record.issuer_id)?
        .ok_or_else(|| deny("authority issuer identity is not trusted"))?;
    if !key.eq_ignore_ascii_case(record.public_key_hex.trim()) {
        return Err(deny("authority issuer key does not match its finality signer identity").into());
    }
    Ok(())
}
pub(in crate::world) fn proof_check<'a, S: FactsV1<'a>>(
    source: &mut S,
    record: &CapabilityAuthorityRecord,
    proof: &CapabilityAuthorityFinalityProof,
) -> Result<(), S::Error> {
    let n = source.record(proof)?;
    if proof.proof_version != CapabilityAuthorityFinalityProof::PROOF_VERSION_V1 {
        return Err(deny("capability authority finality proof version is unsupported").into());
    }
    let expected = CapabilityAuthorityFinalityBinding::from_record(record)
        .map_err(|e| deny(format!("capability authority binding: {e}")))?;
    if proof.binding != expected {
        return Err(
            deny("capability authority finality proof does not match authority record").into(),
        );
    }
    authority_certificate(source, record, &proof.certificate)?;
    source.work(n.saturating_mul(128) + 8192)?;
    let signers: BTreeSet<&String> = proof.certificate.signatures.keys().collect();
    let proof_signers: BTreeSet<&String> = proof.signatures.keys().collect();
    if signers != proof_signers {
        return Err(deny(
            "capability authority finality proof signer set does not match certificate",
        )
        .into());
    }
    for (node, encoded) in &proof.signatures {
        source.work(n.saturating_mul(8) + 8192)?;
        let encoded = encoded
            .strip_prefix(CapabilityAuthorityFinalityProof::SIGNATURE_PREFIX_ED25519_V1)
            .ok_or_else(|| {
                deny(format!(
                    "capability authority proof signature prefix mismatch for {node}"
                ))
            })?;
        let key = source.node(node)?.ok_or_else(|| {
            deny(format!(
                "capability authority proof signer is not trusted: {node}"
            ))
        })?;
        let key: [u8; 32] = hex::decode(key)
            .map_err(|_| {
                deny(format!(
                    "capability authority proof signer key is invalid: {node}"
                ))
            })?
            .try_into()
            .map_err(|_| {
                deny(format!(
                    "capability authority proof signer key length is invalid: {node}"
                ))
            })?;
        let signature: [u8; 64] = hex::decode(encoded)
            .map_err(|_| {
                deny(format!(
                    "capability authority proof signature is invalid: {node}"
                ))
            })?
            .try_into()
            .map_err(|_| {
                deny(format!(
                    "capability authority proof signature length is invalid: {node}"
                ))
            })?;
        let key = VerifyingKey::from_bytes(&key).map_err(|_| {
            deny(format!(
                "capability authority proof signer key is invalid: {node}"
            ))
        })?;
        let payload = proof
            .signing_payload_v1(node)
            .map_err(|e| deny(format!("capability authority proof payload: {e}")))?;
        key.verify(&payload, &Signature::from_bytes(&signature))
            .map_err(|_| {
                deny(format!(
                    "capability authority proof signature failed: {node}"
                ))
            })?;
    }
    Ok(())
}
pub(in crate::world) fn parents<'a, S: FactsV1<'a>>(
    source: &mut S,
    grant: &CapabilityGrantV2,
    tick: u64,
    visited: &mut BTreeSet<String>,
) -> Result<(), S::Error> {
    if grant.delegation_depth == 1 && grant.parent_grant_id.is_none() {
        return Err(deny("delegated grant must carry an explicit parent authorization").into());
    }
    let mut owned = None;
    loop {
        let child = owned.as_ref().unwrap_or(grant);
        let Some(id) = &child.parent_grant_id else {
            return Ok(());
        };
        source.work(id.len().saturating_mul(256) + 8192)?;
        if !visited.insert(id.clone()) {
            return Err(deny("delegated grant parent chain contains a cycle").into());
        }
        let raw = source
            .grant(id)?
            .ok_or_else(|| deny("parent grant is not in the durable registry"))?;
        source.record(raw.value())?;
        let parent: CapabilityGrantV2 = serde::Deserialize::deserialize(raw.value())
            .map_err(|_| deny("parent grant is malformed"))?;
        source.record(&parent)?;
        source.validate_grant(&parent)?;
        if !source.grant_body_matches(&parent)? || !source.grant_id_matches(&parent)? {
            return Err(deny("parent grant canonical body hash or id mismatch").into());
        }
        if parent.expires_at_tick.is_none()
            || parent.expires_at_tick.is_some_and(|t| tick > t)
            || parent.issued_at_tick > tick
        {
            return Err(deny("parent grant lifetime is not currently valid").into());
        }
        issuer(source, &parent)?;
        revocation(source, &parent)?;
        if parent.subject != child.subject
            || parent.audience != child.audience
            || parent.issuer.issuer_id != child.issuer.issuer_id
            || parent.issuer.issuer_kind != child.issuer.issuer_kind
            || parent.issuer.governance_epoch != child.issuer.governance_epoch
            || parent.issuer.finalized_receipt_id != child.issuer.finalized_receipt_id
            || parent.issuer.key_id != child.issuer.key_id
            || parent.issuer.issuer_key_epoch != child.issuer.issuer_key_epoch
            || parent.issuer.authority_rotation_receipt_id
                != child.issuer.authority_rotation_receipt_id
        {
            return Err(deny(
                "delegated grant child issuer, subject, or audience is not parent-bound",
            )
            .into());
        }
        let attenuates = match (parent.expires_at_tick, child.expires_at_tick) {
            (Some(p), Some(c)) => c <= p,
            _ => false,
        };
        if parent.status != "verified"
            || parent.delegation_depth <= child.delegation_depth
            || !attenuates
            || !source.scope_contains(&parent.scope, &child.scope)?
        {
            return Err(deny("delegated grant does not attenuate its parent").into());
        }
        owned = Some(parent);
    }
}

pub(in crate::world) struct Legacy<'a>(pub(in crate::world) &'a crate::world::World);
impl sealed::Sealed for Legacy<'_> {}
impl<'a> FactsV1<'a> for Legacy<'a> {
    type Error = WorldError;
    fn tick(&mut self) -> Result<u64, WorldError> {
        Ok(self.0.state.time)
    }
    fn grant(&mut self, id: &str) -> Result<Option<GrantDataV1<'a>>, WorldError> {
        Ok(self.0.capability_grants_v2.get(id).map(GrantDataV1::Legacy))
    }
    fn grant_exists(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self.0.capability_grants_v2.contains_key(id))
    }
    fn root(&mut self) -> Result<(), WorldError> {
        self.0.verify_capability_authorization_root()
    }
    fn legacy(&mut self, id: &str) -> Result<(), WorldError> {
        let grant = self
            .0
            .capabilities
            .get(id)
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!("module cap missing {id}"),
            })?;
        if grant.is_expired(self.0.state.time) {
            return Err(WorldError::ModuleChangeInvalid {
                reason: format!("module cap expired {id}"),
            });
        }
        Ok(())
    }
    fn authority(&mut self, id: &str) -> Result<Option<AuthorityDataV1<'a>>, WorldError> {
        Ok(self
            .0
            .capability_revocation_state
            .authority_records
            .get(id)
            .map(AuthorityDataV1::Legacy))
    }
    fn proof(&mut self, id: &str) -> Result<Option<ProofDataV1<'a>>, WorldError> {
        Ok(self
            .0
            .capability_revocation_state
            .authority_finality_proofs
            .get(id)
            .map(ProofDataV1::Legacy))
    }
    fn epoch(&mut self) -> Result<u64, WorldError> {
        Ok(self.0.capability_revocation_state.epoch)
    }
    fn revoked(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self
            .0
            .capability_revocation_state
            .revoked_grant_ids
            .contains(id))
    }
    fn superseded(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self
            .0
            .capability_revocation_state
            .superseded_by
            .contains_key(id))
    }
    fn node(&mut self, id: &str) -> Result<Option<String>, WorldError> {
        Ok(self.0.node_identity_public_key(id).map(str::to_owned))
    }
    fn snapshot_exists(&mut self, epoch: u64) -> Result<bool, WorldError> {
        Ok(self
            .0
            .governance_finality_epoch_snapshots
            .contains_key(&epoch))
    }
    fn proposal_hash(&mut self, id: u64) -> Result<Option<Result<String, ()>>, WorldError> {
        Ok(self.0.proposals.get(&id).map(|p| match &p.status {
            ProposalStatus::Approved { manifest_hash, .. }
            | ProposalStatus::Applied { manifest_hash } => Ok(manifest_hash.clone()),
            _ => Err(()),
        }))
    }
    fn certificate(
        &mut self,
        certificate: &GovernanceFinalityCertificate,
    ) -> Result<(), WorldError> {
        self.0
            .validate_governance_finality_certificate(
                certificate.proposal_id,
                &certificate.manifest_hash,
                certificate.epoch_id,
                certificate,
            )
            .map_err(|e| deny(format!("authority finality certificate: {e:?}")))
    }
    fn work(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn record<T: serde::Serialize>(&mut self, _: &T) -> Result<usize, WorldError> {
        Ok(0)
    }
}

#[path = "incremental_module_capability_admission_current.rs"]
mod current;
pub(in crate::world) use current::{
    CurrentModuleGovernanceFinalityErrorV1, ModuleCapabilityAdmissionFeesV1,
    ModuleCapabilityAdmissionSourcesV1, PaidCurrentLocalModuleGovernanceFinalityV1,
    PaidCurrentModuleEpochSnapshotV1, PaidCurrentModuleGovernanceFinalityV1,
    PaidModuleCapabilityAdmissionV1, build_current_local_module_governance_finality_v1,
    read_current_module_epoch_snapshot_v1, validate_current_module_governance_finality_v1,
    validate_current_module_required_capability_v1,
};

pub(in crate::world) use current::{
    BootstrapCapabilityCheckV1, CurrentBootstrapPolicyErrorV1, PaidCurrentBootstrapPolicyV1,
    PaidCurrentBootstrapRegistryV1, read_current_bootstrap_policy_v1,
    read_current_bootstrap_registry_v1, verify_current_bootstrap_capability_v1,
};
