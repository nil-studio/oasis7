use super::*;
use crate::viewer::runtime_live::decision_trace::is_trace_only_overflow;

impl RuntimeLlmSidecar {
    /// Poll the native actor without entering any synchronous authority adapter.
    pub(in crate::viewer::runtime_live) fn poll_hosted_provider_decision(
        &mut self,
        world: &RuntimeWorld,
    ) -> Result<Option<async_support::RuntimeLlmDecision>, String> {
        let Some(runner) = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
        else {
            return Ok(None);
        };
        let completed = runner.poll_completed().map_err(|error| error.to_string())?;
        let _ = runner.take_completed();
        for outcome in completed {
            let request = outcome
                .prepared_context
                .zip(outcome.prepared_request_context)
                .map(
                    |(turn_context, request_context)| cognition_context::ProviderContextState {
                        turn_context,
                        request_context,
                    },
                )
                .ok_or("hosted provider outcome missing original prepared context")?;
            if self.provider_terminal_matches_request(&outcome.agent_id, &request.request_context) {
                continue;
            }
            let lease = outcome
                .cognition_lease
                .ok_or("hosted provider outcome missing original lease")?;
            self.validate_provider_cognition_lease_for_request(
                world,
                &outcome.agent_id,
                &request.request_context,
                &lease,
                "outcome",
            )?;
            let cognition = RuntimeProviderActionContext {
                request,
                response: outcome
                    .prepared_response_context
                    .ok_or("hosted provider outcome missing response artifact")?,
                cognition_lease: Some(lease),
                memory_write_intents: outcome.memory_write_intents.clone(),
            };
            let decision = async_support::RuntimeLlmDecision {
                agent_id: outcome.agent_id,
                decision: outcome
                    .decision
                    .ok_or("hosted provider actor produced no decision")?,
                decision_trace: outcome.decision_trace,
                cognition: Some(cognition),
                memory_write_intents: outcome.memory_write_intents,
                continuation_admitted: false,
            };
            self.provider_completed_decisions.push_back(decision);
        }
        self.persist_provider_lineage()?;
        let Some(decision) = self.provider_completed_decisions.front() else {
            return Ok(None);
        };
        if decision.decision_trace.as_ref().is_some_and(|trace| {
            trace.parse_error.is_some()
                || (trace.llm_error.is_some() && !is_trace_only_overflow(trace))
        }) {
            return Err("hosted provider outcome validation failed; original turn retained".into());
        }
        Ok(Some(decision.clone()))
    }

    pub(in crate::viewer::runtime_live) fn retire_hosted_provider_outcome(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
    ) -> Result<(), String> {
        if self
            .provider_completed_decisions
            .front()
            .and_then(|decision| decision.cognition.as_ref())
            .is_some_and(|cognition| &cognition.request.request_context == request)
        {
            self.provider_completed_decisions.pop_front();
        }
        self.persist_provider_lineage()
    }
}

impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn validate_hosted_settlement_receipt(
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        lease: &crate::runtime::CognitionLeaseV1,
        value: serde_json::Value,
    ) -> Result<(), String> {
        lineage_generation_recovery::validate_provider_lease_identity(
            &request.agent_subject,
            request,
            lease,
        )?;
        let receipt: crate::runtime::CognitionReceiptV1 =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        receipt.validate().map_err(|error| error.to_string())?;
        if receipt.operation != "settle"
            || receipt.status != crate::runtime::CognitionLeaseStatusV1::Settled
            || receipt.lease_id != lease.lease_id
            || receipt.idempotency_key != lease.idempotency_key
            || receipt.account_id != lease.account_id
            || receipt.agent_id != lease.agent_id
            || receipt.agent_session_id != lease.agent_session_id
            || receipt.agent_turn_id != lease.agent_turn_id
            || receipt.decision_request_id != lease.decision_request_id
            || receipt.request_digest != lease.request_digest
            || receipt.quote != lease.quote
            || receipt.reserved_amount != lease.reserved_amount
            || receipt.consumed_amount != lease.reserved_amount
            || receipt.net_amount != lease.reserved_amount
            || receipt.released_amount != 0
            || receipt.refunded_amount != 0
        {
            return Err("canonical settlement receipt identity or accounting mismatch".into());
        }
        Ok(())
    }
}
