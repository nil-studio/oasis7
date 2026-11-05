//! The original Journal scalar transition, shared with Runtime validation.
//! This function neither interprets IDs nor grants storage/domain authority.
pub const COGNITION_LIFECYCLE_KEY_FIELDS_V1: [&str; 5] = [
    "agent_id",
    "agent_session_id",
    "agent_turn_id",
    "decision_request_id",
    "request_digest",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CognitionLifecycleStateV1 {
    Started = 1,
    ContextCaptured = 2,
    Dispatched = 3,
    ResponseRecorded = 4,
    Submitted = 5,
    Validated = 6,
    Linked = 7,
    Failed = 8,
    Cancelled = 9,
    Completed = 10,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CognitionLifecycleErrorV1 {
    TransportInvalid,
    TransportGap,
    Order,
}
impl CognitionLifecycleErrorV1 {
    pub fn message(self) -> &'static str {
        match self {
            Self::TransportInvalid => "cognition_transport_attempt_invalid",
            Self::TransportGap => "cognition_transport_attempt_gap",
            Self::Order => "cognition_lifecycle_order_invalid",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CognitionLifecycleAdvanceV1 {
    pub state: CognitionLifecycleStateV1,
    /// Only RequestDispatched changes the original attempt map.
    pub transport_attempt: Option<u64>,
}
pub fn is_cognition_lifecycle_kind_v1(kind: &str) -> bool {
    matches!(
        kind,
        "TurnStarted"
            | "ContextCaptured"
            | "RequestDispatched"
            | "ResponseRecorded"
            | "DecisionEnvelopeSubmitted"
            | "DecisionValidated"
            | "WorldReceiptLinked"
            | "DecisionRejected"
            | "CognitionTurnFailed"
            | "CognitionTurnCancelled"
            | "CognitionTurnCompleted"
            | "LateResponseRejected"
    )
}
/// The caller preserves the original five String tuple and sparse-identity
/// compatibility gate. Transport is queried only for the original valid
/// dispatch branch, preserving validation/error and map mutation order.
pub fn advance_cognition_lifecycle_v1<T>(
    kind: &str,
    state: Option<CognitionLifecycleStateV1>,
    transport: T,
) -> Result<Option<CognitionLifecycleAdvanceV1>, CognitionLifecycleErrorV1>
where
    T: FnOnce() -> Result<(u64, u64), CognitionLifecycleErrorV1>,
{
    use CognitionLifecycleStateV1::*;
    if !is_cognition_lifecycle_kind_v1(kind) {
        return Ok(None);
    }
    let mut transport_attempt = None;
    let next = match kind {
        "TurnStarted" if state.is_none() => Started,
        "DecisionRejected"
            if matches!(
                state,
                None | Some(
                    Started
                        | ContextCaptured
                        | Dispatched
                        | ResponseRecorded
                        | Submitted
                        | Validated
                )
            ) =>
        {
            Cancelled
        }
        "ContextCaptured" if matches!(state, Some(Started)) => ContextCaptured,
        "RequestDispatched" if matches!(state, Some(ContextCaptured | Dispatched)) => {
            let (previous, attempt) = transport()?;
            if attempt != previous.saturating_add(1) {
                return Err(CognitionLifecycleErrorV1::TransportGap);
            }
            transport_attempt = Some(attempt);
            Dispatched
        }
        "ResponseRecorded" if matches!(state, Some(Dispatched)) => ResponseRecorded,
        "DecisionEnvelopeSubmitted" if matches!(state, Some(ResponseRecorded)) => Submitted,
        "DecisionValidated"
            if matches!(
                state,
                Some(ContextCaptured | Dispatched | ResponseRecorded | Submitted)
            ) =>
        {
            Validated
        }
        "WorldReceiptLinked" if matches!(state, Some(Validated)) => Linked,
        "CognitionTurnFailed" if matches!(state, Some(ContextCaptured | Dispatched)) => Failed,
        "CognitionTurnCancelled" if !matches!(state, None | Some(Completed)) => Cancelled,
        "CognitionTurnCompleted"
            if matches!(state, Some(Failed | Cancelled | Validated | Linked)) =>
        {
            Completed
        }
        "LateResponseRejected" if matches!(state, Some(Cancelled | Completed)) => {
            state.expect("matched state")
        }
        _ => return Err(CognitionLifecycleErrorV1::Order),
    };
    Ok(Some(CognitionLifecycleAdvanceV1 {
        state: next,
        transport_attempt,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_turn_path_and_retry_attempts() {
        use CognitionLifecycleStateV1::*;
        let expected = [
            ("TurnStarted", Started, None),
            ("ContextCaptured", ContextCaptured, None),
            ("RequestDispatched", Dispatched, Some(1)),
            ("RequestDispatched", Dispatched, Some(2)),
            ("ResponseRecorded", ResponseRecorded, None),
            ("DecisionEnvelopeSubmitted", Submitted, None),
            ("DecisionValidated", Validated, None),
            ("WorldReceiptLinked", Linked, None),
            ("CognitionTurnCompleted", Completed, None),
            ("LateResponseRejected", Completed, None),
        ];
        let mut state = None;
        let mut attempt = 0;
        for (kind, next, expected_attempt) in expected {
            let mut queried = false;
            let update = advance_cognition_lifecycle_v1(kind, state, || {
                queried = true;
                Ok((attempt, expected_attempt.expect("dispatch only")))
            })
            .unwrap()
            .unwrap();
            assert_eq!(update.state, next);
            assert_eq!(update.transport_attempt, expected_attempt);
            assert_eq!(queried, expected_attempt.is_some());
            if let Some(next) = update.transport_attempt {
                attempt = next;
            }
            state = Some(update.state);
        }
    }
    #[test]
    fn original_terminal_and_error_branches_keep_their_order() {
        use CognitionLifecycleStateV1::*;
        assert_eq!(
            advance_cognition_lifecycle_v1("DecisionRejected", None, || panic!())
                .unwrap()
                .unwrap()
                .state,
            Cancelled
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("RequestDispatched", None, || panic!()),
            Err(CognitionLifecycleErrorV1::Order)
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("RequestDispatched", Some(ContextCaptured), || Err(
                CognitionLifecycleErrorV1::TransportInvalid
            )),
            Err(CognitionLifecycleErrorV1::TransportInvalid)
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("RequestDispatched", Some(ContextCaptured), || Ok((
                0, 2
            ))),
            Err(CognitionLifecycleErrorV1::TransportGap)
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("RequestDispatched", Some(Dispatched), || Ok((
                u64::MAX,
                u64::MAX
            )))
            .unwrap()
            .unwrap()
            .transport_attempt,
            Some(u64::MAX)
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("CognitionTurnFailed", Some(Started), || panic!()),
            Err(CognitionLifecycleErrorV1::Order)
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("ResponseRecorded", Some(ContextCaptured), || panic!()),
            Err(CognitionLifecycleErrorV1::Order)
        );
        assert_eq!(
            advance_cognition_lifecycle_v1("SchedulerWake", Some(Started), || panic!()).unwrap(),
            None
        );
    }
}
