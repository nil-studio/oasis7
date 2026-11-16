//! Permanent typed terminal facts. Not an installation or signature authority.
use oasis7_agent_api::game::{GameTaskClosedV2, GameTaskRevokeAndCloseV2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Complete Owner signed original is retained for strict Runtime revalidation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskTerminalRecordV2 {
    pub schema_revision: u32,
    pub original: crate::service_admission::SignedReadRequest<GameTaskRevokeAndCloseV2>,
    pub committed_height: u64,
    pub committed_at_unix_ms: u64,
    /// Whole canonical predecessor bytes, decoded and verified by Runtime.
    pub stop_predecessor_canonical_cbor: Option<Vec<u8>>,
    pub closed: GameTaskClosedV2,
}
/// No default fields: missing records/version in a present V2 value is corrupt.
/// This index is authority state, never eligible for cold-result eviction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskTerminalIndexV2 {
    pub schema_revision: u32,
    pub records: BTreeMap<String, TaskTerminalRecordV2>,
}
/// The original index header gate, shared by whole-index and paid per-record
/// consumers. This performs no signature or installation authorization.
pub fn validate_task_terminal_index_header_v2(index: &TaskTerminalIndexV2) -> Result<(), String> {
    if index.schema_revision != 2 || index.records.is_empty() {
        return Err("invalid permanent task terminal index version or empty records".into());
    }
    Ok(())
}

/// The original fixed record predicates in their original order. Bounded
/// consumers admit this record's actual shape before invoking this kernel.
pub fn validate_task_terminal_record_v2(
    task_id: &str,
    record: &TaskTerminalRecordV2,
) -> Result<(), String> {
    let p = &record.original.request;
    p.validate()?;
    let c = &record.closed;
    if record.schema_revision != 2
        || record.committed_height == 0
        || record.committed_at_unix_ms == 0
        || c.schema_revision != 2
        || record.committed_at_unix_ms > p.cleanup_until_ms.0
        || p.api_revision != 2
        || task_id != c.initial_fence.task_id
        || c.initial_fence != p.permitted_branches.exact_initial_active
        || c.close_operation_id != p.client_operation_id
        || c.termination_plan_digest != p.termination_plan_digest
        || c.revoked_signer_public_key != p.signer_public_key
        || c.revoked_generation != p.revoke_generation
        || c.permanent_nonce_high_water != p.revoke_nonce
        || p.revoke_generation.0 <= p.signer_generation.0
        || p.revoke_nonce.0 <= p.signer_nonce.0
    {
        return Err("invalid permanent task terminal record".into());
    }
    Ok(())
}
impl TaskTerminalIndexV2 {
    pub fn validate(&self) -> Result<(), String> {
        match validate_task_terminal_index_with_work_v2(self,&mut|_,_|Ok::<(),std::convert::Infallible>(())) {Ok(())=>Ok(()),Err(TaskTerminalIndexValidationErrorV2::Invalid(note))=>Err(note),Err(TaskTerminalIndexValidationErrorV2::Work(never))=>match never{}}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_index_requires_explicit_v2_records() {
        for invalid in [
            serde_json::json!({}),
            serde_json::json!({"schema_revision":2}),
            serde_json::json!({"records":{}}),
            serde_json::json!({"schema_revision":1,"records":{}}),
        ] {
            if let Ok(index) = serde_json::from_value::<TaskTerminalIndexV2>(invalid) {
                assert!(index.validate().is_err());
            }
        }
    }
    #[test]
    fn legacy_empty_revocation_shape_has_no_implicit_terminal_index() {
        let old = crate::CapabilityRevocationState::default();
        let value = serde_json::to_value(&old).unwrap();
        assert!(value.get("task_terminals_v2").is_none());
        let restored: crate::CapabilityRevocationState =
            serde_json::from_value(value.clone()).unwrap();
        assert!(restored.task_terminals_v2.is_none());
        assert_eq!(serde_json::to_value(restored).unwrap(), value);
        let mut present = value;
        present["task_terminals_v2"] = serde_json::json!({"schema_revision":2});
        assert!(serde_json::from_value::<crate::CapabilityRevocationState>(present).is_err());
    }
}

#[path="task_terminal_v2_layout.rs"]
mod owned_layout;
pub use owned_layout::{task_terminal_record_owned_layout_v2,task_terminal_index_owned_layout_v2};

/// Work callbacks cannot replace a record predicate or change its ordering.
/// The fixed bounded caller admits the actual flat record graph and any error
/// string before the original shape/comparison kernel runs.
pub enum TaskTerminalIndexValidationErrorV2<E>{Work(E),Invalid(String)}
pub fn validate_task_terminal_index_with_work_v2<E>(index:&TaskTerminalIndexV2,work:&mut impl FnMut(&str,&TaskTerminalRecordV2)->Result<(),E>)->Result<(),TaskTerminalIndexValidationErrorV2<E>>{
 validate_task_terminal_index_header_v2(index).map_err(TaskTerminalIndexValidationErrorV2::Invalid)?;
 for(task_id,record)in &index.records{work(task_id,record).map_err(TaskTerminalIndexValidationErrorV2::Work)?;validate_task_terminal_record_v2(task_id,record).map_err(TaskTerminalIndexValidationErrorV2::Invalid)?;}Ok(())
}
