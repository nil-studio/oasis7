use std::collections::BTreeSet;

use super::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeStableCheckpoint {
    pub(super) batch_id: String,
    pub(super) snapshot: RuntimeSnapshot,
    pub(super) journal: RuntimeJournal,
    pub(super) log_cursor: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum RuntimeBatchChallengeState {
    None,
    Challenged,
    ResolvedNoFraud,
    ResolvedFraudSlashed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeAuthoritativeChallengeRecord {
    pub(super) challenge_id: String,
    pub(super) batch_id: String,
    pub(super) watcher_id: String,
    pub(super) recomputed_state_root: String,
    pub(super) recomputed_data_root: String,
    pub(super) status: AuthoritativeChallengeStatus,
    pub(super) submitted_at_tick: u64,
    pub(super) resolved_at_tick: Option<u64>,
    pub(super) slash_applied: bool,
    pub(super) slash_reason: Option<String>,
}

impl RuntimeAuthoritativeChallengeRecord {
    pub(super) fn as_ack(&self) -> AuthoritativeChallengeAck<u64> {
        AuthoritativeChallengeAck {
            challenge_id: self.challenge_id.clone(),
            batch_id: self.batch_id.clone(),
            watcher_id: self.watcher_id.clone(),
            status: self.status,
            submitted_at_tick: self.submitted_at_tick,
            resolved_at_tick: self.resolved_at_tick,
            slash_applied: self.slash_applied,
            slash_reason: self.slash_reason.clone(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RuntimeAuthoritativeBatchRecord {
    pub(super) batch_id: String,
    pub(super) tx_hash: String,
    pub(super) commit_tick: u64,
    pub(super) confirm_height: u64,
    pub(super) final_height: u64,
    pub(super) state_root: String,
    pub(super) data_root: String,
    pub(super) event_seq_start: Option<u64>,
    pub(super) event_seq_end: Option<u64>,
    pub(super) finality_state: AuthoritativeFinalityState,
    pub(super) challenge_state: RuntimeBatchChallengeState,
    pub(super) active_challenge_id: Option<String>,
    pub(super) events: Vec<WorldEvent>,
}

impl RuntimeAuthoritativeBatchRecord {
    fn has_valid_commit_roots(&self) -> bool {
        is_valid_root_hash(self.state_root.as_str()) && is_valid_root_hash(self.data_root.as_str())
    }

    fn expected_finality(&self, current_tick: u64) -> AuthoritativeFinalityState {
        if current_tick >= self.final_height {
            AuthoritativeFinalityState::Final
        } else if current_tick >= self.confirm_height {
            AuthoritativeFinalityState::Confirmed
        } else {
            AuthoritativeFinalityState::Pending
        }
    }

    pub(super) fn as_wire(
        &self,
        gate: &RuntimeSettlementRankingGate,
    ) -> AuthoritativeBatchFinality {
        AuthoritativeBatchFinality {
            batch_id: self.batch_id.clone(),
            tx_hash: self.tx_hash.clone(),
            commit_tick: self.commit_tick,
            confirm_height: self.confirm_height,
            final_height: self.final_height,
            state_root: self.state_root.clone(),
            data_root: self.data_root.clone(),
            finality_state: self.finality_state,
            event_seq_start: self.event_seq_start,
            event_seq_end: self.event_seq_end,
            settlement_ready: gate.settlement_allowed(self.batch_id.as_str(), self.finality_state),
            ranking_ready: gate.ranking_allowed(self.batch_id.as_str(), self.finality_state),
            challenge_open: self.challenge_state == RuntimeBatchChallengeState::Challenged,
            slashed: self.challenge_state == RuntimeBatchChallengeState::ResolvedFraudSlashed,
            active_challenge_id: self.active_challenge_id.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct RuntimeSettlementRankingGate {
    settlement_ready_batches: BTreeSet<String>,
    ranking_ready_batches: BTreeSet<String>,
}

impl RuntimeSettlementRankingGate {
    pub(super) fn promote_final(&mut self, batch_id: &str) {
        self.settlement_ready_batches.insert(batch_id.to_string());
        self.ranking_ready_batches.insert(batch_id.to_string());
    }

    pub(super) fn evict_batch(&mut self, batch_id: &str) {
        self.settlement_ready_batches.remove(batch_id);
        self.ranking_ready_batches.remove(batch_id);
    }

    fn settlement_allowed(
        &self,
        batch_id: &str,
        finality_state: AuthoritativeFinalityState,
    ) -> bool {
        finality_state == AuthoritativeFinalityState::Final
            && self.settlement_ready_batches.contains(batch_id)
    }

    fn ranking_allowed(&self, batch_id: &str, finality_state: AuthoritativeFinalityState) -> bool {
        finality_state == AuthoritativeFinalityState::Final
            && self.ranking_ready_batches.contains(batch_id)
    }
}

impl ViewerRuntimeLiveServer {
    pub(super) fn register_authoritative_batch(
        &mut self,
        events: &[WorldEvent],
    ) -> Result<AuthoritativeBatchFinality, ViewerRuntimeLiveServerError> {
        let commit_tick = self.world.state().time;
        let batch_id = format!(
            "{}-batch-{:020}",
            self.config.world_id, self.next_authoritative_batch_id
        );
        self.next_authoritative_batch_id = self.next_authoritative_batch_id.saturating_add(1);
        let state_root = compute_runtime_state_root(&self.world)?;
        let data_root = compute_batch_data_root(events)?;
        let tx_hash = compute_batch_tx_hash(
            batch_id.as_str(),
            state_root.as_str(),
            data_root.as_str(),
            commit_tick,
        )?;
        let record = RuntimeAuthoritativeBatchRecord {
            batch_id,
            tx_hash,
            commit_tick,
            confirm_height: commit_tick.saturating_add(AUTHORITATIVE_BATCH_CONFIRM_DELAY_TICKS),
            final_height: commit_tick
                .saturating_add(AUTHORITATIVE_BATCH_CONFIRM_DELAY_TICKS)
                .saturating_add(AUTHORITATIVE_BATCH_FINALITY_WINDOW_TICKS),
            state_root,
            data_root,
            event_seq_start: events.first().map(|event| event.id),
            event_seq_end: events.last().map(|event| event.id),
            finality_state: AuthoritativeFinalityState::Pending,
            challenge_state: RuntimeBatchChallengeState::None,
            active_challenge_id: None,
            events: events.to_vec(),
        };
        let response = record.as_wire(&self.settlement_ranking_gate);
        self.authoritative_batches.push_back(record);
        self.prune_authoritative_batch_history();
        Ok(response)
    }

    pub(super) fn advance_authoritative_batch_finality(
        &mut self,
        current_tick: u64,
    ) -> Result<Vec<AuthoritativeBatchFinality>, ViewerRuntimeLiveServerError> {
        let mut changed_indexes = Vec::new();
        let mut newly_finalized_batch_ids = Vec::new();
        for index in 0..self.authoritative_batches.len() {
            let batch = self
                .authoritative_batches
                .get_mut(index)
                .expect("batch index is valid");
            if batch.challenge_state == RuntimeBatchChallengeState::Challenged
                || batch.challenge_state == RuntimeBatchChallengeState::ResolvedFraudSlashed
            {
                continue;
            }
            if !batch.has_valid_commit_roots() {
                crate::observability::emit_stderr_or_event(
                    tracing::Level::WARN,
                    format!(
                        "viewer runtime live: authoritative batch remains pending due missing/invalid roots batch_id={} state_root={} data_root={}",
                        batch.batch_id,
                        batch.state_root,
                        batch.data_root
                    )
                    .as_str(),
                    "viewer runtime live authoritative batch pending due to invalid roots",
                );
                continue;
            }
            let expected_data_root = compute_batch_data_root(batch.events.as_slice())?;
            if expected_data_root != batch.data_root {
                crate::observability::emit_stderr_or_event(
                    tracing::Level::WARN,
                    format!(
                        "viewer runtime live: authoritative batch remains pending due data_root mismatch batch_id={} expected={} actual={}",
                        batch.batch_id,
                        expected_data_root,
                        batch.data_root
                    )
                    .as_str(),
                    "viewer runtime live authoritative batch pending due to data_root mismatch",
                );
                continue;
            }
            let expected_state = batch.expected_finality(current_tick);
            if expected_state > batch.finality_state {
                batch.finality_state = expected_state;
                if batch.finality_state == AuthoritativeFinalityState::Final {
                    newly_finalized_batch_ids.push(batch.batch_id.clone());
                }
                changed_indexes.push(index);
            }
        }

        for batch_id in newly_finalized_batch_ids {
            self.settlement_ranking_gate
                .promote_final(batch_id.as_str());
            self.capture_stable_checkpoint(batch_id.as_str())?;
        }

        let mut responses = Vec::new();
        for index in changed_indexes {
            if let Some(batch) = self.authoritative_batches.get(index) {
                responses.push(batch.as_wire(&self.settlement_ranking_gate));
            }
        }
        Ok(responses)
    }

    pub(super) fn handle_authoritative_challenge(
        &mut self,
        command: AuthoritativeChallengeCommand,
    ) -> Result<
        (
            AuthoritativeChallengeAck<u64>,
            Option<AuthoritativeBatchFinality>,
        ),
        AuthoritativeChallengeError,
    > {
        match command {
            AuthoritativeChallengeCommand::Submit { request } => {
                self.submit_authoritative_challenge(request)
            }
            AuthoritativeChallengeCommand::Resolve { request } => {
                self.resolve_authoritative_challenge(request)
            }
        }
    }

    fn submit_authoritative_challenge(
        &mut self,
        request: AuthoritativeChallengeSubmitRequest,
    ) -> Result<
        (
            AuthoritativeChallengeAck<u64>,
            Option<AuthoritativeBatchFinality>,
        ),
        AuthoritativeChallengeError,
    > {
        if !is_valid_root_hash(request.recomputed_state_root.as_str()) {
            return Err(challenge_error(
                "invalid_recomputed_state_root",
                format!(
                    "recomputed_state_root must be 64 hex chars, got {}",
                    request.recomputed_state_root
                ),
                None,
                Some(request.batch_id.clone()),
            ));
        }
        if !is_valid_root_hash(request.recomputed_data_root.as_str()) {
            return Err(challenge_error(
                "invalid_recomputed_data_root",
                format!(
                    "recomputed_data_root must be 64 hex chars, got {}",
                    request.recomputed_data_root
                ),
                None,
                Some(request.batch_id.clone()),
            ));
        }

        let challenge_id = request.challenge_id.unwrap_or_else(|| {
            let generated = format!(
                "{}-challenge-{:020}",
                self.config.world_id, self.next_authoritative_challenge_id
            );
            self.next_authoritative_challenge_id =
                self.next_authoritative_challenge_id.saturating_add(1);
            generated
        });

        if let Some(existing) = self
            .authoritative_challenges
            .iter()
            .find(|record| record.challenge_id == challenge_id)
        {
            if existing.batch_id == request.batch_id
                && existing.watcher_id == request.watcher_id
                && existing.recomputed_state_root == request.recomputed_state_root
                && existing.recomputed_data_root == request.recomputed_data_root
            {
                let maybe_batch = self
                    .authoritative_batches
                    .iter()
                    .find(|batch| batch.batch_id == request.batch_id)
                    .map(|batch| batch.as_wire(&self.settlement_ranking_gate));
                return Ok((existing.as_ack(), maybe_batch));
            }
            return Err(challenge_error(
                "challenge_id_conflict",
                format!(
                    "challenge_id {} already exists with different payload",
                    challenge_id
                ),
                Some(challenge_id),
                Some(request.batch_id),
            ));
        }

        let current_tick = self.world.state().time;
        let Some(batch_index) = self
            .authoritative_batches
            .iter()
            .position(|batch| batch.batch_id == request.batch_id)
        else {
            return Err(challenge_error(
                "batch_not_found",
                format!("authoritative batch {} not found", request.batch_id),
                Some(challenge_id),
                Some(request.batch_id),
            ));
        };

        let batch = self
            .authoritative_batches
            .get_mut(batch_index)
            .expect("batch index is valid");
        if batch.finality_state == AuthoritativeFinalityState::Final
            || current_tick > batch.final_height
        {
            return Err(challenge_error(
                "challenge_window_closed",
                format!(
                    "challenge window closed for batch {} at tick={}",
                    batch.batch_id, current_tick
                ),
                Some(challenge_id),
                Some(batch.batch_id.clone()),
            ));
        }
        if batch.challenge_state == RuntimeBatchChallengeState::ResolvedFraudSlashed {
            return Err(challenge_error(
                "batch_already_slashed",
                format!("batch {} is already slashed", batch.batch_id),
                Some(challenge_id),
                Some(batch.batch_id.clone()),
            ));
        }
        if batch.challenge_state == RuntimeBatchChallengeState::Challenged {
            return Err(challenge_error(
                "batch_already_challenged",
                format!("batch {} already has an open challenge", batch.batch_id),
                Some(challenge_id),
                Some(batch.batch_id.clone()),
            ));
        }

        batch.challenge_state = RuntimeBatchChallengeState::Challenged;
        batch.active_challenge_id = Some(challenge_id.clone());
        let batch_wire = batch.as_wire(&self.settlement_ranking_gate);

        let record = RuntimeAuthoritativeChallengeRecord {
            challenge_id: challenge_id.clone(),
            batch_id: request.batch_id,
            watcher_id: request.watcher_id,
            recomputed_state_root: request.recomputed_state_root,
            recomputed_data_root: request.recomputed_data_root,
            status: AuthoritativeChallengeStatus::Challenged,
            submitted_at_tick: current_tick,
            resolved_at_tick: None,
            slash_applied: false,
            slash_reason: None,
        };
        let ack = record.as_ack();
        self.authoritative_challenges.push_back(record);
        self.prune_authoritative_challenge_history();
        Ok((ack, Some(batch_wire)))
    }

    fn resolve_authoritative_challenge(
        &mut self,
        request: AuthoritativeChallengeResolveRequest,
    ) -> Result<
        (
            AuthoritativeChallengeAck<u64>,
            Option<AuthoritativeBatchFinality>,
        ),
        AuthoritativeChallengeError,
    > {
        let current_tick = self.world.state().time;
        let Some(challenge_index) = self
            .authoritative_challenges
            .iter()
            .position(|record| record.challenge_id == request.challenge_id)
        else {
            return Err(challenge_error(
                "challenge_not_found",
                format!("challenge {} not found", request.challenge_id),
                Some(request.challenge_id),
                None,
            ));
        };

        let batch_id = self.authoritative_challenges[challenge_index]
            .batch_id
            .clone();
        let Some(batch_index) = self
            .authoritative_batches
            .iter()
            .position(|batch| batch.batch_id == batch_id)
        else {
            return Err(challenge_error(
                "batch_not_found",
                format!("authoritative batch {} not found", batch_id),
                Some(request.challenge_id),
                Some(batch_id),
            ));
        };

        let challenge = self
            .authoritative_challenges
            .get_mut(challenge_index)
            .expect("challenge index is valid");
        if challenge.status != AuthoritativeChallengeStatus::Challenged {
            return Err(challenge_error(
                "challenge_already_resolved",
                format!("challenge {} already resolved", challenge.challenge_id),
                Some(challenge.challenge_id.clone()),
                Some(challenge.batch_id.clone()),
            ));
        }

        let challenge_id = challenge.challenge_id.clone();
        let batch_id = challenge.batch_id.clone();
        let expected_state_root = challenge.recomputed_state_root.clone();
        let expected_data_root = challenge.recomputed_data_root.clone();

        let mut batch_wire = {
            let batch = self
                .authoritative_batches
                .get_mut(batch_index)
                .expect("batch index is valid");
            let state_root_match = expected_state_root == batch.state_root;
            let data_root_match = expected_data_root == batch.data_root;
            if state_root_match && data_root_match {
                challenge.status = AuthoritativeChallengeStatus::ResolvedNoFraud;
                challenge.resolved_at_tick = Some(current_tick);
                challenge.slash_applied = false;
                challenge.slash_reason = None;
                batch.challenge_state = RuntimeBatchChallengeState::ResolvedNoFraud;
                batch.active_challenge_id = None;
            } else {
                challenge.status = AuthoritativeChallengeStatus::ResolvedFraudSlashed;
                challenge.resolved_at_tick = Some(current_tick);
                challenge.slash_applied = true;
                let slash_reason = if !state_root_match && !data_root_match {
                    "state_root_and_data_root_mismatch"
                } else if !state_root_match {
                    "state_root_mismatch"
                } else {
                    "data_root_mismatch"
                };
                challenge.slash_reason = Some(slash_reason.to_string());
                batch.challenge_state = RuntimeBatchChallengeState::ResolvedFraudSlashed;
                batch.active_challenge_id = None;
            }
            batch.as_wire(&self.settlement_ranking_gate)
        };

        if self.authoritative_challenges[challenge_index].status
            == AuthoritativeChallengeStatus::ResolvedNoFraud
        {
            let updates = self
                .advance_authoritative_batch_finality(current_tick)
                .map_err(|err| {
                    challenge_error(
                        "resolve_failed",
                        format!("{err:?}"),
                        Some(challenge_id.clone()),
                        Some(batch_id.clone()),
                    )
                })?;
            if let Some(update) = updates
                .into_iter()
                .find(|update| update.batch_id == batch_id)
            {
                batch_wire = update;
            }
        }

        let ack = self.authoritative_challenges[challenge_index].as_ack();
        Ok((ack, Some(batch_wire)))
    }

    pub(super) fn capture_stable_checkpoint(
        &mut self,
        batch_id: &str,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let snapshot = self.world.snapshot();
        let journal = self.world.journal().clone();
        let checkpoint = RuntimeStableCheckpoint {
            batch_id: batch_id.to_string(),
            snapshot,
            journal,
            log_cursor: latest_runtime_event_seq(&self.world),
        };
        if let Some(index) = self
            .stable_checkpoints
            .iter()
            .position(|entry| entry.batch_id == batch_id)
        {
            let _ = self.stable_checkpoints.remove(index);
        }
        self.stable_checkpoints.push_back(checkpoint);
        self.prune_stable_checkpoint_history();
        Ok(())
    }

    fn prune_stable_checkpoint_history(&mut self) {
        while self.stable_checkpoints.len() > MAX_AUTHORITATIVE_STABLE_CHECKPOINTS {
            let _ = self.stable_checkpoints.pop_front();
        }
    }

    pub(super) fn rebuild_settlement_ranking_gate(&mut self) {
        let mut gate = RuntimeSettlementRankingGate::default();
        for batch in &self.authoritative_batches {
            if batch.finality_state == AuthoritativeFinalityState::Final
                && batch.challenge_state != RuntimeBatchChallengeState::ResolvedFraudSlashed
                && batch.challenge_state != RuntimeBatchChallengeState::Challenged
            {
                gate.promote_final(batch.batch_id.as_str());
            }
        }
        self.settlement_ranking_gate = gate;
    }

    pub(super) fn emit_authoritative_batch_snapshot(
        &self,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        for batch in &self.authoritative_batches {
            send_response(
                writer,
                &ViewerResponse::AuthoritativeBatch {
                    batch: batch.as_wire(&self.settlement_ranking_gate),
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn emit_authoritative_challenge_snapshot(
        &self,
        writer: &mut dyn Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        for challenge in &self.authoritative_challenges {
            send_response(
                writer,
                &ViewerResponse::AuthoritativeChallengeAck {
                    ack: challenge.as_ack(),
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn prune_authoritative_batch_history(&mut self) {
        while self.authoritative_batches.len() > MAX_AUTHORITATIVE_BATCH_HISTORY {
            let Some(evicted) = self.authoritative_batches.pop_front() else {
                break;
            };
            self.settlement_ranking_gate
                .evict_batch(evicted.batch_id.as_str());
            self.authoritative_challenges
                .retain(|challenge| challenge.batch_id != evicted.batch_id);
            self.stable_checkpoints
                .retain(|entry| entry.batch_id != evicted.batch_id);
        }
    }

    fn prune_authoritative_challenge_history(&mut self) {
        while self.authoritative_challenges.len() > MAX_AUTHORITATIVE_CHALLENGE_HISTORY {
            let _ = self.authoritative_challenges.pop_front();
        }
    }
}

fn compute_runtime_state_root(
    world: &RuntimeWorld,
) -> Result<String, ViewerRuntimeLiveServerError> {
    let snapshot = world.snapshot();
    compute_runtime_snapshot_hash(&snapshot)
}

pub(super) fn compute_runtime_snapshot_hash(
    snapshot: &RuntimeSnapshot,
) -> Result<String, ViewerRuntimeLiveServerError> {
    let bytes = serde_json::to_vec(snapshot).map_err(|err| {
        ViewerRuntimeLiveServerError::Serde(format!(
            "serialize runtime snapshot hash payload failed: {err}"
        ))
    })?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn compute_batch_data_root(events: &[WorldEvent]) -> Result<String, ViewerRuntimeLiveServerError> {
    let bytes = serde_json::to_vec(events).map_err(|err| {
        ViewerRuntimeLiveServerError::Serde(format!(
            "serialize authoritative batch events for data_root failed: {err}"
        ))
    })?;
    Ok(blake3_hex(bytes.as_slice()))
}

fn compute_batch_tx_hash(
    batch_id: &str,
    state_root: &str,
    data_root: &str,
    commit_tick: u64,
) -> Result<String, ViewerRuntimeLiveServerError> {
    let payload = serde_json::json!({
        "batch_id": batch_id,
        "state_root": state_root,
        "data_root": data_root,
        "commit_tick": commit_tick,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|err| {
        ViewerRuntimeLiveServerError::Serde(format!(
            "serialize authoritative batch tx payload failed: {err}"
        ))
    })?;
    Ok(blake3_hex(bytes.as_slice()))
}

pub(super) fn is_valid_root_hash(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn challenge_error(
    code: impl Into<String>,
    message: impl Into<String>,
    challenge_id: Option<String>,
    batch_id: Option<String>,
) -> AuthoritativeChallengeError {
    AuthoritativeChallengeError {
        code: code.into(),
        message: message.into(),
        challenge_id,
        batch_id,
    }
}
