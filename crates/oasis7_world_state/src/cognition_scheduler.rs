//! Durable scheduler policy, wake, cursor, and metrics values.
//! Queue selection, capacity, and recovery policy are owned by Runtime.

use serde::{Deserialize, Serialize};
use std::fmt;

use super::cognition::is_canonical_identifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerError {
    code: &'static str,
}

impl SchedulerError {
    pub fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &str {
        self.code
    }
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl std::error::Error for SchedulerError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerPolicyV1 {
    pub schema_version: String,
    pub max_total_wakes_per_tick: usize,
    pub max_wakes_per_agent_per_tick: usize,
    pub aging_after_ticks: u64,
    pub max_starvation_ticks: u64,
    pub initial_priority: i64,
    pub comparator: String,
    pub service_order: String,
}

impl SchedulerPolicyV1 {
    pub const SCHEMA_VERSION: &'static str = "scheduler-policy.v1";
    pub const COMPARATOR: &'static str = "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc";
    pub const SERVICE_ORDER: &'static str = "stable_round_robin";

    pub fn validate(&self) -> Result<(), SchedulerError> {
        if self.schema_version != Self::SCHEMA_VERSION
            || self.max_total_wakes_per_tick == 0
            || self.max_total_wakes_per_tick > 4096
            || self.max_wakes_per_agent_per_tick == 0
            || self.max_wakes_per_agent_per_tick > self.max_total_wakes_per_tick
            || self.aging_after_ticks == 0
            || self.max_starvation_ticks == 0
            || self.initial_priority != 0
            || self.comparator != Self::COMPARATOR
            || self.service_order != Self::SERVICE_ORDER
        {
            return Err(SchedulerError::new("invalid_scheduler_policy"));
        }
        Ok(())
    }

    pub fn policy_config_digest(&self) -> String {
        let payload = SchedulerPolicyDigestInput {
            max_total_wakes_per_tick: self.max_total_wakes_per_tick,
            max_wakes_per_agent_per_tick: self.max_wakes_per_agent_per_tick,
            aging_after_ticks: self.aging_after_ticks,
            max_starvation_ticks: self.max_starvation_ticks,
            initial_priority: self.initial_priority,
            comparator: &self.comparator,
            service_order: &self.service_order,
        };
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.runtime.scheduler-policy.v1",
            &payload,
        ))
        .expect("scheduler policy must be canonically encodable");
        format!("blake3:{}", blake3::hash(&bytes))
    }
}

#[derive(Debug, Serialize)]
struct SchedulerPolicyDigestInput<'a> {
    max_total_wakes_per_tick: usize,
    max_wakes_per_agent_per_tick: usize,
    aging_after_ticks: u64,
    max_starvation_ticks: u64,
    initial_priority: i64,
    comparator: &'a str,
    service_order: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerWakeV1 {
    pub schema_version: String,
    pub wake_id: String,
    pub continuation_id: String,
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default)]
    pub finality_block_hash: Option<String>,
    pub finality_status: String,
    pub reorg_epoch: u64,
    pub runtime_manifest_hash: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    /// The request digest that originated this continuation wake. Legacy
    /// scheduler snapshots may omit it; Runtime adapters must fail closed
    /// until an authoritative migration supplies the digest.
    #[serde(default)]
    pub request_digest: String,
    pub next_wake_tick: u64,
    pub eligible_since_tick: u64,
    pub starvation_deadline_tick: u64,
    pub initial_priority: i64,
    pub wake_seq: u64,
    #[serde(default)]
    pub retry_seq: u64,
    pub status: String,
    pub pending_reason: String,
}

impl SchedulerWakeV1 {
    pub const SCHEMA_VERSION: &'static str = "scheduler-wake.v1";

    pub fn validate(&self) -> Result<(), SchedulerError> {
        let bounded = |value: &str| is_canonical_identifier(value, 128);
        if self.schema_version != Self::SCHEMA_VERSION
            || !bounded(&self.wake_id)
            || !bounded(&self.continuation_id)
            || !bounded(&self.world_id)
            || !bounded(&self.branch_id)
            || self
                .finality_block_hash
                .as_deref()
                .is_some_and(|hash| !bounded(hash))
            || !bounded(&self.finality_status)
            || !bounded(&self.runtime_manifest_hash)
            || !bounded(&self.agent_id)
            || !bounded(&self.agent_session_id)
            || !bounded(&self.agent_turn_id)
            || !bounded(&self.decision_request_id)
            || (!self.request_digest.is_empty() && !bounded(&self.request_digest))
            || !bounded(&self.status)
            || !bounded(&self.pending_reason)
            || self.status != "pending"
            || !crate::cognition::finality_binding_is_legal(
                &self.finality_status,
                self.finality_block_hash.as_deref(),
            )
        {
            return Err(SchedulerError::new("invalid_scheduler_wake"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchedulerEnqueueOutcome {
    pub disposition: String,
    pub reason: String,
    pub provider_invocation_count: u64,
    pub world_event_count: u64,
    pub effect_count: u64,
    pub debit_count: u64,
    pub receipt_count: u64,
    pub world_receipt_linked_count: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerExecutionMetrics {
    pub recovery_wake_count: u64,
    pub provider_invocation_count: u64,
    pub effect_count: u64,
    pub debit_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerCursorV1 {
    pub schema_version: String,
    pub logical_tick: u64,
    pub last_served_agent_id: Option<String>,
    pub cursor_seq: u64,
    pub policy_config_digest: String,
}

impl SchedulerCursorV1 {
    pub fn new(policy: &SchedulerPolicyV1) -> Self {
        Self {
            schema_version: "scheduler-cursor.v1".to_string(),
            logical_tick: 0,
            last_served_agent_id: None,
            cursor_seq: 0,
            policy_config_digest: policy.policy_config_digest(),
        }
    }
}
