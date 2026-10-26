//! Validator governance is authorized by the immutable qualified parent, never
//! by configuration or by a registry modified during candidate execution.
use super::{World, WorldError};
use crate::{Action, ActionEnvelope, GovernanceEvent, GovernanceMainTokenControllerRegistry};
use oasis7_agent_api::world_action::{
    GovernanceAuthorizationV1, governance_validator_admission_auth_required,
};
use std::collections::BTreeMap;

#[cfg(test)]
#[path = "governance_authorization_tests.rs"]
mod tests;

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::GovernancePolicyInvalid {
        reason: reason.into(),
    }
}

pub(super) fn policy_digest(
    registry: &GovernanceMainTokenControllerRegistry,
) -> Result<String, WorldError> {
    let bytes = serde_cbor::to_vec(&("oasis7.governance.controller-policy.v1", registry))
        .map_err(|error| invalid(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

struct AuthorizationParent {
    world_id: String,
    chain_id: String,
    genesis_digest: String,
    position: u64,
    record_id: String,
    state_root: String,
    registry: GovernanceMainTokenControllerRegistry,
}

impl World {
    fn governance_authorization_parent(&self) -> Result<AuthorizationParent, WorldError> {
        let formal = self
            .formal_history_reader
            .as_ref()
            .and_then(|history| history.parent.as_ref());
        if let Some(parent) = &self.bft_governance_parent {
            self.validate_formal_handoff_authorization_parent(parent)?;
            return Ok(AuthorizationParent {
                world_id: parent.schedule.scope().world_id.clone(),
                chain_id: parent.schedule.scope().chain_id.clone(),
                genesis_digest: parent.schedule.scope().genesis_hash.clone(),
                position: parent.schedule.parent_tick(),
                record_id: parent.schedule.parent_commit_ref().into(),
                state_root: parent.schedule.parent_state_root().into(),
                registry: parent
                    .controller_registry
                    .clone()
                    .ok_or_else(|| invalid("BFT governance parent controller policy missing"))?,
            });
        }
        let parent = formal
            .ok_or_else(|| invalid("governance authorization requires a qualified parent"))?;
        let head = parent
            .head()
            .ok_or_else(|| invalid("governance parent commit reference missing"))?;
        let world = parent
            .effect_world()
            .ok_or_else(|| invalid("governance parent world missing"))?;
        let registry = parent
            .controller_registry()
            .ok_or_else(|| invalid("governance parent controller policy missing"))?;
        Ok(AuthorizationParent {
            world_id: world.world_id.clone(),
            chain_id: world.chain_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            position: head.position,
            record_id: head.record_id.clone(),
            state_root: parent.after_snapshot_root().into(),
            registry: registry.clone(),
        })
    }
    /// Read-only ingress feedback. Execution repeats this against its own
    /// qualified parent; enqueue-time success cannot survive a parent change.
    pub fn verify_governance_action_for_next_commit(
        &self,
        action: &Action,
        proof: &GovernanceAuthorizationV1,
    ) -> Result<(), WorldError> {
        self.verify_governance_proof(action, proof)
    }
    pub(super) fn replay_from(&mut self, start_index: usize) -> Result<(), WorldError> {
        self.governance_replaying = true;
        let result = self.replay_from_inner(start_index);
        self.governance_replaying = false;
        result
    }

    /// Public signing context for the next canonical controlled or BFT commit. This
    /// does not authorize a command; the returned envelope still needs the
    /// committed controller threshold and is rechecked during execution.
    pub fn governance_authorization_for_next_commit(
        &self,
        nonce: u64,
    ) -> Result<GovernanceAuthorizationV1, WorldError> {
        let parent = self.governance_authorization_parent()?;
        let registry = &parent.registry;
        let position = parent
            .position
            .checked_add(1)
            .ok_or_else(|| invalid("governance position exhausted"))?;
        let proof = GovernanceAuthorizationV1 {
            schema_version: 1,
            world_id: parent.world_id,
            chain_id: parent.chain_id,
            genesis_digest: parent.genesis_digest,
            parent_position: parent.position,
            parent_record_id: parent.record_id,
            parent_state_root: parent.state_root,
            policy_digest: policy_digest(registry)?,
            controller_account_id: registry.genesis_controller_account_id.clone(),
            nonce,
            valid_from_position: position,
            valid_through_position: position,
            signatures: BTreeMap::new(),
        };
        proof.validate().map_err(invalid)?;
        Ok(proof)
    }

    pub(super) fn validate_governance_action_authorization(
        &self,
        envelope: &ActionEnvelope,
    ) -> Result<(), WorldError> {
        if !governance_validator_admission_auth_required(&envelope.action) {
            if envelope.governance_authorization.is_some() {
                return Err(invalid("governance proof attached to unsupported action"));
            }
            return Ok(());
        }
        let proof = envelope
            .governance_authorization
            .as_ref()
            .ok_or_else(|| invalid("missing governance authorization"))?;
        if !self.governance_execution_is_qualified() {
            return Err(invalid(
                "governance execution requires a qualified formal successor",
            ));
        }
        self.verify_governance_proof(&envelope.action, proof)
    }

    fn verify_governance_proof(
        &self,
        action: &Action,
        proof: &GovernanceAuthorizationV1,
    ) -> Result<(), WorldError> {
        let bytes = proof.signing_bytes(action).map_err(invalid)?;
        let expected = self.governance_authorization_for_next_commit(proof.nonce)?;
        if proof.world_id != expected.world_id
            || proof.chain_id != expected.chain_id
            || proof.genesis_digest != expected.genesis_digest
            || proof.parent_position != expected.parent_position
            || proof.parent_record_id != expected.parent_record_id
            || proof.parent_state_root != expected.parent_state_root
        {
            return Err(invalid("governance world or qualified parent mismatch"));
        }
        let parent = self.governance_authorization_parent()?;
        let registry = &parent.registry;
        let position = proof
            .parent_position
            .checked_add(1)
            .ok_or_else(|| invalid("governance position exhausted"))?;
        if !(proof.valid_from_position..=proof.valid_through_position).contains(&position)
            || proof.policy_digest != policy_digest(registry)?
            || proof.controller_account_id != registry.genesis_controller_account_id
        {
            return Err(invalid("governance policy or valid position mismatch"));
        }
        if proof.nonce
            <= self
                .state
                .governance_authorization_nonces
                .get(&proof.controller_account_id)
                .copied()
                .unwrap_or(0)
        {
            return Err(invalid("governance nonce replay"));
        }
        let policy = registry
            .controller_signer_policies
            .get(&proof.controller_account_id)
            .ok_or_else(|| invalid("governance controller signer policy missing"))?;
        if policy.threshold == 0 || proof.signatures.len() < usize::from(policy.threshold) {
            return Err(invalid("governance signature threshold not met"));
        }
        for (public, signature) in &proof.signatures {
            if !policy.allowed_public_keys.contains(public) {
                return Err(invalid(
                    "governance signer is not authorized by parent policy",
                ));
            }
            let key: [u8; 32] = hex::decode(public)
                .map_err(|e| invalid(e.to_string()))?
                .try_into()
                .map_err(|_| invalid("governance public key length"))?;
            let signature: [u8; 64] = hex::decode(signature)
                .map_err(|e| invalid(e.to_string()))?
                .try_into()
                .map_err(|_| invalid("governance signature length"))?;
            ed25519_dalek::VerifyingKey::from_bytes(&key)
                .map_err(|e| invalid(e.to_string()))?
                .verify_strict(&bytes, &ed25519_dalek::Signature::from_bytes(&signature))
                .map_err(|_| invalid("invalid governance signature"))?;
        }
        Ok(())
    }

    pub(super) fn prepare_governance_event_authorization(
        &self,
        event: &GovernanceEvent,
    ) -> Result<BTreeMap<String, u64>, WorldError> {
        let mut nonces = self.state.governance_authorization_nonces.clone();
        let Some(record) = event_authorization(event) else {
            return Ok(nonces);
        };
        let Some(record) = record else {
            // Explicit legacy journal prefix: it is historical state, never
            // authentication of a newly submitted command. Once signed
            // governance starts, unsigned history cannot resume.
            if self.governance_replaying && nonces.is_empty() {
                return Ok(nonces);
            }
            return Err(invalid(
                "unsigned governance event outside legacy replay prefix",
            ));
        };
        if !self.governance_replaying && !self.governance_execution_is_qualified() {
            return Err(invalid(
                "governance publication requires qualified execution",
            ));
        }
        let proof = &record.authorization;
        // Recompute the event using the original signed input. Normalization,
        // inferred revoke targets and epochs must be deterministic, too.
        let expected = self.action_to_event_policy_contract(0, &record.action, Some(proof))?;
        if expected != crate::WorldEventBody::Governance(event.clone()) {
            return Err(invalid("governance signed action/effect mismatch"));
        }
        self.verify_governance_proof(&record.action, proof)?;
        nonces.insert(proof.controller_account_id.clone(), proof.nonce);
        Ok(nonces)
    }
}

fn event_authorization(
    event: &GovernanceEvent,
) -> Option<Option<&oasis7_agent_api::world_action::GovernanceAuthorizedActionV1>> {
    match event {
        GovernanceEvent::ValidatorAdmissionSubmitted { authorization, .. }
        | GovernanceEvent::ValidatorAdmissionApproved { authorization, .. }
        | GovernanceEvent::ValidatorAdmissionActivated { authorization, .. }
        | GovernanceEvent::ValidatorAdmissionRevoked { authorization, .. } => {
            Some(authorization.as_ref())
        }
        _ => None,
    }
}

pub(super) fn governance_record(
    action: &Action,
    proof: Option<&GovernanceAuthorizationV1>,
) -> Option<oasis7_agent_api::world_action::GovernanceAuthorizedActionV1> {
    proof.map(
        |authorization| oasis7_agent_api::world_action::GovernanceAuthorizedActionV1 {
            action: Box::new(action.clone()),
            authorization: authorization.clone(),
        },
    )
}
