//! V2 uses the same named manager ports; only explicit version two selects it.
use super::*;
use oasis7_custody_core::approval::CustodyIntentV1;
fn enabled(env: &Env) -> Result<(), JsValue> {
    if env
        .var("CUSTODY_TEST_SESSION_V2_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Err(denied());
    }
    Ok(())
}
macro_rules! management_v2 {
    ($name:ident,$rpc:ty,$response:ty,$path:literal,$valid:expr) => {
        pub(super) async fn $name(env: &Env, raw: &str) -> Result<String, JsValue> {
            enabled(env)?;
            if raw.len() > 32768 {
                return Err(denied());
            }
            let rpc: $rpc = serde_json::from_str(raw).map_err(|_| denied())?;
            rpc.request.validate().map_err(|_| denied())?;
            let lookup = serde_json::to_string(&ReadRpc {
                manager_session: rpc.manager_session.clone(),
                provision_id: rpc.provision_id.clone(),
            })
            .map_err(|_| denied())?;
            let raw_view = crate::owner_directory::call_directory(
                env,
                "/internal/read",
                &lookup,
                &rpc.manager_session,
            )
            .await
            .map_err(|_| denied())?;
            let view: ProvisionDraftViewV1 =
                serde_json::from_str(&raw_view).map_err(|_| denied())?;
            if view.entry.provision_id != rpc.provision_id {
                return Err(denied());
            }
            let raw_reply = fetch_account(env, &view.entry.custody_account_id, $path, raw).await?;
            let reply: $response = serde_json::from_str(&raw_reply).map_err(|_| denied())?;
            if !($valid)(&reply, &rpc, &view) {
                return Err(denied());
            }
            serde_json::to_string(&reply).map_err(|_| denied())
        }
    };
}
management_v2!(
    basis_v2,
    TestSessionBasisRpcV2,
    TestSessionBasisV2,
    "/internal/test-session-basis",
    |reply: &TestSessionBasisV2, rpc: &TestSessionBasisRpcV2, view: &ProvisionDraftViewV1| reply
        .api_version
        == 2
        && reply.provision_id == rpc.provision_id
        && reply.custody_account_id == view.entry.custody_account_id
        && reply.termination.validate_scope(&reply.scope).is_ok()
);
management_v2!(
    prepare_v2,
    TestSessionPrepareRpcV2,
    oasis7_custody_core::approval::ApprovalOptionsRequestV1,
    "/internal/test-session-prepare",
    |reply: &oasis7_custody_core::approval::ApprovalOptionsRequestV1,
     rpc: &TestSessionPrepareRpcV2,
     view: &ProvisionDraftViewV1| reply.approval_request_id == rpc.request.approval_request_id
        && reply.context.custody_account_id == view.entry.custody_account_id
        && matches!(&reply.intent,CustodyIntentV1::TestSessionV2{plan,scope} if plan.as_ref()==&rpc.request.plan && scope.as_ref()==&rpc.request.scope)
);
management_v2!(
    approve_v2,
    TestSessionApproveRpcV2,
    TestSessionApprovalProgressV2,
    "/internal/test-session-approve",
    |reply: &TestSessionApprovalProgressV2,
     rpc: &TestSessionApproveRpcV2,
     view: &ProvisionDraftViewV1| reply.api_version == 2
        && reply.provision_id == rpc.provision_id
        && reply.custody_account_id == view.entry.custody_account_id
        && reply.session_id == rpc.request.input.operation_id
);

management_v2!(
    continue_v2,
    TestSessionContinueRpcV2,
    TestSessionActivationProgressV2,
    "/internal/test-session-continue",
    |reply: &TestSessionActivationProgressV2,
     rpc: &TestSessionContinueRpcV2,
     view: &ProvisionDraftViewV1| reply.api_version == 2
        && reply.custody_account_id == view.entry.custody_account_id
        && reply.provision_id == rpc.provision_id
        && reply.session_id == rpc.request.session_id
);
