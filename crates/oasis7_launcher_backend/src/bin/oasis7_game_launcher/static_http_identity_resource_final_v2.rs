//! Both paid World sends are outside the global Native identity mutex.
use super::*;
use crate::hosted_identity::{IdentityReply, store::StoreError};
fn unavailable() -> IdentityReply {
    IdentityReply::error(StoreError::Rejected("resource_final_unavailable".into()))
}
pub(super) fn present(headers: &str) -> bool {
    headers
        .lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(name, _)| {
            name.trim()
                .eq_ignore_ascii_case(oasis7_custody_core::provisioning::RESOURCE_READ_PROOF_HEADER)
        })
}
pub(super) fn handle(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    headers: &str,
    body: &[u8],
) -> IdentityReply {
    let work = {
        let Ok(mut lock) = runtime.identity.lock() else {
            return unavailable();
        };
        let Some(service) = lock.as_mut() else {
            return unavailable();
        };
        match service.prepare_owner_resource_final_v2(method, headers, body) {
            Ok(work) => work,
            Err(e) => return IdentityReply::error(e),
        }
    };
    let first = match work.send() {
        Ok(work) => work,
        Err(e) => return IdentityReply::error(e),
    };
    let work = {
        let Ok(mut lock) = runtime.identity.lock() else {
            return unavailable();
        };
        let Some(service) = lock.as_mut() else {
            return unavailable();
        };
        match service.finish_owner_resource_preflight_v2(first) {
            Ok(work) => work,
            Err(e) => return IdentityReply::error(e),
        }
    };
    let completed = match work.send() {
        Ok(work) => work,
        Err(e) => return IdentityReply::error(e),
    };
    let bytes = {
        let Ok(mut lock) = runtime.identity.lock() else {
            return unavailable();
        };
        let Some(service) = lock.as_mut() else {
            return unavailable();
        };
        match service.finish_owner_resource_final_v2(completed) {
            Ok(bytes) => bytes,
            Err(e) => return IdentityReply::error(e),
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => IdentityReply::ok(value),
        Err(_) => unavailable(),
    }
}
