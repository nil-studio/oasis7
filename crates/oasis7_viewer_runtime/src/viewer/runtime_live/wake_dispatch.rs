use super::*;
use crate::runtime::{CognitionWakeDispositionV1, ContinuationStatusV1};

pub(super) fn ensure_viewer_runtime_binding(
    world: &mut RuntimeWorld,
    config: &ViewerRuntimeLiveServerConfig,
) -> Result<(), ViewerRuntimeLiveServerError> {
    #[cfg(test)]
    let binding_override = config.test_cognition_runtime_binding.as_ref().map(
        |(branch_id, finality_epoch, finality_block_hash, finality_status, reorg_epoch)| {
            (
                branch_id.as_str(),
                *finality_epoch,
                finality_block_hash.clone(),
                finality_status.as_str(),
                *reorg_epoch,
            )
        },
    );
    #[cfg(test)]
    let (branch_id, finality_epoch, finality_block_hash, finality_status, reorg_epoch) =
        binding_override.unwrap_or(("main", 0, None, "pending", 0));
    #[cfg(not(test))]
    let (branch_id, finality_epoch, finality_block_hash, finality_status, reorg_epoch) =
        ("main", 0, None, "pending", 0);
    let world_id = config.world_id.as_str();
    match world.cognition().get("runtime_binding") {
        None | Some(serde_json::Value::Null) => world
            .bind_cognition_runtime(
                world_id,
                branch_id,
                finality_epoch,
                finality_block_hash,
                finality_status,
                reorg_epoch,
            )
            .map_err(ViewerRuntimeLiveServerError::Runtime),
        Some(_) => {
            let binding = world
                .current_cognition_runtime_binding()
                .map_err(ViewerRuntimeLiveServerError::Runtime)?;
            if binding.world_id != world_id {
                return Err(ViewerRuntimeLiveServerError::Init(format!(
                    "Viewer Runtime binding world_id mismatch: expected {world_id}, got {}",
                    binding.world_id
                )));
            }
            Ok(())
        }
    }
}

impl ViewerRuntimeLiveServer {
    pub(super) fn handoff_runtime_wake_for_agent(
        &mut self,
        agent_id: &str,
        status: ContinuationStatusV1,
        reason: &str,
    ) -> Result<bool, String> {
        let wake_id = self
            .llm_sidecar
            .provider_recovery_context(agent_id)
            .and_then(|context| {
                self.llm_sidecar
                    .pending_runtime_wake_id_for_context(agent_id, &context)
                    .map(str::to_string)
            })
            .or_else(|| {
                self.llm_sidecar
                    .pending_runtime_wake_id_for_terminal(agent_id)
                    .map(str::to_string)
            });
        let Some(wake_id) = wake_id else {
            if self
                .llm_sidecar
                .has_pending_runtime_wake_for_agent(agent_id)
            {
                return Err(format!(
                    "Runtime cognition wake identity unavailable for agent {agent_id}"
                ));
            }
            return Ok(false);
        };
        let had_recovery = self
            .llm_sidecar
            .provider_wake_recovery_pending(agent_id)
            .is_some();
        self.llm_sidecar
            .update_provider_wake_recovery(agent_id, status, reason)?;
        self.world
            .consume_cognition_wake(&wake_id, |_wake| {
                Ok(CognitionWakeDispositionV1::Terminal {
                    status,
                    reason: reason.to_string(),
                })
            })
            .map_err(|error| format!("Runtime cognition wake handoff failed: {error:?}"))?;
        self.llm_sidecar.clear_runtime_wake(&wake_id);
        if had_recovery {
            self.llm_sidecar.complete_provider_wake_recovery(agent_id)?;
        }
        Ok(true)
    }

    /// Retry a wake whose provider identity was already terminalized but whose
    /// Runtime handoff failed. The retry is exact and agent-scoped; no fresh
    /// provider decision may be selected while the marker remains.
    pub(super) fn retry_provider_wake_recovery(&mut self) -> Result<(), String> {
        let Some(agent_id) = self.llm_sidecar.provider_wake_recovery_pending_agent() else {
            return Ok(());
        };
        let Some(recovery) = self.llm_sidecar.provider_wake_recovery_pending(&agent_id) else {
            return Ok(());
        };
        // A previous pass may have failed while expiring the actor-local
        // Runtime turn. Retry that authority step before terminalizing the
        // scheduler wake; both operations remain tied to this exact identity.
        self.llm_sidecar
            .release_provider_turn_checked(agent_id.as_str())
            .map_err(|error| format!("provider wake actor release remains pending: {error}"))?;
        if self.handoff_runtime_wake_for_agent(
            agent_id.as_str(),
            recovery.status,
            recovery.reason.as_str(),
        )? || !self
            .llm_sidecar
            .has_pending_runtime_wake_for_agent(&agent_id)
        {
            self.llm_sidecar
                .complete_provider_wake_recovery(agent_id.as_str())?;
        }
        Ok(())
    }

    pub(super) fn sync_runtime_wake_projection(
        &mut self,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        self.llm_sidecar
            .sync_runtime_wakes(&self.world)
            .map_err(ViewerRuntimeLiveServerError::Init)
    }
}
