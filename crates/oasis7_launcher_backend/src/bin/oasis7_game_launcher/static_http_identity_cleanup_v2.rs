//! One owned registration World request, outside the global Native mutex.
use super::super::hosted_identity::{IdentityReply, store::StoreError};
use super::*;
#[path = "static_http_identity_execution_prepare_v2.rs"]
mod execution_prepare_v2;
fn unavailable() -> IdentityReply {
    IdentityReply::error(StoreError::Rejected(
        "cleanup_registration_unavailable".into(),
    ))
}
pub(super) fn registration(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    headers: &str,
    body: &[u8],
) -> IdentityReply {
    let work = {
        let Ok(mut guard) = runtime.identity.lock() else {
            return unavailable();
        };
        let Some(service) = guard.as_mut() else {
            return unavailable();
        };
        match service.prepare_cleanup_registration_v2(method, headers, body) {
            Ok(work) => work,
            Err(error) => return IdentityReply::error(error),
        }
    };
    // The unique physical claim and independent restore journal have committed.
    // Failure never retries this work or returns it to a reusable queue.
    let completed = match work.send() {
        Ok(completed) => completed,
        Err(error) => return IdentityReply::error(error),
    };
    let Ok(mut guard) = runtime.identity.lock() else {
        return unavailable();
    };
    let Some(service) = guard.as_mut() else {
        return unavailable();
    };
    match service.finish_cleanup_registration_v2(completed) {
        Ok(reply) => match serde_json::to_value(reply) {
            Ok(reply) => IdentityReply::ok(reply),
            Err(_) => unavailable(),
        },
        Err(error) => IdentityReply::error(error),
    }
}

pub(super) fn activate(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    body: &[u8],
) -> IdentityReply {
    let Ok(mut guard) = runtime.identity.lock() else {
        return unavailable();
    };
    let Some(service) = guard.as_mut() else {
        return unavailable();
    };
    match service.activate_cleanup_registration_v2(method, body) {
        Ok(reply) => match serde_json::to_value(reply) {
            Ok(reply) => IdentityReply::ok(reply),
            Err(_) => unavailable(),
        },
        Err(error) => IdentityReply::error(error),
    }
}

pub(super) fn current(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    body: &[u8],
) -> IdentityReply {
    let Ok(mut guard) = runtime.identity.lock() else {
        return unavailable();
    };
    let Some(service) = guard.as_mut() else {
        return unavailable();
    };
    match service.observe_cleanup_current_v2(method, body) {
        Ok(reply) => match serde_json::to_value(reply) {
            Ok(reply) => IdentityReply::ok(reply),
            Err(_) => unavailable(),
        },
        Err(error) => IdentityReply::error(error),
    }
}

pub(super) fn term_current(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    body: &[u8],
) -> IdentityReply {
    let Ok(mut guard) = runtime.identity.lock() else {
        return unavailable();
    };
    let Some(service) = guard.as_mut() else {
        return unavailable();
    };
    match service.observe_cleanup_term_current_v2(method, body) {
        Ok(reply) => match serde_json::to_value(reply) {
            Ok(reply) => IdentityReply::ok(reply),
            Err(_) => unavailable(),
        },
        Err(error) => IdentityReply::error(error),
    }
}

fn preflight_world(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    path: &str,
    headers: &str,
    body: &[u8],
) -> IdentityReply {
    let work = {
        let Ok(mut guard) = runtime.identity.lock() else {
            return unavailable();
        };
        let Some(service) = guard.as_mut() else {
            return unavailable();
        };
        match service.prepare_preflight_world_v2(method, path, headers, body) {
            Ok(work) => work,
            Err(error) => return IdentityReply::error(error),
        }
    };
    let completed = match work.send() {
        Ok(work) => work,
        Err(error) => return IdentityReply::error(error),
    };
    let Ok(mut guard) = runtime.identity.lock() else {
        return unavailable();
    };
    let Some(service) = guard.as_mut() else {
        return unavailable();
    };
    match service.finish_preflight_world_v2(completed) {
        Ok(reply) => IdentityReply::ok(reply),
        Err(error) => IdentityReply::error(error),
    }
}

pub(super) fn handle(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    target: &str,
    headers: &str,
    body: &[u8],
) -> Option<IdentityReply> {
    use oasis7_custody_core::native_cleanup_v2::{
        NATIVE_ACTIVATE_V2_PATH, NATIVE_CURRENT_V2_PATH, NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
        NATIVE_PREFLIGHT_POLICY_V2_PATH, NATIVE_REGISTER_V2_PATH, NATIVE_TERM_CURRENT_V2_PATH,
    };
    Some(match target {
        oasis7_custody_core::native_execution_prepare_v2::NATIVE_EXECUTION_PREPARE_PATH_V2 => {
            execution_prepare_v2::run(runtime, method, target, headers, body)
        }
        NATIVE_PREFLIGHT_POLICY_V2_PATH | NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH => {
            preflight_world(runtime, method, target, headers, body)
        }
        NATIVE_REGISTER_V2_PATH => registration(runtime, method, headers, body),
        NATIVE_ACTIVATE_V2_PATH => activate(runtime, method, body),
        NATIVE_CURRENT_V2_PATH => current(runtime, method, body),
        NATIVE_TERM_CURRENT_V2_PATH => term_current(runtime, method, body),
        _ => return None,
    })
}
