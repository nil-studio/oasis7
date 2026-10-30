//! Bootstrap lineage remains anchored to the immutable raw registry, independently
//! of admitted-validator generations and service/account identity.
use super::governance_bft_parent::{BftGovernanceSchedule, FormalBftHandoffBinding};
use super::governance_finality_key_rotation::FinalityKeyRotationParentV1;
use super::*;
use crate::Action;
use crate::governance::{
    BootstrapFinalityKeyRotationRecordV1, bootstrap_finality_registry_digest_v1,
    validate_finality_key_rotation_history_capacity,
};
use ed25519_dalek::{Signature, VerifyingKey};
use oasis7_agent_api::world_action::{
    BootstrapFinalityKeyRotationBodyV1, BootstrapFinalityKeyRotationProofV1,
    BootstrapFinalityKeyRotationRoleV1, GovernanceAuthorizationV1, GovernanceAuthorizedActionV1,
};

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::GovernancePolicyInvalid {
        reason: reason.into(),
    }
}
fn verify_key_proof(
    body: &BootstrapFinalityKeyRotationBodyV1,
    proof: &BootstrapFinalityKeyRotationProofV1,
    role: BootstrapFinalityKeyRotationRoleV1,
) -> Result<(), WorldError> {
    proof.validate_for(body, role).map_err(invalid)?;
    let public = match role {
        BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization => &body.old_public_key_hex,
        BootstrapFinalityKeyRotationRoleV1::NewKeyPossession => &body.new_public_key_hex,
    };
    let bytes: [u8; 32] = hex::decode(public)
        .map_err(|e| invalid(e.to_string()))?
        .try_into()
        .map_err(|_| invalid("bootstrap rotation public key length"))?;
    let key = VerifyingKey::from_bytes(&bytes).map_err(|e| invalid(e.to_string()))?;
    let signature = Signature::from_slice(
        &hex::decode(&proof.signature_hex).map_err(|e| invalid(e.to_string()))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    if key.is_weak() {
        return Err(invalid("bootstrap rotation weak public key"));
    }
    key.verify_strict(&body.signing_bytes(role).map_err(invalid)?, &signature)
        .map_err(|_| invalid("bootstrap rotation key proof rejected"))
}
impl World {
    /// Retained histories enforce source continuity and internal uniqueness.
    /// Later Admission origins cannot retroactively reserve a key against an
    /// earlier rotation. Exact then-current reservations are rechecked by the
    /// canonical candidate replay, not inferred from today's admission map.
    pub(super) fn validate_retained_rotation_namespace(
        &self,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
    ) -> Result<(), WorldError> {
        validate_finality_key_rotation_history_capacity(
            self.state.governance_finality_key_rotations.len(),
            self.state.governance_bootstrap_finality_key_rotations.len(),
        )
        .map_err(invalid)?;
        let mut keys = std::collections::BTreeSet::new();
        if let Some(raw) = &self.state.governance_finality_signer_registry {
            keys.extend(raw.signer_bindings.values().cloned());
        }
        for record in self.state.governance_finality_key_rotations.values() {
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
        for (id, record) in &self.state.governance_bootstrap_finality_key_rotations {
            let body = record.body().map_err(invalid)?;
            if self
                .state
                .governance_finality_key_rotations
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
    /// All sources share one ID/key namespace and one capacity budget. Include
    /// current-batch reservations as well as the immutable parent authorities.
    pub(super) fn validate_rotation_reservation(
        &self,
        parent: &FinalityKeyRotationParentV1,
        rotation_id: &str,
        new_key: &str,
    ) -> Result<(), WorldError> {
        validate_finality_key_rotation_history_capacity(
            self.state.governance_finality_key_rotations.len(),
            self.state
                .governance_bootstrap_finality_key_rotations
                .len()
                .checked_add(1)
                .ok_or_else(|| invalid("rotation capacity overflow"))?,
        )
        .map_err(invalid)?;
        if self
            .state
            .governance_finality_key_rotations
            .contains_key(rotation_id)
            || self
                .state
                .governance_bootstrap_finality_key_rotations
                .contains_key(rotation_id)
            || parent
                .bootstrap_registry
                .as_ref()
                .is_some_and(|r| r.signer_bindings.values().any(|k| k == new_key))
            || parent
                .registry
                .as_ref()
                .is_some_and(|r| r.signer_bindings.values().any(|k| k == new_key))
            || parent
                .admissions
                .values()
                .any(|a| a.finality_signer_public_key == new_key)
            || self
                .state
                .governance_validator_admissions
                .values()
                .any(|a| a.finality_signer_public_key == new_key)
        {
            return Err(invalid(
                "rotation ID, original key or shared capacity rejected",
            ));
        }
        for record in self.state.governance_finality_key_rotations.values() {
            let b = record.body().map_err(invalid)?;
            if b.old_public_key_hex == new_key || b.new_public_key_hex == new_key {
                return Err(invalid(
                    "rotation cannot reuse admitted historical finality key",
                ));
            }
        }
        for record in self
            .state
            .governance_bootstrap_finality_key_rotations
            .values()
        {
            let b = record.body().map_err(invalid)?;
            if b.old_public_key_hex == new_key || b.new_public_key_hex == new_key {
                return Err(invalid(
                    "rotation cannot reuse bootstrap historical finality key",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn project_bootstrap_finality_key_rotations(
        &self,
        registry: &mut GovernanceFinalitySignerRegistry,
        admissions: &BTreeMap<String, GovernanceValidatorAdmissionRecord>,
        epoch: u64,
    ) -> Result<(), WorldError> {
        validate_finality_key_rotation_history_capacity(
            self.state.governance_finality_key_rotations.len(),
            self.state.governance_bootstrap_finality_key_rotations.len(),
        )
        .map_err(invalid)?;
        if self
            .state
            .governance_bootstrap_finality_key_rotations
            .is_empty()
        {
            return Ok(());
        }
        let raw = self
            .state
            .governance_finality_signer_registry
            .as_ref()
            .ok_or_else(|| invalid("bootstrap rotation source registry missing"))?;
        let source_digest = bootstrap_finality_registry_digest_v1(raw).map_err(invalid)?;
        let authority = Self::governance_validator_authority_records_at_epoch(admissions, epoch);
        let mut groups: BTreeMap<&str, Vec<&BootstrapFinalityKeyRotationRecordV1>> =
            BTreeMap::new();
        for (id, record) in &self.state.governance_bootstrap_finality_key_rotations {
            record.validate().map_err(invalid)?;
            let body = record.body().map_err(invalid)?;
            if id != &body.rotation_id
                || self
                    .state
                    .governance_finality_key_rotations
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
                        .checked_mul(self.governance_execution_policy().epoch_length_ticks)
                        != Some(body.activation_height)
                {
                    return Err(invalid(
                        "bootstrap rotation history generation, key or boundary discontinuity",
                    ));
                }
                verify_key_proof(
                    body,
                    old,
                    BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
                )?;
                verify_key_proof(
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
    pub(super) fn prepare_bootstrap_finality_key_rotation(
        &self,
        action: &Action,
        authorization: &GovernanceAuthorizationV1,
    ) -> Result<BootstrapFinalityKeyRotationRecordV1, WorldError> {
        let Action::RotateGovernanceBootstrapFinalityKeyV1 {
            controller_account_id,
            body,
            old_key_proof,
            new_key_proof,
        } = action
        else {
            return Err(invalid("expected typed bootstrap finality rotation"));
        };
        body.validate_authorization(authorization)
            .map_err(invalid)?;
        self.verify_governance_action_for_next_commit(action, authorization)?;
        if controller_account_id != &body.authorization.controller_account_id {
            return Err(invalid("bootstrap rotation controller mismatch"));
        }
        let parent = self
            .bft_governance_parent
            .as_ref()
            .ok_or_else(|| invalid("bootstrap rotation requires qualified wire-3 parent"))?;
        let BftGovernanceSchedule::HeaderV3(schedule) = &parent.schedule else {
            return Err(invalid(
                "bootstrap rotation requires wire-3 membership execution",
            ));
        };
        if !matches!(
            parent.formal_handoff,
            Some(FormalBftHandoffBinding::HeaderV3(_))
        ) || schedule.pending_transition().is_some()
            || body.decision_height != schedule.height()
            || body
                .target_epoch
                .checked_mul(schedule.policy().epoch_length_ticks)
                != Some(body.activation_height)
        {
            return Err(invalid(
                "bootstrap rotation is outside exact free h/h+2 epoch boundary",
            ));
        }
        let original = &parent.rotation_parent;
        let raw = original
            .bootstrap_registry
            .as_ref()
            .ok_or_else(|| invalid("bootstrap source registry missing"))?;
        if bootstrap_finality_registry_digest_v1(raw).map_err(invalid)?
            != body.bootstrap_registry_digest_hex
            || raw.signer_bindings.get(&body.node_id) != Some(&body.bootstrap_public_key_hex)
            || Self::governance_validator_authority_records_at_epoch(
                &original.admissions,
                original.epoch,
            )
            .contains_key(&body.node_id)
        {
            return Err(invalid(
                "bootstrap rotation source is not the parent's effective authority",
            ));
        }
        if original
            .registry
            .as_ref()
            .and_then(|r| r.signer_bindings.get(&body.node_id))
            != Some(&body.old_public_key_hex)
            || !schedule.active_set().validators().values().any(|v| {
                v.validator_id == body.node_id && v.public_key_hex == body.old_public_key_hex
            })
        {
            return Err(invalid(
                "bootstrap old key is not parent's active finality key",
            ));
        }
        self.validate_rotation_reservation(original, &body.rotation_id, &body.new_public_key_hex)?;
        let mut generation = 0_u64;
        for record in original.bootstrap_rotations.values() {
            let previous = record.body().map_err(invalid)?;
            // A changed raw registry must never reset even an unrelated node's
            // retained source history to a newly claimed generation zero.
            if previous.bootstrap_registry_digest_hex != body.bootstrap_registry_digest_hex {
                return Err(invalid("bootstrap rotation source registry changed"));
            }
            if previous.node_id == body.node_id {
                if previous.bootstrap_public_key_hex != body.bootstrap_public_key_hex
                    || previous.target_epoch > original.epoch
                {
                    return Err(invalid(
                        "bootstrap rotation source conflicts or already pending",
                    ));
                }
                generation = generation.max(
                    previous
                        .expected_generation
                        .checked_add(1)
                        .ok_or_else(|| invalid("bootstrap generation exhausted"))?,
                );
            }
        }
        if body.expected_generation != generation {
            return Err(invalid("bootstrap rotation generation mismatch"));
        }
        for record in self
            .state
            .governance_bootstrap_finality_key_rotations
            .values()
        {
            let previous = record.body().map_err(invalid)?;
            if previous.node_id == body.node_id
                && previous.expected_generation >= body.expected_generation
            {
                return Err(invalid("bootstrap rotation generation already consumed"));
            }
        }
        verify_key_proof(
            body,
            old_key_proof,
            BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        )?;
        verify_key_proof(
            body,
            new_key_proof,
            BootstrapFinalityKeyRotationRoleV1::NewKeyPossession,
        )?;
        Ok(BootstrapFinalityKeyRotationRecordV1 {
            authorized_action: GovernanceAuthorizedActionV1 {
                action: Box::new(action.clone()),
                authorization: authorization.clone(),
            },
            applied_at_tick: self.state.time,
        })
    }
}

#[cfg(test)]
#[path = "governance_bootstrap_finality_key_rotation_tests.rs"]
mod tests;
