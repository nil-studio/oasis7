//! Exact lineage and wake binding comparisons shared by checkpoint recovery.
use super::*;

pub(in super::super) fn provider_context_matches_wake(
    context: &cognition_context::ProviderContextState,
    wake: &crate::runtime::SchedulerWakeV1,
) -> bool {
    context.request_context.agent_subject == wake.agent_id
        && context.request_context.agent_session_id == wake.agent_session_id
        && context.request_context.agent_turn_id == wake.agent_turn_id
        && context.request_context.decision_request_id == wake.decision_request_id
        && !wake.request_digest.is_empty()
        && context.request_context.request_digest.to_string() == wake.request_digest
}

pub(in super::super) fn runtime_binding_advances_same_lineage(
    original: &crate::simulator::RuntimeBindingV1,
    current: &crate::simulator::RuntimeBindingV1,
) -> bool {
    original.world_id == current.world_id
        && original.branch_id == current.branch_id
        && original.finality_epoch == current.finality_epoch
        && original.finality_block_hash == current.finality_block_hash
        && original.finality_status == current.finality_status
        && original.reorg_epoch == current.reorg_epoch
        && original.runtime_manifest_hash == current.runtime_manifest_hash
        && current.base_tick >= original.base_tick
        && (current.base_tick != original.base_tick
            || current.base_world_hash == original.base_world_hash)
}

pub(in super::super) fn handoff_checkpoint_binding_matches_origin_and_wake(
    original: &crate::simulator::RuntimeBindingV1,
    wake: &crate::runtime::SchedulerWakeV1,
    captured: &crate::runtime::RuntimeCognitionBaseBindingV1,
) -> bool {
    let original_finality_hash = original
        .finality_block_hash
        .as_ref()
        .map(ToString::to_string);
    let captured_finality_hash = captured.finality_block_hash.as_deref();
    wake.world_id == original.world_id
        && wake.branch_id == original.branch_id
        && wake.finality_epoch == original.finality_epoch
        && wake.finality_block_hash == original_finality_hash
        && wake.finality_status == original.finality_status
        && wake.reorg_epoch == original.reorg_epoch
        && wake.runtime_manifest_hash == original.runtime_manifest_hash.to_string()
        && captured.world_id == original.world_id
        && captured.branch_id == original.branch_id
        && captured.finality_epoch == original.finality_epoch
        && captured_finality_hash == original_finality_hash.as_deref()
        && captured.finality_status == original.finality_status
        && captured.reorg_epoch == original.reorg_epoch
        && captured.runtime_manifest_hash == original.runtime_manifest_hash.to_string()
        && captured.base_tick >= original.base_tick
        && (captured.base_tick != original.base_tick
            || captured.base_world_hash == original.base_world_hash.to_string())
}

pub(in super::super) fn handoff_checkpoint_matches_verified_binding(
    original: &crate::simulator::RuntimeBindingV1,
    wake: &crate::runtime::SchedulerWakeV1,
    captured: &crate::runtime::RuntimeCognitionBaseBindingV1,
    current: &crate::simulator::RuntimeBindingV1,
) -> bool {
    handoff_checkpoint_binding_matches_origin_and_wake(original, wake, captured)
        && runtime_binding_advances_same_lineage(original, current)
        && captured.base_tick <= current.base_tick
        && (captured.base_tick != current.base_tick
            || captured.base_world_hash == current.base_world_hash.to_string())
}
