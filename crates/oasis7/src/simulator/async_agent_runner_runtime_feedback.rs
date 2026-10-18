use super::feedback::RuntimeReceiptReadbackVerifier;
use super::*;
#[cfg(test)]
use crate::runtime::RuntimeFeedbackOutboxRecordV1;
use oasis7_agent_harness::MemoryWritePolicyContextV1 as HarnessMemoryWritePolicyContextV1;

fn runtime_receipt_from_api_projection(
    receipt: &oasis7_agent_api::RuntimeReceiptLineageV1,
) -> RuntimeReceiptLineageV1 {
    RuntimeReceiptLineageV1 {
        schema_version: receipt.schema_version.clone(),
        status: receipt.status.clone(),
        receipt_id: receipt.receipt_id.clone(),
        receipt_digest: receipt.receipt_digest.clone(),
        envelope_digest: receipt.envelope_digest.clone(),
        action_id: receipt.action_id.clone(),
        agent_id: receipt.agent_id.clone(),
        agent_session_id: receipt.agent_session_id.clone(),
        agent_turn_id: receipt.agent_turn_id.clone(),
        decision_request_id: receipt.decision_request_id.clone(),
        request_digest: receipt.request_digest.clone(),
        feedback_id: receipt.feedback_id.clone(),
    }
}

/// Short-lived Harness authority created only after the strict verifier has
/// returned an identity-matching opaque World-readback handle. The handle is
/// carried to the write boundary; this adapter adds no serialized or
/// caller-mintable proof and performs no substitute readback.
struct VerifiedWorldMemoryAuthority<'a> {
    turn: &'a ContinuousAgentTurnContextV1,
    feedback: &'a FeedbackEnvelopeV1,
    receipt: &'a RuntimeReceiptLineageV1,
    readback: &'a super::feedback::RuntimeReceiptReadbackHandleV1,
}

impl RuntimeAuthority for VerifiedWorldMemoryAuthority<'_> {
    type Lease = ();
    type Receipt = RuntimeReceiptLineageV1;
    type Continuation = ();

    fn validate_lease(
        &self,
        _lease: &Self::Lease,
        _request: &oasis7_agent_api::ContinuousAgentRequestContextV1,
    ) -> Result<oasis7_agent_api::CognitionLeaseConsumptionViewV1, oasis7_agent_api::CognitionError>
    {
        Err(memory_authority_scope_error())
    }

    fn verify_receipt_readback(
        &self,
        _feedback: &oasis7_agent_api::FeedbackEnvelopeV1,
        _receipt: &Self::Receipt,
        _response_identity: &oasis7_agent_api::ResponseArtifactIdentityV1,
    ) -> Result<oasis7_agent_api::RuntimeReceiptLineageV1, oasis7_agent_api::CognitionError> {
        Err(memory_authority_scope_error())
    }

    fn verify_memory_receipt(
        &self,
        receipt: &Self::Receipt,
        context: Option<&HarnessMemoryWritePolicyContextV1>,
    ) -> Result<oasis7_agent_api::RuntimeReceiptLineageV1, oasis7_agent_api::CognitionError> {
        let context = context.ok_or_else(memory_authority_scope_error)?;
        if receipt != self.receipt
            || receipt.validate().is_err()
            || receipt.receipt_id != self.readback.receipt_id()
            || receipt.receipt_digest != self.readback.receipt_digest().as_str()
            || self.readback.verified_lineage() != receipt
            || !self
                .readback
                .response_artifact_digest()
                .is_canonical_blake3()
            || self.feedback.status != "committed"
            || self.feedback.runtime_receipt_id.as_deref() != Some(receipt.receipt_id.as_str())
            || self.feedback.feedback_id != receipt.feedback_id
            || self.feedback.agent_subject != self.turn.agent_id
            || self.feedback.agent_session_id != self.turn.agent_session_id
            || self.feedback.agent_turn_id != self.turn.agent_turn_id
            || self.feedback.decision_request_id != self.turn.decision_request_id
            || self.feedback.request_digest != self.turn.request_digest
            || context.agent_id != self.turn.agent_id
            || context.agent_session_id != self.turn.agent_session_id
            || context.agent_turn_id != self.turn.agent_turn_id
            || context.request_digest != self.turn.request_digest.to_string()
            || context.source != "provider"
            || context.provenance != "provider_unverified"
        {
            return Err(oasis7_agent_api::CognitionError::new(
                "runtime_memory_readback_mismatch",
                "verified World readback does not match the exact committed feedback context",
            ));
        }
        Ok(oasis7_agent_api::RuntimeReceiptLineageV1::from(receipt))
    }

    fn validated_feedback_history(
        &self,
    ) -> Result<
        Vec<oasis7_agent_harness::FeedbackHistoryProjection>,
        oasis7_agent_api::CognitionError,
    > {
        Err(memory_authority_scope_error())
    }

    fn continuation_projection(
        &self,
        _continuation: &Self::Continuation,
    ) -> Result<oasis7_agent_harness::ContinuationHostProjection, oasis7_agent_api::CognitionError>
    {
        Err(memory_authority_scope_error())
    }
}

