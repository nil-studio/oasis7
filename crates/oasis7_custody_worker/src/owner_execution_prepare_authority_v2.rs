//! Fixed wrapper selects actual V2 original and Source, never caller authority.
use super::*;
use oasis7_custody_core::provisioning::ExecutionPrepareCurrentPortsV2;
pub(crate) async fn execution_prepare_observe_v2(
    env: &Env,
    saved: &ProvisionApprovalSnapshot,
    request: &TestSessionSelectorV1,
    source: &str,
    funding: ExecutionPrepareCurrentPortsV2,
) -> Result<authority::ExecutionPrepareObservationV2> {
    enabled_v2(env)?;
    let input = input_v2(saved, request)?;
    if funding.session_id() != request.session_id
        || funding.read().source_operation_id != source
        || funding.approved_input_digest()
            != saved
                .state
                .test_session_v2_credential_input_digest(&request.session_id)
                .map_err(|_| denied())?
    {
        return Err(denied());
    }
    let signed = saved
        .state
        .test_session_v2_source(&request.session_id, source)
        .and_then(|s| s.source.verification.as_ref())
        .ok_or_else(denied)?;
    authority::observe_execution_prepare_v2(env, saved.state.draft(), input, signed, funding).await
}
