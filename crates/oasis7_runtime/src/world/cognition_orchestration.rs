use super::World;
use super::cognition_persistence_validation::{
    append_cognition_event, strict_optional_finality_hash,
};
use crate::cognition::{
    finality_binding_is_legal, is_canonical_identifier, world_state_binding_digest_v1,
};
use crate::cognition_recovery::{RuntimeCognitionBaseBindingV1, cognition_digest_v1};
use crate::cognition_retention::{
    RetentionExecutionProbe, RetentionRecordV1, RetentionReplayRequestV1,
};
use crate::cognition_scheduler::{
    CognitionScheduler, SchedulerEnqueueOutcome, SchedulerPolicyV1, SchedulerWakeV1,
};
use crate::cognition_wake::{
    AgentContinuation, CognitionContinuationProposalV1, CognitionWakeDispositionV1,
    ContinuationStatusV1, ContinuationTransition, WakeConditionValidator,
};
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};

const CONTINUATION_SCHEMA: &str = "agent-continuation.v1";

#[path = "cognition_finality_projection.rs"]
mod cognition_finality_projection;
#[path = "cognition_orchestration_support.rs"]
mod cognition_orchestration_support;
#[path = "cognition_reorg.rs"]
mod cognition_reorg;
#[path = "cognition_turn_lifecycle.rs"]
mod cognition_turn_lifecycle;
#[path = "cognition_wake_orchestration.rs"]
mod cognition_wake_orchestration;

impl World {
    pub(in crate::world) fn is_terminal_continuation_status(
        status: ContinuationStatusV1,
    ) -> bool {
        matches!(
            status,
            ContinuationStatusV1::Completed
                | ContinuationStatusV1::Cancelled
                | ContinuationStatusV1::Invalidated
                | ContinuationStatusV1::Expired
                | ContinuationStatusV1::Rejected
        )
    }

