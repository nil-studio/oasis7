//! Bounded, deterministic scheduling for durable cognition wakes.
//!
//! This module deliberately contains no provider or World calls.  Enqueue and
//! selection are small state-machine operations so a full queue can be
//! represented as durable pending state rather than making a tick worker wait.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::cognition::is_canonical_identifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerError {
    code: &'static str,
}

impl SchedulerError {
    fn new(code: &'static str) -> Self {
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
    fn new(policy: &SchedulerPolicyV1) -> Self {
        Self {
            schema_version: "scheduler-cursor.v1".to_string(),
            logical_tick: 0,
            last_served_agent_id: None,
            cursor_seq: 0,
            policy_config_digest: policy.policy_config_digest(),
        }
    }
}

/// A bounded queue plus the durable overflow set.  `capacity` counts queued
/// and in-flight slots; it never causes a caller to block.
#[derive(Debug, Serialize, Deserialize)]
pub struct CognitionScheduler {
    policy: SchedulerPolicyV1,
    capacity: usize,
    active: Vec<SchedulerWakeV1>,
    backpressure: BTreeMap<String, SchedulerWakeV1>,
    backpressure_priority: BTreeMap<String, i64>,
    recovered_wakes: BTreeSet<String>,
    /// Selected wakes remain durable until their owner reports completion,
    /// failure, cancellation, or recovery.  Keeping the full identity here
    /// prevents a restart from releasing an anonymous capacity slot and
    /// losing the wake that occupied it.
    in_flight: BTreeMap<String, SchedulerWakeV1>,
    logical_tick: u64,
    cursor: SchedulerCursorV1,
    metrics: SchedulerExecutionMetrics,
}

impl CognitionScheduler {
    pub fn try_new(policy: SchedulerPolicyV1, capacity: usize) -> Result<Self, SchedulerError> {
        policy.validate()?;
        if capacity == 0 {
            return Err(SchedulerError::new("invalid_scheduler_capacity"));
        }
        Ok(Self::new(policy, capacity))
    }

    pub fn new(policy: SchedulerPolicyV1, capacity: usize) -> Self {
        Self {
            cursor: SchedulerCursorV1::new(&policy),
            policy,
            capacity,
            active: Vec::new(),
            backpressure: BTreeMap::new(),
            backpressure_priority: BTreeMap::new(),
            recovered_wakes: BTreeSet::new(),
            in_flight: BTreeMap::new(),
            logical_tick: 0,
            metrics: SchedulerExecutionMetrics::default(),
        }
    }

    pub fn try_enqueue(
        &mut self,
        mut wake: SchedulerWakeV1,
    ) -> Result<SchedulerEnqueueOutcome, SchedulerError> {
        self.policy.validate()?;
        wake.validate()?;
        if self.active.iter().any(|item| item.wake_id == wake.wake_id)
            || self.backpressure.contains_key(&wake.wake_id)
            || self.in_flight.contains_key(&wake.wake_id)
        {
            return Err(SchedulerError::new("wake_id_conflict"));
        }
        // Eligibility is runtime-owned at the current logical tick; preserve
        // a pre-captured eligibility tick from the same Runtime transaction
        // when it is ahead of this scheduler cursor.  Caller priority and
        // starvation deadline are always normalized below.
        wake.eligible_since_tick = wake.eligible_since_tick.max(self.logical_tick);
        wake.initial_priority = self.policy.initial_priority;
        wake.starvation_deadline_tick = wake
            .eligible_since_tick
            .saturating_add(self.policy.max_starvation_ticks);
        wake.status = "pending".to_string();
        if self.available_slots() > 0 {
            wake.pending_reason = "capacity_available".to_string();
            self.active.push(wake);
            Ok(Self::accepted_outcome())
        } else {
            wake.pending_reason = "scheduler_backpressure".to_string();
            let wake_id = wake.wake_id.clone();
            self.backpressure_priority
                .insert(wake_id.clone(), self.effective_priority(&wake));
            self.backpressure.insert(wake_id, wake);
            // Queue-full is still a service attempt. Persisting this cursor
            // advancement makes repeated overload deterministic and prevents
            // a restored scheduler from replaying an unbounded stale prefix.
            self.cursor.logical_tick = self.logical_tick;
            self.cursor.cursor_seq = self.cursor.cursor_seq.saturating_add(1);
            Ok(SchedulerEnqueueOutcome {
                disposition: "pending".to_string(),
                reason: "scheduler_backpressure".to_string(),
                provider_invocation_count: 0,
                world_event_count: 0,
                effect_count: 0,
                debit_count: 0,
                receipt_count: 0,
                world_receipt_linked_count: 0,
            })
        }
    }

