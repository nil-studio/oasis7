//! Normal admitted-validator key rotation. Authority is pinned before the batch;
//! the resulting record never rewrites service identity or its original admission.
use super::governance_bft_parent::{BftGovernanceSchedule, FormalBftHandoffBinding};
use super::*;
use crate::{Action, GovernanceValidatorAdmissionStatus};
use crate::governance::{FinalityKeyRotationRecordV1, MAX_FINALITY_KEY_ROTATION_RECORDS_V1};
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_agent_api::world_action::{
    FinalityKeyRotationBodyV1, FinalityKeyRotationProofV1, FinalityKeyRotationRoleV1,
    GovernanceAuthorizationV1, GovernanceAuthorizedActionV1,
};

#[derive(Debug, Clone)]
pub(super) struct FinalityKeyRotationParentV1 {
    registry: Option<GovernanceFinalitySignerRegistry>,
    admissions: BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    rotations: BTreeMap<String, FinalityKeyRotationRecordV1>,
    epoch: u64,
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::GovernancePolicyInvalid { reason: reason.into() }
}

fn verify_key_proof(
    body: &FinalityKeyRotationBodyV1,
    proof: &FinalityKeyRotationProofV1,
    role: FinalityKeyRotationRoleV1,
) -> Result<(), WorldError> {
    proof.validate_for(body, role).map_err(invalid)?;
    let public = match role {
        FinalityKeyRotationRoleV1::OldKeyAuthorization => &body.old_public_key_hex,
        FinalityKeyRotationRoleV1::NewKeyPossession => &body.new_public_key_hex,
    };
    let bytes: [u8; 32] = hex::decode(public).map_err(|e| invalid(e.to_string()))?
        .try_into().map_err(|_| invalid("rotation public key length"))?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|e| invalid(e.to_string()))?;
    let signature = Signature::from_slice(&hex::decode(&proof.signature_hex)
        .map_err(|e| invalid(e.to_string()))?).map_err(|e| invalid(e.to_string()))?;
    if key.is_weak() {
        return Err(invalid("rotation weak public key"));
    }
    key.verify_strict(&body.signing_bytes(role).map_err(invalid)?, &signature)
        .map_err(|_| invalid("rotation key proof rejected"))
}

