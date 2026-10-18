use super::super::cognition_persistence_validation::{
    cognition_validation, strict_optional_finality_hash,
};
use super::{CognitionRuntimeAuthority, World, canonical_runtime_binding_digest};
use crate::cognition::{finality_binding_is_legal, world_state_binding_digest_v1};
use crate::cognition_recovery::RuntimeCognitionBaseBindingV1;
use crate::cognition_scheduler::{CognitionScheduler, SchedulerWakeV1};
use crate::cognition_wake::{AgentContinuation, ContinuationStatusV1};
use crate::error::WorldError;
use oasis7_agent_api::Digest32;
use oasis7_agent_api::RuntimeBindingV1;
use serde_json::Value as JsonValue;

impl World {
    pub fn current_cognition_base_binding(
        &self,
    ) -> Result<RuntimeCognitionBaseBindingV1, WorldError> {
        let authority = self.current_cognition_runtime_authority()?;
        Ok(self.cognition_runtime_base_binding(&authority))
    }
    pub fn cognition(&self) -> &JsonValue {
        &self.cognition
    }

    pub(super) fn current_cognition_runtime_authority(
        &self,
    ) -> Result<CognitionRuntimeAuthority, WorldError> {
        let binding = self
            .cognition
            .get("runtime_binding")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| cognition_validation("runtime_binding_missing"))?;
        let read_string = |field: &str| {
            binding
                .get(field)
                .and_then(JsonValue::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| cognition_validation("runtime_binding_invalid"))
        };
        let world_id = read_string("world_id")?;
        let branch_id = read_string("branch_id")?;
        let manifest_world_id = self.chain_resource_manifest().world_id.as_str();
        if manifest_world_id != "unbound" && manifest_world_id != world_id.as_str() {
            return Err(cognition_validation("runtime_world_id_mismatch"));
        }
        let finality_status = read_string("finality_status")?;
        let finality_epoch = binding
            .get("finality_epoch")
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| cognition_validation("runtime_binding_invalid"))?;
        let reorg_epoch = binding
            .get("reorg_epoch")
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| cognition_validation("runtime_binding_invalid"))?;
        let finality_block_hash =
            strict_optional_finality_hash(binding.get("finality_block_hash"))?;
        if !finality_binding_is_legal(&finality_status, finality_block_hash.as_deref()) {
            return Err(cognition_validation("runtime_binding_invalid"));
        }
        let runtime_manifest_hash = self.current_manifest_hash()?;
        if binding
            .get("runtime_manifest_hash")
            .and_then(JsonValue::as_str)
            != Some(runtime_manifest_hash.as_str())
        {
            return Err(cognition_validation("runtime_manifest_mismatch"));
        }
        Ok(CognitionRuntimeAuthority {
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
            runtime_manifest_hash,
            base_world_hash: self.current_state_root_hash()?,
        })
    }

    pub(super) fn cognition_runtime_base_binding(
        &self,
        authority: &CognitionRuntimeAuthority,
    ) -> RuntimeCognitionBaseBindingV1 {
        RuntimeCognitionBaseBindingV1 {
            world_id: authority.world_id.clone(),
            branch_id: authority.branch_id.clone(),
            finality_epoch: authority.finality_epoch,
            finality_block_hash: authority.finality_block_hash.clone(),
            finality_status: authority.finality_status.clone(),
            base_tick: self.state.time,
            base_world_hash: world_state_binding_digest_v1(
                &authority.world_id,
                &authority.branch_id,
                authority.finality_epoch,
                authority.finality_block_hash.as_deref(),
                &authority.finality_status,
                self.state.time,
                &authority.base_world_hash,
                authority.reorg_epoch,
                &authority.runtime_manifest_hash,
            ),
            reorg_epoch: authority.reorg_epoch,
            runtime_manifest_hash: canonical_runtime_binding_digest(
                &authority.runtime_manifest_hash,
            ),
        }
    }

    pub fn current_cognition_runtime_binding(&self) -> Result<RuntimeBindingV1, WorldError> {
        let authority = self.current_cognition_runtime_authority()?;
        let base = self.cognition_runtime_base_binding(&authority);
        let binding = RuntimeBindingV1 {
            world_id: base.world_id.clone(),
            branch_id: base.branch_id.clone(),
            finality_epoch: base.finality_epoch,
            finality_block_hash: base.finality_block_hash.clone().map(Digest32::from),
            finality_status: base.finality_status.clone(),
            base_tick: base.base_tick,
            base_world_hash: Digest32::from(base.base_world_hash.clone()),
            reorg_epoch: base.reorg_epoch,
            runtime_manifest_hash: Digest32::from(base.runtime_manifest_hash.clone()),
        };
        binding
            .validate()
            .map_err(|error| cognition_validation(error.code()))?;
        Ok(binding)
    }

    /// Expose only validated, non-terminal Runtime continuation records to a
    /// production adapter. The JSON projection remains an internal storage
    /// detail and cannot be used as an authority substitute.
    pub fn active_cognition_continuations(&self) -> Result<Vec<AgentContinuation>, WorldError> {
        self.cognition_continuations_typed()?
            .into_iter()
            .filter(|continuation| {
                !matches!(
                    continuation.status,
                    ContinuationStatusV1::Completed
                        | ContinuationStatusV1::Cancelled
                        | ContinuationStatusV1::Invalidated
                        | ContinuationStatusV1::Expired
                        | ContinuationStatusV1::Rejected
                )
            })
            .map(normalize_cognition_continuation)
            .collect()
    }

    /// Read back one exact durable wake identity from the scheduler and
    /// enforce its World/continuation binding before returning it.
    pub fn cognition_wake_readback(
        &self,
        wake_id: &str,
    ) -> Result<Option<SchedulerWakeV1>, WorldError> {
        if wake_id.trim().is_empty() {
            return Err(cognition_validation("wake_id_invalid"));
        }
        let Some(state) = self
            .cognition
            .get("scheduler_state")
            .filter(|state| !state.is_null())
        else {
            return Ok(None);
        };
        let scheduler_state = self.hydrate_legacy_wake_request_digests(state)?;
        let scheduler = CognitionScheduler::from_snapshot_json(scheduler_state)
            .map_err(|error| cognition_validation(error.code()))?;
        let Some(wake) = scheduler.wake_by_id(wake_id) else {
            return Ok(None);
        };
        self.validate_cognition_wake_binding(&wake)?;
        Ok(Some(wake))
    }
}

/// Normalize one sparse legacy continuation into the typed authority view.
/// Older snapshots omitted the Runtime-issued status digest; deriving it from
/// the durable continuation fields makes that record eligible for identity
/// hydration while leaving the persisted sparse projection unchanged.
fn normalize_cognition_continuation(
    mut continuation: AgentContinuation,
) -> Result<AgentContinuation, WorldError> {
    if continuation.continuation_status_digest.is_none() {
        continuation.refresh_status_digest();
    }
    continuation
        .validate_authoritative()
        .map_err(|error| cognition_validation(error.code()))?;
    Ok(continuation)
}