fn memory_authority_scope_error() -> oasis7_agent_api::CognitionError {
    oasis7_agent_api::CognitionError::new(
        "runtime_memory_authority_scope",
        "verified World readback authorizes only its matching memory transition",
    )
}

impl AsyncAgentRunner {
    /// Restore only the bounded replay/collision verifier from an Oasis-owned
    /// Runtime authority projection. The Harness receives validated history,
    /// never the Runtime World or authority itself.
    pub(crate) fn restore_host_validated_feedback_history<H: RuntimeAuthority>(
        &mut self,
        authority: &H,
    ) -> Result<(), AsyncAgentRunnerError> {
        // Never refresh shared replay state while any actor turn is active.
        // The authenticated scoped-view caller still checks that Agent/session
        // against its projection and the already-loaded verifier below.
        if self.harness.active_turn_count() > 0 && self.harness.feedback_recovery_loaded() {
            return Ok(());
        }
        // Read once from the real host authority, then give the Harness and
        // this Oasis-side replay cache the same authenticated snapshot. The
        // Harness recovery adapter is history-only and cannot mint receipts,
        // leases, memory authority, or continuations.
        let mut history = authority
            .validated_feedback_history()
            .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
        for projection in &history {
            let key = (
                projection.feedback.agent_subject.clone(),
                projection.feedback.agent_session_id.clone(),
                projection.feedback.feedback_id.clone(),
            );
            let receipt_lineage = projection
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.receipt_lineage.as_ref())
                .map(runtime_receipt_from_api_projection);
            if self
                .runtime_feedback_replays
                .get(&key)
                .is_some_and(|accepted| accepted.receipt_lineage != receipt_lineage)
            {
                return Err(AsyncAgentRunnerError::Cognition(
                    "Runtime feedback history conflicts with accepted receipt lineage".into(),
                ));
            }
        }
        history.sort_by(|left, right| {
            left.feedback
                .agent_subject
                .cmp(&right.feedback.agent_subject)
                .then_with(|| {
                    left.feedback
                        .agent_session_id
                        .cmp(&right.feedback.agent_session_id)
                })
                .then_with(|| left.feedback.feedback_seq.cmp(&right.feedback.feedback_seq))
        });
        let history_authority =
            super::agent_harness_adapter::ScopedFeedbackHistoryAuthority::new(history.clone());
        self.harness
            .restore_feedback_history(&history_authority)
            .map_err(map_harness_error)?;

