//! No World or Cloud I/O: exact once and local Current run under Native mutex.
use super::*;
pub(super) fn handle(
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
    match service.observe_cleanup_term_direct_v3(method, body) {
        Ok(reply) => match serde_json::to_value(reply) {
            Ok(reply) => IdentityReply::ok(reply),
            Err(_) => unavailable(),
        },
        Err(error) => IdentityReply::error(error),
    }
}
