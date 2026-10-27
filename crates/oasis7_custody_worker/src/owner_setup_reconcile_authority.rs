//! Fresh authority is derived from the independent reconcile approval, never the old stage.
use super::*;
use oasis7_custody_core::{
    approval::SignedApprovalVerificationV1, approval_admission::ApprovalAdmissionInput,
};
pub(super) fn source(
    saved: &ProvisionApprovalSnapshot,
    id: &str,
    attempt: Option<&str>,
) -> Result<(ApprovalAdmissionInput, SignedApprovalVerificationV1)> {
    let r = saved
        .state
        .owner_setup_reconcile_original(id)
        .ok_or_else(denied)?;
    let a = saved
        .state
        .owner_setup_reconciles()
        .get(id)
        .ok_or_else(denied)?;
    let inspection = if let Some(attempt) = attempt {
        &a.attempts.get(attempt).ok_or_else(denied)?.source
    } else {
        a.source.as_ref().ok_or_else(denied)?
    };
    let signed = inspection
        .verification
        .clone()
        .or_else(|| a.source.as_ref().and_then(|s| s.verification.clone()))
        .or_else(|| r.verification().cloned())
        .ok_or_else(denied)?;
    Ok((r.input().clone(), signed))
}
pub(super) async fn observe(
    state: &JsValue,
    env: &Env,
    id: &str,
    attempt: Option<&str>,
    inspection_only: bool,
) -> Result<authority::Observations> {
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let (input, signed) = source(&saved, id, attempt)?;
    setup_authority::observe(env, &saved, input, &signed, inspection_only).await
}
pub(super) async fn prepare_source(
    state: &JsValue,
    env: &Env,
    id: &str,
    attempt: Option<&str>,
) -> Result<()> {
    let store = OwnerProvisionApprovalStore::open(state.clone()).map_err(|_| denied())?;
    let saved = store.snapshot().map_err(|_| denied())?;
    let a = saved
        .state
        .owner_setup_reconciles()
        .get(id)
        .ok_or_else(denied)?;
    let inspection = if let Some(attempt) = attempt {
        &a.attempts.get(attempt).ok_or_else(denied)?.source
    } else {
        a.source.as_ref().ok_or_else(denied)?
    };
    if inspection.verification.is_some() {
        return Ok(());
    }
    if inspection.claimed_at.is_some() {
        return Err(denied());
    }
    let before = observe(state, env, id, attempt, true).await?;
    let request = store
        .claim_owner_setup_reconcile_source(id, attempt, || {
            setup_authority::current(state, env, &before, false)
        })
        .map_err(|_| denied())?;
    let raw = serde_json::to_string(&request)?;
    let signed = serde_json::from_str(&super::super::super::super::text(
        super::super::super::super::verify_native(env, &raw)
            .await
            .map_err(|_| denied())?,
    )?)
    .map_err(|_| denied())?;
    let (input, _) = source(&saved, id, attempt)?;
    let after = setup_authority::observe(env, &saved, input, &signed, false).await?;
    store
        .complete_owner_setup_reconcile_source(id, attempt, signed, || {
            setup_authority::current(state, env, &after, true)
        })
        .await
        .map_err(|_| denied())
}
