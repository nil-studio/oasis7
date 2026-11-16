//! The original certificate/rotation algorithms over borrowed source DATA.
use super::*;
use crate::{GovernanceValidatorAdmissionStatus,GovernanceValidatorAdmissionRecord};
use crate::governance::{FinalityKeyRotationRecordV1,BootstrapFinalityKeyRotationRecordV1,MAX_FINALITY_KEY_ROTATION_RECORDS_V1,bootstrap_finality_registry_digest_v1,validate_finality_key_rotation_history_capacity};
use oasis7_agent_api::world_action::{FinalityKeyRotationRoleV1,BootstrapFinalityKeyRotationRoleV1};
fn invalid(reason:impl Into<String>)->WorldError{WorldError::GovernancePolicyInvalid{reason:reason.into()}}
pub(in crate::world) struct ModuleFinalityRecordsV1<'a>{
 pub registry:&'a Option<GovernanceFinalitySignerRegistry>,
 pub admissions:&'a BTreeMap<String,GovernanceValidatorAdmissionRecord>,
 pub rotations:&'a BTreeMap<String,FinalityKeyRotationRecordV1>,
 pub bootstrap_rotations:&'a BTreeMap<String,BootstrapFinalityKeyRotationRecordV1>,
 pub epoch_length:u64,
}
impl<'a> ModuleFinalityRecordsV1<'a>{
 pub(in crate::world) fn build_local_certificate_v1(snapshot:&GovernanceFinalityEpochSnapshot,proposal_id:u64,manifest_hash:&str,consensus_height:u64,epoch_id:u64)->Result<GovernanceFinalityCertificate,WorldError>{
    let min_unique_signers=snapshot.effective_min_unique_signers();
    let mut signatures=BTreeMap::new();
    for (node_id,seed_label) in LOCAL_GOVERNANCE_FINALITY_SIGNERS {
        let payload=GovernanceFinalityCertificate::signing_payload_v1(proposal_id,manifest_hash,consensus_height,epoch_id,&snapshot.validator_set_hash,&snapshot.stake_root,snapshot.threshold_bps,min_unique_signers,node_id);
        let signing_key=local_governance_finality_signing_key(seed_label);
        let signature=signing_key.sign(payload.as_slice());
        signatures.insert(node_id.to_string(),format!("{}{}",GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,hex::encode(signature.to_bytes())));
    }
    Ok(GovernanceFinalityCertificate{proposal_id,manifest_hash:manifest_hash.to_owned(),consensus_height,epoch_id,validator_set_hash:snapshot.validator_set_hash.clone(),stake_root:snapshot.stake_root.clone(),threshold_bps:snapshot.threshold_bps,min_unique_signers,threshold:min_unique_signers,signatures})
 }

