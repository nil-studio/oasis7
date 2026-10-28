//! Capture the actual public request. No proof or authority is produced here.
use crate::{
    owner_test_session_rpc::TestSessionSelectorV1,
    test_session_client_route::{ClientRoute, parse},
    test_session_client_rpc::{TestSessionClientAdmissionRpc, TestSessionClientHttpRpc},
};
use oasis7_custody_core::{
    client_enrollment::ClientCredentialClaimRequestV1,
    http_pop::{
        CanonicalHttpTarget, HttpHeader, HttpHeaderRepresentation, HttpPopRoute,
        http_pop_nonce_hint,
    },
    provisioning::{
        SessionChallengeRequestV1, SessionClientHeaderV1, SessionClientHttpRequestV1,
        SessionClientOperation, SessionControlKindV1, SessionControlRequestV1,
    },
};
use wasm_bindgen::prelude::*;
use worker::{Env, Method, Request};

#[wasm_bindgen(module = "/platform/sql.mjs")]
extern "C" {
    #[wasm_bindgen(catch, js_name = readBoundedText)]
    async fn bounded(body: &JsValue, limit: u32) -> Result<JsValue, JsValue>;
}
fn rejected() -> JsValue {
    JsValue::from_str("test session client request unavailable")
}
pub(crate) struct CapturedClientHttp {
    pub admission: TestSessionClientAdmissionRpc,
    pub operation: SessionClientOperation,
}

/// The caller must apply the separate provider-IP ingress limit before invoking
/// this bounded capture. These selectors never authorize an Account mutation.
pub(crate) async fn capture(env: &Env, request: &Request) -> Result<CapturedClientHttp, JsValue> {
    if env
        .var("CUSTODY_TEST_SESSION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
        || request.method() != Method::Post
    {
        return Err(rejected());
    }
    let url = request.url().map_err(|_| rejected())?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(rejected());
    }
    let selected = parse(url.path()).ok_or_else(rejected)?;
    let body = zeroize::Zeroizing::new(
        bounded(request.inner().as_ref(), 32768)
            .await?
            .as_string()
            .ok_or_else(rejected)?,
    );
    let (inner, operation, session_id) = match &selected.route {
        ClientRoute::Challenge => {
            let q: SessionChallengeRequestV1 =
                serde_json::from_str(&body).map_err(|_| rejected())?;
            (
                HttpPopRoute::ProofChallenges,
                SessionClientOperation::Challenge,
                q.session_id,
            )
        }
        ClientRoute::Claim { enrollment_id } => {
            let q: ClientCredentialClaimRequestV1 =
                serde_json::from_str(&body).map_err(|_| rejected())?;
            if q.enrollment_id != *enrollment_id {
                return Err(rejected());
            }
            (
                HttpPopRoute::EnrollmentClaim(enrollment_id.clone()),
                SessionClientOperation::CredentialClaim,
                q.test_session_id,
            )
        }
        route => {
            let (id, kind, inner) = match route {
                ClientRoute::Initialize { session_id } => (
                    session_id,
                    SessionControlKindV1::Initialize,
                    HttpPopRoute::Initialize(session_id.clone()),
                ),
                ClientRoute::ToolsList { session_id } => (
                    session_id,
                    SessionControlKindV1::ToolsList,
                    HttpPopRoute::ToolsList(session_id.clone()),
                ),
                ClientRoute::Heartbeat { session_id } => (
                    session_id,
                    SessionControlKindV1::Heartbeat,
                    HttpPopRoute::Heartbeat(session_id.clone()),
                ),
                ClientRoute::Finish { session_id } => (
                    session_id,
                    SessionControlKindV1::Finish,
                    HttpPopRoute::Finish(session_id.clone()),
                ),
                ClientRoute::Abort { session_id } => (
                    session_id,
                    SessionControlKindV1::Abort,
                    HttpPopRoute::Abort(session_id.clone()),
                ),
                _ => return Err(rejected()),
            };
            let q: SessionControlRequestV1 = serde_json::from_str(&body).map_err(|_| rejected())?;
            if q.session_id != *id || q.kind != kind {
                return Err(rejected());
            }
            (inner, SessionClientOperation::Control(kind), q.session_id)
        }
    };
    let origin = env
        .var("CUSTODY_PUBLIC_ORIGIN")
        .map_err(|_| rejected())?
        .to_string();
    let target = CanonicalHttpTarget::new(
        &origin,
        HttpPopRoute::ScopedClient {
            provision_id: selected.provision_id.clone(),
            custody_account_id: selected.account_id.clone(),
            route: Box::new(inner),
        },
    )
    .map_err(|_| rejected())?;
    if url.as_str() != target.uri() {
        return Err(rejected());
    }
    let headers: Vec<_> = request
        .headers()
        .entries()
        .map(|(name, value)| SessionClientHeaderV1 { name, value })
        .collect();
    let borrowed: Vec<_> = headers
        .iter()
        .map(|h| HttpHeader {
            name: &h.name,
            value: &h.value,
        })
        .collect();
    let nonce_hint =
        http_pop_nonce_hint(&borrowed, HttpHeaderRepresentation::WorkerMergedSingletons)
            .map_err(|_| rejected())?;
    Ok(CapturedClientHttp {
        operation,
        admission: TestSessionClientAdmissionRpc {
            ingress_attempt_id: crate::client_enrollment_authority::nonce()?,
            http: TestSessionClientHttpRpc {
                selector: TestSessionSelectorV1 {
                    api_version: 1,
                    custody_account_id: selected.account_id,
                    provision_id: selected.provision_id,
                    session_id: session_id.clone(),
                },
                original: SessionClientHttpRequestV1 {
                    session_id,
                    method: "POST".into(),
                    target_uri: url.into(),
                    headers,
                    body: body.to_string(),
                    nonce_hint,
                },
            },
        },
    })
}
