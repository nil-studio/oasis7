//! Explicit native executor task authority; never synthesizes provider grants.
use crate::World;
use oasis7_agent_api::{game::GameTaskFenceV1, h_v1};
use oasis7_client_api::world_service::task_context::{
    NativeTaskAuthorityV1, TASK_CONTEXT_LEASE_TICKS,
};
impl World {
    pub fn canonical_native_task_authority(
        &self,
        fence: &GameTaskFenceV1,
    ) -> Result<NativeTaskAuthorityV1, String> {
        let executor = fence
            .executor_binding
            .as_ref()
            .ok_or("native executor missing")?;
        crate::service_admission::game_task::validate_task_executor(
            self,
            fence,
            &executor.executor_public_key,
        )?;
        let signer = self
            .capability_revocation_state()
            .agent_signer_delegations
            .get(&fence.agent_id)
            .ok_or("native signer delegation missing")?
            .clone();
        let root = self.capability_authorization_root().to_string();
        let base = self
            .current_cognition_base_binding()
            .map_err(|e| format!("{e:?}"))?;
        crate::cognition::validate_native_task_base_authority(self, &base)
            .map_err(|error| error.code().to_string())?;
        let authority = NativeTaskAuthorityV1 {
            schema_revision: 1,
            authority_mode: oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1,
            fence: fence.clone(),
            signer_delegation: signer,
            capability_snapshot_hash: h_v1("oasis7.runtime.manifest.v1", &root).to_string(),
            authority_context_hash: h_v1("oasis7.runtime.authority-context.v1", &root).to_string(),
            capability_authorization_root: root,
            native_authorization_root: self
                .native_task_authorization_root(&fence.agent_id)
                .map_err(|e| format!("{e:?}"))?,
            native_delegation_authorizations: self
                .agent_delegation_authorizations(&fence.agent_id)
                .map_err(|e| format!("{e:?}"))?,
            logical_tick: self.state().time,
            valid_until_tick: self
                .state()
                .time
                .checked_add(TASK_CONTEXT_LEASE_TICKS)
                .ok_or("native lease overflow")?,
        };
        authority.validate_for(fence, &base)?;
        Ok(authority)
    }
}
