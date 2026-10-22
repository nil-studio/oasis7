//! Native task authority is not a module catalog or provider invocation.
use super::*;
use crate::world_service_wire::AgentSignerDelegationChangeV1;
use oasis7_agent_api::{h_v1, world_quote::AgentDelegationAuthorizationV1};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTaskAuthorityV1 {
    pub schema_revision: u32,
    pub authority_mode: TaskContextAuthorityModeV1,
    pub fence: GameTaskFenceV1,
    pub signer_delegation: AgentSignerDelegationChangeV1,
    pub capability_authorization_root: String,
    pub capability_snapshot_hash: String,
    pub authority_context_hash: String,
    pub native_authorization_root: String,
    pub native_delegation_authorizations: Vec<AgentDelegationAuthorizationV1>,
    pub logical_tick: u64,
    pub valid_until_tick: u64,
}
impl NativeTaskAuthorityV1 {
    pub fn validate_for(
        &self,
        fence: &GameTaskFenceV1,
        base: &RuntimeCognitionBaseBindingV1,
    ) -> Result<(), String> {
        fence.validate()?;
        base.validate().map_err(|e| e.to_string())?;
        let delegate = fence
            .executor_binding
            .as_ref()
            .ok_or("native task executor missing")?;
        let signer = &self.signer_delegation;
        if self.schema_revision != 1
            || self.authority_mode != TaskContextAuthorityModeV1::NativeExecutorV1
            || &self.fence != fence
            || signer.revoked
            || signer.world_id != fence.world_id
            || signer.branch_id != fence.branch_id
            || signer.agent_id != fence.agent_id
            || signer.owner_binding != fence.owner_binding
            || signer.agent_identity_generation != fence.owner_generation.0
            || signer.delegate_public_key != delegate.executor_public_key
            || signer.generation != delegate.delegation_generation.0
            || base.world_id != fence.world_id
            || base.branch_id != fence.branch_id
            || base.reorg_epoch != fence.reorg_epoch.0
            || self.logical_tick != base.base_tick
            || self.valid_until_tick
                != self
                    .logical_tick
                    .checked_add(TASK_CONTEXT_LEASE_TICKS)
                    .ok_or("native lease overflow")?
            || !digest(&self.capability_authorization_root)
            || !digest(&self.native_authorization_root)
            || self.capability_snapshot_hash
                != h_v1(
                    "oasis7.runtime.manifest.v1",
                    &self.capability_authorization_root,
                )
                .to_string()
            || self.authority_context_hash
                != h_v1(
                    "oasis7.runtime.authority-context.v1",
                    &self.capability_authorization_root,
                )
                .to_string()
            || self.native_delegation_authorizations.iter().any(|a| {
                a.grant.agent_id != fence.agent_id
                    || a.spent_units.checked_add(a.remaining_units) != Some(a.grant.limit_units)
                    || a.cost_units != 0
            })
        {
            return Err("native task authority differs from current executor fence".into());
        }
        Ok(())
    }
}
fn digest(value: &str) -> bool {
    value.strip_prefix("blake3:").unwrap_or(value).len() == 64
        && value
            .strip_prefix("blake3:")
            .unwrap_or(value)
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Clone, Copy)]
pub enum TaskAuthorityRef<'a> {
    Provider(&'a WorldServiceAgentContext),
    Native(&'a NativeTaskAuthorityV1),
}
impl<'a> TaskAuthorityRef<'a> {
    pub fn select(
        provider: Option<&'a WorldServiceAgentContext>,
        native: Option<&'a NativeTaskAuthorityV1>,
    ) -> Result<Self, String> {
        match (provider, native) {
            (Some(p), None) => Ok(Self::Provider(p)),
            (None, Some(n)) => Ok(Self::Native(n)),
            _ => Err("task context requires exactly one authority".into()),
        }
    }
    pub fn mode(self) -> Option<TaskContextAuthorityModeV1> {
        match self {
            Self::Provider(_) => None,
            Self::Native(_) => Some(TaskContextAuthorityModeV1::NativeExecutorV1),
        }
    }
    pub fn agent_id(self) -> &'a str {
        match self {
            Self::Provider(p) => &p.agent_id,
            Self::Native(n) => &n.fence.agent_id,
        }
    }
    pub fn hashes(self) -> (&'a str, &'a str, &'a str) {
        match self {
            Self::Provider(p) => (
                &p.capability_authorization_root,
                &p.capability_snapshot_hash,
                &p.authority_context_hash,
            ),
            Self::Native(n) => (
                &n.capability_authorization_root,
                &n.capability_snapshot_hash,
                &n.authority_context_hash,
            ),
        }
    }
    pub fn native_authorization_root(self) -> Option<&'a str> {
        match self {
            Self::Provider(_) => None,
            Self::Native(n) => Some(&n.native_authorization_root),
        }
    }
    pub fn valid_until_tick(self) -> u64 {
        match self {
            Self::Provider(p) => p.capability_catalog.valid_until_tick,
            Self::Native(n) => n.valid_until_tick,
        }
    }
    pub fn validate_for(
        self,
        f: &GameTaskFenceV1,
        b: &RuntimeCognitionBaseBindingV1,
    ) -> Result<(), String> {
        match self {
            Self::Native(n) => n.validate_for(f, b),
            Self::Provider(p) => {
                let catalog = &p.capability_catalog;
                let invocation = &p.capability_invocation_context;
                catalog
                    .validate()
                    .map_err(|_| "invalid current task catalog")?;
                if p.agent_id != f.agent_id
                    || b.world_id != f.world_id
                    || b.branch_id != f.branch_id
                    || b.reorg_epoch != f.reorg_epoch.0
                    || catalog.world_id != f.world_id
                    || catalog.branch_id != f.branch_id
                    || catalog.logical_tick != b.base_tick
                    || !matches!(&catalog.subject,oasis7_wasm_abi::CapabilitySubject::Agent{agent_id,owner_binding,generation} if agent_id==&f.agent_id && owner_binding==&f.owner_binding && *generation==f.owner_generation.0)
                    || invocation.catalog_snapshot_id != catalog.snapshot_id
                    || invocation.subject != catalog.subject
                    || invocation.presenter != catalog.presenter
                    || invocation.audience != catalog.audience
                {
                    return Err("task provider authority differs from task fence".into());
                }
                Ok(())
            }
        }
    }
}
impl GoalExecutionSnapshotV1 {
    pub fn authority(&self) -> Result<TaskAuthorityRef<'_>, String> {
        TaskAuthorityRef::select(
            self.provider_authority.as_ref(),
            self.native_authority.as_ref(),
        )
    }
    pub fn authority_mode(&self) -> Result<Option<TaskContextAuthorityModeV1>, String> {
        Ok(self.authority()?.mode())
    }
    pub fn authority_valid_until_tick(&self) -> Result<u64, String> {
        Ok(self.authority()?.valid_until_tick())
    }
    pub fn authority_hashes(&self) -> Result<(&str, &str, &str), String> {
        Ok(self.authority()?.hashes())
    }
}
impl TaskPublicContextV1 {
    pub fn authority(&self) -> Result<TaskAuthorityRef<'_>, String> {
        TaskAuthorityRef::select(
            self.provider_authority.as_ref(),
            self.native_authority.as_ref(),
        )
    }
    pub fn authority_mode(&self) -> Result<Option<TaskContextAuthorityModeV1>, String> {
        Ok(self.authority()?.mode())
    }
    pub fn authority_valid_until_tick(&self) -> Result<u64, String> {
        Ok(self.authority()?.valid_until_tick())
    }
    pub fn authority_hashes(&self) -> Result<(&str, &str, &str), String> {
        Ok(self.authority()?.hashes())
    }
}
