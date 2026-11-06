//! Exact Owner profile identity. A receipt proves installation, not dispatch authority.
use super::*;
use crate::role_key_slot::RoleKeySlotScopeV1;
pub const OWNER_TERMINATION_PROFILE_DOMAIN_V3: &str = "oasis7.custody.owner-termination-profile.v3";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerTerminationProfileV3 {
    pub plan: TerminationPlanV3,
    pub slot: RoleKeySlotScopeV1,
    pub profile_revision: DecimalU64,
}
impl OwnerTerminationProfileV3 {
    pub fn validate(&self) -> Result<()> {
        self.plan.validate_at(0).map_err(|_| invalid())?;
        self.slot.validate().map_err(|_| invalid())?;
        let identity = &self.slot.context.identity;
        if self.profile_revision.0 != 1
            || identity.role != crate::http::Role::Owner
            || self.slot.provision_id != self.plan.provision_id
            || identity.account_id != self.plan.custody_account_id
            || identity.key_id != self.plan.owner_key_ref
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        digest(&(OWNER_TERMINATION_PROFILE_DOMAIN_V3, self))
    }
    pub fn unsigned_stop(&self) -> Result<oasis7_agent_api::game::GameTaskStopV2> {
        self.validate()?;
        let p = &self.plan;
        Ok(oasis7_agent_api::game::GameTaskStopV2 {
            api_revision: 2,
            client_operation_id: p.stop_operation_id.clone(),
            custody_account_id: p.custody_account_id.clone(),
            termination_plan_digest: p.digest().map_err(|_| invalid())?,
            initial_fence: p.initial_fence.clone(),
            target_executor_public_key: p.original_delegation.delegate_public_key.clone(),
            target_executor_generation: p.original_delegation.generation,
            target_executor_nonce: p.original_delegation.nonce,
            execute_until_ms: DecimalU64(p.cleanup_until.get()),
            observation_grant: None,
        })
    }
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        oasis7_client_api::world_service::signing_bytes(OWNER_TERMINATION_PROFILE_DOMAIN_V3, self)
            .map_err(|_| invalid())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerTerminationProfileReceiptV3 {
    pub profile: OwnerTerminationProfileV3,
    pub subject_public_key: String,
    pub signature_hex: String,
}
impl OwnerTerminationProfileReceiptV3 {
    pub fn verify(&self) -> Result<()> {
        self.profile.validate()?;
        if self.subject_public_key != self.profile.plan.initial_fence.owner_public_key {
            return Err(invalid());
        }
        oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
            OWNER_TERMINATION_PROFILE_DOMAIN_V3,
            &self.profile,
            &self.subject_public_key,
            &self.signature_hex,
        )
        .map_err(|_| invalid())
    }
}
/// The two effect slots are fixed; a lookup never allocates another effect slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerTerminationEffectV3 {
    Stop,
    RevokeAndClose,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OwnerTerminationRequestV3 {
    Stop(oasis7_agent_api::game::GameTaskStopV2),
    RevokeAndClose(oasis7_agent_api::game::GameTaskRevokeAndCloseV3),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "original",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum OwnerTerminationSignedV3 {
    Stop(oasis7_agent_api::game::SignedGameTaskStopV2),
    RevokeAndClose(
        oasis7_client_api::world_service::SignedReadRequest<
            oasis7_agent_api::game::GameTaskRevokeAndCloseV3,
        >,
    ),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerTerminationGrantV3 {
    pub profile: OwnerTerminationProfileV3,
    pub invocation_id: String,
    pub request: OwnerTerminationRequestV3,
}
impl OwnerTerminationGrantV3 {
    pub fn effect(&self) -> OwnerTerminationEffectV3 {
        match self.request {
            OwnerTerminationRequestV3::Stop(_) => OwnerTerminationEffectV3::Stop,
            OwnerTerminationRequestV3::RevokeAndClose(_) => {
                OwnerTerminationEffectV3::RevokeAndClose
            }
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_shape()?;
        // Shape permits only the new GuardedStopV2/CleanupV3 codecs. Ordinary
        // Production dispatch still rejects the development-only plan policy.
        Ok(())
    }
    fn validate_shape(&self) -> Result<()> {
        self.profile.validate()?;
        if !id(&self.invocation_id) {
            return Err(invalid());
        }
        let p = &self.profile.plan;
        match &self.request {
            OwnerTerminationRequestV3::Stop(r) => {
                if r != &self.profile.unsigned_stop()? {
                    return Err(invalid());
                }
            }
            OwnerTerminationRequestV3::RevokeAndClose(r) => {
                r.validate().map_err(|_| invalid())?;
                if r.client_operation_id != p.close_operation_id
                    || r.custody_account_id != p.custody_account_id
                    || r.termination_plan_digest != p.digest().map_err(|_| invalid())?
                    || r.permitted_branches.exact_initial_active != p.initial_fence
                    || r.signer_public_key != p.original_delegation.delegate_public_key
                    || r.signer_generation.0 != p.original_delegation.generation.0
                    || r.signer_nonce.0 != p.original_delegation.nonce.0
                    || r.cleanup_until_ms.0 != p.cleanup_until.get()
                {
                    return Err(invalid());
                }
                // The Account producer additionally derives checked max+1 from
                // all allocated/unknown originals. Shape cannot attest inventory.
                let own = &r.permitted_branches.own_stop_intent;
                let stop = self.profile.unsigned_stop()?;
                let expected_intent = oasis7_wasm_abi::canonical_hash(&(
                    oasis7_agent_api::game::GAME_TASK_STOP_DOMAIN_V2,
                    &p.initial_fence.owner_public_key,
                    &stop,
                ))
                .map_err(|_| invalid())?;
                if own.operation_id != p.stop_operation_id || own.intent_digest != expected_intent {
                    return Err(invalid());
                }
            }
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        digest(&("oasis7.custody.owner-termination-grant.v3", self))
    }
}