    pub fn enqueue_for_test(&mut self, wake: SchedulerWakeV1) {
        // The fixture intentionally bypasses queue capacity.  This models a
        // restored ready set and keeps selection tests independent of enqueue.
        if !self.active.iter().any(|item| item.wake_id == wake.wake_id) {
            self.active.push(wake);
        }
    }

    pub fn pending_backpressure_count(&self) -> usize {
        self.backpressure.len()
    }

    pub fn pending_backpressure(&self, wake_id: &str) -> Value {
        let Some(wake) = self.backpressure.get(wake_id) else {
            return Value::Null;
        };
        serde_json::json!({
            "wake_id": wake.wake_id,
            "continuation_id": wake.continuation_id,
            "retry_seq": wake.retry_seq,
            "eligible_since_tick": wake.eligible_since_tick,
            "effective_priority": self.backpressure_priority.get(wake_id).copied().unwrap_or_default(),
            "reason": "scheduler_backpressure"
        })
    }

    pub fn is_backpressured(&self, wake_id: &str) -> bool {
        self.backpressure.contains_key(wake_id)
    }

    pub fn advance_logical_tick(&mut self, tick: u64) {
        self.logical_tick = self.logical_tick.max(tick);
        self.cursor.logical_tick = self.logical_tick;
    }

    pub fn release_capacity(&mut self) {
        // Compatibility API for scheduler-only callers. World-owned paths
        // use `release_in_flight` or `deactivate_wake` with the exact ID.
        if let Some(wake_id) = self.in_flight.keys().next().cloned() {
            self.in_flight.remove(&wake_id);
        }
    }

    /// Release one selected wake by its durable identity. Terminal lifecycle
    /// transitions must not free an unrelated in-flight slot.
    pub fn release_in_flight(&mut self, wake_id: &str) -> Result<SchedulerWakeV1, SchedulerError> {
        self.in_flight
            .remove(wake_id)
            .ok_or_else(|| SchedulerError::new("in_flight_wake_missing"))
    }

    /// Remove a wake from every scheduler bucket. This is used by terminal,
    /// cancellation, and reorg transitions so a stale selected wake cannot be
    /// delivered after its continuation has become inactive.
    pub fn deactivate_wake(
        &mut self,
        wake_id: &str,
    ) -> Result<Option<SchedulerWakeV1>, SchedulerError> {
        let mut removed = self.in_flight.remove(wake_id);
        if removed.is_none()
            && let Some(index) = self.active.iter().position(|wake| wake.wake_id == wake_id)
        {
            removed = Some(self.active.remove(index));
        }
        if removed.is_none() {
            removed = self.backpressure.remove(wake_id);
            self.backpressure_priority.remove(wake_id);
        }
        Ok(removed)
    }

    /// Remove wakes that no longer have an active Runtime continuation. The
    /// predicate is evaluated across every durable bucket, including an
    /// in-flight wake restored from a prior process.
    pub fn prune_wakes_if<F>(&mut self, mut keep: F) -> Vec<SchedulerWakeV1>
    where
        F: FnMut(&SchedulerWakeV1) -> bool,
    {
        let mut removed = Vec::new();
        self.active.retain(|wake| {
            if keep(wake) {
                true
            } else {
                removed.push(wake.clone());
                false
            }
        });
        let backpressure_ids: Vec<String> = self
            .backpressure
            .iter()
            .filter(|(_, wake)| !keep(wake))
            .map(|(wake_id, _)| wake_id.clone())
            .collect();
        for wake_id in backpressure_ids {
            if let Some(wake) = self.backpressure.remove(&wake_id) {
                self.backpressure_priority.remove(&wake_id);
                removed.push(wake);
            }
        }
        self.in_flight.retain(|wake_id, wake| {
            if keep(wake) {
                true
            } else {
                let _ = wake_id;
                removed.push(wake.clone());
                false
            }
        });
        removed
    }

