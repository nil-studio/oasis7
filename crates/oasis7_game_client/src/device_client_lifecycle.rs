//! Facts about one retained logical operation. Querying never creates or sends
//! a business original; explicit termination is a separate write port.
use super::*;
use crate::operation_lifecycle::{
    self as policy, DeliveryPhase, DomainDisposition, OperationPolicy, RecoveryAction,
    RecoveryInput, RecoveryIntent,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginalKind {
    Prefix,
    Action,
    Outcome,
}

#[derive(Debug, Clone, Serialize)]
pub struct OriginalIdentity {
    pub kind: OriginalKind,
    pub cache_operation: String,
    pub correlation: RequestCorrelation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OperationDisposition {
    Vacant,
    ContextReady,
    ActionCommitted,
    ContextTerminated { reason: String },
    Received,
    Pending,
    Unknown,
    HistoryUnavailable,
    Rejected { reason: WorldServiceErrorKind },
    Expired,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationObservation {
    pub logical_operation: String,
    pub originals: Vec<OriginalIdentity>,
    /// Native legacy cache has no durable dispatch marker. Never infer unsent.
    pub delivery_phase: DeliveryPhase,
    pub next_action: RecoveryAction,
    pub response: Option<IntentResponse<Value>>,
    pub disposition: OperationDisposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextTermination {
    BudgetExhausted,
    RetireExpired,
    Abort,
}

fn outcome_reason(request: &SubmitIntentRequest<WorldServicePayloadV1>) -> Result<&str, String> {
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return Err("invalid original context outcome".into());
    };
    let SchedulerOperationV1::TaskProviderFailure { reason, .. } = &signed.request.operation else {
        return Err("invalid original context outcome".into());
    };
    Ok(reason)
}

fn disposition(
    kind: OriginalKind,
    reason: Option<&str>,
    response: &IntentResponse<Value>,
) -> OperationDisposition {
    match &response.outcome {
        IntentOutcome::Committed { .. } => match kind {
            OriginalKind::Prefix => OperationDisposition::ContextReady,
            OriginalKind::Action => OperationDisposition::ActionCommitted,
            OriginalKind::Outcome => OperationDisposition::ContextTerminated {
                reason: reason.unwrap_or_default().into(),
            },
        },
        IntentOutcome::Received { .. } => OperationDisposition::Received,
        IntentOutcome::Pending => OperationDisposition::Pending,
        IntentOutcome::Unknown => OperationDisposition::Unknown,
        IntentOutcome::HistoryUnavailable => OperationDisposition::HistoryUnavailable,
        IntentOutcome::Rejected { reason } => OperationDisposition::Rejected {
            reason: reason.clone(),
        },
        IntentOutcome::Expired => OperationDisposition::Expired,
    }
}

fn observation(
    operation: &str,
    originals: Vec<OriginalIdentity>,
    disposition: OperationDisposition,
    response: Option<IntentResponse<Value>>,
) -> OperationObservation {
    let status = match disposition {
        OperationDisposition::ContextReady
        | OperationDisposition::ActionCommitted
        | OperationDisposition::ContextTerminated { .. } => DomainDisposition::Committed,
        OperationDisposition::Received => DomainDisposition::Received,
        OperationDisposition::Pending => DomainDisposition::Pending,
        OperationDisposition::HistoryUnavailable => DomainDisposition::HistoryUnavailable,
        OperationDisposition::Rejected { .. } => DomainDisposition::Rejected,
        OperationDisposition::Expired => DomainDisposition::Expired,
        _ => DomainDisposition::Unknown,
    };
    let next_action = policy::next_action(RecoveryInput {
        intent: RecoveryIntent::QueryOnly,
        policy: OperationPolicy::QueryOnly,
        phase: DeliveryPhase::Unknown,
        disposition: status,
        proof_verified: response.is_some(),
        original_present: !originals.is_empty(),
        scope_current: true,
        read_allowed: true,
        write_allowed: false,
        stopped: false,
    });
    OperationObservation {
        logical_operation: operation.into(),
        originals,
        delivery_phase: DeliveryPhase::Unknown,
        disposition,
        response,
        next_action,
    }
}

impl Client {
    /// Read-only recovery of one slot. The run-wide scan/barrier belongs to the
    /// caller. All cached lanes are checked before any network observation.
    pub fn recover_operation(&self, operation: &str) -> Result<OperationObservation, String> {
        let action = self.cached_intent(ClientStateKind::Action, operation)?;
        let outcome = self.cached_intent(ClientStateKind::Outcome, operation)?;
        let prefix = self.cached_intent(ClientStateKind::Prefix, operation)?;
        if action.is_some() && outcome.is_some() {
            return Err("context has conflicting original action and outcome; retain both".into());
        }
        let originals = [
            (OriginalKind::Prefix, &prefix),
            (OriginalKind::Action, &action),
            (OriginalKind::Outcome, &outcome),
        ]
        .into_iter()
        .filter_map(|(kind, request)| {
            request.as_ref().map(|request| OriginalIdentity {
                kind,
                cache_operation: operation.into(),
                correlation: request.correlation.clone(),
            })
        })
        .collect();
        let selected = action
            .as_ref()
            .map(|r| (OriginalKind::Action, r))
            .or_else(|| outcome.as_ref().map(|r| (OriginalKind::Outcome, r)))
            .or_else(|| prefix.as_ref().map(|r| (OriginalKind::Prefix, r)));
        let (observed, response) = if let Some((kind, original)) = selected {
            let reason = if kind == OriginalKind::Outcome {
                Some(outcome_reason(original)?)
            } else {
                None
            };
            let response = self.poll_original(original)?;
            (disposition(kind, reason, &response), Some(response))
        } else {
            (OperationDisposition::Vacant, None)
        };
        Ok(observation(operation, originals, observed, response))
    }

    /// Explicit single-context write, permitted only after the Harness has
    /// finished its full-run scan. Its immutable initial fence is never inferred
    /// from this slot; the caller supplies it and retains Stop/budget policy.
    pub fn terminate_context(
        &self,
        operation: &str,
        initial_fence: &GameTaskFenceV1,
        intent: ContextTermination,
    ) -> Result<Option<OperationObservation>, String> {
        if self
            .cached_intent(ClientStateKind::Action, operation)?
            .is_some()
        {
            return Err("context has an original action; recover that result instead".into());
        }
        if let Some(original) = self.cached_intent(ClientStateKind::Outcome, operation)? {
            if intent == ContextTermination::Abort
                && outcome_reason(&original)? != "executor_aborted"
            {
                return Err("context already has a different original outcome".into());
            }
            return self.recover_operation(operation).map(Some);
        }
        let prefix = self
            .cached_intent(ClientStateKind::Prefix, operation)?
            .ok_or("original context prefix unavailable")?;
        if intent == ContextTermination::RetireExpired {
            let response = self.poll_original(&prefix)?;
            if !matches!(response.outcome, IntentOutcome::Committed { .. }) {
                return Ok(Some(observation(
                    operation,
                    vec![OriginalIdentity {
                        kind: OriginalKind::Prefix,
                        cache_operation: operation.into(),
                        correlation: prefix.correlation.clone(),
                    }],
                    disposition(OriginalKind::Prefix, None, &response),
                    Some(response),
                )));
            }
        }
        let WorldServicePayloadV1::Scheduler(signed) = &prefix.signed_payload else {
            return Err("invalid original context prefix".into());
        };
        let SchedulerOperationV1::TaskProviderPrefix { context } = &signed.request.operation else {
            return Err("invalid original context prefix".into());
        };
        let current = self.public_view()?;
        if current.task_fence() != Some(initial_fence)
            || current.task_fence() != Some(&context.fence)
            || context.fence.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active
        {
            return Err(match intent {
                ContextTermination::BudgetExhausted | ContextTermination::Abort => {
                    "task control changed before context termination"
                }
                ContextTermination::RetireExpired => {
                    "task control changed before expired context retirement"
                }
            }
            .into());
        }
        if intent == ContextTermination::RetireExpired
            && current.logical_tick() <= context.valid_until_tick.0
        {
            return Ok(None);
        }
        let reason = match intent {
            ContextTermination::BudgetExhausted => "transport_exhausted",
            ContextTermination::RetireExpired => "context_expired",
            ContextTermination::Abort => "executor_aborted",
        };
        let response = self.outcome(operation, reason)?;
        let original = self
            .cached_intent(ClientStateKind::Outcome, operation)?
            .ok_or("original context outcome unavailable")?;
        Ok(Some(observation(
            operation,
            vec![
                OriginalIdentity {
                    kind: OriginalKind::Prefix,
                    cache_operation: operation.into(),
                    correlation: prefix.correlation,
                },
                OriginalIdentity {
                    kind: OriginalKind::Outcome,
                    cache_operation: operation.into(),
                    correlation: original.correlation,
                },
            ],
            disposition(OriginalKind::Outcome, Some(reason), &response),
            Some(response),
        )))
    }
}
