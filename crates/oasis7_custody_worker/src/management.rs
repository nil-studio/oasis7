//! Same-origin management routes. Native Passkey/Origin/CSRF checks precede Owner RPC.
use crate::{directory_rpc::*, native_identity::NativeIdentityConfig};
use oasis7_custody_core::{
    http::{ErrorCode, ErrorResponse},
    provisioning::*,
};
use worker::*;
fn unavailable() -> Result<Response> {
    Ok(Response::from_json(&ErrorResponse::new(ErrorCode::NotReady))?.with_status(503))
}
pub(crate) fn is_management_path(path: &str) -> bool {
    path == "/v1/management/session-status"
        || path == "/v1/account-provisions"
        || path.starts_with("/v1/account-provisions/")
        || path == "/v1/manager-revocations"
}
pub(crate) async fn handle(request: Request, env: Env) -> Result<Response> {
    let url = request.url()?;
    if url.query().is_some() {
        return Response::error("not found", 404);
    }
    let path = url.path();
    let status_route = path == "/v1/management/session-status";
    let draft = path == "/v1/account-provisions";
    let freeze = path == "/v1/manager-revocations";
    let provision_path = path.strip_prefix("/v1/account-provisions/");
    let approve = provision_path.and_then(|id| id.strip_suffix("/approve"));
    let advance = provision_path.and_then(|id| id.strip_suffix("/continue"));
    let progress = provision_path.and_then(|id| id.strip_suffix("/progress"));
    let get =
        provision_path.filter(|_| approve.is_none() && advance.is_none() && progress.is_none());
    if let Some(id) = approve.or(advance).or(progress) {
        if id.is_empty() || id.len() > 128 || id.contains('/') {
            return Response::error("not found", 404);
        }
    }
    if (status_route || get.is_some() || progress.is_some()) && request.method() != Method::Get
        || (draft || freeze || approve.is_some() || advance.is_some())
            && request.method() != Method::Post
    {
        return Response::error("method not allowed", 405);
    }
    if approve.is_some()
        && env
            .var("CUSTODY_PROVISION_APPROVAL_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return unavailable();
    }
    if advance.is_some()
        && env
            .var("CUSTODY_PROVISION_KEYS_ENABLED")
            .ok()
            .map(|v| v.to_string())
            .as_deref()
            != Some("true")
    {
        return unavailable();
    }
    let config = match NativeIdentityConfig::from_env(&env) {
        Ok(Some(v)) => v,
        _ => return unavailable(),
    };
    let manager = match config.management_session(&request).await {
        Ok(v) => v,
        Err(_) => return unavailable(),
    };
    if status_route {
        let mut response = Response::from_json(&manager)?;
        response.headers_mut().set("Cache-Control", "no-store")?;
        return Ok(response);
    }
    // The request never supplies a manager selector. Metadata remains inside this private hop.
    let result = if let Some(id) = advance {
        let body = match request_text(&request).await {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let value: ProvisionDriveRequestV1 = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        if value.validate().is_err() {
            return Response::error("invalid request", 400);
        }
        advance_owner(
            &env,
            &serde_json::to_string(&DriveRpc {
                manager_session: manager,
                provision_id: id.into(),
                request: value,
            })?,
        )
        .await
    } else if let Some(id) = progress {
        read_progress(
            &env,
            &serde_json::to_string(&ReadRpc {
                manager_session: manager,
                provision_id: id.into(),
            })?,
        )
        .await
    } else if let Some(id) = approve {
        let body = match request_text(&request).await {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let value: ProvisionApproveRequestV1 = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        if value.validate().is_err() {
            return Response::error("invalid request", 400);
        }
        let rpc = ApproveRpc {
            manager_session: manager,
            provision_id: id.into(),
            request: value,
        };
        approve_owner(&env, &serde_json::to_string(&rpc)?).await
    } else if draft {
        let body = match request_text(&request).await {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let value: ProvisionDraftRequestV1 = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let rpc = ReserveRpc {
            manager_session: manager,
            request: value,
        };
        reserve_owner(&env, &serde_json::to_string(&rpc)?).await
    } else if freeze {
        let body = match request_text(&request).await {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let value: ManagerFreezeRequestV1 = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(_) => return Response::error("invalid request", 400),
        };
        let rpc = FreezeRpc {
            manager_session: manager,
            request: value,
        };
        freeze_owner(&env, &serde_json::to_string(&rpc)?).await
    } else if let Some(id) = get {
        if id.is_empty() || id.len() > 128 || id.contains('/') {
            return Response::error("not found", 404);
        }
        let rpc = ReadRpc {
            manager_session: manager,
            provision_id: id.into(),
        };
        read_owner(&env, &serde_json::to_string(&rpc)?).await
    } else {
        return Response::error("not found", 404);
    };
    let raw = match result {
        Ok(v) => v.as_string(),
        Err(_) => None,
    };
    let Some(raw) = raw.filter(|v| v.len() <= 32768) else {
        return unavailable();
    };
    // Validate the actual response DTO in Rust before passing its original JSON bytes.
    if advance.is_some() || progress.is_some() {
        let value: ProvisionProgressViewV1 = serde_json::from_str(&raw)?;
        if value.api_version != 1 || Some(value.provision_id.as_str()) != advance.or(progress) {
            return unavailable();
        }
    } else if approve.is_some() {
        let value: ProvisionApprovalViewV1 = serde_json::from_str(&raw)?;
        if value.api_version != 1 || Some(value.provision_id.as_str()) != approve {
            return unavailable();
        }
    } else if freeze {
        let _: ManagerClosureViewV1 = serde_json::from_str(&raw)?;
    } else {
        let _: ProvisionDraftViewV1 = serde_json::from_str(&raw)?;
    }
    let mut response = Response::ok(raw)?;
    response
        .headers_mut()
        .set("Content-Type", "application/json")?;
    response.headers_mut().set("Cache-Control", "no-store")?;
    Ok(response)
}
