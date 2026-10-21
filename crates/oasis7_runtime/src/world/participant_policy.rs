//! Proof-bearing administration; issuer attests external completeness, quorum authorizes trust.
use super::{World,WorldError};
use crate::{CapabilityAuthorizationEvent,capability_authorization::CapabilityRevocationState};
use crate::service_admission::{participant::*,participant_policy::*,request_digest};
use oasis7_client_api::world_service::private_goal::verify_owner_signature_strict;
use std::collections::BTreeSet;
fn error(s:impl Into<String>)->WorldError {WorldError::DistributedValidationFailed {reason:s.into()}}
impl World {
    pub fn participant_registry_digest(&self)->Result<String,String> { request_digest("participant-registry",&self.capability_revocation_state.participants) }
    /// Pins every legacy control source, including unbound Agents, without inferring external accounts.
    pub fn participant_legacy_owner_state_digest(&self)->Result<String,String> {
        request_digest("participant-legacy-owner-state",&(&self.state.agents.keys().collect::<Vec<_>>(),&self.state.starter_oc_claims,
            &self.state.agent_claims,&self.capability_revocation_state.agent_identities,&self.state.world_rules))
    }
    pub fn install_participant_policy_with_finality_proof(&mut self,policy:ParticipantEntryPolicyV1,proof:ParticipantPolicyProofV1)->Result<(),WorldError> {
        self.verify_capability_authorization_root()?;
        self.append_capability_authorization_event_batch(vec![CapabilityAuthorizationEvent::ParticipantPolicyInstalled {policy,proof:Some(proof)}])?;
        Ok(())
    }
    pub(crate) fn validate_participant_policy_proof(&self,policy:&ParticipantEntryPolicyV1,proof:&ParticipantPolicyProofV1,install:bool)->Result<(),WorldError> {
        let c=&proof.certificate;
        if !self.governance_finality_epoch_snapshots.contains_key(&c.epoch_id) {return Err(error("participant policy historical finality snapshot missing"));}
        let proposal=self.proposals.get(&c.proposal_id).ok_or_else(||error("participant policy finality proposal missing"))?;
        let hash=match &proposal.status {
            crate::governance::ProposalStatus::Approved {manifest_hash,..}|crate::governance::ProposalStatus::Applied {manifest_hash}=>manifest_hash,
            _=>return Err(error("participant policy proposal is not approved"))
        };
        if crate::util::hash_json(&proposal.manifest)? != *hash {
            return Err(error("participant policy approved proposal manifest hash mismatch"));
        }
        self.validate_governance_finality_certificate(c.proposal_id,hash,c.epoch_id,c)?;
        if proposal.manifest.content.get("participant_policy_authorization") != Some(&policy_proposal_commitment(policy,&proof.manifest).map_err(error)?) {
            return Err(error("participant policy proof is not committed by its approved proposal"));
        }
        let signers:BTreeSet<_>=proof.signatures.keys().collect();
        if signers != c.signatures.keys().collect() {return Err(error("participant policy proof signer set mismatch"));}
        let domain=if install {PARTICIPANT_POLICY_INSTALL_DOMAIN} else {PARTICIPANT_POLICY_UPDATE_DOMAIN};
        for (node,sig) in &proof.signatures {
            let public=self.node_identity_public_key(node).ok_or_else(||error("participant policy signer unavailable"))?;
            verify_owner_signature_strict(domain,&policy_signing_payload(policy,proof),public,sig).map_err(error)?;
        }
        self.validate_participant_manifest(policy, &proof.manifest)
    }
    pub(crate) fn validate_participant_manifest(&self,policy:&ParticipantEntryPolicyV1,manifest:&crate::service_admission::SignedReadRequest<ParticipantMigrationManifestV1>)->Result<(),WorldError> {
        if manifest.subject_public_key != policy.issuer_public_key {return Err(error("participant migration issuer differs from policy"));}
        verify_owner_signature_strict(PARTICIPANT_MIGRATION_MANIFEST_DOMAIN,&manifest.request,&manifest.subject_public_key,&manifest.signature_hex).map_err(error)?;
        let m=&manifest.request;
        if policy.api_rev != 1 || policy.generation == 0 || !policy.registry_complete
            || policy.max_grant_ticks == 0 || policy.max_grant_ticks > 256 || policy.max_participants == 0
            || policy.identity_namespace.instance_epoch.0 == 0 || policy.identity_namespace.service_id.trim().is_empty()
            || policy.identity_namespace.environment_id.trim().is_empty() || policy.world_id != self.chain_resource_manifest.world_id
            || self.chain_resource_manifest.genesis_ref.as_ref() != Some(&policy.genesis_digest)
            || m.api_rev != 1 || !m.complete || m.identity_namespace != policy.identity_namespace
            || m.world_id != policy.world_id || m.genesis_digest != policy.genesis_digest
            || m.policy_digest != policy_digest(policy).map_err(error)? || m.source_snapshot_epoch == 0
            || m.source_watermark.trim().is_empty() || m.source_watermark.len()>256
            || !m.source_snapshot_digest.strip_prefix("blake3:").is_some_and(|h|h.len()==64 && h.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
            || m.entries.len()>4096 {
            return Err(error("participant migration completeness or policy evidence invalid"));
        }
        Ok(())
    }
    pub(crate) fn validate_participant_policy_history(&self) -> Result<(), WorldError> {
        let registry = &self.capability_revocation_state.participants;
        if self.devlocal_participant_authority.is_some() {
            self.validate_devlocal_participant_context()?;
        }
        if registry.policy.is_some()
            && registry.policy_history.is_empty()
            && registry.devlocal_policy_history.is_empty()
            && !cfg!(test)
        {
            return Err(error("participant policy checkpoint has no finality proof"));
        }
        if !registry.devlocal_policy_history.is_empty() && !registry.policy_history.is_empty() {
            return Err(error("participant authority sources cannot mix"));
        }
        let history: Vec<(
            &ParticipantEntryPolicyV1,
            &crate::service_admission::SignedReadRequest<ParticipantMigrationManifestV1>,
        )> = if !registry.devlocal_policy_history.is_empty() {
            registry
                .devlocal_policy_history
                .iter()
                .map(|signed| {
                    self.validate_devlocal_participant_authorization(
                        &signed.request.policy,
                        signed,
                    )?;
                    Ok((&signed.request.policy, &signed.request.manifest))
                })
                .collect::<Result<_, WorldError>>()?
        } else {
            registry
                .policy_history
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    self.validate_participant_policy_proof(&a.policy, &a.proof, i == 0)?;
                    Ok((&a.policy, &a.proof.manifest))
                })
                .collect::<Result<_, WorldError>>()?
        };
        for (i, (policy, manifest)) in history.iter().enumerate() {
            if policy.generation != i as u64 + 1 {
                return Err(error("participant policy history generation gap"));
            }
            if i == 0
                && manifest.request.previous_registry_digest
                    != request_digest("participant-registry", &ParticipantRegistryV1::default())
                        .map_err(error)?
            {
                return Err(error(
                    "participant initial policy history has no empty registry predecessor",
                ));
            }
            if i > 0 {
                let (previous_policy, previous_manifest) = history[i - 1];
                if policy.world_id != previous_policy.world_id
                    || policy.genesis_digest != previous_policy.genesis_digest
                    || !policy
                        .identity_namespace
                        .same_authority(&previous_policy.identity_namespace)
                    || policy.identity_namespace.instance_epoch.0
                        < previous_policy.identity_namespace.instance_epoch.0
                    || !manifest.request.entries.is_empty()
                    || manifest.request.source_snapshot_epoch
                        < previous_manifest.request.source_snapshot_epoch
                {
                    return Err(error(
                        "participant policy history changes namespace or repeats migration",
                    ));
                }
            }
            for entry in &manifest.request.entries {
                verify_owner_signature_strict(
                    PARTICIPANT_MIGRATION_OWNER_DOMAIN,
                    &entry.request,
                    &entry.subject_public_key,
                    &entry.signature_hex,
                )
                .map_err(error)?;
            }
        }
        if let Some((last, _)) = history.last()
            && registry.policy.as_ref() != Some(*last)
        {
            return Err(error(
                "participant policy checkpoint does not match authorized history",
            ));
        }
        for (key, mapping) in &registry.migrated_bindings {
            let receipt = registry
                .bindings
                .get(key)
                .ok_or_else(|| error("participant imported binding missing"))?;
            let expected = migration_receipt(mapping, receipt.registered_tick.0).map_err(error)?;
            if &expected != receipt
                || !history
                    .iter()
                    .any(|(_, m)| m.request.entries.iter().any(|e| &e.request == mapping))
            {
                return Err(error(
                    "participant imported binding has no approved manifest",
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn project_policy(
    world: &World,
    registry: &mut CapabilityRevocationState,
    policy: &ParticipantEntryPolicyV1,
    proof: &ParticipantPolicyProofV1,
) -> Result<(), WorldError> {
    if !registry.participants.devlocal_policy_history.is_empty() {
        return Err(error("local participant authority cannot become governed"));
    }
    let install = registry.participants.policy.is_none();
    world.validate_participant_policy_proof(policy, proof, install)?;
    project_authorized_policy(world, registry, policy, &proof.manifest)?;
    registry
        .participants
        .policy_history
        .push(ParticipantPolicyAuthorizationV1 {
            policy: policy.clone(),
            proof: proof.clone(),
        });
    Ok(())
}
pub(super) fn project_authorized_policy(
    world: &World,
    registry: &mut CapabilityRevocationState,
    policy: &ParticipantEntryPolicyV1,
    manifest: &crate::service_admission::SignedReadRequest<ParticipantMigrationManifestV1>,
) -> Result<(), WorldError> {
    let binding = world.current_cognition_runtime_binding()?;
    let m = &manifest.request;
    let mut projected = world.clone();
    projected.capability_revocation_state = registry.clone();
    if m.previous_registry_digest != projected.participant_registry_digest().map_err(error)?
        || m.legacy_owner_state_digest
            != world
                .participant_legacy_owner_state_digest()
                .map_err(error)?
        || m.branch_id != binding.branch_id
        || m.reorg_epoch != binding.reorg_epoch
    {
        return Err(error(
            "participant policy migration predecessor/history mismatch",
        ));
    }
    if let Some(old) = &registry.participants.policy {
        if policy.generation
            != old
                .generation
                .checked_add(1)
                .ok_or_else(|| error("participant policy generation overflow"))?
            || policy.world_id != old.world_id
            || policy.genesis_digest != old.genesis_digest
            || !policy
                .identity_namespace
                .same_authority(&old.identity_namespace)
            || policy.identity_namespace.instance_epoch.0 < old.identity_namespace.instance_epoch.0
            || !m.entries.is_empty()
            || policy.max_participants < registry.participants.bindings.len() as u64
        {
            return Err(error(
                "participant policy update cannot reset namespace, migrate again or erase bindings",
            ));
        }
        let old_manifest = registry
            .participants
            .policy_history
            .last()
            .map(|a| &a.proof.manifest)
            .or_else(|| {
                registry
                    .participants
                    .devlocal_policy_history
                    .last()
                    .map(|a| &a.request.manifest)
            })
            .ok_or_else(|| error("fixture policy cannot become production authority"))?;
        if m.source_snapshot_epoch < old_manifest.request.source_snapshot_epoch {
            return Err(error("participant migration snapshot epoch regressed"));
        }
    } else if policy.generation != 1 || !registry.participants.is_empty() {
        return Err(error("participant initial policy registry conflict"));
    }
    let mut accounts=BTreeSet::new();let mut agents=BTreeSet::new();
    let mut imports=Vec::new();
    for signed in &m.entries {
        let entry=&signed.request;let r=&entry.registration;r.validate().map_err(error)?;
        verify_owner_signature_strict(PARTICIPANT_MIGRATION_OWNER_DOMAIN,entry,&signed.subject_public_key,&signed.signature_hex).map_err(error)?;
        let key=account_key(r).map_err(error)?;let op=operation_key(r).map_err(error)?;
        let owner=world.current_agent_owner(&entry.agent_id).ok_or_else(||error("participant migration current owner unavailable"))?;
        let identity=world.current_agent_identity(&entry.agent_id).ok_or_else(||error("participant migration current identity unavailable"))?;
        if r.world_id != policy.world_id || r.genesis_digest != policy.genesis_digest || r.identity_namespace != policy.identity_namespace
            || signed.subject_public_key != r.owner_public_key || owner.public_key.as_ref()!=Some(&r.owner_public_key)
            || owner.player_id != entry.owner_binding || identity.owner_binding != entry.owner_binding || identity.generation != entry.owner_generation
            || world.state.is_manufactured_agent(&entry.agent_id)
            || world.state.agent_claims.get(&entry.agent_id).is_some_and(|claim|claim.claim_owner_id!=entry.agent_id || claim.control_source.is_some())
            || !accounts.insert(key.clone()) || !agents.insert(entry.agent_id.clone())
            || registry.participants.bindings.contains_key(&key) || registry.participants.operations.contains_key(&op)
            || registry.participants.bindings.values().any(|b|b.agent_id==entry.agent_id) {
            return Err(error("participant migration duplicate, owner or scope conflict"));
        }
        let receipt=migration_receipt(entry,world.state.time).map_err(error)?;
        imports.push((key,op,entry.clone(),receipt));
    }
    if registry.participants.bindings.len()+imports.len()>policy.max_participants as usize {return Err(error("participant migrated registry exceeds quota"));}
    registry.participants.policy=Some(policy.clone());
    for (key,op,entry,receipt) in imports {
        registry.participants.migrated_bindings.insert(key.clone(),entry.clone());
        registry.participants.bindings.insert(key,receipt.clone());
        registry.participants.operations.insert(op,ParticipantOperationV1 {request:entry.registration,receipt});
    }
    Ok(())
}
