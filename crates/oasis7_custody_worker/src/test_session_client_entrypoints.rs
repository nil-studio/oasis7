//! Native client lane. No cookies, model-provided Current or arbitrary RPC methods.
use worker::{Env, Request, Response};

pub(crate) async fn handle(env: &Env, request: Request) -> worker::Result<Response> {
    if env
        .var("CUSTODY_TEST_SESSION_ENABLED")
        .ok()
        .map(|v| v.to_string())
        .as_deref()
        != Some("true")
    {
        return Response::error("not found", 404);
    }
    if crate::client_enrollment_entrypoints::limit_client_http(env, &request)
        .await
        .is_err()
    {
        return Response::error("client ingress unavailable", 429);
    }
    if crate::test_session_client_route::parse(request.url()?.path()).is_some_and(|target| {
        matches!(
            target.route,
            crate::test_session_client_route::ClientRoute::ModelReservation { .. }
        )
    }) {
        // Both real provider/realm Actor guards above stay in this new profile.
        return match crate::identity_native_model_runtime_v2::run(env, &request).await {
            Ok(raw) => {
                let mut response = Response::ok(raw)?;
                response
                    .headers_mut()
                    .set("content-type", "application/json")?;
                response.headers_mut().set("cache-control", "no-store")?;
                Ok(response)
            }
            Err(_) => Response::error("native model reservation pending", 409),
        };
    }
    let captured = match crate::test_session_client_ingress::capture(env, &request).await {
        Ok(captured) => captured,
        Err(_) => return Response::error("invalid client request", 400),
    };
    let result = if matches!(
        captured.operation,
        oasis7_custody_core::provisioning::SessionClientOperation::ToolNonce
    ) {
        crate::identity_native_runtime::client::run_tool_nonce(env, captured.admission).await
    } else {
        crate::identity_native_runtime::client::run(env, captured.admission, captured.operation)
            .await
    };
    match result {
        Ok(raw) => {
            let mut response = Response::ok(raw)?;
            response
                .headers_mut()
                .set("content-type", "application/json")?;
            response.headers_mut().set("cache-control", "no-store")?;
            Ok(response)
        }
        Err(_) => Response::error("client operation pending", 409),
    }
}
