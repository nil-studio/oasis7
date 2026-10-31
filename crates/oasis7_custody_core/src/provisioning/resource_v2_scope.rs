//! Exact intent checks reuse the caller's existing authoritative Book. These
//! functions do not create a second approval book or consume/issue an authority.
use super::*;
use crate::approval_admission::{
    ApprovalAdmissionBook, ApprovalAdmissionContext, ApprovalAdmissionError,
    ApprovalAdmissionInput, ExactSignatureChecks,
};
pub fn resource_v2_owner_setup_checks(
    book: &ApprovalAdmissionBook,
    input: &ApprovalAdmissionInput,
    plan: &OwnerSetupPlanV2,
    nonce: &str,
    current: &ApprovalAdmissionContext,
    now: u64,
) -> Result<ExactSignatureChecks, ApprovalAdmissionError> {
    if input.operation_id != plan.setup_id
        || input.request.intent != plan.intent().map_err(|_| ApprovalAdmissionError::Invalid)?
    {
        return Err(ApprovalAdmissionError::Invalid);
    }
    book.begin_checks(input, nonce, current, now)
}
pub fn resource_v2_test_session_checks(
    book: &ApprovalAdmissionBook,
    input: &ApprovalAdmissionInput,
    plan: &TestSessionPlanV2,
    scope: &ResourceTestSessionScopeV2,
    nonce: &str,
    current: &ApprovalAdmissionContext,
    now: u64,
) -> Result<ExactSignatureChecks, ApprovalAdmissionError> {
    if input.request.intent
        != plan
            .intent(scope.clone(), now)
            .map_err(|_| ApprovalAdmissionError::Invalid)?
    {
        return Err(ApprovalAdmissionError::Invalid);
    }
    if input.operation_id != scope.session_id
        || scope.client_pairing.target.custody_account_id
            != input.request.context.custody_account_id
    {
        return Err(ApprovalAdmissionError::Invalid);
    }
    book.begin_checks(input, nonce, current, now)
}
