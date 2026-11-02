//! An immutable commitment derived from the three actual born V2 Roots.
//! It excludes mutable spend, credentials and later revocation state.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RootBirthEntryV2 {
    purpose: SessionPurposeV1,
    approved_input_digest: String,
    deadline: UnixMillis,
    request_limit: DecimalU64,
    transport_limit: DecimalU64,
    business_limit: DecimalU64,
}
fn root_entry(s: &TestSessionAuthorityV2, purpose: SessionPurposeV1) -> Result<RootBirthEntryV2> {
    let root = s.roots.get(&purpose).ok_or_else(invalid)?;
    if root.purpose != purpose || root.approved_input_digest != s.input_digest()? {
        return Err(invalid());
    }
    Ok(RootBirthEntryV2 {
        purpose,
        approved_input_digest: root.approved_input_digest.clone(),
        deadline: root.deadline,
        request_limit: DecimalU64(root.budgets.requests.approved_limit()),
        transport_limit: DecimalU64(root.budgets.transport.approved_limit()),
        business_limit: DecimalU64(root.budgets.business.approved_limit()),
    })
}
pub(in crate::provisioning) fn actual_root_birth_digest_v2(
    s: &TestSessionAuthorityV2,
) -> Result<String> {
    if s.roots.len() != 3 {
        return Err(invalid());
    }
    digest(&(
        "oasis7.custody.test-session-root-birth.v2",
        s.input_digest()?,
        s.approved_at.ok_or_else(invalid)?,
        [
            root_entry(s, SessionPurposeV1::Execution)?,
            root_entry(s, SessionPurposeV1::ReconcileRead)?,
            root_entry(s, SessionPurposeV1::TerminationPlan)?,
        ],
    ))
}