 pub(in crate::world) fn registry_epoch_snapshot_v1(&self,epoch_id:u64,current_epoch:u64)->Option<GovernanceFinalityEpochSnapshot>{
    let registry=match self.resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(self.admissions,current_epoch){Ok(Some(v))=>v,Ok(None)=>return None,Err(_)=>(*self.registry).clone()?};
    let signer_node_ids:Vec<String>=registry.signer_bindings.keys().cloned().collect();let threshold=registry.threshold;
    let threshold_bps=if registry.threshold_bps>0{registry.threshold_bps}else{governance_threshold_bps(threshold,signer_node_ids.len())};
    Some(GovernanceFinalityEpochSnapshot{epoch_id,threshold_bps,min_unique_signers:threshold,
        validator_set_hash:governance_finality_validator_set_hash(&signer_node_ids),stake_root:governance_finality_stake_root(&signer_node_ids,&registry.validator_stakes),threshold,signer_node_ids,validator_stakes:registry.validator_stakes.clone()})
 }
 pub(in crate::world) fn fallback_epoch_snapshot_v1(&self,epoch_id:u64,current_epoch:u64)->GovernanceFinalityEpochSnapshot{
    if let Some(snapshot)=self.registry_epoch_snapshot_v1(epoch_id,current_epoch){return snapshot}
    let signer_node_ids:Vec<String>=LOCAL_GOVERNANCE_FINALITY_SIGNERS.iter().map(|(id,_)|(*id).to_owned()).collect();
    let min_unique_signers=LOCAL_GOVERNANCE_FINALITY_SIGNERS.len()as u16;
    GovernanceFinalityEpochSnapshot{epoch_id,threshold_bps:10000,min_unique_signers,
        validator_set_hash:governance_finality_validator_set_hash(&signer_node_ids),stake_root:governance_finality_stake_root(&signer_node_ids,&BTreeMap::new()),threshold:min_unique_signers,signer_node_ids,validator_stakes:BTreeMap::new()}
 }
 pub(in crate::world) fn from_world(world:&'a World)->Self{Self{
 registry:&world.state.governance_finality_signer_registry,admissions:&world.state.governance_validator_admissions,
 rotations:&world.state.governance_finality_key_rotations,bootstrap_rotations:&world.state.governance_bootstrap_finality_key_rotations,
 epoch_length:world.governance_execution_policy().epoch_length_ticks}}
    pub(in crate::world) fn resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
        &self,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
        effective_epoch: u64,
    ) -> Result<Option<GovernanceFinalitySignerRegistry>, WorldError> {
        let Some(mut registry) = (*self.registry).clone() else {
            if !self.bootstrap_rotations
                .is_empty()
                || !self.rotations.is_empty()
            {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: "rotation source registry missing".into(),
                });
            }
            return Ok(None);
        };
        // Administrative progress must not shadow the last effective authority
        // decision. In particular, a newer application cannot remove a prior
        // revocation and thereby restore a bootstrap binding from the base.
        let authority_records =
            World::governance_validator_authority_records_at_epoch(admissions, effective_epoch);
        for record in authority_records.values() {
            match record.status {
                GovernanceValidatorAdmissionStatus::Revoked => {
                    registry.signer_bindings.remove(record.node_id.as_str());
                    registry.validator_stakes.remove(record.node_id.as_str());
                }
                GovernanceValidatorAdmissionStatus::ProbationReady
                | GovernanceValidatorAdmissionStatus::Active => {
                    let activation_epoch = record.activation_epoch.unwrap_or(effective_epoch);
                    if activation_epoch <= effective_epoch {
                        registry.signer_bindings.insert(
                            record.node_id.clone(),
                            record.finality_signer_public_key.clone(),
                        );
                        registry
                            .validator_stakes
                            .insert(record.node_id.clone(), record.stake);
                    }
                }
                GovernanceValidatorAdmissionStatus::Applied
                | GovernanceValidatorAdmissionStatus::ApprovedCandidate => {}
            }
        }
        self.project_finality_key_rotations(
            &mut registry,
            admissions,
            &authority_records,
            effective_epoch,
        )?;
        self.project_bootstrap_finality_key_rotations(
            &mut registry,
            &authority_records,
            effective_epoch,
        )?;
        self.validate_governance_finality_signer_registry(registry)
            .map(Some)
    }

    pub(in crate::world) fn project_finality_key_rotations(
        &self,
        registry: &mut GovernanceFinalitySignerRegistry,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
        selected: &BTreeMap<String, &GovernanceValidatorAdmissionRecord>,
        epoch: u64,
    ) -> Result<(), WorldError> {
        self.validate_retained_rotation_namespace(admissions)?;
        if self.rotations.len() > MAX_FINALITY_KEY_ROTATION_RECORDS_V1
        {
            return Err(invalid("rotation history capacity exceeded"));
        }
        let mut groups: BTreeMap<&str, Vec<&FinalityKeyRotationRecordV1>> = BTreeMap::new();
        for (id, record) in self.rotations {
            record.validate().map_err(invalid)?;
            let body = record.body().map_err(invalid)?;
            if id != &body.rotation_id {
                return Err(invalid("rotation record identity mismatch"));
            }
            groups.entry(&body.candidate_id).or_default().push(record);
        }
        for (candidate_id, mut records) in groups {
            let admission = admissions
                .get(candidate_id)
                .ok_or_else(|| invalid("rotation original admission missing"))?;
            records.sort_by_key(|r| r.body().map(|b| b.expected_generation).unwrap_or(u64::MAX));
            let mut generation = 0_u64;
            let mut key = admission.finality_signer_public_key.clone();
            let mut effective_key = key.clone();
            let mut previous_activation = None;
            for record in records {
                let (body, old, new) = record.typed_parts().map_err(invalid)?;
                if body.node_id != admission.node_id
                    || body.expected_generation != generation
                    || body.old_public_key_hex != key
                    || previous_activation.is_some_and(|height| body.decision_height < height)
                    || body
                        .target_epoch
                        .checked_mul(self.epoch_length)
                        != Some(body.activation_height)
                {
                    return Err(invalid(
                        "rotation history key, generation or boundary discontinuity",
                    ));
                }
                crate::world::governance_finality_key_rotation::verify_key_proof(body, old, FinalityKeyRotationRoleV1::OldKeyAuthorization)?;
                crate::world::governance_finality_key_rotation::verify_key_proof(body, new, FinalityKeyRotationRoleV1::NewKeyPossession)?;
                key = body.new_public_key_hex.clone();
                generation = generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("rotation generation exhausted"))?;
                previous_activation = Some(body.activation_height);
                if body.target_epoch <= epoch {
                    effective_key = key.clone();
                }
            }
            if selected
                .get(&admission.node_id)
                .map(|a| a.candidate_id.as_str())
                == Some(candidate_id)
                && registry.signer_bindings.contains_key(&admission.node_id)
                && matches!(
                    admission.status,
                    GovernanceValidatorAdmissionStatus::Active
                        | GovernanceValidatorAdmissionStatus::ProbationReady
                )
                && admission
                    .activation_epoch
                    .is_some_and(|height| height <= epoch)
            {
                registry
                    .signer_bindings
                    .insert(admission.node_id.clone(), effective_key);
            }
        }
        Ok(())
    }

    pub(in crate::world) fn project_bootstrap_finality_key_rotations(
        &self,
        registry: &mut GovernanceFinalitySignerRegistry,
        authority: &BTreeMap<String, &GovernanceValidatorAdmissionRecord>,
        epoch: u64,
    ) -> Result<(), WorldError> {
        validate_finality_key_rotation_history_capacity(
            self.rotations.len(),
            self.bootstrap_rotations.len(),
        )
        .map_err(invalid)?;
        if self.bootstrap_rotations
            .is_empty()
        {
            return Ok(());
        }
        let raw = self.registry
            .as_ref()
            .ok_or_else(|| invalid("bootstrap rotation source registry missing"))?;
        let source_digest = bootstrap_finality_registry_digest_v1(raw).map_err(invalid)?;
        let mut groups: BTreeMap<&str, Vec<&BootstrapFinalityKeyRotationRecordV1>> =
            BTreeMap::new();
        for (id, record) in self.bootstrap_rotations {
            record.validate().map_err(invalid)?;
            let body = record.body().map_err(invalid)?;
            if id != &body.rotation_id
                || self.rotations
                    .contains_key(id)
                || body.bootstrap_registry_digest_hex != source_digest
                || raw.signer_bindings.get(&body.node_id) != Some(&body.bootstrap_public_key_hex)
            {
                return Err(invalid(
                    "bootstrap rotation original source or identity mismatch",
                ));
            }
            groups.entry(&body.node_id).or_default().push(record);
        }
        for (node, mut records) in groups {
            records.sort_by_key(|r| r.body().map(|b| b.expected_generation).unwrap_or(u64::MAX));
            let mut key = raw
                .signer_bindings
                .get(node)
                .cloned()
                .ok_or_else(|| invalid("bootstrap rotation source node missing"))?;
            let mut effective = key.clone();
            let mut generation = 0_u64;
            let mut previous_activation = None;
            for record in records {
                let (body, old, new) = record.typed_parts().map_err(invalid)?;
                if body.expected_generation != generation
                    || body.old_public_key_hex != key
                    || previous_activation.is_some_and(|height| body.decision_height < height)
                    || body
                        .target_epoch
                        .checked_mul(self.epoch_length)
                        != Some(body.activation_height)
                {
                    return Err(invalid(
                        "bootstrap rotation history generation, key or boundary discontinuity",
                    ));
                }
                crate::world::governance_bootstrap_finality_key_rotation::verify_key_proof(
                    body,
                    old,
                    BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
                )?;
                crate::world::governance_bootstrap_finality_key_rotation::verify_key_proof(
                    body,
                    new,
                    BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
                )?;
                key = body.new_public_key_hex.clone();
                generation = generation
                    .checked_add(1)
                    .ok_or_else(|| invalid("bootstrap generation exhausted"))?;
                previous_activation = Some(body.activation_height);
                if body.target_epoch <= epoch {
                    effective = key.clone();
                }
            }
            // Effective authority records, rather than equality of keys, own
            // the source. A revocation or same-key Admission cannot be overlaid.
            if !authority.contains_key(node)
                && let Some(binding) = registry.signer_bindings.get_mut(node)
            {
                *binding = effective;
            }
        }
        Ok(())
    }

    pub(in crate::world) fn validate_retained_rotation_namespace(
        &self,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    ) -> Result<(), WorldError> {
        validate_finality_key_rotation_history_capacity(
            self.rotations.len(),
            self.bootstrap_rotations.len(),
        )
        .map_err(invalid)?;
        let mut keys = std::collections::BTreeSet::new();
        if let Some(raw) = self.registry {
            keys.extend(raw.signer_bindings.values().cloned());
        }
        for record in self.rotations.values() {
            let body = record.body().map_err(invalid)?;
            let origin = admissions
                .get(&body.candidate_id)
                .ok_or_else(|| invalid("rotation original admission missing"))?;
            if body.new_public_key_hex == origin.finality_signer_public_key
                || !keys.insert(body.new_public_key_hex.clone())
            {
                return Err(invalid(
                    "retained rotation reuses its origin or a historical new key",
                ));
            }
        }
        for (id, record) in self.bootstrap_rotations {
            let body = record.body().map_err(invalid)?;
            if self.rotations
                .contains_key(id)
                || !keys.insert(body.new_public_key_hex.clone())
            {
                return Err(invalid(
                    "retained bootstrap rotation reuses a rotation ID or key",
                ));
            }
        }
        Ok(())
    }

    pub(in crate::world) fn validate_governance_finality_signer_registry(
        &self,
        mut registry: GovernanceFinalitySignerRegistry,
    ) -> Result<GovernanceFinalitySignerRegistry, WorldError> {
        registry.slot_id = registry.slot_id.trim().to_string();
        if registry.slot_id.is_empty() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "finality signer registry slot_id cannot be empty".to_string(),
            });
        }
        if registry.threshold == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality signer registry threshold must be > 0 slot_id={}",
                    registry.slot_id
                ),
            });
        }
        let mut normalized_bindings = BTreeMap::new();
        let mut unique_public_keys = BTreeSet::new();
        for (node_id, public_key_hex) in registry.signer_bindings {
            let node_id = node_id.trim().to_string();
            if node_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry node_id cannot be empty slot_id={}",
                        registry.slot_id
                    ),
                });
            }
            let public_key_hex = public_key_hex.trim().to_string();
            decode_hex_array::<32>(
                public_key_hex.as_str(),
                format!(
                    "finality signer public key slot_id={} node_id={node_id}",
                    registry.slot_id
                )
                .as_str(),
            )?;
            if !unique_public_keys.insert(public_key_hex.clone()) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry public key must be unique slot_id={} public_key={}",
                        registry.slot_id, public_key_hex
                    ),
                });
            }
            normalized_bindings.insert(node_id, public_key_hex);
        }
        let mut normalized_stakes = BTreeMap::new();
        for (node_id, stake) in registry.validator_stakes {
            let node_id = node_id.trim().to_string();
            if node_id.is_empty() {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry stake node_id cannot be empty slot_id={}",
                        registry.slot_id
                    ),
                });
            }
            if !normalized_bindings.contains_key(node_id.as_str()) {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry stake references unknown signer slot_id={} node_id={}",
                        registry.slot_id, node_id
                    ),
                });
            }
            if stake == 0 {
                return Err(WorldError::GovernancePolicyInvalid {
                    reason: format!(
                        "finality signer registry stake must be > 0 slot_id={} node_id={}",
                        registry.slot_id, node_id
                    ),
                });
            }
            normalized_stakes.insert(node_id, stake);
        }
        for node_id in normalized_bindings.keys() {
            normalized_stakes
                .entry(node_id.clone())
                .or_insert(DEFAULT_GOVERNANCE_VALIDATOR_STAKE);
        }
        if normalized_bindings.len() < usize::from(registry.threshold) {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality signer registry threshold exceeds signer count slot_id={} threshold={} signers={}",
                    registry.slot_id,
                    registry.threshold,
                    normalized_bindings.len()
                ),
            });
        }
        if registry.threshold_bps == 0 {
            registry.threshold_bps =
                governance_threshold_bps(registry.threshold, normalized_bindings.len());
        }
        if registry.threshold_bps == 0 || registry.threshold_bps > 10_000 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "finality signer registry threshold_bps must be within 1..=10000 slot_id={} threshold_bps={}",
                    registry.slot_id, registry.threshold_bps
                ),
            });
        }
        registry.signer_bindings = normalized_bindings;
        registry.validator_stakes = normalized_stakes;
        Ok(registry)
    }

    pub(in crate::world) fn validate_certificate_header_v1(
        snapshot: &GovernanceFinalityEpochSnapshot,
        proposal_id: ProposalId,
        manifest_hash: &str,
        epoch_id: u64,
        certificate: &GovernanceFinalityCertificate,
    ) -> Result<(), WorldError> {
        let snapshot_min_unique_signers = snapshot.effective_min_unique_signers();
        if certificate.proposal_id != proposal_id {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "proposal_id mismatch: expected={} found={}",
                    proposal_id, certificate.proposal_id
                ),
            });
        }
        if certificate.manifest_hash != manifest_hash {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "manifest_hash mismatch".to_string(),
            });
        }
        if certificate.consensus_height == 0 {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "consensus_height must be > 0".to_string(),
            });
        }
        if certificate.epoch_id != epoch_id {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "epoch_id mismatch: expected={} found={}",
                    epoch_id, certificate.epoch_id
                ),
            });
        }
        if certificate.validator_set_hash.trim().is_empty() {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "validator_set_hash must be non-empty".to_string(),
            });
        }
        if certificate.validator_set_hash != snapshot.validator_set_hash {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "validator_set_hash mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate.validator_set_hash, snapshot.validator_set_hash
                ),
            });
        }
        if certificate.stake_root.trim().is_empty() {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "stake_root must be non-empty".to_string(),
            });
        }
        if certificate.stake_root != snapshot.stake_root {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "stake_root mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate.stake_root, snapshot.stake_root
                ),
            });
        }
        if certificate.threshold_bps == 0 || certificate.threshold_bps > 10_000 {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "threshold_bps must be within 1..=10000: {}",
                    certificate.threshold_bps
                ),
            });
        }
        if certificate.threshold_bps != snapshot.threshold_bps {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "threshold_bps mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate.threshold_bps, snapshot.threshold_bps
                ),
            });
        }
        let certificate_min_unique_signers = certificate.effective_min_unique_signers();
        if certificate_min_unique_signers == 0 {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: "min_unique_signers must be > 0".to_string(),
            });
        }
        if certificate_min_unique_signers != snapshot_min_unique_signers {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "min_unique_signers mismatch: epoch_id={} certificate={} snapshot={}",
                    epoch_id, certificate_min_unique_signers, snapshot_min_unique_signers
                ),
            });
        }
        if certificate.signatures.len() < certificate_min_unique_signers as usize {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "signatures below min_unique_signers: signatures={} min_unique_signers={}",
                    certificate.signatures.len(),
                    certificate_min_unique_signers
                ),
            });
        }
        Ok(())
    }
    fn validate_certificate_inner_v1(
        &self,
        snapshot: &GovernanceFinalityEpochSnapshot,
        proposal_id: ProposalId,
        manifest_hash: &str,
        epoch_id: u64,
        certificate: &GovernanceFinalityCertificate,
        mut node:impl FnMut(&str)->Option<String>,
    ) -> Result<(), WorldError> {
        Self::validate_certificate_header_v1(snapshot,proposal_id,manifest_hash,epoch_id,certificate)?;
        let snapshot_min_unique_signers=snapshot.effective_min_unique_signers();
        let certificate_min_unique_signers=certificate.effective_min_unique_signers();
        let epoch_signers:BTreeSet<String>=snapshot.signer_node_ids.iter().cloned().collect();
        // Finality authority follows the certificate epoch and its retained
        // admission/rotation history. Node identity remains the service binding.
        let mut historical_admissions = (*self.admissions).clone();
        for admission in historical_admissions.values_mut() {
            if admission.status == GovernanceValidatorAdmissionStatus::Revoked {
                let revoked = admission.revoked_at_epoch.ok_or_else(|| {
                    WorldError::GovernanceFinalityInvalid {
                        reason: format!(
                            "historical admission {} lacks revocation epoch",
                            admission.candidate_id
                        ),
                    }
                })?;
                if epoch_id < revoked {
                    // A retained revoked record still carries its original key,
                    // stake and activation boundary. Restore only authority that
                    // was already active at the certificate epoch.
                    admission.status = if admission
                        .activation_epoch
                        .is_some_and(|epoch| epoch <= epoch_id)
                    {
                        GovernanceValidatorAdmissionStatus::Active
                    } else {
                        GovernanceValidatorAdmissionStatus::Applied
                    };
                }
            }
        }
        let epoch_registry = self
            .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                &historical_admissions,
                epoch_id,
            )?;
        for (node_id, signature_with_prefix) in &certificate.signatures {
            if !epoch_signers.contains(node_id) {
                return Err(WorldError::GovernanceFinalityInvalid {
                    reason: format!(
                        "signer node_id={} is not part of finality epoch snapshot epoch_id={}",
                        node_id, epoch_id
                    ),
                });
            }
            let fallback;
            let signer_public_key = match &epoch_registry {
                Some(registry) => registry.signer_bindings.get(node_id).map(String::as_str),
                // Preserve the explicit legacy local-signer contract when no
                // registry is configured; never fall back for a missing member.
                None => {fallback=node(node_id);fallback.as_deref()},
            }
            .ok_or_else(|| WorldError::GovernanceFinalityInvalid {
                reason: format!("untrusted finality signer node_id: {node_id} epoch: {epoch_id}"),
            })?;
            let signature_hex = signature_with_prefix
                .strip_prefix(GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1)
                .ok_or_else(|| WorldError::GovernanceFinalityInvalid {
                    reason: format!("signature prefix mismatch for signer {node_id}"),
                })?;
            let payload = GovernanceFinalityCertificate::signing_payload_v1(
                certificate.proposal_id,
                certificate.manifest_hash.as_str(),
                certificate.consensus_height,
                certificate.epoch_id,
                certificate.validator_set_hash.as_str(),
                certificate.stake_root.as_str(),
                certificate.threshold_bps,
                certificate_min_unique_signers,
                node_id.as_str(),
            );
            let public_key_bytes =
                decode_hex_array::<32>(signer_public_key, "governance finality signer public key")?;
            let signature_bytes =
                decode_hex_array::<64>(signature_hex, "governance finality signature")?;
            let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|_| {
                WorldError::GovernanceFinalityInvalid {
                    reason: format!("invalid signer public key for {node_id}"),
                }
            })?;
            let signature = Signature::from_bytes(&signature_bytes);
            verifying_key
                .verify(payload.as_slice(), &signature)
                .map_err(|error| WorldError::GovernanceFinalityInvalid {
                    reason: format!("signature verification failed for {node_id}: {error}"),
                })?;
        }
        let signed_signer_node_ids = certificate.signatures.keys().cloned().collect();
        let signed_stake_bps = governance_finality_signed_stake_bps(
            snapshot.signer_node_ids.as_slice(),
            &snapshot.validator_stakes,
            &signed_signer_node_ids,
        );
        if signed_stake_bps < certificate.threshold_bps {
            return Err(WorldError::GovernanceFinalityInvalid {
                reason: format!(
                    "signed stake below threshold_bps: epoch_id={} signed_stake_bps={} threshold_bps={}",
                    epoch_id, signed_stake_bps, certificate.threshold_bps
                ),
            });
        }
        Ok(())
        }
    pub(in crate::world) fn validate_governance_finality_certificate(
        &self,
        snapshot: &GovernanceFinalityEpochSnapshot,
        proposal_id: ProposalId,
        manifest_hash: &str,
        epoch_id: u64,
        certificate: &GovernanceFinalityCertificate,
        node_keys:&BTreeMap<String,String>,
    ) -> Result<(), WorldError> {
        self.validate_certificate_inner_v1(snapshot,proposal_id,manifest_hash,epoch_id,certificate,|id|node_keys.get(id).cloned())
    }
    pub(in crate::world) fn validate_certificate_from_source_v1<'s,S:crate::world::incremental_module_capability_admission_rules::FactsV1<'s>>(
        &self,snapshot:&GovernanceFinalityEpochSnapshot,certificate:&GovernanceFinalityCertificate,source:&mut S,
    )->Result<(),S::Error>{
        self.validate_certificate_from_source_bound_v1(snapshot,certificate.proposal_id,&certificate.manifest_hash,certificate.epoch_id,certificate,source)
    }
    pub(in crate::world) fn validate_certificate_from_source_bound_v1<'s,S:crate::world::incremental_module_capability_admission_rules::FactsV1<'s>>(
        &self,snapshot:&GovernanceFinalityEpochSnapshot,proposal_id:u64,approved_hash:&str,epoch:u64,certificate:&GovernanceFinalityCertificate,source:&mut S,
    )->Result<(),S::Error>{
        let mut first=None;
        let result=self.validate_certificate_inner_v1(snapshot,proposal_id,approved_hash,epoch,certificate,|id|{
            match source.node(id){Ok(value)=>value,Err(error)=>{first=Some(error);None}}
        });
        if let Some(error)=first{return Err(error)}
        result.map_err(Into::into)
    }

}