    /// Requeue all selected wakes after process recovery, retaining their
    /// complete identity and preserving the existing cursor. Overflow wakes
    /// remain in the durable backpressure map rather than being dropped.
    pub fn recover_in_flight(&mut self, tick: u64) -> Vec<SchedulerWakeV1> {
        self.advance_logical_tick(tick);
        let mut wakes: Vec<SchedulerWakeV1> = self.in_flight.values().cloned().collect();
        wakes.sort_by(|left, right| self.compare(left, right));
        self.in_flight.clear();
        let mut recovered = Vec::new();
        for mut wake in wakes {
            wake.pending_reason = "scheduler_recovery".to_string();
            if self.available_slots() > 0 {
                self.active.push(wake.clone());
                recovered.push(wake);
            } else {
                let wake_id = wake.wake_id.clone();
                let priority = self.effective_priority(&wake);
                self.backpressure_priority.insert(wake_id.clone(), priority);
                self.backpressure.insert(wake_id, wake.clone());
                recovered.push(wake);
            }
        }
        recovered
    }

    pub fn recover_capacity(&mut self, tick: u64) -> Vec<SchedulerWakeV1> {
        self.recover_capacity_if(tick, |wake| Self::is_ready_at(wake, tick))
    }

    /// Recover pending capacity with a World-owned eligibility predicate.
    ///
    /// The caller's predicate is authoritative for recovery readiness. World
    /// callers use it to re-evaluate every wake condition from committed
    /// evidence, including mixed tick-plus-evidence wakes. Scheduler-only
    /// callers should use [`Self::recover_capacity`], which supplies the
    /// timer/starvation readiness predicate.
    pub fn recover_capacity_if<F>(&mut self, tick: u64, mut eligible: F) -> Vec<SchedulerWakeV1>
    where
        F: FnMut(&SchedulerWakeV1) -> bool,
    {
        self.advance_logical_tick(tick);
        let mut candidates: Vec<SchedulerWakeV1> = self.backpressure.values().cloned().collect();
        candidates.sort_by(|left, right| self.compare(left, right));
        let mut recovered = Vec::new();
        for wake in candidates {
            if self.available_slots() == 0 {
                break;
            }
            let ready = eligible(&wake);
            if !ready {
                continue;
            }
            self.backpressure.remove(&wake.wake_id);
            self.backpressure_priority.remove(&wake.wake_id);
            self.recovered_wakes.insert(wake.wake_id.clone());
            self.active.push(wake.clone());
            self.metrics.recovery_wake_count = self.metrics.recovery_wake_count.saturating_add(1);
            recovered.push(wake);
        }
        recovered
    }

    /// Recover pending capacity while preserving the durable service cursor.
    /// The cursor is committed by `select_ready`; restoring a process must
    /// not advance it merely because an abandoned in-flight slot is released.
    pub fn recover_capacity_preserving_cursor(&mut self, tick: u64) -> Vec<SchedulerWakeV1> {
        self.recover_capacity_if_preserving_cursor(tick, |wake| Self::is_ready_at(wake, tick))
    }

    /// Evidence-aware capacity recovery that leaves the durable cursor and
    /// logical tick untouched, just like the scheduler-only recovery path.
    pub fn recover_capacity_if_preserving_cursor<F>(
        &mut self,
        tick: u64,
        eligible: F,
    ) -> Vec<SchedulerWakeV1>
    where
        F: FnMut(&SchedulerWakeV1) -> bool,
    {
        let cursor = self.cursor.clone();
        let logical_tick = self.logical_tick;
        let recovered = self.recover_capacity_if(tick, eligible);
        self.cursor = cursor;
        self.logical_tick = logical_tick;
        recovered
    }

    /// Recover selected wakes without changing the committed service cursor
    /// or logical tick. This is the crash-recovery counterpart to selection.
    pub fn recover_in_flight_preserving_cursor(&mut self, tick: u64) -> Vec<SchedulerWakeV1> {
        let cursor = self.cursor.clone();
        let logical_tick = self.logical_tick;
        let recovered = self.recover_in_flight(tick);
        self.cursor = cursor;
        self.logical_tick = logical_tick;
        recovered
    }