    pub(in crate::world) fn continuation_lifecycle_event_kind(
        status: ContinuationStatusV1,
    ) -> &'static str {
        match status {
            ContinuationStatusV1::Completed => "ContinuationCompleted",
            ContinuationStatusV1::Cancelled => "ContinuationCancelled",
            ContinuationStatusV1::Invalidated => "ContinuationInvalidated",
            ContinuationStatusV1::Expired => "ContinuationExpired",
            ContinuationStatusV1::Rejected => "ContinuationRejected",
            _ => "ContinuationTransitioned",
        }
    }

    pub fn with_cognition_scheduler(mut self, policy: SchedulerPolicyV1, capacity: usize) -> Self {
        let scheduler = CognitionScheduler::try_new(policy, capacity)
            .expect("invalid cognition scheduler configuration");
        self.cognition_commit_scheduler_transaction(&scheduler, "SchedulerConfigured", None)
            .expect("scheduler configuration must be durable");
        self
    }

    pub fn try_with_cognition_scheduler(
        mut self,
        policy: SchedulerPolicyV1,
        capacity: usize,
    ) -> Result<Self, WorldError> {
        let scheduler = CognitionScheduler::try_new(policy, capacity)
            .map_err(|error| scheduler_error(error.code()))?;
        self.cognition_commit_scheduler_transaction(&scheduler, "SchedulerConfigured", None)?;
        Ok(self)
    }

    pub fn bind_cognition_runtime(
        &mut self,
        world_id: impl Into<String>,
        branch_id: impl Into<String>,
        finality_epoch: u64,
        finality_block_hash: Option<String>,
        finality_status: impl Into<String>,
        reorg_epoch: u64,
    ) -> Result<(), WorldError> {
        let mut transaction = self.clone();
        transaction.bind_cognition_runtime_inner(
            world_id,
            branch_id,
            finality_epoch,
            finality_block_hash,
            finality_status,
            reorg_epoch,
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    fn bind_cognition_runtime_inner(
        &mut self,
        world_id: impl Into<String>,
        branch_id: impl Into<String>,
        finality_epoch: u64,
        finality_block_hash: Option<String>,
        finality_status: impl Into<String>,
        reorg_epoch: u64,
    ) -> Result<(), WorldError> {
        let binding = json!({
            "world_id": world_id.into(),
            "branch_id": branch_id.into(),
            "finality_epoch": finality_epoch,
            "finality_block_hash": finality_block_hash,
            "finality_status": finality_status.into(),
            "reorg_epoch": reorg_epoch,
            "runtime_manifest_hash": self.current_manifest_hash()?,
        });
        if !binding["world_id"]
            .as_str()
            .is_some_and(|value| is_canonical_identifier(value, 128))
            || !binding["branch_id"]
                .as_str()
                .is_some_and(|value| is_canonical_identifier(value, 128))
            || !binding["finality_status"]
                .as_str()
                .is_some_and(|value| is_canonical_identifier(value, 128))
        {
            return Err(cognition_validation_error("runtime_binding_invalid"));
        }
        let world_id = binding["world_id"]
            .as_str()
            .ok_or_else(|| cognition_validation_error("runtime_binding_invalid"))?;
        let branch_id = binding["branch_id"]
            .as_str()
            .ok_or_else(|| cognition_validation_error("runtime_binding_invalid"))?;
        let finality_status = binding["finality_status"]
            .as_str()
            .ok_or_else(|| cognition_validation_error("runtime_binding_invalid"))?;
        if !finality_binding_is_legal(finality_status, binding["finality_block_hash"].as_str()) {
            return Err(cognition_validation_error("runtime_binding_invalid"));
        }
        let base_world_hash = self.current_state_root_hash()?;
        let runtime_manifest_hash = self.current_manifest_hash()?;
        RuntimeCognitionBaseBindingV1 {
            world_id: world_id.to_string(),
            branch_id: branch_id.to_string(),
            finality_epoch,
            finality_block_hash: binding["finality_block_hash"].as_str().map(str::to_string),
            finality_status: finality_status.to_string(),
            base_tick: self.state.time,
            base_world_hash: world_state_binding_digest_v1(
                world_id,
                branch_id,
                finality_epoch,
                binding["finality_block_hash"].as_str(),
                finality_status,
                self.state.time,
                &base_world_hash,
                reorg_epoch,
                &runtime_manifest_hash,
            ),
            reorg_epoch,
            runtime_manifest_hash: cognition_digest_v1(
                "oasis7.runtime.manifest.v1",
                &runtime_manifest_hash,
            ),
        }
        .validate()
        .map_err(|error| cognition_validation_error(error.code()))?;
        let mut projection = self.cognition.as_object().cloned().unwrap_or_default();
        if let Some(existing) = projection
            .get("runtime_binding")
            .filter(|value| *value != &JsonValue::Null)
            && existing != &binding
        {
            // A parent binding is immutable while it has live work. Reorg
            // invalidation records a one-shot epoch which authorizes the next
            // parent binding; the invalidation path itself never mutates the
            // historical binding or rebases any continuation.
            let previous_reorg_epoch = existing
                .get("reorg_epoch")
                .and_then(JsonValue::as_u64)
                .unwrap_or_default();
            let authorized_epoch = projection
                .get("reorg_invalidation_epoch")
                .and_then(JsonValue::as_u64);
            if reorg_epoch <= previous_reorg_epoch || authorized_epoch != Some(reorg_epoch) {
                return Err(cognition_validation_error("runtime_binding_conflict"));
            }
        }
        projection.insert("runtime_binding".to_string(), binding);
        projection.remove("reorg_invalidation_epoch");
        self.cognition = JsonValue::Object(projection);
        Ok(())
    }

    pub fn start_cognition_turn(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
    ) -> Result<(), WorldError> {
        let bounded = |value: &str| is_canonical_identifier(value, 256);
        if self.cognition_runtime_is_unbound()
            || !bounded(agent_id)
            || !bounded(agent_session_id)
            || !bounded(agent_turn_id)
            || !bounded(decision_request_id)
            || !bounded(request_digest)
        {
            return Err(cognition_validation_error("cognition_turn_invalid"));
        }
        if !self.state.agents.is_empty() && !self.state.agents.contains_key(agent_id) {
            return Err(cognition_validation_error("cognition_agent_missing"));
        }
        let mut transaction = self.clone();
        append_cognition_event(
            &mut transaction.cognition,
            "TurnStarted",
            json!({
                "agent_id": agent_id,
                "agent_session_id": agent_session_id,
                "agent_turn_id": agent_turn_id,
                "decision_request_id": decision_request_id,
                "request_digest": request_digest,
            }),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    pub fn enqueue_cognition_wake(
        &mut self,
        wake: SchedulerWakeV1,
    ) -> Result<SchedulerEnqueueOutcome, WorldError> {
        let mut transaction = self.clone();
        let outcome = transaction.enqueue_cognition_wake_inner(wake)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(outcome)
    }

    fn enqueue_cognition_wake_inner(
        &mut self,
        wake: SchedulerWakeV1,
    ) -> Result<SchedulerEnqueueOutcome, WorldError> {
        let wake = self.hydrate_legacy_scheduler_wake(wake)?;
        self.validate_cognition_wake_binding(&wake)?;
        let mut scheduler = self.cognition_scheduler()?;
        scheduler.advance_logical_tick(self.state.time);
        let outcome = scheduler
            .try_enqueue(wake.clone())
            .map_err(|error| scheduler_error(error.code()))?;
        self.cognition_commit_scheduler_transaction(
            &scheduler,
            "SchedulerWakeEnqueued",
            Some(&wake),
        )?;
        Ok(outcome)
    }

    #[cfg(test)]
    pub(crate) fn enqueue_cognition_wake_for_test(
        &mut self,
        wake: SchedulerWakeV1,
    ) -> Result<SchedulerEnqueueOutcome, WorldError> {
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        scheduler.advance_logical_tick(transaction.state.time);
        let outcome = scheduler
            .try_enqueue(wake.clone())
            .map_err(|error| scheduler_error(error.code()))?;
        transaction.cognition_commit_scheduler_transaction(
            &scheduler,
            "SchedulerWakeEnqueued",
            Some(&wake),
        )?;
        *self = transaction;
        Ok(outcome)
    }

    pub fn select_ready_cognition_wakes(
        &mut self,
        tick: u64,
    ) -> Result<Vec<SchedulerWakeV1>, WorldError> {
        self.select_ready_cognition_wakes_inner(tick, false)
    }

    pub(in crate::world) fn select_ready_cognition_wakes_inner(
        &mut self,
        tick: u64,
        evidence_only: bool,
    ) -> Result<Vec<SchedulerWakeV1>, WorldError> {
        self.expire_cognition_continuations_at_tick(tick)?;
        if let Some(state) = self
            .cognition
            .get("scheduler_state")
            .filter(|state| !state.is_null())
        {
            self.validate_persisted_cognition_wakes(state)?;
        }
        let mut scheduler = self.cognition_scheduler()?;
        let continuations = self.cognition_continuations_typed()?;
        let stale = scheduler.prune_wakes_if(|wake| {
            self.cognition_wake_has_active_continuation(wake, &continuations)
        });
        let selected = scheduler.select_ready_if(tick, |wake| {
            self.cognition_wake_conditions_ready(wake, &continuations, tick)
                && (!evidence_only
                    || continuations
                        .iter()
                        .find(|continuation| continuation.continuation_id == wake.continuation_id)
                        .is_some_and(Self::cognition_wake_has_committed_evidence_condition))
        });
        let mut transaction = self.clone();
        // Keep evaluations for every durable in-flight wake. A later lease
        // must not erase the committed evidence needed to recover an earlier
        // lease after a process restart.
        let mut evaluations = transaction
            .cognition
            .get("continuation_evaluations")
            .and_then(JsonValue::as_object)
            .cloned()
            .unwrap_or_default();
        for wake in &selected {
            if let Some(continuation) = continuations
                .iter()
                .find(|continuation| continuation.continuation_id == wake.continuation_id)
            {
                let evaluation = transaction
                    .evaluate_cognition_wake_at_tick(&continuation.wake_conditions, tick)?;
                evaluations.insert(
                    continuation.continuation_id.clone(),
                    json!({
                        "evaluation_tick": evaluation.evaluation_tick,
                        "evaluation_head_digest": transaction.current_state_root_hash()?,
                        "evaluation_digest": evaluation.evaluation_digest,
                        "conditions_digest": evaluation.conditions_digest,
                    }),
                );
            }
        }
        if !evaluations.is_empty() {
            transaction.cognition["continuation_evaluations"] = JsonValue::Object(evaluations);
        }
        for wake in &stale {
            transaction.cognition_commit_scheduler_transaction(
                &scheduler,
                "SchedulerWakeDeactivated",
                Some(wake),
            )?;
        }
        if selected.is_empty() && !evidence_only {
            transaction.cognition_commit_scheduler_transaction(
                &scheduler,
                "SchedulerWakeSelected",
                None,
            )?;
        } else {
            for wake in &selected {
                transaction.cognition_commit_scheduler_transaction(
                    &scheduler,
                    "SchedulerWakeSelected",
                    Some(wake),
                )?;
                if !wake.continuation_id.trim().is_empty() {
                    transaction.cognition_commit_scheduler_transaction(
                        &scheduler,
                        "ContinuationWoken",
                        Some(wake),
                    )?;
                }
            }
        }
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(selected)
    }

    /// Expire validity-window continuations as a durable terminal transition.
    /// This runs before selection and is also called by restore/handoff paths,
    /// so an expired wake can never consume a lease or continuation budget.
    pub(super) fn expire_cognition_continuations_at_tick(
        &mut self,
        tick: u64,
    ) -> Result<(), WorldError> {
        if self
            .cognition
            .get("scheduler_state")
            .is_none_or(JsonValue::is_null)
        {
            return Ok(());
        }
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let mut continuations = transaction.cognition_continuations_typed()?;
        let mut expired = Vec::new();
        for continuation in &mut continuations {
            if Self::is_terminal_continuation_status(continuation.status)
                || continuation
                    .valid_until_tick
                    .is_none_or(|valid_until| tick <= valid_until)
            {
                continue;
            }
            ContinuationTransition::apply_at_tick(
                continuation,
                ContinuationStatusV1::Expired,
                tick,
            )
            .map_err(|error| cognition_validation_error(error.code()))?;
            let deactivated = scheduler
                .deactivate_wake(continuation.wake_id.as_str())
                .map_err(|error| scheduler_error(error.code()))?;
            expired.push((continuation.clone(), deactivated));
        }
        if expired.is_empty() {
            return Ok(());
        }
        transaction.cognition["continuations"] =
            serde_json::to_value(&continuations).map_err(WorldError::from)?;
        for (continuation, deactivated) in &expired {
            transaction.cognition_commit_continuation_lifecycle_transaction(
                &continuations,
                &scheduler,
                "ContinuationExpired",
                continuation,
                deactivated.as_ref(),
            )?;
        }
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    pub fn service_cognition_scheduler_tick(
        &mut self,
        tick: u64,
    ) -> Result<Vec<SchedulerWakeV1>, WorldError> {
        // Expiry is independent of queue visibility. In-flight leases can be
        // the only scheduler entries after a crash or provider stall, and
        // they must be terminalized before the early no-ready return below.
        self.expire_cognition_continuations_at_tick(tick)?;
        let Some(state) = self
            .cognition
            .get("scheduler_state")
            .filter(|state| !state.is_null())
        else {
            return Ok(Vec::new());
        };
        let has_ready_or_pending = state
            .get("active")
            .and_then(JsonValue::as_array)
            .is_some_and(|wakes| !wakes.is_empty())
            || state
                .get("backpressure")
                .and_then(JsonValue::as_object)
                .is_some_and(|wakes| !wakes.is_empty());
        if !has_ready_or_pending {
            return Ok(Vec::new());
        }
        self.select_ready_cognition_wakes(tick)
    }

    pub fn release_cognition_wake(&mut self, wake_id: &str) -> Result<SchedulerWakeV1, WorldError> {
        if wake_id.trim().is_empty() {
            return Err(scheduler_error("in_flight_wake_missing"));
        }
        if let Some(contexts) = self
            .cognition
            .get("continuation_contexts")
            .and_then(JsonValue::as_object)
        {
            let scheduler = self.cognition_scheduler()?;
            if let Some(wake) = scheduler
                .in_flight_wakes()
                .into_iter()
                .find(|wake| wake.wake_id == wake_id)
                && contexts.contains_key(&wake.continuation_id)
            {
                return Err(scheduler_error("wake_disposition_required"));
            }
        }
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let wake = scheduler
            .release_in_flight(wake_id)
            .map_err(|error| scheduler_error(error.code()))?;
        transaction.cognition_commit_scheduler_transaction(
            &scheduler,
            "SchedulerWakeReleased",
            Some(&wake),
        )?;
        let continuations = transaction.cognition_continuations_typed()?;
        let tick = transaction.state.time;
        let recovered = scheduler.recover_capacity_if_preserving_cursor(tick, |wake| {
            transaction.cognition_wake_conditions_ready(wake, &continuations, tick)
        });
        for recovered_wake in &recovered {
            transaction.cognition_commit_scheduler_transaction(
                &scheduler,
                "SchedulerWakeRecovered",
                Some(recovered_wake),
            )?;
        }
        let _ = transaction.select_ready_cognition_wakes_inner(transaction.state.time, true)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(wake)
    }

    pub fn recover_cognition_scheduler(
        &mut self,
        tick: u64,
    ) -> Result<Vec<SchedulerWakeV1>, WorldError> {
        let mut transaction = self.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let mut recovered = scheduler.recover_in_flight_preserving_cursor(tick);
        let mut continuations = transaction.cognition_continuations_typed()?;
        recovered.extend(
            scheduler.recover_capacity_if_preserving_cursor(tick, |wake| {
                transaction.cognition_wake_conditions_ready(wake, &continuations, tick)
            }),
        );
        let mut status_recovered = false;
        for continuation in &mut continuations {
            if continuation.status == ContinuationStatusV1::Pending
                && scheduler.wake_by_id(&continuation.wake_id).is_some()
                && !scheduler.is_backpressured(&continuation.wake_id)
            {
                ContinuationTransition::apply_at_tick(
                    continuation,
                    ContinuationStatusV1::Scheduled,
                    tick,
                )
                .map_err(|error| cognition_validation_error(error.code()))?;
                continuation.refresh_status_digest();
                status_recovered = true;
            }
        }
        if status_recovered {
            transaction.cognition_set_continuations(&continuations)?;
        }
        if recovered.is_empty() {
            transaction.cognition_commit_scheduler_transaction(
                &scheduler,
                "SchedulerWakeRecovered",
                None,
            )?;
        } else {
            for wake in &recovered {
                transaction.cognition_commit_scheduler_transaction(
                    &scheduler,
                    "SchedulerWakeRecovered",
                    Some(wake),
                )?;
            }
        }
        let _ = transaction.select_ready_cognition_wakes_inner(tick, true)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(recovered)
    }

    pub fn cognition_scheduler_snapshot(&self) -> JsonValue {
        self.cognition
            .get("scheduler_state")
            .cloned()
            .unwrap_or(JsonValue::Null)
    }

    pub fn cognition_in_flight_wakes(&self) -> Result<Vec<SchedulerWakeV1>, WorldError> {
        if self
            .cognition
            .get("scheduler_state")
            .is_none_or(JsonValue::is_null)
        {
            return Ok(Vec::new());
        }
        Ok(self.cognition_scheduler()?.in_flight_wakes())
    }

    pub fn consume_cognition_wake<F>(
        &mut self,
        wake_id: &str,
        dispatch: F,
    ) -> Result<SchedulerWakeV1, WorldError>
    where
        F: FnOnce(&SchedulerWakeV1) -> Result<CognitionWakeDispositionV1, WorldError>,
    {
        let wake = self
            .cognition_in_flight_wakes()?
            .into_iter()
            .find(|wake| wake.wake_id == wake_id)
            .ok_or_else(|| scheduler_error("in_flight_wake_missing"))?;
        let disposition = dispatch(&wake)?;
        Ok(self.handoff_cognition_wake(wake_id, disposition)?.wake)
    }

    #[cfg(test)]
    pub(crate) fn install_cognition_continuation_for_test(
        &mut self,
        continuation: AgentContinuation,
    ) -> Result<(), WorldError> {
        let mut continuations = self.cognition_continuations_typed()?;
        continuations.push(continuation);
        self.cognition_set_continuations(&continuations)
    }

    pub fn cognition_execution_metrics(&self) -> JsonValue {
        self.cognition
            .get("scheduler_state")
            .and_then(|state| state.get("metrics"))
            .cloned()
            .or_else(|| self.cognition.get("metrics").cloned())
            .unwrap_or_else(|| {
                json!({
                    "recovery_wake_count": 0,
                    "provider_invocation_count": 0,
                    "effect_count": 0,
                    "debit_count": 0
                })
            })
    }

    pub fn admit_cognition_continuation(
        &mut self,
        proposal: CognitionContinuationProposalV1,
    ) -> Result<AgentContinuation, WorldError> {
        let mut transaction = self.clone();
        let admitted = transaction.admit_cognition_continuation_inner(proposal, false)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(admitted)
    }

    pub(in crate::world) fn admit_cognition_continuation_inner(
        &mut self,
        mut proposal: CognitionContinuationProposalV1,
        allow_existing_budget_chain: bool,
    ) -> Result<AgentContinuation, WorldError> {
        if self.cognition_runtime_is_unbound() {
            return Err(cognition_validation_error("runtime_binding_required"));
        }
        self.bind_cognition_proposal_fields(&mut proposal)?;
        proposal
            .validate()
            .map_err(|error| cognition_validation_error(error.code()))?;
        self.validate_cognition_proposal_binding(&proposal)?;
        let wake_conditions = WakeConditionValidator::canonicalize(proposal.wake_conditions)
            .map_err(|error| cognition_validation_error(error.code()))?;
        let derived_next_wake_tick =
            WakeConditionValidator::next_wake_tick_at(&wake_conditions, self.state.time)
                .map_err(|error| cognition_validation_error(error.code()))?;
        if proposal.next_wake_tick.is_none() {
            proposal.next_wake_tick = derived_next_wake_tick;
        }
        if proposal.world_id.trim().is_empty()
            || proposal.branch_id.trim().is_empty()
            || proposal.finality_status.trim().is_empty()
            || proposal.runtime_manifest_hash.trim().is_empty()
            || proposal.agent_id.trim().is_empty()
            || proposal.agent_session_id.trim().is_empty()
            || proposal.agent_turn_id.trim().is_empty()
            || proposal.decision_request_id.trim().is_empty()
            || proposal.origin_turn_id.trim().is_empty()
            || proposal.origin_request_digest.trim().is_empty()
            || proposal.continuation_proposal_id.trim().is_empty()
            || proposal.proposal_digest.trim().is_empty()
            || proposal.precondition_digest.trim().is_empty()
            || proposal.remaining_budget.value == 0
            || !matches!(proposal.remaining_budget.unit.as_str(), "steps" | "ticks")
            || proposal.next_wake_tick != derived_next_wake_tick
            || proposal
                .valid_until_tick
                .is_some_and(|valid_until| valid_until < self.state.time)
        {
            return Err(cognition_validation_error("continuation_proposal_invalid"));
        }
        let mut continuations = self.cognition_continuations_typed()?;
        if !self.cognition_turn_is_registered(
            &proposal.agent_id,
            &proposal.agent_session_id,
            &proposal.agent_turn_id,
            &proposal.decision_request_id,
            &proposal.origin_request_digest,
        ) {
            return Err(cognition_validation_error("continuation_turn_unregistered"));
        }
        if continuations
            .iter()
            .any(|existing| existing.continuation_proposal_id == proposal.continuation_proposal_id)
        {
            return Err(cognition_validation_error("continuation_proposal_conflict"));
        }
        if !allow_existing_budget_chain
            && continuations.iter().any(|existing| {
                existing.agent_id == proposal.agent_id
                    && existing.agent_session_id == proposal.agent_session_id
                    && existing.agent_turn_id == proposal.agent_turn_id
                    && existing.decision_request_id == proposal.decision_request_id
            })
        {
            return Err(cognition_validation_error(
                "continuation_budget_chain_conflict",
            ));
        }
        let allocation = self.allocate_next_proposal_id();
        let continuation_id = format!("continuation:{allocation}");
        let wake_id = format!("wake:{allocation}");
        let wake_seq = continuations
            .iter()
            .filter(|existing| existing.agent_id == proposal.agent_id)
            .map(|existing| existing.wake_seq)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let proposal_context = json!({
            "baseline_observation_digest": proposal.baseline_observation_digest.clone(),
            "goal_digest": proposal.goal_digest.clone(),
            "policy_digest": proposal.policy_digest.clone(),
            "policy_revision": proposal.policy_revision,
            "precondition_summary": proposal.precondition_summary.clone(),
            "precondition_digest": proposal.precondition_digest.clone(),
        });
        let mut continuation = AgentContinuation {
            schema_version: CONTINUATION_SCHEMA.to_string(),
            continuation_id,
            wake_id,
            world_id: proposal.world_id,
            branch_id: proposal.branch_id,
            finality_epoch: proposal.finality_epoch,
            finality_block_hash: proposal.finality_block_hash,
            finality_status: proposal.finality_status,
            reorg_epoch: proposal.reorg_epoch,
            runtime_manifest_hash: proposal.runtime_manifest_hash,
            agent_id: proposal.agent_id,
            agent_session_id: proposal.agent_session_id,
            agent_turn_id: proposal.agent_turn_id,
            decision_request_id: proposal.decision_request_id,
            origin_turn_id: proposal.origin_turn_id,
            origin_request_digest: proposal.origin_request_digest,
            continuation_proposal_id: proposal.continuation_proposal_id,
            proposal_digest: proposal.proposal_digest,
            action_or_envelope_digest: proposal.action_or_envelope_digest,
            wake_conditions,
            next_wake_tick: proposal.next_wake_tick,
            remaining_budget: proposal.remaining_budget,
            valid_until_tick: proposal.valid_until_tick,
            precondition_digest: proposal.precondition_digest,
            wake_seq,
            logical_tick: self.state.time,
            status: ContinuationStatusV1::Scheduled,
            continuation_status_digest: None,
            terminal_disposition: None,
        };
        continuation.refresh_status_digest();
        continuation
            .validate_authoritative()
            .map_err(|error| cognition_validation_error(error.code()))?;
        let next_wake_tick = continuation.next_wake_tick.unwrap_or(u64::MAX);
        let mut scheduler = self.cognition_scheduler()?;
        scheduler.advance_logical_tick(self.state.time);
        let wake = SchedulerWakeV1 {
            schema_version: SchedulerWakeV1::SCHEMA_VERSION.to_string(),
            wake_id: continuation.wake_id.clone(),
            continuation_id: continuation.continuation_id.clone(),
            world_id: continuation.world_id.clone(),
            branch_id: continuation.branch_id.clone(),
            finality_epoch: continuation.finality_epoch,
            finality_block_hash: continuation.finality_block_hash.clone(),
            finality_status: continuation.finality_status.clone(),
            reorg_epoch: continuation.reorg_epoch,
            runtime_manifest_hash: continuation.runtime_manifest_hash.clone(),
            agent_id: continuation.agent_id.clone(),
            agent_session_id: continuation.agent_session_id.clone(),
            agent_turn_id: continuation.agent_turn_id.clone(),
            decision_request_id: continuation.decision_request_id.clone(),
            request_digest: continuation.origin_request_digest.clone(),
            next_wake_tick,
            eligible_since_tick: self.state.time,
            starvation_deadline_tick: self
                .state
                .time
                .saturating_add(scheduler.policy().max_starvation_ticks),
            initial_priority: scheduler.policy().initial_priority,
            wake_seq: continuation.wake_seq,
            retry_seq: 0,
            status: "pending".to_string(),
            pending_reason: "capacity_available".to_string(),
        };
        let enqueue_outcome = scheduler
            .try_enqueue(wake.clone())
            .map_err(|error| scheduler_error(error.code()))?;
        if enqueue_outcome.disposition == "pending" {
            ContinuationTransition::apply_at_tick(
                &mut continuation,
                ContinuationStatusV1::Pending,
                self.state.time,
            )
            .map_err(|error| cognition_validation_error(error.code()))?;
            continuation.refresh_status_digest();
        }
        continuations.push(continuation.clone());
        let mut context_registry = self
            .cognition
            .get("continuation_contexts")
            .and_then(JsonValue::as_object)
            .cloned()
            .unwrap_or_default();
        context_registry.insert(continuation.continuation_id.clone(), proposal_context);
        self.cognition["continuation_contexts"] = JsonValue::Object(context_registry);
        self.cognition_commit_continuation_transaction(&continuations, &scheduler, &wake)?;
        Ok(continuation)
    }

    pub fn schedule_cognition_continuation(
        &mut self,
        _continuation: AgentContinuation,
    ) -> Result<(), WorldError> {
        Err(cognition_validation_error("legacy_continuation_projection"))
    }

    pub fn transition_cognition_continuation(
        &mut self,
        continuation_id: &str,
        to: ContinuationStatusV1,
        logical_tick: u64,
    ) -> Result<AgentContinuation, WorldError> {
        let mut transaction = self.clone();
        let mut continuations = transaction.cognition_continuations_typed()?;
        let continuation = continuations
            .iter_mut()
            .find(|continuation| continuation.continuation_id == continuation_id)
            .ok_or_else(|| cognition_validation_error("continuation_missing"))?;
        ContinuationTransition::apply_at_tick(continuation, to, logical_tick)
            .map_err(|error| cognition_validation_error(error.code()))?;
        continuation
            .validate_authoritative()
            .map_err(|error| cognition_validation_error(error.code()))?;
        let transitioned = continuation.clone();
        let mut scheduler = transaction.cognition_scheduler()?;
        let deactivated = if Self::is_terminal_continuation_status(transitioned.status) {
            scheduler
                .deactivate_wake(transitioned.wake_id.as_str())
                .map_err(|error| scheduler_error(error.code()))?
        } else {
            None
        };
        let event_kind = Self::continuation_lifecycle_event_kind(transitioned.status);
        transaction.cognition_commit_continuation_lifecycle_transaction(
            &continuations,
            &scheduler,
            event_kind,
            &transitioned,
            deactivated.as_ref(),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(transitioned)
    }

    pub fn cognition_continuations(&self) -> JsonValue {
        self.cognition
            .get("continuations")
            .cloned()
            .unwrap_or_else(|| JsonValue::Array(Vec::new()))
    }

    pub fn record_cognition_terminal(
        &mut self,
        record: RetentionRecordV1,
    ) -> Result<(), WorldError> {
        let mut transaction = self.clone();
        let mut store = transaction.cognition_retention_store()?;
        store.insert(record);
        transaction.cognition_set_retention_store(&store)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    pub fn pin_cognition_reference(
        &mut self,
        key: &str,
        reference: &str,
    ) -> Result<(), WorldError> {
        let mut transaction = self.clone();
        let mut store = transaction.cognition_retention_store()?;
        store.pin_reference(key, reference);
        transaction.cognition_set_retention_store(&store)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    pub fn gc_cognition(
        &mut self,
        now_tick: u64,
        gc_floor_tick: u64,
    ) -> Result<crate::RetentionGcReport, WorldError> {
        let mut transaction = self.clone();
        let mut store = transaction.cognition_retention_store()?;
        transaction.migrate_cognition_retention_and_pin_wake_events(&mut store)?;
        let report = store
            .gc(now_tick, gc_floor_tick)
            .map_err(|error| cognition_validation_error(error.code()))?;
        transaction.cognition_set_retention_store(&store)?;
        transaction.compact_cognition_projection(&report.deleted_keys)?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(report)
    }

    pub fn replay_cognition_terminal(
        &self,
        request: RetentionReplayRequestV1,
    ) -> Result<JsonValue, WorldError> {
        let world_id = self
            .cognition
            .get("runtime_binding")
            .and_then(|binding| binding.get("world_id"))
            .and_then(JsonValue::as_str)
            .filter(|value| is_canonical_identifier(value, 128))
            .ok_or_else(|| cognition_validation_error("runtime_binding_required"))?;
        if request.world_id.as_deref() != Some(world_id) {
            return Err(cognition_validation_error("foreign_replay_request"));
        }
        let store = self.cognition_retention_store()?;
        let mut probe = RetentionExecutionProbe::default();
        let result = store
            .replay_v1(request, &mut probe)
            .map_err(|error| cognition_validation_error(error.code()))?;
        serde_json::to_value(result).map_err(WorldError::from)
    }

    pub(in crate::world) fn cognition_scheduler(
        &self,
    ) -> Result<CognitionScheduler, WorldError> {
        let state = self
            .cognition
            .get("scheduler_state")
            .filter(|state| !state.is_null())
            .cloned()
            .ok_or_else(|| cognition_validation_error("scheduler_unconfigured"))?;
        let state = self.hydrate_legacy_wake_request_digests(&state)?;
        CognitionScheduler::from_snapshot_json(state).map_err(|error| scheduler_error(error.code()))
    }

    /// V1 scheduler snapshots predate the wake request digest. Rehydrate that
    /// field from the authoritative continuation origin before exposing any
    /// wake to Runtime callers. An unresolvable legacy wake remains empty and
    /// is therefore rejected by the provider sidecar's fail-closed handoff.
    pub(in crate::world) fn hydrate_legacy_wake_request_digests(
        &self,
        scheduler_state: &JsonValue,
    ) -> Result<JsonValue, WorldError> {
        let continuations = self.active_cognition_continuations()?;
        let mut state = scheduler_state.clone();
        for bucket in ["active", "backpressure", "in_flight"] {
            let Some(values) = state.get_mut(bucket) else {
                continue;
            };
            match values {
                JsonValue::Array(entries) => {
                    for entry in entries {
                        hydrate_wake_digest(entry, &continuations);
                    }
                }
                JsonValue::Object(entries) => {
                    for entry in entries.values_mut() {
                        hydrate_wake_digest(entry, &continuations);
                    }
                }
                _ => {}
            }
        }
        Ok(state)
    }

    /// Hydrate the request digest on a legacy direct-ingress wake only from
    /// the matching, already validated Runtime continuation.  A supplied
    /// digest is left untouched so the full binding check below rejects any
    /// contradictory explicit identity.
    fn hydrate_legacy_scheduler_wake(
        &self,
        mut wake: SchedulerWakeV1,
    ) -> Result<SchedulerWakeV1, WorldError> {
        if !wake.request_digest.is_empty() {
            return Ok(wake);
        }
        let continuation = self
            .active_cognition_continuations()?
            .into_iter()
            .find(|continuation| continuation.continuation_id == wake.continuation_id);
        if let Some(continuation) = continuation {
            wake.request_digest = continuation.origin_request_digest;
        }
        Ok(wake)
    }

    fn cognition_wake_has_active_continuation(
        &self,
        wake: &SchedulerWakeV1,
        continuations: &[AgentContinuation],
    ) -> bool {
        let Some(continuation) = continuations
            .iter()
            .find(|continuation| continuation.continuation_id == wake.continuation_id)
        else {
            return false;
        };
        if let Some(contexts) = self
            .cognition
            .get("continuation_contexts")
            .and_then(JsonValue::as_object)
            && !contexts.contains_key(&continuation.continuation_id)
        {
            return false;
        }
        !matches!(
            continuation.status,
            ContinuationStatusV1::Completed
                | ContinuationStatusV1::Cancelled
                | ContinuationStatusV1::Invalidated
                | ContinuationStatusV1::Expired
                | ContinuationStatusV1::Rejected
        )
    }

    pub(in crate::world) fn cognition_wake_conditions_ready(
        &self,
        wake: &SchedulerWakeV1,
        continuations: &[AgentContinuation],
        tick: u64,
    ) -> bool {
        let Some(continuation) = continuations
            .iter()
            .find(|continuation| continuation.continuation_id == wake.continuation_id)
        else {
            return false;
        };
        if !self.cognition_wake_has_active_continuation(wake, continuations) {
            return false;
        }
        if continuation
            .valid_until_tick
            .is_some_and(|valid_until| tick > valid_until)
        {
            return false;
        }
        self.evaluate_cognition_wake_at_tick(&continuation.wake_conditions, tick)
            .map(|evaluation| evaluation.status == "ready")
            .unwrap_or(false)
    }

    fn cognition_wake_has_committed_evidence_condition(continuation: &AgentContinuation) -> bool {
        continuation.wake_conditions.iter().any(|condition| {
            matches!(
                condition.kind.as_str(),
                "world_event_committed" | "receipt_linked" | "state_predicate"
            )
        })
    }

    pub(super) fn validate_cognition_wake_binding(
        &self,
        wake: &SchedulerWakeV1,
    ) -> Result<(), WorldError> {
        wake.validate()
            .map_err(|error| scheduler_error(error.code()))?;
        if let Some(binding) = self.cognition.get("runtime_binding") {
            let block_hash = strict_optional_finality_hash(binding.get("finality_block_hash"))?;
            if binding["world_id"].as_str() != Some(wake.world_id.as_str())
                || binding["branch_id"].as_str() != Some(wake.branch_id.as_str())
                || binding["finality_epoch"].as_u64() != Some(wake.finality_epoch)
                || block_hash != wake.finality_block_hash
                || binding["finality_status"].as_str() != Some(wake.finality_status.as_str())
                || binding["reorg_epoch"].as_u64() != Some(wake.reorg_epoch)
                || binding["runtime_manifest_hash"].as_str()
                    != Some(wake.runtime_manifest_hash.as_str())
            {
                return Err(cognition_validation_error("foreign_scheduler_wake"));
            }
        }
        let continuations = self.cognition_continuations_typed()?;
        let Some(continuation) = continuations
            .iter()
            .find(|continuation| continuation.continuation_id == wake.continuation_id)
        else {
            return Err(cognition_validation_error("foreign_scheduler_wake"));
        };
        if continuation.wake_id != wake.wake_id
            || continuation.world_id != wake.world_id
            || continuation.branch_id != wake.branch_id
            || continuation.finality_epoch != wake.finality_epoch
            || continuation.finality_block_hash != wake.finality_block_hash
            || continuation.finality_status != wake.finality_status
            || continuation.reorg_epoch != wake.reorg_epoch
            || continuation.runtime_manifest_hash != wake.runtime_manifest_hash
            || continuation.agent_id != wake.agent_id
            || continuation.agent_session_id != wake.agent_session_id
            || continuation.agent_turn_id != wake.agent_turn_id
            || continuation.decision_request_id != wake.decision_request_id
            || continuation.origin_request_digest != wake.request_digest
        {
            return Err(cognition_validation_error("foreign_scheduler_wake"));
        }
        Ok(())
    }

    pub(super) fn cognition_runtime_is_unbound(&self) -> bool {
        self.cognition
            .get("runtime_binding")
            .is_none_or(JsonValue::is_null)
    }
}

fn cognition_validation_error(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition validation failed: {code}"),
    }
}

fn hydrate_wake_digest(entry: &mut JsonValue, continuations: &[AgentContinuation]) {
    let Some(object) = entry.as_object_mut() else {
        return;
    };
    let missing_digest = object
        .get("request_digest")
        .and_then(JsonValue::as_str)
        .is_none_or(str::is_empty);
    if !missing_digest {
        return;
    }
    let Some(continuation_id) = object.get("continuation_id").and_then(JsonValue::as_str) else {
        return;
    };
    let Some(continuation) = continuations
        .iter()
        .find(|continuation| continuation.continuation_id == continuation_id)
    else {
        return;
    };
    object.insert(
        "request_digest".to_string(),
        JsonValue::String(continuation.origin_request_digest.clone()),
    );
}

fn scheduler_error(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition scheduler failed: {code}"),
    }
}
