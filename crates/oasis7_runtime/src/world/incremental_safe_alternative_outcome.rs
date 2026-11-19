//! Fixed original outcome predicate. Only a genuine persisted event can enter
//! the private producer; DATA projection alone never grants a mutation.
use super::incremental_action_execution::SelectedPendingActionV1;
use super::incremental_body::BodyMutationError;
use super::incremental_module_execution_event::PersistedWorldEventV1;
use super::*;
use crate::{CausedBy, DomainEvent, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use serde_json::{Value, json};
fn error(reason: &str) -> WorldError {
    super::safe_alternative::error(reason)
}
pub(super) fn project_outcome_record_v1(
    mut r: Value,
    action_id: u64,
    event: &WorldEvent,
) -> Result<Value, WorldError> {
    if r["authorization"]["action_id"].as_u64() != Some(action_id) {
        return Err(error("identity_conflict"));
    }
    if !matches!(event.caused_by,Some(CausedBy::Action(id)) if id==action_id) {
        return Err(error("outcome_unknown"));
    }
    let (status, reason) = match &event.body {
        WorldEventBody::Domain(DomainEvent::AgentMoved { agent_id, from, to }) => {
            let candidate = r["candidates"]
                .as_array()
                .and_then(|v| {
                    v.iter()
                        .find(|v| v["candidate_id"] == r["confirmation"]["candidate_id"])
                })
                .ok_or_else(|| error("outcome_unknown"))?;
            if json!(agent_id) != r["agent_id"]
                || json!(from) != r["initial_position"]
                || json!(to) != candidate["to"]
            {
                return Err(error("identity_conflict"));
            }
            ("applied", Value::Null)
        }
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            action_id: rejected_id,
            reason,
        }) if *rejected_id == action_id => ("rejected", json!(reason)),
        _ => return Err(error("outcome_unknown")),
    };
    let outcome =
        json!({"status":status,"action_id":action_id,"event_id":event.id,"reason":reason});
    if !r["outcome"].is_null() && r["outcome"] != outcome {
        return Err(error("identity_conflict"));
    }
    r["outcome"] = outcome;
    r["state"] = json!(status);
    r["authorization"]["state"] = json!(if status == "applied" {
        "consumed"
    } else {
        "revoked"
    });
    Ok(r)
}
/// This paid DATA requires the still-private matching owner before publication.
/// Keeping this distinct prevents a valid arbitrary MoveAgent event from
/// granting SafeAlternative execution retroactively.
pub(super) struct PreparedSafeAlternativeOutcomeDataV1 {
    record: ReservedValue<Value>,
    event_id: u64,
    action_id: u64,
    guard: PreparedGuard,
    core: WorkIdentityV1,
}
impl PreparedSafeAlternativeOutcomeDataV1 {
    pub(super) fn record(&self) -> &Value {
        &self.record
    }
    pub(super) fn event_id(&self) -> u64 {
        self.event_id
    }
    pub(super) fn action_id(&self) -> u64 {
        self.action_id
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_transaction_identity(&self.guard)?;
        if self.core != common.work().identity() || !self.record.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid("SafeAlternative outcome source"));
        }
        Ok(())
    }
}
#[derive(Debug)]
struct SafeAlternativeWorldErrorV1(WorldError);
impl std::fmt::Display for SafeAlternativeWorldErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for SafeAlternativeWorldErrorV1 {}
/// The closed JSON clone/error projection is admitted from actual measured
/// original bytes; no caller-reported heap amount controls this charge.
fn json_node_bound() -> Option<usize> {
    use std::mem::{align_of, size_of};
    use std::ptr::NonNull;
    let a = align_of::<String>()
        .max(align_of::<Value>())
        .max(align_of::<Option<NonNull<()>>>())
        .max(align_of::<u16>());
    (size_of::<String>() + size_of::<Value>())
        .checked_mul(11)?
        .checked_add(
            size_of::<Option<NonNull<()>>>() + 2 * size_of::<u16>() + 12 * size_of::<NonNull<()>>(),
        )?
        .checked_add((a - 1) * 7)
}
fn json_clone_bound(
    value: &Value,
    depth: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    if depth > 64 {
        return Err(ValueTreeError::ResourceLimited("SafeAlternative JSON depth").into());
    }
    common
        .consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let limit = || ValueTreeError::ResourceLimited("SafeAlternative JSON layout");
    let mut bytes = std::mem::size_of::<Value>();
    match value {
        Value::String(text) => bytes = bytes.checked_add(text.len()).ok_or_else(limit)?,
        Value::Array(values) => {
            bytes = bytes
                .checked_add(
                    values
                        .len()
                        .checked_mul(4 * std::mem::size_of::<Value>())
                        .ok_or_else(limit)?,
                )
                .ok_or_else(limit)?;
            for child in values {
                bytes = bytes
                    .checked_add(json_clone_bound(child, depth + 1, common)?)
                    .ok_or_else(limit)?;
            }
        }
        Value::Object(values) => {
            bytes = bytes
                .checked_add(
                    values
                        .len()
                        .checked_mul(json_node_bound().ok_or_else(limit)?)
                        .ok_or_else(limit)?,
                )
                .ok_or_else(limit)?;
            for (key, child) in values {
                common
                    .charge(1, key.len() as u64)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                bytes = bytes.checked_add(key.len()).ok_or_else(limit)?;
                bytes = bytes
                    .checked_add(json_clone_bound(child, depth + 1, common)?)
                    .ok_or_else(limit)?;
            }
        }
        _ => {}
    }
    Ok(bytes)
}
pub(super) fn prepare_outcome_data_v1<M: MerkleNodeReader>(
    record: &Value,
    selected: &SelectedPendingActionV1,
    event: &PersistedWorldEventV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PreparedSafeAlternativeOutcomeDataV1, BodyMutationError> {
    let result = (|| {
        selected.validate(tx)?;
        event.validate(tx)?;
        let actual =
            oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid(
                event.event(),
                64 << 20,
                common,
                "SafeAlternative result bound",
                "SafeAlternative actual result",
            )
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        // Each emitted CBOR token occupies at least one byte. A closed JSON
        // projection can allocate at most one full pinned BTree node and one Vec
        // growth allowance per token, plus owned UTF8 bytes. This uses the actual
        // String/Value/pointer ABI, not an unexplained per-byte multiplier.
        let node = json_node_bound().ok_or(ValueTreeError::ResourceLimited(
            "SafeAlternative outcome node",
        ))?;
        let per_token = node
            .checked_add(4 * std::mem::size_of::<Value>() + std::mem::size_of::<String>() + 1)
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative outcome ABI",
            ))?;
        let record_bytes = json_clone_bound(record, 0, common)?;
        let bytes = actual
            .len()
            .checked_mul(per_token)
            .and_then(|n| n.checked_add(record_bytes))
            .and_then(|n| {
                n.checked_add(
                    8 * node + std::mem::size_of::<PreparedSafeAlternativeOutcomeDataV1>() + 512,
                )
            })
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative outcome heap",
            ))?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let record_work =
            super::incremental_safe_alternative_allocation::safe_alternative_owned_bound_v1(
                record, common,
            )?;
        // Work follows actual source tokens/text, separately from pinned JSON
        // node heap. encode_paid above already paid the actual event encoding.
        let event_tokens = u64::try_from(actual.len())
            .map_err(|_| ValueTreeError::ResourceLimited("SafeAlternative event work"))?;
        let visits = event_tokens
            .checked_mul(16)
            .and_then(|n| n.checked_add(1024))
            .and_then(|n| n.checked_add(record_work.visits))
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative outcome visits",
            ))?;
        let text = event_tokens
            .checked_mul(14 * 16)
            .and_then(|n| n.checked_add(4096))
            .and_then(|n| n.checked_add(record_work.bytes))
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative outcome bytes",
            ))?;
        common
            .charge(visits, text)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let result =
            project_outcome_record_v1(record.clone(), selected.envelope().id, event.event())
                .map_err(|e| {
                    BodyMutationError::Original(Box::new(SafeAlternativeWorldErrorV1(e)))
                })?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(PreparedSafeAlternativeOutcomeDataV1 {
            record: fee.retain(result),
            event_id: event.event().id,
            action_id: selected.envelope().id,
            guard: tx.guard(),
            core: common.work().identity(),
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
            .contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
