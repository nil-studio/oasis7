//! Full wake binding and actual registered/admitted V2 source collection.
use super::*;
use crate::{
    cognition_scheduler::{CognitionScheduler, SchedulerWakeV1},
    cognition_wake::{AgentContinuation, ContinuationStatusV1},
};
use oasis7_agent_api::iws_scheduler_v2::IWS_CURRENT_COMMITMENT_SCHEME_V1;
use oasis7_world_state::cognition_wake::IWS_CONTINUATION_SCHEMA_V2;
pub(super) struct PreparedServiceRowsV2 {
    rows: Vec<PreparedWakeInputV2>,
    _fee: ReservedValue<()>,
}
impl PreparedServiceRowsV2 {
    pub(super) fn rows(&self) -> &[PreparedWakeInputV2] {
        &self.rows
    }
}
fn terminal(status: ContinuationStatusV1) -> bool {
    matches!(
        status,
        ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected
    )
}
pub(super) fn normalize(
    rows: &mut [AgentContinuation],
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    for value in rows.iter_mut().filter(|value| !terminal(value.status)) {
        if value.schema_version == IWS_CONTINUATION_SCHEMA_V2 {
            // All fields are already selected-shape paid; the V2 condition decoder has
            // finite scalar/position/text layouts before any canonical allocation.
            common.charge(32768, 256 << 10).map_err(formal)?;
            let _phase = common
                .memory()
                .reserve(256 << 10, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            oasis7_world_state::cognition_wake::validate_iws_continuation_fields_v2(value)
                .map_err(formal)?;
            let digest = values::digest(
                oasis7_world_state::cognition_wake::IWS_CONTINUATION_STATUS_DOMAIN_V2,
                &oasis7_world_state::cognition_wake::iws_continuation_status_digest_input_v2(value),
                &mut work.hashes,
                common,
            )?;
            common.charge(1, digest.len() as u64).map_err(formal)?;
            if value.continuation_status_digest.as_deref() != Some(digest.as_str()) {
                return Err(formal(crate::cognition_wake::WakeConditionError::new(
                    "recovery_pending",
                )));
            }
        } else {
            if value.continuation_status_digest.is_none() {
                let digest = values::digest(
                    "oasis7.cognition.continuation-status.v1",
                    &value.status_digest_input_v1(),
                    &mut work.hashes,
                    common,
                )?;
                common.charge(1, digest.len() as u64).map_err(formal)?;
                value.continuation_status_digest = Some(digest.to_string());
            }
            common.charge(8192, 32768).map_err(formal)?;
            let _phase = common
                .memory()
                .reserve(32768, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            value.validate_authoritative().map_err(formal)?;
        }
    }
    Ok(())
}
pub(super) fn hydrate(
    value: &mut serde_json::Value,
    continuations: &[AgentContinuation],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    for bucket in ["active", "backpressure", "in_flight"] {
        let Some(entries) = value.get_mut(bucket) else {
            continue;
        };
        let mut apply = |entry: &mut serde_json::Value| -> Result<(), BodyMutationError> {
            common
                .charge(
                    continuations.len() as u64,
                    continuations
                        .iter()
                        .map(|row| row.continuation_id.len() + row.origin_request_digest.len())
                        .sum::<usize>() as u64,
                )
                .map_err(formal)?;
            let Some(object) = entry.as_object_mut() else {
                return Ok(());
            };
            if object
                .get("request_digest")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| !value.is_empty())
            {
                return Ok(());
            }
            let id = object
                .get("continuation_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if let Some(row) = continuations
                .iter()
                .filter(|row| !terminal(row.status))
                .find(|row| row.continuation_id == id)
            {
                common
                    .charge(1, row.origin_request_digest.len() as u64)
                    .map_err(formal)?;
                object.insert(
                    "request_digest".into(),
                    serde_json::Value::String(row.origin_request_digest.clone()),
                );
            }
            Ok(())
        };
        match entries {
            serde_json::Value::Array(rows) => {
                for entry in rows {
                    apply(entry)?
                }
            }
            serde_json::Value::Object(rows) => {
                for entry in rows.values_mut() {
                    apply(entry)?
                }
            }
            _ => {}
        }
    }
    Ok(())
}
fn same_wake(wake: &SchedulerWakeV1, value: &AgentContinuation) -> bool {
    value.wake_id == wake.wake_id
        && value.world_id == wake.world_id
        && value.branch_id == wake.branch_id
        && value.finality_epoch == wake.finality_epoch
        && value.finality_block_hash == wake.finality_block_hash
        && value.finality_status == wake.finality_status
        && value.reorg_epoch == wake.reorg_epoch
        && value.runtime_manifest_hash == wake.runtime_manifest_hash
        && value.agent_id == wake.agent_id
        && value.agent_session_id == wake.agent_session_id
        && value.agent_turn_id == wake.agent_turn_id
        && value.decision_request_id == wake.decision_request_id
        && value.origin_request_digest == wake.request_digest
}
fn binding_matches(wake: &SchedulerWakeV1, binding: &serde_json::Value) -> bool {
    let hash = match binding.get("finality_block_hash") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) if !value.trim().is_empty() => Some(value.as_str()),
        _ => return false,
    };
    binding.get("world_id").and_then(serde_json::Value::as_str) == Some(wake.world_id.as_str())
        && binding.get("branch_id").and_then(serde_json::Value::as_str)
            == Some(wake.branch_id.as_str())
        && binding
            .get("finality_epoch")
            .and_then(serde_json::Value::as_u64)
            == Some(wake.finality_epoch)
        && hash == wake.finality_block_hash.as_deref()
        && binding
            .get("finality_status")
            .and_then(serde_json::Value::as_str)
            == Some(wake.finality_status.as_str())
        && binding
            .get("reorg_epoch")
            .and_then(serde_json::Value::as_u64)
            == Some(wake.reorg_epoch)
        && binding
            .get("runtime_manifest_hash")
            .and_then(serde_json::Value::as_str)
            == Some(wake.runtime_manifest_hash.as_str())
}
fn validate_admission_data(
    context: &IwsContinuationContextV2,
    continuation: &AgentContinuation,
    work: &mut CurrentCognitionExpiryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    common.charge(32768, 256 << 10).map_err(formal)?;
    let _phase = common
        .memory()
        .reserve(256 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if context.schema_version != "iws-continuation-context.v2"
        || context.commitment_scheme != IWS_CURRENT_COMMITMENT_SCHEME_V1
        || context.continuation_id != continuation.continuation_id
        || context.wake_id != continuation.wake_id
        || context.admission_commitment.len() != 71
        || !context.admission_commitment.starts_with("sha256:")
    {
        return Err(bad().into());
    }
    context.proposal.validate_data().map_err(formal)?;
    let data = context.proposal.as_data();
    if data.continuation_proposal_id != continuation.continuation_proposal_id
        || data.proposal_digest != continuation.proposal_digest
        || data.world_id != continuation.world_id
        || data.branch_id != continuation.branch_id
        || data.finality_epoch != continuation.finality_epoch
        || data.finality_block_hash != continuation.finality_block_hash
        || data.finality_status != continuation.finality_status
        || data.reorg_epoch != continuation.reorg_epoch
        || data.runtime_manifest_hash != continuation.runtime_manifest_hash
        || data.agent_id != continuation.agent_id
        || data.agent_session_id != continuation.agent_session_id
        || data.agent_turn_id != continuation.agent_turn_id
        || data.decision_request_id != continuation.decision_request_id
        || data.origin_turn_id != continuation.origin_turn_id
        || data.origin_request_digest != continuation.origin_request_digest
        || data.action_or_envelope_digest != continuation.action_or_envelope_digest
        || data.wake_conditions != continuation.wake_conditions
        || data.valid_until_tick != continuation.valid_until_tick
        || data.precondition_digest != continuation.precondition_digest
    {
        return Err(bad().into());
    }
    context.admission_frame.validate_data()?;
    let wire = oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
        &context.admission_frame,
        work.max_selected_heap,
        common,
        "IWS admitted frame",
        "IWS admitted frame encode",
    )
    .map_err(formal)?;
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    for chunk in wire.chunks(4096) {
        common.charge(1, chunk.len() as u64).map_err(formal)?;
        hash.update(chunk)
    }
    let digest = hash.finalize();
    let mut text = [0u8; 71];
    text[..7].copy_from_slice(b"sha256:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (i, byte) in digest.iter().enumerate() {
        text[7 + 2 * i] = HEX[(byte >> 4) as usize];
        text[8 + 2 * i] = HEX[(byte & 15) as usize];
    }
    if context.admission_commitment.as_bytes() != text {
        return Err(bad().into());
    }
    let digest = values::digest(
        "oasis7.cognition.iws-continuation-admission.v2",
        &context.context_core_input(),
        &mut work.hashes,
        common,
    )?;
    common
        .charge(1, (digest.len() + context.context_core_digest.len()) as u64)
        .map_err(formal)?;
    if digest.as_str() != context.context_core_digest {
        return Err(bad().into());
    }
    Ok(())
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(super) fn prepare_current_cognition_service_rows_v2<P, PE, PC>(
        &mut self,
        sources: &CurrentCognitionServiceSourcesV2<'_, '_>,
        state: &state::CurrentServiceStateV2,
        pending: &PendingCurrentCognitionJournalV1<'_>,
        scheduler: &CognitionScheduler,
        continuations: &[AgentContinuation],
        binding: &serde_json::Value,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PreparedServiceRowsV2, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let count = scheduler.snapshot_order_wakes_v2().count();
        let bytes = count
            .checked_mul(std::mem::size_of::<PreparedWakeInputV2>())
            .ok_or_else(bad)?;
        common
            .charge(count as u64, bytes as u64)
            .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?
            .retain(());
        let mut rows = Vec::new();
        rows.try_reserve_exact(count)
            .map_err(|_| ValueTreeError::ResourceLimited("service wake inputs"))?;
        if rows.capacity() > count {
            return Err(ValueTreeError::ResourceLimited("service wake input capacity").into());
        }
        let evidence = {
            let ctx = &mut self.ctx;
            let roots = sources.parent.refs().roots;
            let mut read = WorldFieldReadContextV1 {
                root: roots.state_root(
                    oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts,
                ),
                state: parent_read.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            crate::world::incremental_normal_cognition_service_evidence::prepare_current_cognition_wake_evidence_v2(sources.evidence,sources.accepted_ledger,sources.parent,sources.ledger,pending,ctx.transaction,&mut read,work.expiry,work.path,common,&live_owner()).map_err(|e|match e{CurrentEvidenceErrorV2::Body(e)=>RootModuleExecutionErrorV1::Body(e),CurrentEvidenceErrorV2::Business(e)=>RootModuleExecutionErrorV1::Business(e)})?
        };
        for (wake_index, wake) in scheduler.snapshot_order_wakes_v2().enumerate() {
            common
                .charge(
                    1,
                    crate::cognition_scheduler::scheduler_wake_text_bytes_v1(wake) as u64,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            wake.validate()
                .map_err(|e| business::<O::Error>(e.code(), common))?;
            common
                .charge(
                    continuations.len() as u64,
                    continuations
                        .iter()
                        .map(|row| row.continuation_id.len())
                        .sum::<usize>() as u64,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let value = continuations
                .iter()
                .find(|value| value.continuation_id == wake.continuation_id)
                .ok_or_else(|| business::<O::Error>("foreign_scheduler_wake", common))?;
            if !binding_matches(wake, binding) || !same_wake(wake, value) {
                return Err(business("foreign_scheduler_wake", common));
            }
            let contexts = state
                .contexts
                .as_ref()
                .and_then(|owner| owner.value().as_object());
            let keep = !terminal(value.status)
                && contexts.is_none_or(|map| map.contains_key(&value.continuation_id));
            let row_fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<PreparedWakeInputV2>()
                        + wake.wake_id.len()
                        + value.continuation_id.len(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            if !keep {
                rows.push(PreparedWakeInputV2 {
                    wake_id: wake.wake_id.clone(),
                    continuation_id: value.continuation_id.clone(),
                    keep: false,
                    evaluation: None,
                    _fee: row_fee,
                });
                continue;
            }
            if wake_index >= scheduler.active_count_v2() {
                rows.push(PreparedWakeInputV2 {
                    wake_id: wake.wake_id.clone(),
                    continuation_id: value.continuation_id.clone(),
                    keep: true,
                    evaluation: None,
                    _fee: row_fee,
                });
                continue;
            }
            if value.schema_version != IWS_CONTINUATION_SCHEMA_V2 {
                return Err(business("iws_legacy_current_world_root_required", common));
            }
            let context = contexts
                .and_then(|map| map.get(&value.continuation_id))
                .ok_or_else(|| business::<O::Error>("iws_continuation_context_required", common))?;
            let context_json = values::json(context, work.expiry.max_selected_heap, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let context = IwsContinuationContextV2::deserialize(&context_json.value)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            validate_admission_data(&context, value, work.expiry, common)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let registration = self.prepare_current_registered_cognition_turn_v2(
                sources.parent,
                &context.proposal,
                Some(pending),
                work.expiry,
                common,
                live_owner,
            )?;
            let admitted = self.prepare_current_admitted_iws_continuation_v2(
                sources.parent,
                &context,
                Some(pending),
                work.expiry,
                common,
                live_owner,
            )?;
            let spec_fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<PreparedIwsWakeConditionsV2>() + (256 << 10),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?
                .retain(());
            common
                .charge(value.wake_conditions.len() as u64, 16 << 10)
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let conditions = value
                .wake_conditions
                .iter()
                .cloned()
                .map(IwsWakeConditionV2::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| RootModuleExecutionErrorV1::Body(formal(e)))?;
            let predicates = self.read_current_cognition_predicate_values_v2(
                sources,
                &conditions,
                &wake.world_id,
                work,
                parent_read,
                common,
                &live_owner(),
            )?;
            let mut events = Vec::new();
            let mut receipts = Vec::new();
            for condition in &conditions {
                if let Some(digest) = condition.event_digest() {
                    if evidence
                        .contains_event(digest, self.ctx.transaction, common, &live_owner())
                        .map_err(RootModuleExecutionErrorV1::Body)?
                    {
                        events.push(digest.to_owned())
                    }
                }
                if let Some(id) = condition.receipt_id() {
                    if evidence
                        .contains_receipt(id, self.ctx.transaction, common, &live_owner())
                        .map_err(RootModuleExecutionErrorV1::Body)?
                    {
                        receipts.push(id.to_owned())
                    }
                }
            }

            rows.push(PreparedWakeInputV2 {
                wake_id: wake.wake_id.clone(),
                continuation_id: value.continuation_id.clone(),
                keep: true,
                evaluation: Some(PreparedIwsWakeConditionsV2 {
                    conditions,
                    predicates,
                    events,
                    receipts,
                    registration: context.proposal.registration().clone(),
                    valid_until: value.valid_until_tick,
                    _registration: registration,
                    _admitted: admitted,
                    _context: context_json,
                    _fee: spec_fee,
                }),
                _fee: row_fee,
            });
        }
        drop(evidence);
        Ok(PreparedServiceRowsV2 { rows, _fee: fee })
    }
}
