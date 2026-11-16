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
        resolve_runtime_authority_v1(
            self.cognition.get("runtime_binding"),
            self.chain_resource_manifest().world_id.as_str(),
            || self.current_manifest_hash(),
            || self.current_state_root_hash(),
        )
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

    pub(in crate::world) fn cognition_runtime_authority_aliases_from_current_fields_v1(
        binding: &JsonValue,
        manifest_world_id: &str,
        manifest_hash: &str,
    ) -> Result<(String, String, u64, Option<String>, String, u64, String), WorldError> {
        resolve_runtime_authority_fields_v1(Some(binding), manifest_world_id, || {
            Ok(manifest_hash.to_owned())
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn cognition_runtime_binding_from_current_fields_bounded_v1(
        binding: &JsonValue,
        manifest_world_id: &str,
        manifest_hash: &str,
        raw_state_hash: &str,
        tick: u64,
        common:&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<
        oasis7_proto::memory_budget::ReservedValue<RuntimeBindingV1>,
        crate::world::incremental_body::BodyMutationError,
    > {
        use crate::world::incremental_body::BodyMutationError;
        use oasis7_formal_protocol::incremental_value::ValueTreeError;
        use oasis7_proto::memory_budget::MemoryClass;
        fn resource() -> BodyMutationError {
            BodyMutationError::Invalid("Current Runtime binding output layout")
        }
        #[derive(Debug)]
        struct Cause(WorldError);
        impl std::fmt::Display for Cause {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:?}", self.0)
            }
        }
        impl std::error::Error for Cause {}
        let map_count = binding.as_object().map_or(0, |v| v.len());
        let lookups = u64::from(usize::BITS - map_count.max(1).leading_zeros())
            .checked_mul(7 * 64)
            .ok_or_else(resource)?;
        common
            .charge(lookups, lookups)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let mut bytes = raw_state_hash
            .len()
            .checked_add(manifest_hash.len())
            .ok_or_else(resource)?;
        for field in [
            "world_id",
            "branch_id",
            "finality_status",
            "finality_block_hash",
            "runtime_manifest_hash",
        ] {
            bytes = bytes
                .checked_add(
                    binding
                        .get(field)
                        .and_then(JsonValue::as_str)
                        .map_or(0, str::len),
                )
                .ok_or_else(resource)?;
        }
        let peak = bytes
            .checked_mul(16)
            .and_then(|n| n.checked_add(std::mem::size_of::<RuntimeBindingV1>() + 4096))
            .ok_or_else(resource)?;
        common
            .charge(peak as u64, peak as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let phase = common
            .memory()
            .reserve(peak, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let a = resolve_runtime_authority_v1(
            Some(binding),
            manifest_world_id,
            || Ok(manifest_hash.to_owned()),
            || Ok(raw_state_hash.to_owned()),
        )
        .map_err(|e| BodyMutationError::Original(Box::new(Cause(e))))?;
        let (base,manifest)=crate::world::normal_bootstrap_current::hash::with_current_meter_v1(common,canonical,|meter|{
            let base=oasis7_world_state::cognition::world_state_binding_digest_with_work_v1(&a.world_id,&a.branch_id,a.finality_epoch,a.finality_block_hash.as_deref(),&a.finality_status,tick,&a.base_world_hash,a.reorg_epoch,&a.runtime_manifest_hash,meter)?;
            let manifest=crate::world::cognition_persistence::canonical_runtime_binding_digest_with_work_v1(&a.runtime_manifest_hash,meter)?;
            Ok::<_,oasis7_wasm_abi::CanonicalCborStreamError>((base,manifest))
        })?.map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        let copy = base
            .as_str()
            .len()
            .checked_add(manifest.as_str().len())
            .ok_or_else(resource)?;
        common
            .charge(1, copy as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let output = RuntimeBindingV1 {
            world_id: a.world_id,
            branch_id: a.branch_id,
            finality_epoch: a.finality_epoch,
            finality_block_hash: a.finality_block_hash.map(Digest32::from),
            finality_status: a.finality_status,
            base_tick: tick,
            base_world_hash: Digest32::from(base.as_str().to_owned()),
            reorg_epoch: a.reorg_epoch,
            runtime_manifest_hash: Digest32::from(manifest.as_str().to_owned()),
        };
        output.validate().map_err(|e| {
            BodyMutationError::Original(Box::new(Cause(cognition_validation(e.code()))))
        })?;
        Ok(phase.retain(output))
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
fn normalize_cognition_continuation(continuation:AgentContinuation)->Result<AgentContinuation,WorldError>{
    match normalize_cognition_continuation_with_work_v1(continuation,&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,&mut |_|Ok(())){
        Ok(value)=>value,Err(_)=>unreachable!("legacy continuation work cannot fail")
    }
}
pub(in crate::world) fn normalize_cognition_continuation_with_work_v1<A:oasis7_wasm_abi::CapabilityCanonicalWorkV1>(mut continuation:AgentContinuation,source:&A,before_copy:&mut impl FnMut(usize)->Result<(),oasis7_wasm_abi::CanonicalCborStreamError>)->Result<Result<AgentContinuation,WorldError>,oasis7_wasm_abi::CanonicalCborStreamError>{
    if continuation.continuation_status_digest.is_none(){
        let digest=continuation.status_digest_with_work_v1(source)?;
        before_copy(digest.as_str().len())?;
        source.charge_work(1,digest.as_str().len()as u64)?;
        continuation.continuation_status_digest=Some(digest.as_str().to_owned());
    }
    if let Err(error)=continuation.validate_authoritative_with_work_v1(source)?{
        before_copy(256)?;
        return Ok(Err(cognition_validation(error.code())))
    }
    Ok(Ok(continuation))
}

/// Work/source adapters only replace how the original fields are read. Every
/// authority predicate and its original first-error order stays in this kernel.
fn resolve_runtime_authority_v1(
    binding: Option<&JsonValue>,
    manifest_world_id: &str,
    manifest_hash: impl FnOnce() -> Result<String, WorldError>,
    raw_state_hash: impl FnOnce() -> Result<String, WorldError>,
) -> Result<CognitionRuntimeAuthority, WorldError> {
    let (
        world_id,
        branch_id,
        finality_epoch,
        finality_block_hash,
        finality_status,
        reorg_epoch,
        runtime_manifest_hash,
    ) = resolve_runtime_authority_fields_v1(binding, manifest_world_id, manifest_hash)?;
    Ok(CognitionRuntimeAuthority {
        world_id,
        branch_id,
        finality_epoch,
        finality_block_hash,
        finality_status,
        reorg_epoch,
        runtime_manifest_hash,
        base_world_hash: raw_state_hash()?,
    })
}
fn resolve_runtime_authority_fields_v1(
    binding: Option<&JsonValue>,
    manifest_world_id: &str,
    manifest_hash: impl FnOnce() -> Result<String, WorldError>,
) -> Result<(String, String, u64, Option<String>, String, u64, String), WorldError> {
    let binding = binding
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
    let finality_block_hash = strict_optional_finality_hash(binding.get("finality_block_hash"))?;
    if !finality_binding_is_legal(&finality_status, finality_block_hash.as_deref()) {
        return Err(cognition_validation("runtime_binding_invalid"));
    }
    let runtime_manifest_hash = manifest_hash()?;
    if binding
        .get("runtime_manifest_hash")
        .and_then(JsonValue::as_str)
        != Some(runtime_manifest_hash.as_str())
    {
        return Err(cognition_validation("runtime_manifest_mismatch"));
    }
    Ok((
        world_id,
        branch_id,
        finality_epoch,
        finality_block_hash,
        finality_status,
        reorg_epoch,
        runtime_manifest_hash,
    ))
}
