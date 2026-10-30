//! Explicit permanent-close policy metadata. Shape checks never consume MFA,
//! allocate a signer nonce, pre-sign an original, or authorize a transport.
use super::*;
type PlanResult<T> = core::result::Result<T, String>;
pub const TERMINATION_PLAN_DOMAIN_V2: &str = "oasis7.custody.termination-plan.v2";
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldTaskClosePolicyV2 {
    LiteralPersistentClosedV2,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationPlanV2 {
    pub api_version: u8,
    pub session_id: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub prepared_result_digest: String,
    pub world: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub initial_fence: GameTaskFenceV1,
    #[serde(with = "super::delegation_metadata")]
    #[cfg_attr(
        test,
        schemars(with = "super::delegation_metadata::DelegationMetadataV1")
    )]
    pub original_delegation: AgentSignerDelegationChangeV1,
    /// Stable planned ID is not an allocated Stop request, nonce or signature.
    pub stop_operation_id: String,
    pub close_operation_id: String,
    pub cleanup_until: UnixMillis,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub trigger: TerminationTriggerV1,
    pub nonce_policy: TerminationNoncePolicyV1,
    pub close_policy: WorldTaskClosePolicyV2,
}
fn id_v2(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= 256
}
fn hash_v2(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl TerminationPlanV2 {
    pub fn digest(&self) -> PlanResult<String> {
        self.validate()?;
        crate::provisioning::types::digest(&(TERMINATION_PLAN_DOMAIN_V2, self))
            .map_err(|e| e.to_string())
    }
    pub fn expected_stop_operation_id(&self) -> PlanResult<String> {
        super::termination_id("stop", &self.session_id, &self.prepared_result_digest)
            .map_err(|e| e.to_string())
    }
    pub fn expected_close_operation_id(&self) -> PlanResult<String> {
        super::termination_id("close-v2", &self.session_id, &self.prepared_result_digest)
            .map_err(|e| e.to_string())
    }
    pub fn validate(&self) -> PlanResult<()> {
        self.initial_fence.validate()?;
        let f = &self.initial_fence;
        let d = &self.original_delegation;
        let binding = f
            .executor_binding
            .as_ref()
            .ok_or("termination V2 requires bound initial executor")?;
        if self.api_version != 2
            || ![
                &self.session_id,
                &self.custody_account_id,
                &self.provision_id,
                &self.setup_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
            ]
            .iter()
            .all(|v| id_v2(v))
            || !hash_v2(&self.prepared_result_digest)
            || !self.world.valid()
            || f.world_id != self.world.world_id
            || f.genesis_digest != self.world.genesis_digest
            || f.branch_id != self.world.branch_id
            || f.reorg_epoch != self.world.reorg_epoch
            || f.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active
            || f.active_goal_reference.is_none()
            || d.world_id != f.world_id
            || d.branch_id != f.branch_id
            || d.agent_id != f.agent_id
            || d.owner_binding != f.owner_binding
            || d.agent_identity_generation != f.owner_generation.0
            || d.revoked
            || d.generation == 0
            || d.nonce == 0
            || d.delegate_public_key != binding.executor_public_key
            || d.generation != binding.delegation_generation.0
            || self.cleanup_until.get() == 0
            || self.request_limit.0 != 2
            || self.transport_limit.0 != 24
            || self.stop_operation_id != self.expected_stop_operation_id()?
            || self.close_operation_id != self.expected_close_operation_id()?
            || self.stop_operation_id == self.close_operation_id
        {
            return Err("invalid termination V2 plan shape".into());
        }
        Ok(())
    }
    /// Links the one actual V2 header scope. The caller still validates the
    /// complete signed top-level request and actual prepared/live evidence.
    pub fn validate_scope(
        &self,
        scope: &crate::provisioning::ResourceTestSessionScopeV2,
    ) -> PlanResult<()> {
        self.validate()?;
        if self.custody_account_id != scope.client_pairing.target.custody_account_id
            || self.provision_id != scope.client_pairing.target.provision_id
            || self.session_id != scope.session_id
            || self.world != scope.world_identity
            || self.initial_fence != scope.task_fence
            || self.executor_key_ref != scope.executor_key_ref
            || self.digest()? != scope.termination_plan_digest
        {
            return Err("termination V2 scope mismatch".into());
        }
        Ok(())
    }
}