    /// Encode the complete scheduler state used by the World projection.
    /// The extra count is an observability convenience and is ignored by
    /// serde when restoring the typed state.
    pub fn snapshot_json(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "backpressure_count".to_string(),
                serde_json::json!(self.backpressure.len()),
            );
        }
        value
    }

    pub fn policy_config_digest(&self) -> String {
        self.policy.policy_config_digest()
    }

    pub fn cursor_seq(&self) -> u64 {
        self.cursor.cursor_seq
    }

    /// Return selected leases in canonical wake-id order for the production
    /// dispatcher. The leases remain occupied until an exact identity is
    /// released or a terminal/recovery path requeues them.
    pub fn in_flight_wakes(&self) -> Vec<SchedulerWakeV1> {
        self.in_flight.values().cloned().collect()
    }

    /// Return one exact durable wake identity from any scheduler bucket.
    /// Callers must still validate its World binding before dispatching it.
    pub fn wake_by_id(&self, wake_id: &str) -> Option<SchedulerWakeV1> {
        self.active
            .iter()
            .chain(self.backpressure.values())
            .chain(self.in_flight.values())
            .find(|wake| wake.wake_id == wake_id)
            .cloned()
    }

    pub fn policy(&self) -> &SchedulerPolicyV1 {
        &self.policy
    }

    pub fn from_snapshot_json(value: Value) -> Result<Self, SchedulerError> {
        let scheduler: Self = serde_json::from_value(value.clone())
            .map_err(|_| SchedulerError::new("invalid_scheduler_state"))?;
        if value
            .get("backpressure_count")
            .and_then(Value::as_u64)
            .is_some_and(|count| count != scheduler.backpressure.len() as u64)
        {
            return Err(SchedulerError::new("scheduler_backpressure_mismatch"));
        }
        scheduler.policy.validate()?;
        if scheduler.capacity == 0
            || scheduler.in_flight.len() > scheduler.capacity
            || scheduler
                .active
                .len()
                .saturating_add(scheduler.in_flight.len())
                > scheduler.capacity
            || scheduler.logical_tick != scheduler.cursor.logical_tick
            || scheduler.cursor.schema_version != "scheduler-cursor.v1"
            || scheduler.cursor.policy_config_digest != scheduler.policy.policy_config_digest()
            || scheduler
                .cursor
                .last_served_agent_id
                .as_deref()
                .is_some_and(|agent_id| !is_canonical_identifier(agent_id, 128))
        {
            return Err(SchedulerError::new("scheduler_policy_digest_mismatch"));
        }
        let mut wake_ids = BTreeSet::new();
        for wake in scheduler
            .active
            .iter()
            .chain(scheduler.backpressure.values())
            .chain(scheduler.in_flight.values())
        {
            wake.validate()?;
            if !wake_ids.insert(wake.wake_id.as_str()) {
                return Err(SchedulerError::new("scheduler_wake_conflict"));
            }
            if wake.starvation_deadline_tick
                != wake
                    .eligible_since_tick
                    .saturating_add(scheduler.policy.max_starvation_ticks)
                || wake.initial_priority != scheduler.policy.initial_priority
            {
                return Err(SchedulerError::new("scheduler_wake_binding_mismatch"));
            }
        }
        for (wake_id, wake) in &scheduler.in_flight {
            if wake.wake_id != *wake_id {
                return Err(SchedulerError::new("scheduler_in_flight_identity_mismatch"));
            }
        }
        for (wake_id, priority) in &scheduler.backpressure_priority {
            let Some(wake) = scheduler.backpressure.get(wake_id) else {
                return Err(SchedulerError::new("scheduler_backpressure_mismatch"));
            };
            if !(0..=7).contains(priority) || wake.wake_id != *wake_id {
                return Err(SchedulerError::new("scheduler_backpressure_mismatch"));
            }
        }
        if scheduler.backpressure_priority.len() != scheduler.backpressure.len() {
            return Err(SchedulerError::new("scheduler_backpressure_mismatch"));
        }
        if scheduler.recovered_wakes.iter().any(|wake_id| {
            !is_canonical_identifier(wake_id, 128) || scheduler.backpressure.contains_key(wake_id)
        }) {
            return Err(SchedulerError::new("scheduler_recovered_wake_mismatch"));
        }
        Ok(scheduler)
    }

    pub fn select_ready(&mut self, tick: u64) -> Vec<SchedulerWakeV1> {
        self.select_ready_if(tick, |_| true)
    }

    /// Select ready wakes while applying a World-owned eligibility predicate.
    /// Untimed event/receipt/state wakes must be admitted by their committed
    /// evidence, not by a synthetic tick or starvation timeout.
    pub fn select_ready_if<F>(&mut self, tick: u64, mut eligible: F) -> Vec<SchedulerWakeV1>
    where
        F: FnMut(&SchedulerWakeV1) -> bool,
    {
        self.advance_logical_tick(tick);
        let mut indexed: Vec<(usize, SchedulerWakeV1)> = self
            .active
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, wake)| {
                eligible(wake) && (self.is_ready(wake, tick) || wake.next_wake_tick == u64::MAX)
            })
            .collect();
        indexed.sort_by(|(_, left), (_, right)| self.compare(left, right));

        let mut served_agents: BTreeMap<String, usize> = BTreeMap::new();
        let mut selected_ids = BTreeSet::new();
        let mut selected = Vec::new();
        for (_, wake) in indexed {
            if selected.len() >= self.policy.max_total_wakes_per_tick {
                break;
            }
            let served = served_agents.get(&wake.agent_id).copied().unwrap_or(0);
            if served >= self.policy.max_wakes_per_agent_per_tick {
                continue;
            }
            served_agents.insert(wake.agent_id.clone(), served.saturating_add(1));
            selected_ids.insert(wake.wake_id.clone());
            self.cursor.last_served_agent_id = Some(wake.agent_id.clone());
            self.cursor.cursor_seq = self.cursor.cursor_seq.saturating_add(1);
            selected.push(wake);
        }
        self.active
            .retain(|wake| !selected_ids.contains(&wake.wake_id));
        for wake in &selected {
            self.in_flight.insert(wake.wake_id.clone(), wake.clone());
        }
        selected
    }

    pub fn metrics(&self) -> SchedulerExecutionMetrics {
        self.metrics
    }

    fn accepted_outcome() -> SchedulerEnqueueOutcome {
        SchedulerEnqueueOutcome {
            disposition: "accepted".to_string(),
            reason: "capacity_available".to_string(),
            provider_invocation_count: 0,
            world_event_count: 0,
            effect_count: 0,
            debit_count: 0,
            receipt_count: 0,
            world_receipt_linked_count: 0,
        }
    }

    fn available_slots(&self) -> usize {
        self.capacity
            .saturating_sub(self.active.len().saturating_add(self.in_flight.len()))
    }

    fn is_ready(&self, wake: &SchedulerWakeV1, tick: u64) -> bool {
        Self::is_ready_at(wake, tick)
    }

    fn is_ready_at(wake: &SchedulerWakeV1, tick: u64) -> bool {
        wake.next_wake_tick <= tick || wake.starvation_deadline_tick <= tick
    }

    fn effective_priority(&self, wake: &SchedulerWakeV1) -> i64 {
        let age = self.logical_tick.saturating_sub(wake.eligible_since_tick);
        let promotion = if self.policy.aging_after_ticks == 0 {
            age
        } else {
            age.checked_div(self.policy.aging_after_ticks)
                .unwrap_or_default()
        } as i64;
        (wake.initial_priority + promotion).clamp(0, 7)
    }

    fn cursor_distance(&self, agent_id: &str) -> usize {
        let mut ids: Vec<&str> = self
            .active
            .iter()
            .map(|wake| wake.agent_id.as_str())
            .chain(
                self.backpressure
                    .values()
                    .map(|wake| wake.agent_id.as_str()),
            )
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let Some(cursor) = self.cursor.last_served_agent_id.as_deref() else {
            return ids.iter().position(|id| *id == agent_id).unwrap_or(0);
        };
        let Some(start) = ids.iter().position(|id| *id == cursor) else {
            return ids.iter().position(|id| *id == agent_id).unwrap_or(0);
        };
        // The cursor records the agent served by the previous selection. The
        // next round starts at its successor, not at the same agent again.
        let start = (start + 1) % ids.len();
        let target = ids.iter().position(|id| *id == agent_id).unwrap_or(start);
        (target + ids.len() - start) % ids.len().max(1)
    }

    fn compare(&self, left: &SchedulerWakeV1, right: &SchedulerWakeV1) -> Ordering {
        let left_due = left.starvation_deadline_tick <= self.logical_tick;
        let right_due = right.starvation_deadline_tick <= self.logical_tick;
        right_due
            .cmp(&left_due)
            // A due item is first; ordinary ready wakes then follow the
            // policy's canonical ascending next-wake tick order.
            .then_with(|| left.next_wake_tick.cmp(&right.next_wake_tick))
            .then_with(|| {
                self.effective_priority(right)
                    .cmp(&self.effective_priority(left))
            })
            .then_with(|| {
                left.starvation_deadline_tick
                    .cmp(&right.starvation_deadline_tick)
            })
            .then_with(|| {
                self.cursor_distance(left.agent_id.as_str())
                    .cmp(&self.cursor_distance(right.agent_id.as_str()))
            })
            .then_with(|| left.agent_id.cmp(&right.agent_id))
            .then_with(|| left.continuation_id.cmp(&right.continuation_id))
            .then_with(|| left.wake_seq.cmp(&right.wake_seq))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerCrashPoint {
    BeforeCursorCommit,
    AfterCursorCommitBeforeDelivery,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchedulerCursorRecoveryReport {
    pub cursor: SchedulerCursorV1,
    pub delivered_wake_count: u64,
    pub provider_invocation_count: u64,
}

/// Deterministic fixture for the cursor transaction.  It models the two
/// crash prefixes without invoking a provider or a World effect.
#[derive(Debug)]
pub struct SchedulerCursorRecoveryFixture {
    scheduler: CognitionScheduler,
    cursor: SchedulerCursorV1,
    pending_delivery: Option<SchedulerWakeV1>,
    delivered: bool,
}

impl SchedulerCursorRecoveryFixture {
    pub fn new(policy: SchedulerPolicyV1, wakes: Vec<SchedulerWakeV1>) -> Self {
        let mut scheduler = CognitionScheduler::new(policy, wakes.len().max(1));
        for wake in wakes {
            scheduler.enqueue_for_test(wake);
        }
        let cursor = scheduler.cursor.clone();
        Self {
            scheduler,
            cursor,
            pending_delivery: None,
            delivered: false,
        }
    }

    pub fn run_tick_with_crash(
        &mut self,
        tick: u64,
        crash: SchedulerCrashPoint,
    ) -> Result<SchedulerCursorRecoveryReport, SchedulerError> {
        match crash {
            SchedulerCrashPoint::BeforeCursorCommit => {
                let mut probe = self.scheduler.clone_for_probe();
                let _ = probe.select_ready(tick);
            }
            SchedulerCrashPoint::AfterCursorCommitBeforeDelivery => {
                let selected = self.scheduler.select_ready(tick);
                let Some(wake) = selected.into_iter().next() else {
                    return Err(SchedulerError::new("no_ready_wake"));
                };
                self.cursor.logical_tick = tick;
                self.cursor.cursor_seq = self.cursor.cursor_seq.saturating_add(1);
                self.cursor.last_served_agent_id = Some(wake.agent_id.clone());
                self.pending_delivery = Some(wake);
            }
        }
        Ok(SchedulerCursorRecoveryReport {
            cursor: self.cursor.clone(),
            delivered_wake_count: 0,
            provider_invocation_count: 0,
        })
    }

    pub fn recover_and_deliver(
        &mut self,
        _tick: u64,
    ) -> Result<Vec<SchedulerWakeV1>, SchedulerError> {
        if self.delivered {
            return Ok(Vec::new());
        }
        let Some(wake) = self.pending_delivery.take() else {
            return Ok(Vec::new());
        };
        self.delivered = true;
        Ok(vec![wake])
    }
}

impl CognitionScheduler {
    fn clone_for_probe(&self) -> Self {
        Self {
            policy: self.policy.clone(),
            capacity: self.capacity,
            active: self.active.clone(),
            backpressure: self.backpressure.clone(),
            backpressure_priority: self.backpressure_priority.clone(),
            recovered_wakes: self.recovered_wakes.clone(),
            in_flight: self.in_flight.clone(),
            logical_tick: self.logical_tick,
            cursor: self.cursor.clone(),
            metrics: self.metrics,
        }
    }
}