        for projection in history {
            if self
                .harness
                .replay_feedback(&projection.feedback)
                .map_err(map_harness_error)?
            {
                let receipt_lineage = projection
                    .lineage
                    .as_ref()
                    .and_then(|lineage| lineage.receipt_lineage.as_ref())
                    .map(runtime_receipt_from_api_projection);
                self.remember_runtime_feedback_replay(
                    &super::super::agent_api_compat::feedback_from_api(projection.feedback),
                    receipt_lineage,
                );
            }
        }
        Ok(())
    }

    /// Release exactly the completed production turn around a host durability
    /// boundary. The Harness keeps an identity-bound handoff capability until
    /// persistence succeeds; on failure both the actor replay fence and the
    /// local Runtime-awaiting maps are restored before the caller can retry.
    ///
    /// The callback receives only an immutable runner view, so it cannot
    /// enqueue a successor provider turn while the release is provisional.
    pub(crate) fn with_runtime_turn_release_for_continuation<F>(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        F: FnOnce(&Self) -> Result<(), String>,
    {
        let mut matches = self.awaiting_outcomes.iter().filter(|(_, outcome)| {
            outcome.agent_id == agent_id
                && outcome.prepared_context.as_ref().is_some_and(|context| {
                    context.agent_session_id == agent_session_id
                        && context.agent_turn_id == agent_turn_id
                        && context.decision_request_id == decision_request_id
                        && context.request_digest.to_string() == request_digest
                })
                && outcome
                    .prepared_request_context
                    .as_ref()
                    .is_some_and(|request| {
                        request.agent_subject == agent_id
                            && request.agent_session_id == agent_session_id
                            && request.agent_turn_id == agent_turn_id
                            && request.decision_request_id == decision_request_id
                            && request.request_digest.to_string() == request_digest
                    })
        });
        let Some((&turn_id, outcome)) = matches.next() else {
            return Err(AsyncAgentRunnerError::Cognition(
                "unknown pending Runtime turn".into(),
            ));
        };
        if matches.next().is_some()
            || self.awaiting_runtime.get(agent_id) != Some(&turn_id)
            || self
                .awaiting_outcomes
                .values()
                .filter(|outcome| outcome.agent_id == agent_id)
                .count()
                != 1
        {
            return Err(AsyncAgentRunnerError::Cognition(
                "pending Runtime turn identity or actor lane conflicts".into(),
            ));
        }
        let outcome = outcome.clone();
        let harness_turn = *self.harness_turns.get(&turn_id).ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("Harness turn identity is unavailable".into())
        })?;
        let handoff = self
            .harness
            .prepare_turn_release(harness_turn)
            .map_err(map_harness_error)?;

        // Staging removes only this Runtime turn. The authenticated replay
        // fence and turn record remain owned by the Harness capability until
        // commit or rollback below.
        self.awaiting_runtime.remove(agent_id);
        self.awaiting_outcomes.remove(&turn_id);
        match persist(self) {
            Ok(()) => self
                .harness
                .commit_turn_release(handoff)
                .map_err(map_harness_error),
            Err(persist_error) => {
                self.awaiting_runtime.insert(agent_id.to_string(), turn_id);
                self.awaiting_outcomes.insert(turn_id, outcome);
                self.harness
                    .rollback_turn_release(handoff)
                    .map_err(|rollback_error| {
                        AsyncAgentRunnerError::Cognition(format!(
                            "Runtime turn release persistence failed ({persist_error}); exact-turn rollback failed: {rollback_error}"
                        ))
                    })?;
                Err(AsyncAgentRunnerError::Cognition(persist_error))
            }
        }
    }

    pub(crate) fn needs_runtime_feedback_recovery(&self) -> bool {
        self.harness.needs_feedback_history_refresh()
    }

    /// True after the Harness has received a complete host-validated history
    /// projection. A loaded projection may still fence individual damaged
    /// Agent/session partitions; that must not be confused with having no
    /// history at all when deciding whether unrelated Agents may dispatch.
    pub(crate) fn runtime_feedback_history_loaded(&self) -> bool {
        self.harness.feedback_recovery_loaded()
    }

    pub(crate) fn feedback_recovery_blocked(&self, agent: &str, session: &str) -> bool {
        self.harness.feedback_recovery_blocked(agent, session)
    }

    #[cfg(test)]
    pub(crate) fn agent_feedback_recovery_blocked(&self, agent: &str) -> bool {
        self.harness.agent_feedback_recovery_blocked(agent)
    }

    /// Restore only the bounded replay/collision verifier from Runtime's
    /// durable outbox. Feedback is not consumed again and memory is untouched.
    #[cfg(test)]
    pub(crate) fn restore_runtime_feedback_outbox(
        &mut self,
        records: &[RuntimeFeedbackOutboxRecordV1],
    ) -> Result<(), AsyncAgentRunnerError> {
        if !self.needs_runtime_feedback_recovery() {
            return Ok(());
        }
        let projections = records
            .iter()
            .map(|record| {
                record.validate().map_err(|error| {
                    AsyncAgentRunnerError::Cognition(format!("invalid feedback record: {error}"))
                })?;
                let feedback = serde_json::from_value::<FeedbackEnvelopeV1>(record.payload.clone())
                    .map_err(|error| {
                        AsyncAgentRunnerError::Cognition(format!(
                            "invalid feedback payload: {error}"
                        ))
                    })?;
                if feedback.status == "committed" {
                    return Err(AsyncAgentRunnerError::Cognition(
                        "committed feedback history requires Runtime receipt authority".into(),
                    ));
                }
                Ok(oasis7_agent_harness::FeedbackHistoryProjection {
                    feedback: super::super::agent_api_compat::feedback_to_api(&feedback),
                    acknowledged: record.state == "acked",
                    lineage: Some(oasis7_agent_harness::FeedbackHistoryLineage {
                        original_envelope_digest: record.envelope_digest.clone(),
                        delivery_state: record.state.clone(),
                        receipt_lineage: None,
                    }),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let authority =
            super::agent_harness_adapter::ScopedFeedbackHistoryAuthority::new(projections);
        self.restore_host_validated_feedback_history(&authority)
    }

    /// Caller must have authenticated the Agent-scoped Runtime View.
    pub(crate) fn restore_preverified_scoped_feedback_history(
        &mut self,
        records: &[oasis7_agent_harness::FeedbackHistoryProjection],
    ) -> Result<(), AsyncAgentRunnerError> {
        let authority =
            super::agent_harness_adapter::ScopedFeedbackHistoryAuthority::new(records.to_vec());
        self.restore_host_validated_feedback_history(&authority)
    }

    /// Roll back native verifier/turn ledgers and private memory if the sole
    /// durable consumption checkpoint fails. No actor I/O occurs here.
    pub(crate) fn with_committed_feedback_memory_transaction<A, F>(
        &mut self,
        input: CommittedFeedbackMemoryInput<'_>,
        authority_receipt: &A::Receipt,
        authority: &A,
        memory: &mut MemoryWriteStore,
        persist: F,
    ) -> Result<(), AsyncAgentRunnerError>
    where
        A: RuntimeAuthority,
        F: FnOnce(&mut MemoryWriteStore) -> Result<(), String>,
    {
        let CommittedFeedbackMemoryInput {
            agent,
            feedback,
            receipt,
        } = input;
        let outcome = self
            .find_feedback_outcome(agent, &feedback)
            .ok_or_else(|| AsyncAgentRunnerError::Cognition("unknown actor outcome".into()))?;
        let context = outcome.prepared_context.as_ref().ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("committed feedback requires a host context".into())
        })?;
        if outcome.prepared_request_context.is_none() {
            return Err(AsyncAgentRunnerError::Cognition(
                "committed feedback requires a production Runtime request".into(),
            ));
        }
        validate_feedback(context, agent, &feedback)?;
        validate_runtime_receipt_lineage(context, &feedback, receipt)?;
        let mut staged_memory = memory.clone();
        project_receipt_memory(
            context,
            &feedback,
            receipt,
            authority_receipt,
            authority,
            &outcome.memory_write_intents,
            &mut staged_memory,
        )?;
        let original_memory = memory.clone();
        let harness_turn = *self.harness_turns.get(&outcome.turn_id).ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("Harness turn identity is unavailable".into())
        })?;
        #[cfg(any(test, feature = "test_tier_required"))]
        let native_before = self.rejected_wait_test_ledger_digests();
        let result = self.harness.accept_feedback_with_persistence_observed(
            harness_turn,
            super::super::agent_api_compat::feedback_to_api(&feedback),
            |_harness_cognition_digest| {
                *memory = staged_memory.clone();
                #[cfg(any(test, feature = "test_tier_required"))]
                if std::env::var("PRE2_APP_ADMISSION").ok().as_deref()
                    == Some("memory-ack-write-failure")
                {
                    let root = std::env::var_os("PRE2_METADATA_DIR")
                        .map(std::path::PathBuf::from)
                        .ok_or_else(|| "native ACK probe root missing".to_string())?;
                    if !root.join("feedback-ack-before-persist").exists() {
                        let mut staged = native_before.clone();
                        let mut hash = blake3::Hasher::new();
                        hash.update(b"oasis7.rejected-wait.local-test-ledger.v1\0");
                        hash.update(b"feedback-store\0");
                        hash.update(_harness_cognition_digest.as_bytes());
                        staged[4] = hash.finalize().to_hex().to_string();
                        for (name, value) in [
                            ("feedback-ack-native-before.json", native_before),
                            ("feedback-ack-native-staged.json", staged),
                        ] {
                            let path = root.join(name);
                            std::fs::write(&path, serde_json::to_vec(&value).unwrap())
                                .map_err(|error| error.to_string())?;
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                std::fs::set_permissions(
                                    path,
                                    std::fs::Permissions::from_mode(0o600),
                                )
                                .map_err(|error| error.to_string())?;
                            }
                        }
                    }
                }
                persist(memory).inspect_err(|_error| {
                    *memory = original_memory.clone();
                })
            },
        );
        if result.is_err() {
            *memory = original_memory;
            return result.map_err(map_harness_error);
        }
        self.release_runtime_turn(agent, outcome.turn_id);
        Ok(())
    }

    /// Preserve the historical receipt-lineage lifecycle API. The DTO is
    /// correlation data only; memory intents require the strict World-readback
    /// entry point below.
    pub fn consume_runtime_feedback_with_lineage(
        &mut self,
        agent_id: &str,
        feedback: FeedbackEnvelopeV1,
        runtime_receipt: Option<&RuntimeReceiptLineageV1>,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.consume_runtime_feedback_inner(agent_id, feedback, runtime_receipt, None, store)
    }

    fn consume_runtime_feedback_inner(
        &mut self,
        agent_id: &str,
        feedback: FeedbackEnvelopeV1,
        runtime_receipt: Option<&RuntimeReceiptLineageV1>,
        readback: Option<&super::feedback::RuntimeReceiptReadbackHandleV1>,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError> {
        let harness_feedback = super::super::agent_api_compat::feedback_to_api(&feedback);
        if self
            .harness
            .replay_feedback(&harness_feedback)
            .map_err(map_harness_error)?
        {
            let replay_key = (
                feedback.agent_subject.clone(),
                feedback.agent_session_id.clone(),
                feedback.feedback_id.clone(),
            );
            let accepted = self.runtime_feedback_replays.get(&replay_key);
            if accepted.is_some_and(|accepted| accepted.receipt_lineage.as_ref() != runtime_receipt)
                || (accepted.is_none()
                    && (feedback.status == "committed" || runtime_receipt.is_some()))
            {
                return Err(AsyncAgentRunnerError::Cognition(
                    "Runtime feedback replay receipt lineage mismatch".into(),
                ));
            }
            return Ok(());
        }
        let Some(outcome) = self.find_feedback_outcome(agent_id, &feedback) else {
            return Err(AsyncAgentRunnerError::Cognition(
                "unknown actor outcome".to_string(),
            ));
        };
        let context = outcome.prepared_context.as_ref().ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("runtime feedback requires a host context".to_string())
        })?;
        validate_feedback(context, agent_id, &feedback)?;
        let harness_turn = *self.harness_turns.get(&outcome.turn_id).ok_or_else(|| {
            AsyncAgentRunnerError::Cognition("Harness turn identity is unavailable".into())
        })?;

        let mut staged_memory = None;
        if feedback.status == "committed" {
            let runtime_receipt = runtime_receipt.ok_or_else(|| {
                AsyncAgentRunnerError::Cognition(
                    "committed feedback requires a Runtime-issued receipt lineage".to_string(),
                )
            })?;
            validate_runtime_receipt_lineage(context, &feedback, runtime_receipt)?;
            if let Some(readback) = readback {
                if readback.receipt_id() != runtime_receipt.receipt_id
                    || readback.receipt_digest().as_str() != runtime_receipt.receipt_digest
                {
                    return Err(AsyncAgentRunnerError::Cognition(
                        "Runtime readback handle does not match the accepted receipt".into(),
                    ));
                }
                let mut staged = store.clone();
                let authority = VerifiedWorldMemoryAuthority {
                    turn: context,
                    feedback: &feedback,
                    receipt: runtime_receipt,
                    readback,
                };
                project_receipt_memory(
                    context,
                    &feedback,
                    runtime_receipt,
                    runtime_receipt,
                    &authority,
                    &outcome.memory_write_intents,
                    &mut staged,
                )?;
                staged_memory = Some(staged);
            }
        }

        let production_lane = outcome.prepared_request_context.is_some();
        if production_lane {
            self.harness
                .accept_feedback_with_persistence(
                    harness_turn,
                    super::super::agent_api_compat::feedback_to_api(&feedback),
                    || Ok(()),
                )
                .map_err(map_harness_error)?;
            if let Some(staged) = staged_memory {
                *store = staged;
            }
        } else {
            self.harness
                .accept_compatibility_feedback(
                    harness_turn,
                    super::super::agent_api_compat::feedback_to_api(&feedback),
                )
                .map_err(map_harness_error)?;
            if let Some(staged) = staged_memory {
                *store = staged;
            }
        }

        self.remember_runtime_feedback_replay(
            &feedback,
            runtime_receipt
                .filter(|_| feedback.status == "committed")
                .cloned(),
        );

        // Runtime `pending` is an in-flight disposition, not a terminal
        // outcome. Keep the prepared turn occupied until Runtime resolves it.
        if matches!(feedback.status.as_str(), "rejected" | "failed") {
            self.invalidate_continuation_for_turn(
                agent_id,
                &feedback.agent_session_id,
                &feedback.agent_turn_id,
                &feedback.decision_request_id,
                ContinuationInvalidationReason::Rejected,
            );
            self.release_runtime_turn(agent_id, outcome.turn_id);
        } else if feedback.status == "committed" {
            self.release_runtime_turn(agent_id, outcome.turn_id);
        }
        Ok(())
    }

    pub fn consume_runtime_feedback(
        &mut self,
        agent_id: &str,
        feedback: FeedbackEnvelopeV1,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError> {
        self.consume_runtime_feedback_with_lineage(agent_id, feedback, None, store)
    }

    /// Strict production seam: Runtime must prove the durable World readback
    /// before a committed response/artifact can authorize memory projection.
    /// The compatibility lineage method remains available for older adapters,
    /// but it does not claim this readback proof.
    pub fn consume_runtime_feedback_with_world_readback(
        &mut self,
        agent_id: &str,
        feedback: FeedbackEnvelopeV1,
        runtime_receipt: &RuntimeReceiptLineageV1,
        response_identity: &super::super::continuous_agent_harness::ResponseArtifactIdentityV1,
        verifier: &dyn RuntimeReceiptReadbackVerifier,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError> {
        let outcome = self
            .awaiting_outcomes
            .values()
            .find(|outcome| {
                outcome.agent_id == agent_id
                    && outcome.prepared_context.as_ref().is_some_and(|context| {
                        context.agent_session_id == feedback.agent_session_id
                            && context.agent_turn_id == feedback.agent_turn_id
                            && context.decision_request_id == feedback.decision_request_id
                    })
            })
            .cloned()
            .ok_or_else(|| {
                AsyncAgentRunnerError::Cognition(
                    "Runtime readback requires an awaiting outcome".to_string(),
                )
            })?;
        let readback = if feedback.status == "committed" {
            let context = outcome
                .prepared_context
                .as_ref()
                .expect("outcome response has a prepared context");
            validate_feedback(context, agent_id, &feedback)?;
            validate_runtime_receipt_lineage(context, &feedback, runtime_receipt)?;
            let response = outcome.prepared_response_context.as_ref().ok_or_else(|| {
                AsyncAgentRunnerError::Cognition(
                    "Runtime response artifact identity is unavailable".to_string(),
                )
            })?;
            response
                .validate_response_artifact_identity(response_identity)
                .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
            let handle = verifier.verify_world_readback(
                context,
                &feedback,
                runtime_receipt,
                response_identity,
            )?;
            if handle.receipt_id() != runtime_receipt.receipt_id
                || handle.receipt_digest().as_str() != runtime_receipt.receipt_digest
                || handle.verified_lineage() != runtime_receipt
                || handle.response_artifact_digest() != &response_identity.artifact_digest
            {
                return Err(AsyncAgentRunnerError::Cognition(
                    "Runtime readback verifier returned mismatched identity".to_string(),
                ));
            }
            Some(handle)
        } else {
            None
        };
        self.consume_runtime_feedback_inner(
            agent_id,
            feedback,
            Some(runtime_receipt),
            readback.as_ref(),
            store,
        )
    }

    /// Production helper for the Runtime adapter: derive the expected
    /// response identity from the exact still-awaiting actor outcome rather
    /// than accepting a parallel caller-provided identity at the host seam.
    pub(crate) fn consume_runtime_feedback_with_world_readback_from_outcome(
        &mut self,
        agent_id: &str,
        feedback: FeedbackEnvelopeV1,
        runtime_receipt: &RuntimeReceiptLineageV1,
        verifier: &dyn RuntimeReceiptReadbackVerifier,
        store: &mut MemoryWriteStore,
    ) -> Result<(), AsyncAgentRunnerError> {
        let response_identity = self
            .awaiting_outcomes
            .values()
            .find(|outcome| {
                outcome.agent_id == agent_id
                    && outcome.prepared_context.as_ref().is_some_and(|context| {
                        context.agent_session_id == feedback.agent_session_id
                            && context.agent_turn_id == feedback.agent_turn_id
                            && context.decision_request_id == feedback.decision_request_id
                    })
            })
            .and_then(|outcome| outcome.prepared_response_context.as_ref())
            .map(|response| response.response_artifact_identity())
            .ok_or_else(|| {
                AsyncAgentRunnerError::Cognition(
                    "Runtime response artifact identity is unavailable".to_string(),
                )
            })?;
        self.consume_runtime_feedback_with_world_readback(
            agent_id,
            feedback,
            runtime_receipt,
            &response_identity,
            verifier,
            store,
        )
    }

    /// Explicitly expire a Runtime-owned pending lease. This is the only
    /// non-terminal path that releases a prepared outcome; it does not erase
    /// the request identity, so the same request cannot be replayed as a new
    /// provider invocation after expiry.
    pub fn expire_runtime_turn(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
    ) -> Result<(), AsyncAgentRunnerError> {
        let Some((&turn_id, _outcome)) = self.awaiting_outcomes.iter().find(|(_, outcome)| {
            outcome.agent_id == agent_id
                && outcome.prepared_context.as_ref().is_some_and(|context| {
                    context.agent_session_id == agent_session_id
                        && context.agent_turn_id == agent_turn_id
                        && context.decision_request_id == decision_request_id
                        && context.request_digest.to_string() == request_digest
                })
        }) else {
            return Err(AsyncAgentRunnerError::Cognition(
                "unknown pending Runtime turn".to_string(),
            ));
        };
        self.invalidate_continuation_for_turn(
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            super::ContinuationInvalidationReason::Timeout,
        );
        self.release_runtime_turn(agent_id, turn_id);
        Ok(())
    }

    /// Release the actor's completed outcome after a provider Wait has been
    /// admitted as a continuation. Unlike `expire_runtime_turn`, this keeps
    /// the Harness continuation chain active: the Runtime-owned wake is now
    /// the durable owner of the next invocation, so expiring here would
    /// incorrectly invalidate a successfully admitted wait.
    pub fn release_runtime_turn_for_continuation(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
    ) -> Result<(), AsyncAgentRunnerError> {
        let Some((&turn_id, _outcome)) = self.awaiting_outcomes.iter().find(|(_, outcome)| {
            outcome.agent_id == agent_id
                && outcome.prepared_context.as_ref().is_some_and(|context| {
                    context.agent_session_id == agent_session_id
                        && context.agent_turn_id == agent_turn_id
                        && context.decision_request_id == decision_request_id
                        && context.request_digest.to_string() == request_digest
                })
        }) else {
            return Err(AsyncAgentRunnerError::Cognition(
                "unknown pending Runtime turn".to_string(),
            ));
        };
        self.release_runtime_turn(agent_id, turn_id);
        // A provider Wait is terminal for this logical request once Runtime
        // has admitted its continuation. Clear the single-flight marker so
        // the fresh wake/resume request can enter the actor; request replay
        // protection remains in digest_by_request_id.
        Ok(())
    }
}