impl World {
    pub(super) fn project_finality_key_rotations(
        &self,
        registry: &mut GovernanceFinalitySignerRegistry,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
        epoch: u64,
    ) -> Result<(), WorldError> {
        if self.state.governance_finality_key_rotations.len() > MAX_FINALITY_KEY_ROTATION_RECORDS_V1 {
            return Err(invalid("rotation history capacity exceeded"));
        }
        let selected = Self::governance_effective_validator_admission_records(admissions);
        let mut groups: BTreeMap<&str, Vec<&FinalityKeyRotationRecordV1>> = BTreeMap::new();
        for (id, record) in &self.state.governance_finality_key_rotations {
            record.validate().map_err(invalid)?;
            let body = record.body().map_err(invalid)?;
            if id != &body.rotation_id {
                return Err(invalid("rotation record identity mismatch"));
            }
            groups.entry(&body.candidate_id).or_default().push(record);
        }
        for (candidate_id, mut records) in groups {
            let admission = admissions.get(candidate_id)
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
                    || body.target_epoch.checked_mul(self.governance_execution_policy().epoch_length_ticks)
                        != Some(body.activation_height)
                {
                    return Err(invalid("rotation history key, generation or boundary discontinuity"));
                }
                verify_key_proof(body, old, FinalityKeyRotationRoleV1::OldKeyAuthorization)?;
                verify_key_proof(body, new, FinalityKeyRotationRoleV1::NewKeyPossession)?;
                key = body.new_public_key_hex.clone();
                generation = generation.checked_add(1)
                    .ok_or_else(|| invalid("rotation generation exhausted"))?;
                previous_activation = Some(body.activation_height);
                if body.target_epoch <= epoch {
                    effective_key = key.clone();
                }
            }
            if selected.get(&admission.node_id).map(|a| a.candidate_id.as_str()) == Some(candidate_id)
                && registry.signer_bindings.contains_key(&admission.node_id)
                && matches!(admission.status, GovernanceValidatorAdmissionStatus::Active
                    | GovernanceValidatorAdmissionStatus::ProbationReady)
                && admission.activation_epoch.is_some_and(|height| height <= epoch)
            {
                registry.signer_bindings.insert(admission.node_id.clone(), effective_key);
            }
        }
        Ok(())
    }

    pub(super) fn capture_finality_key_rotation_parent(
        &self,
    ) -> Result<FinalityKeyRotationParentV1, WorldError> {
        Ok(FinalityKeyRotationParentV1 {
            registry: self.resolve_governance_effective_finality_signer_registry()?,
            admissions: self.state.governance_validator_admissions.clone(),
            rotations: self.state.governance_finality_key_rotations.clone(),
            epoch: self.current_governance_epoch(),
        })
    }

    pub(super) fn prepare_finality_key_rotation(
        &self,
        action: &Action,
        authorization: &GovernanceAuthorizationV1,
    ) -> Result<FinalityKeyRotationRecordV1, WorldError> {
        let Action::RotateGovernanceValidatorFinalityKeyV1 {
            controller_account_id, body, old_key_proof, new_key_proof,
        } = action else { return Err(invalid("expected typed finality rotation")); };
        body.validate_authorization(authorization).map_err(invalid)?;
        self.verify_governance_action_for_next_commit(action, authorization)?;
        if controller_account_id != &body.authorization.controller_account_id {
            return Err(invalid("rotation controller mismatch"));
        }
        let parent = self.bft_governance_parent.as_ref()
            .ok_or_else(|| invalid("rotation requires a qualified wire-3 parent"))?;
        let BftGovernanceSchedule::HeaderV3(schedule) = &parent.schedule else {
            return Err(invalid("rotation requires wire-3 membership execution"));
        };
        if !matches!(parent.formal_handoff, Some(FormalBftHandoffBinding::HeaderV3(_)))
            || schedule.pending_transition().is_some()
            || body.decision_height != schedule.height()
            || body.target_epoch.checked_mul(schedule.policy().epoch_length_ticks)
                != Some(body.activation_height)
        {
            return Err(invalid("rotation is outside its exact free h/h+2 epoch boundary"));
        }
        let original = &parent.rotation_parent;
        let admission = original.admissions.get(&body.candidate_id)
            .ok_or_else(|| invalid("rotation requires an original admission"))?;
        let selected = Self::governance_effective_validator_admission_records(&original.admissions);
        if admission.node_id != body.node_id
            || selected.get(&body.node_id).map(|a| a.candidate_id.as_str())
                != Some(body.candidate_id.as_str())
            || !matches!(admission.status,
                GovernanceValidatorAdmissionStatus::Active | GovernanceValidatorAdmissionStatus::ProbationReady)
            || admission.activation_epoch.is_none_or(|epoch| epoch > original.epoch)
        {
            return Err(invalid("rotation admission is not the parent's effective active record"));
        }
        let registry = original.registry.as_ref()
            .ok_or_else(|| invalid("rotation parent registry missing"))?;
        if registry.signer_bindings.get(&body.node_id) != Some(&body.old_public_key_hex)
            || !schedule.active_set().validators().values().any(|v|
                v.validator_id == body.node_id && v.public_key_hex == body.old_public_key_hex)
        {
            return Err(invalid("rotation old key is not the parent's active finality key"));
        }
        let mut generation = 0;
        for record in original.rotations.values() {
            let previous = record.body().map_err(invalid)?;
            if previous.node_id == body.node_id && previous.candidate_id == body.candidate_id {
                if previous.target_epoch > original.epoch {
                    return Err(invalid("rotation already pending for this admission"));
                }
                generation = generation.max(previous.expected_generation.checked_add(1)
                    .ok_or_else(|| invalid("rotation generation exhausted"))?);
            }
            if previous.old_public_key_hex == body.new_public_key_hex
                || previous.new_public_key_hex == body.new_public_key_hex
            {
                return Err(invalid("rotation cannot reuse a historical finality key"));
            }
        }
        if body.expected_generation != generation
            || registry.signer_bindings.values().any(|key| key == &body.new_public_key_hex)
            || original.admissions.values().any(|a| a.finality_signer_public_key == body.new_public_key_hex)
            || self.state.governance_finality_key_rotations.contains_key(&body.rotation_id)
            || self.state.governance_finality_key_rotations.len() >= MAX_FINALITY_KEY_ROTATION_RECORDS_V1
        {
            return Err(invalid("rotation generation, key uniqueness, identity or capacity rejected"));
        }
        // A first action in this batch cannot authorize a second rotation of
        // the same original generation, even though both saw the frozen parent.
        for record in self.state.governance_finality_key_rotations.values() {
            let previous = record.body().map_err(invalid)?;
            if previous.new_public_key_hex == body.new_public_key_hex {
                return Err(invalid("rotation new key already reserved by a prior decision"));
            }
            if previous.node_id == body.node_id && previous.candidate_id == body.candidate_id
                && previous.expected_generation >= body.expected_generation
            {
                return Err(invalid("rotation generation already consumed"));
            }
        }
        verify_key_proof(body, old_key_proof, FinalityKeyRotationRoleV1::OldKeyAuthorization)?;
        verify_key_proof(body, new_key_proof, FinalityKeyRotationRoleV1::NewKeyPossession)?;
        Ok(FinalityKeyRotationRecordV1 {
            authorized_action: GovernanceAuthorizedActionV1 {
                action: Box::new(action.clone()), authorization: authorization.clone(),
            },
            applied_at_tick: self.state.time,
        })
    }
}
