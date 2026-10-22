//! Atomic canonical first binding; HTTP adapters cannot install policies.
use super::{World, WorldError};
use crate::{Action, ActionEnvelope, CapabilityAuthorizationEvent, CapabilityAgentIdentity, WorldEventBody, DomainEvent, CausedBy};
use crate::service_admission::participant::*;
use crate::service_admission::SignedReadRequest;
use oasis7_agent_api::game::*;

impl World {
    pub fn participant_policy_view(&self) -> serde_json::Value {
        let policy = self.capability_revocation_state.participants.policy.as_ref();
        serde_json::json!({"available":policy.is_some_and(|p| p.enabled && p.registry_complete),
            "world_id":self.chain_resource_manifest.world_id,
            "genesis_digest":self.chain_resource_manifest.genesis_ref,
            "logical_tick":self.state.time.to_string(), "authority_source":if !self.capability_revocation_state.participants.devlocal_policy_history.is_empty() {"dev_local_operator"} else if !self.capability_revocation_state.participants.policy_history.is_empty() {"governed"} else {"unconfigured"}, "policy":policy})
    }
    /// Explicit private fixture only. Existing worlds need a separate audited migration.
    /// No production caller, environment toggle or HTTP policy setter is provided.
    #[cfg(test)]
    pub(crate) fn install_private_participant_fixture_policy(&mut self, policy: ParticipantEntryPolicyV1) -> Result<(), WorldError> {
        self.append_capability_authorization_event_batch(vec![CapabilityAuthorizationEvent::ParticipantPolicyInstalled { policy,proof:None }])?;
        Ok(())
    }
    pub fn apply_participant_registration(&mut self,
        owner: &SignedReadRequest<GameParticipantRegisterV1>,
        admission: &SignedReadRequest<GameParticipantAdmissionV1>,
        current_branch: &str, current_reorg: u64) -> Result<GameParticipantReceiptV1, String> {
        validate_registration(self, owner, admission, current_branch, current_reorg)?;
        let r = &owner.request;
        let registry = &self.capability_revocation_state.participants;
        if let Some(old) = registry.operations.get(&operation_key(r)?) {
            if old.request != *r { return Err("participant operation digest conflict".into()); }
            return Ok(old.receipt.clone());
        }
        if let Some(old) = registry.bindings.get(&account_key(r)?) {
            let receipt = old.clone();
            self.append_capability_authorization_event_batch(vec![CapabilityAuthorizationEvent::ParticipantRegistered {
                owner:owner.clone(), admission:admission.clone(), receipt:receipt.clone()
            }]).map_err(|e| format!("{e:?}"))?;
            return Ok(receipt);
        }
        if registry.bindings.len() as u64 >= registry.policy.as_ref().ok_or("participant policy missing")?.max_participants {
            return Err("participant quota exhausted".into());
        }
        let key = account_key(r)?;
        let agent_id = format!("participant:{}", key.strip_prefix("blake3:").unwrap_or(&key));
        if self.state.agents.contains_key(&agent_id) { return Err("participant identity already occupied without binding".into()); }
        let receipt = GameParticipantReceiptV1 { api_rev:1,identity_namespace:r.identity_namespace.clone(),account_id:r.account_id.clone(),
            world_id:r.world_id.clone(),genesis_digest:r.genesis_digest.clone(),agent_id:agent_id.clone(),owner_public_key:r.owner_public_key.clone(),
            owner_binding:key,owner_generation:DecimalU64(1),client_operation_id:r.client_operation_id.clone(),business_digest:business_digest(r)?,registered_tick:DecimalU64(self.state.time), imported:false };
        let mut candidate = self.clone();
        let action_id = candidate.allocate_next_action_id();
        let envelope = ActionEnvelope { id:action_id, governance_authorization: None,
            committed_recipe_origin:None,
            action:Action::RegisterAgent { agent_id:agent_id.clone(),pos:oasis7_agent_api::world_action::GeoPos::new(0,0,0) } };
        let event = candidate.action_to_event(&envelope).map_err(|e| format!("{e:?}"))?;
        if !matches!(&event,WorldEventBody::Domain(DomainEvent::AgentRegistered {agent_id: registered,..}) if registered == &agent_id) {
            return Err("participant RegisterAgent did not produce registration".into());
        }
        candidate.preflight_domain_event(&event).map_err(|e| format!("{e:?}"))?;
        candidate.append_event(event,Some(CausedBy::Action(action_id))).map_err(|e| format!("{e:?}"))?;
        candidate.append_capability_authorization_event_batch(vec![
            CapabilityAuthorizationEvent::AgentIdentityInstalled {agent_id,identity:CapabilityAgentIdentity {owner_binding:receipt.owner_binding.clone(),generation:1}},
            CapabilityAuthorizationEvent::ParticipantRegistered { owner:owner.clone(),admission:admission.clone(),receipt:receipt.clone() }
        ]).map_err(|e| format!("{e:?}"))?;
        validate_registry(&candidate)?;
        *self = candidate;
        Ok(receipt)
    }
}

