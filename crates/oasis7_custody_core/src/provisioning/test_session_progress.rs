use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSessionPhaseV1 {
    PendingApproval,
    ConsumedPendingPreflight,
    Active,
    Closing,
    Blocked,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestSessionTerminationStatusV1 {
    NotAllocated,
    OriginalSaved,
    Unknown,
    Committed,
    Rejected,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionPurposeProgressV1 {
    pub purpose: SessionPurposeV1,
    pub deadline: UnixMillis,
    pub requests_remaining: DecimalU64,
    pub transport_remaining: DecimalU64,
    pub business_remaining: DecimalU64,
    pub revoked: bool,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSessionProgressV1 {
    pub api_version: u8,
    pub session_id: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub stage: TestSessionPhaseV1,
    pub write_closed: bool,
    pub closing_reason: Option<TestSessionClosingReasonV1>,
    pub write_deadline: UnixMillis,
    pub read_until: UnixMillis,
    pub cleanup_until: UnixMillis,
    pub lease_until: Option<UnixMillis>,
    pub purposes: Vec<TestSessionPurposeProgressV1>,
    pub control_requests_remaining: DecimalU64,
    pub control_host_http_remaining: DecimalU64,
    pub control_connector_http_remaining: Option<DecimalU64>,
    pub identity_control_http_remaining: DecimalU64,
    pub stop_status: TestSessionTerminationStatusV1,
    pub revoke_status: TestSessionTerminationStatusV1,
    pub pending_operations: Vec<String>,
}
impl AccountProvisionApprovalState {
    /// Historical, non-secret projection only. No source fact, signed proof, SID selector or capability leaks.
    pub fn test_session_progress(
        &self,
        id: &str,
        now: UnixMillis,
    ) -> Result<TestSessionProgressV1> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let (_, _, write, read, cleanup) = admission::pairing(&s.input.request.intent)?;
        let status = |kind| {
            let Some(o) = s.termination_slots.get(&kind) else {
                return TestSessionTerminationStatusV1::NotAllocated;
            };
            match o.result.as_deref() {
                Some(OwnerSetupResultV1::Canonical { response, .. }) => match response
                    .payload
                    .outcome
                {
                    ws::IntentOutcome::Committed { .. } => {
                        TestSessionTerminationStatusV1::Committed
                    }
                    ws::IntentOutcome::Rejected { .. } => TestSessionTerminationStatusV1::Rejected,
                    _ => TestSessionTerminationStatusV1::Unknown,
                },
                _ => {
                    if o.claimed_at.is_some() {
                        TestSessionTerminationStatusV1::Unknown
                    } else {
                        TestSessionTerminationStatusV1::OriginalSaved
                    }
                }
            }
        };
        let mut pending = s
            .actions
            .values()
            .filter(|a| {
                a.purpose == SessionPurposeV1::Execution && !a.transport_attempts.is_empty()
            })
            .map(|a| a.request_id.clone())
            .collect::<Vec<_>>();
        for o in s
            .termination_slots
            .values()
            .filter(|o| o.claimed_at.is_some() && o.result.is_none())
        {
            pending.push(o.operation_id.clone())
        }
        if s.approval_verification_claimed_at.is_some() && s.consumed_at.is_none() {
            pending.push("native-approval-verification".into())
        }
        if s.preflight_claimed_at.is_some() && s.approved_at.is_none() {
            pending.push("test-session-preflight".into())
        }
        let stage = if self.frozen() {
            TestSessionPhaseV1::Blocked
        } else if s.closing.is_some() {
            TestSessionPhaseV1::Closing
        } else if s.approved_at.is_some() {
            TestSessionPhaseV1::Active
        } else if s.consumed_at.is_some() {
            TestSessionPhaseV1::ConsumedPendingPreflight
        } else {
            TestSessionPhaseV1::PendingApproval
        };
        let control = s.control.as_ref().ok_or_else(invalid)?;
        let purposes = s
            .roots
            .values()
            .map(|r| {
                Ok(TestSessionPurposeProgressV1 {
                    purpose: r.purpose,
                    deadline: r.deadline,
                    requests_remaining: DecimalU64(
                        r.budgets.requests.remaining().map_err(|_| invalid())?,
                    ),
                    transport_remaining: DecimalU64(
                        r.budgets.transport.remaining().map_err(|_| invalid())?,
                    ),
                    business_remaining: DecimalU64(
                        r.budgets.business.remaining().map_err(|_| invalid())?,
                    ),
                    revoked: r.revoked,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TestSessionProgressV1 {
            api_version: 1,
            session_id: id.into(),
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            stage,
            write_closed: self.frozen()
                || s.approved_at.is_none()
                || s.closing.is_some()
                || now.get() >= write
                || session_control_exhausted(s)
                || s.roots
                    .get(&SessionPurposeV1::Execution)
                    .is_none_or(|r| r.revoked),
            closing_reason: s.closing.as_ref().map(|(reason, _)| *reason),
            write_deadline: UnixMillis::new(write),
            read_until: UnixMillis::new(read),
            cleanup_until: UnixMillis::new(cleanup),
            lease_until: s.lease_until,
            purposes,
            control_requests_remaining: DecimalU64(
                control.requests.remaining().map_err(|_| invalid())?,
            ),
            control_host_http_remaining: DecimalU64(
                control.transport.remaining().map_err(|_| invalid())?,
            ),
            control_connector_http_remaining: s
                .connector_transport
                .as_ref()
                .map(|b| b.remaining().map(DecimalU64))
                .transpose()
                .map_err(|_| invalid())?,
            identity_control_http_remaining: DecimalU64(
                s.identity_transport
                    .as_ref()
                    .ok_or_else(invalid)?
                    .remaining()
                    .map_err(|_| invalid())?,
            ),
            stop_status: status(TerminationOperationV1::Stop),
            revoke_status: status(TerminationOperationV1::Revoke),
            pending_operations: pending,
        })
    }
}
