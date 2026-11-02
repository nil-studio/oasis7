//! Closed management response checks against the actual validated request.
//! This adapter neither approves MFA nor grants execution or Current.
use oasis7_custody_core::{
    approval::CustodyIntentV1,
    provisioning::{
        OwnerSetupApprovalProgressV2, OwnerSetupApproveRequestV1, OwnerSetupApproveRequestV2,
        OwnerSetupDriveRequestV1, OwnerSetupPrepareRequestV1, OwnerSetupPrepareRequestV2,
        OwnerSetupPrepareResponseV1, OwnerSetupProgressV1,
    },
};
pub(super) enum ExpectedOwnerSetupResponse {
    PrepareV1(Box<OwnerSetupPrepareRequestV1>),
    PrepareV2(Box<OwnerSetupPrepareRequestV2>),
    ApproveV1(Box<OwnerSetupApproveRequestV1>),
    ApproveV2(Box<OwnerSetupApproveRequestV2>),
    Continue(Box<OwnerSetupDriveRequestV1>),
}
fn common_prepared(
    value: &OwnerSetupPrepareResponseV1,
    request_id: &str,
    intent: CustodyIntentV1,
) -> bool {
    value.approval_request_id == request_id
        && value.intent == intent
        && !value.context.custody_account_id.is_empty()
        && value.context.security_version.0 != 0
}
fn progress(
    value: &OwnerSetupProgressV1,
    version: u8,
    account: &str,
    provision: &str,
    setup: &str,
) -> bool {
    value.api_version == version
        && value.custody_account_id == account
        && value.provision_id == provision
        && value.setup_id == setup
}
impl ExpectedOwnerSetupResponse {
    pub(super) fn matches(&self, raw: &str) -> bool {
        match self {
            Self::PrepareV1(q) => {
                if q.validate().is_err() {
                    return false;
                }
                let (Ok(v), Ok(intent)) = (
                    serde_json::from_str::<OwnerSetupPrepareResponseV1>(raw),
                    q.plan.intent(),
                ) else {
                    return false;
                };
                common_prepared(&v, &q.approval_request_id, intent)
            }
            Self::PrepareV2(q) => {
                if q.validate().is_err() {
                    return false;
                }
                let (Ok(v), Ok(intent)) = (
                    serde_json::from_str::<OwnerSetupPrepareResponseV1>(raw),
                    q.plan.intent(),
                ) else {
                    return false;
                };
                common_prepared(&v, &q.approval_request_id, intent)
            }
            Self::ApproveV1(q) => {
                if q.validate().is_err() {
                    return false;
                }
                let Ok(v) = serde_json::from_str::<OwnerSetupProgressV1>(raw) else {
                    return false;
                };
                progress(
                    &v,
                    1,
                    &q.input.request.context.custody_account_id,
                    &q.plan.provision_id,
                    &q.plan.setup_id,
                )
            }
            Self::ApproveV2(q) => {
                if q.validate().is_err() {
                    return false;
                }
                let Ok(v) = serde_json::from_str::<OwnerSetupApprovalProgressV2>(raw) else {
                    return false;
                };
                let Ok(budget) = q
                    .plan
                    .model_budget
                    .as_ref()
                    .map(|b| b.digest(q.plan.deadline.0))
                    .transpose()
                else {
                    return false;
                };
                v.api_version == 2
                    && v.custody_account_id == q.input.request.context.custody_account_id
                    && v.provision_id == q.plan.provision_id
                    && v.setup_id == q.plan.setup_id
                    && v.model_budget_digest == budget
            }
            Self::Continue(q) => {
                if q.validate_sdk_selector().is_err() {
                    return false;
                }
                // The shared progress structure carries the actual stored engine
                // version. It is distinct from the V2 approval DTO above.
                let Ok(v) = serde_json::from_str::<OwnerSetupProgressV1>(raw) else {
                    return false;
                };
                progress(
                    &v,
                    q.api_version,
                    &q.custody_account_id,
                    &q.provision_id,
                    &q.setup_id,
                )
            }
        }
    }
}