pub(super) fn project_event(world: &World, registry: &mut crate::capability_authorization::CapabilityRevocationState,
    event: &CapabilityAuthorizationEvent) -> Result<(), WorldError> {
    let deny = |s: String| WorldError::DistributedValidationFailed { reason:s };
    match event {
        CapabilityAuthorizationEvent::DevLocalParticipantPolicyInstalled { authorization } => {
            return super::participant_devlocal::project_devlocal_policy(world,registry,authorization);
        }
        CapabilityAuthorizationEvent::ParticipantPolicyInstalled {policy,proof} => {
            if let Some(proof)=proof {
                return super::participant_policy::project_policy(world,registry,policy,proof);
            }
            if !cfg!(test) { return Err(deny("participant fixture policy is unavailable in production".into())); }
            if policy.api_rev != 1 || policy.generation != 1 || !policy.registry_complete || !policy.enabled
                || policy.max_grant_ticks == 0 || policy.max_grant_ticks > 256 || policy.max_participants == 0
                || policy.identity_namespace.instance_epoch.0 == 0
                || policy.identity_namespace.service_id.trim().is_empty() || policy.identity_namespace.environment_id.trim().is_empty()
                || world.chain_resource_manifest.world_id != policy.world_id
                || world.chain_resource_manifest.genesis_ref.as_ref() != Some(&policy.genesis_digest)
                || !world.state.agents.is_empty() || !registry.participants.is_empty() {
                return Err(deny("participant fixture policy requires explicit empty known-complete registry".into()));
            }
            let bytes:[u8;32] = hex::decode(&policy.issuer_public_key).map_err(|e|deny(e.to_string()))?.try_into().map_err(|_|deny("invalid participant issuer key".into()))?;
            let key = ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|e|deny(e.to_string()))?;
            if key.is_weak() { return Err(deny("weak participant issuer key".into())); }
            registry.participants.policy=Some(policy.clone());
        }
        CapabilityAuthorizationEvent::ParticipantRegistered {owner,admission,receipt} => {
            let mut projected=world.clone(); projected.capability_revocation_state=registry.clone();
            let binding=projected.current_cognition_runtime_binding().map_err(|e|deny(format!("{e:?}")))?;
            validate_admission(&projected,owner,admission,&binding.branch_id,binding.reorg_epoch).map_err(deny)?;
            let r=&owner.request;
            let key=account_key(r).map_err(deny)?;
            let op=operation_key(r).map_err(deny)?;
            let expected_agent=format!("participant:{}",key.strip_prefix("blake3:").unwrap_or(&key));
            if let Some(existing) = registry.participants.bindings.get(&key) {
                if existing != receipt || registry.participants.operations.contains_key(&op) {
                    return Err(deny("participant recovery receipt conflict".into()));
                }
                registry.participants.operations.insert(op,ParticipantOperationV1 {request:r.clone(),receipt:receipt.clone()});
                return Ok(());
            }
            if !registry.participants.policy.as_ref().is_some_and(|p|p.enabled)
                || registry.participants.bindings.contains_key(&key) || registry.participants.operations.contains_key(&op)
                || registry.participants.bindings.len() as u64 >= registry.participants.policy.as_ref().ok_or_else(||deny("participant policy missing".into()))?.max_participants
                || receipt.api_rev != 1 || receipt.identity_namespace != r.identity_namespace || receipt.account_id != r.account_id
                || receipt.world_id != r.world_id || receipt.genesis_digest != r.genesis_digest || receipt.owner_public_key != r.owner_public_key
                || receipt.client_operation_id != r.client_operation_id || receipt.business_digest != business_digest(r).map_err(deny)?
                || receipt.imported || receipt.owner_binding != key || receipt.agent_id != expected_agent || receipt.owner_generation.0 != 1 || receipt.registered_tick.0 != world.state.time
                || !world.state.agents.contains_key(&receipt.agent_id)
                || registry.agent_identities.get(&receipt.agent_id) != Some(&CapabilityAgentIdentity {owner_binding:key.clone(),generation:1}) {
                return Err(deny("participant canonical receipt mismatch".into()));
            }
            registry.participants.bindings.insert(key,receipt.clone());
            registry.participants.operations.insert(op,ParticipantOperationV1 {request:r.clone(),receipt:receipt.clone()});
        }
        _=>return Err(deny("not participant event".into()))
    }
    Ok(())
}

pub(super) fn validate_service_receipt(registry:&crate::capability_authorization::CapabilityRevocationState,record:&serde_json::Value)->Result<(),WorldError> {
    let result:crate::service_admission::CanonicalIntentResultV1=serde_json::from_value(record.clone()).map_err(WorldError::from)?;
    if let crate::service_admission::WorldServicePayloadV1::GameParticipantRegister {owner,..}=&result.request.signed_payload
        && result.rejected.is_none() {
        let key=account_key(&owner.request).map_err(|reason|WorldError::DistributedValidationFailed {reason})?;
        let receipt:GameParticipantReceiptV1=serde_json::from_value(result.receipt).map_err(WorldError::from)?;
        if registry.participants.bindings.get(&key)!=Some(&receipt) {
            return Err(WorldError::DistributedValidationFailed {reason:"participant service receipt has no canonical binding".into()});
        }
    }
    Ok(())
}
