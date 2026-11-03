//! Durable actor escrow. Live originals remain in the runner's awaiting map.
use super::*;
use crate::simulator::{AsyncAgentTurnOutcome, AsyncTurnFeedback};

fn same_request_identity(
    a: &crate::simulator::ContinuousAgentRequestContextV1,
    b: &crate::simulator::ContinuousAgentRequestContextV1,
) -> bool {
    a.agent_subject == b.agent_subject
        && a.agent_session_id == b.agent_session_id
        && a.agent_turn_id == b.agent_turn_id
        && a.decision_request_id == b.decision_request_id
        && a.request_digest == b.request_digest
        && a.retry_seq == b.retry_seq
        && a.transport_attempt == b.transport_attempt
}

fn identity(
    outcome: &AsyncAgentTurnOutcome,
    active: &cognition_context::ProviderContextState,
) -> Result<(String, String, String, String, String), String> {
    let request = &active.request_context;
    if outcome.agent_id != request.agent_subject
        || outcome.prepared_context.as_ref().is_some_and(|context| {
            context.agent_session_id != request.agent_session_id
                || context.agent_turn_id != request.agent_turn_id
                || context.decision_request_id != request.decision_request_id
                || context.request_digest != request.request_digest
        })
        || outcome
            .prepared_request_context
            .as_ref()
            .is_some_and(|context| !same_request_identity(context, request))
    {
        return Err(
            "raw outcome original context identities differ; runner originals retained".into(),
        );
    }
    Ok((
        outcome.agent_id.clone(),
        request.agent_session_id.clone(),
        request.agent_turn_id.clone(),
        request.decision_request_id.clone(),
        request.request_digest.to_string(),
    ))
}

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn validate_raw_escrow(
        outcomes: &[AsyncAgentTurnOutcome],
        active: &BTreeMap<String, cognition_context::ProviderContextState>,
    ) -> Result<(), String> {
        if outcomes.len() > 128 {
            return Err("raw outcome escrow capacity exhausted".into());
        }
        let mut identities = std::collections::BTreeSet::new();
        for outcome in outcomes {
            let owner = active
                .get(&outcome.agent_id)
                .ok_or("raw outcome active owner missing")?;
            if !identities.insert(identity(outcome, owner)?) {
                return Err("duplicate raw outcome original identity".into());
            }
        }
        Ok(())
    }

    pub(in crate::viewer::runtime_live) fn hosted_raw_outcomes(
        &self,
    ) -> Result<Vec<AsyncAgentTurnOutcome>, String> {
        let live = match self.runner.as_ref() {
            Some(
                RuntimeDecisionRunner::Builtin(runner)
                | RuntimeDecisionRunner::ProviderBacked(runner),
            ) => Some(runner),
            _ => None,
        };
        let mut originals = BTreeMap::new();
        for outcome in self.provider_raw_outcomes.iter().chain(
            live.into_iter()
                .flat_map(|runner| runner.awaiting_runtime_outcomes()),
        ) {
            // Canonical disposition retires the sidecar marker. The runner still
            // retains its original until the existing feedback ACK releases it.
            let Some(active) = self.provider_active_turns.get(&outcome.agent_id) else {
                continue;
            };
            let key = identity(outcome, active)?;
            if let Some(previous) = originals.insert(key, outcome.clone()) {
                if previous != *outcome {
                    return Err("raw outcome identity collision; originals retained".into());
                }
            }
        }
        if originals.len() > 128 {
            return Err("raw outcome escrow capacity exhausted; runner originals retained".into());
        }
        Ok(originals.into_values().collect())
    }

    pub(super) fn capture_hosted_raw_outcomes(
        &mut self,
        world: &RuntimeWorld,
    ) -> Result<(), String> {
        let mut first_error = None;
        for outcome in self.hosted_raw_outcomes()? {
            let request = self
                .provider_active_turns
                .get(&outcome.agent_id)
                .cloned()
                .ok_or("raw outcome active owner missing")?;
            if self.provider_terminal_matches_request(&outcome.agent_id, &request.request_context) {
                continue;
            }
            let matches = |c: &RuntimeProviderActionContext| {
                same_request_identity(&request.request_context, &c.request.request_context)
            };
            let represented = self
                .provider_completed_decisions
                .iter()
                .chain(self.provider_held_decisions.values())
                .filter_map(|decision| decision.cognition.as_ref())
                .any(matches)
                || self
                    .provider_service_pending
                    .get(&outcome.agent_id)
                    .is_some_and(|pending| matches(&pending.cognition))
                || self
                    .hosted_wait
                    .as_ref()
                    .is_some_and(|wait| wait.owns_provider_context(&request))
                || self
                    .hosted_resume
                    .as_ref()
                    .is_some_and(|resume| resume.owns_provider_context(&request));
            if represented {
                continue;
            }
            let converted = (|| {
                if outcome.prepared_context.is_none() || outcome.prepared_request_context.is_none()
                {
                    return Err("hosted provider outcome missing original prepared context".into());
                }
                let lease = outcome
                    .cognition_lease
                    .clone()
                    .ok_or("hosted provider outcome missing original lease")?;
                self.validate_provider_cognition_lease_for_request(
                    world,
                    &outcome.agent_id,
                    &request.request_context,
                    &lease,
                    "outcome",
                )?;
                let response = outcome
                    .prepared_response_context
                    .clone()
                    .ok_or("hosted provider outcome missing response artifact")?;
                response
                    .validate_response_artifact_identity(&response.response_artifact_identity())
                    .map_err(|error| error.to_string())?;
                let expected = &request.request_context;
                if response.agent_session_id != expected.agent_session_id
                    || response.agent_turn_id != expected.agent_turn_id
                    || response.decision_request_id != expected.decision_request_id
                    || response.request_digest != expected.request_digest
                    || response.retry_seq != expected.retry_seq
                    || response.transport_attempt != expected.transport_attempt
                    || response.base_decision_response.memory_write_intents
                        != outcome.memory_write_intents
                {
                    return Err(
                        "hosted provider response differs from original request or memory intents"
                            .into(),
                    );
                }
                let decision = outcome
                    .decision
                    .clone()
                    .ok_or("hosted provider actor produced no decision")?;
                Ok::<_, String>(async_support::RuntimeLlmDecision {
                    agent_id: outcome.agent_id.clone(),
                    decision,
                    decision_trace: outcome.decision_trace.clone(),
                    cognition: Some(RuntimeProviderActionContext {
                        request: request.clone(),
                        response,
                        cognition_lease: Some(lease),
                        memory_write_intents: outcome.memory_write_intents.clone(),
                    }),
                    memory_write_intents: outcome.memory_write_intents.clone(),
                    continuation_admitted: false,
                })
            })();
            match converted {
                Ok(decision) => self.retain_hosted_outcome(decision),
                Err(error) => {
                    let known = outcome.prepared_context.is_some()
                        && outcome.prepared_request_context.is_some()
                        && outcome.cognition_lease.as_ref().is_some_and(|lease| {
                            self.validate_provider_cognition_lease_for_request(
                                world,
                                &outcome.agent_id,
                                &request.request_context,
                                lease,
                                "failed outcome",
                            )
                            .is_ok()
                        })
                        && matches!(
                            outcome.feedback,
                            AsyncTurnFeedback::ProviderError { .. }
                                | AsyncTurnFeedback::ActorPanicked
                        );
                    let reason = if known {
                        "executor_failed_after_dispatch:known_provider_failure"
                    } else {
                        "actor_outcome_validation_pending"
                    };
                    let reason = format!("{reason}:{error}");
                    let previous = self.provider_recovery_pending.get(&outcome.agent_id);
                    if previous.is_some_and(|pending| {
                        !same_request_identity(
                            &pending.active.request_context,
                            &request.request_context,
                        )
                    }) {
                        return Err(
                            "raw outcome recovery identity collision; escrow retained".into()
                        );
                    }
                    if previous.is_none() {
                        self.provider_recovery_pending.insert(
                            outcome.agent_id.clone(),
                            lineage_persistence::ProviderRecoveryPending {
                                active: request,
                                reason,
                            },
                        );
                        self.hosted_outcome_checkpoint_pending = true;
                        first_error
                            .get_or_insert("hosted actor outcome retained for recovery".to_owned());
                    }
                }
            }
        }
        self.checkpoint_hosted_outcomes()?;
        first_error.map_or(Ok(()), Err)
    }
}
