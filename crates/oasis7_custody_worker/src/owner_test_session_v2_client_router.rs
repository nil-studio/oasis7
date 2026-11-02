//! Same existing first Admit and URI; actual namespace selection happens before here.
use super::*;
use crate::test_session_client_rpc::TestSessionClientAdmissionRpc;
use oasis7_custody_core::provisioning::{
    SessionChallengeKindV1, SessionChallengeRequestV1, SessionClientOperation, SessionControlKindV1,
};
pub(crate) async fn first_admit(
    state: &JsValue,
    env: &Env,
    admission: &TestSessionClientAdmissionRpc,
    operation: SessionClientOperation,
    role: Role,
) -> Result<String> {
    if role != Role::Identity {
        return Err(denied());
    }
    match operation {
        SessionClientOperation::CredentialClaim => {
            credentials_v2::first_admit(state, env, admission, operation, role).await
        }
        SessionClientOperation::Challenge => {
            let request: SessionChallengeRequestV1 =
                serde_json::from_str(&admission.http.original.body).map_err(|_| denied())?;
            match request.kind {
                SessionChallengeKindV1::CredentialClaim => {
                    credentials_v2::first_admit(state, env, admission, operation, role).await
                }
                SessionChallengeKindV1::Control(
                    SessionControlKindV1::Initialize
                    | SessionControlKindV1::ToolsList
                    | SessionControlKindV1::Heartbeat
                    | SessionControlKindV1::Finish
                    | SessionControlKindV1::Abort,
                ) => controls_v2::first_admit(state, env, admission, operation, role).await,
                _ => Err(denied()),
            }
        }
        SessionClientOperation::Control(
            SessionControlKindV1::Initialize
            | SessionControlKindV1::ToolsList
            | SessionControlKindV1::Heartbeat
            | SessionControlKindV1::Finish
            | SessionControlKindV1::Abort,
        ) => controls_v2::first_admit(state, env, admission, operation, role).await,
        // Tool metadata/actions/Termination are not enabled by version routing.
        _ => Err(denied()),
    }
}
