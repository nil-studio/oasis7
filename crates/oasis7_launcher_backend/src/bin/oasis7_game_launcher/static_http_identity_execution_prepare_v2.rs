//! Fixed funded executor Prepare: all World work is outside the Native mutex.
use super::*;
fn unavailable() -> IdentityReply {
    IdentityReply::error(StoreError::Rejected("execution_prepare_unavailable".into()))
}
pub(super) fn run(
    runtime: &StaticHttpRuntimeConfig,
    method: &str,
    target: &str,
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
        match service.prepare_execution_funded_v2(method, target, headers, body) {
            Ok(work) => work,
            Err(error) => return IdentityReply::error(error),
        }
    };
    // No service, signing key, SID or mutex guard crosses this owned send.
    let completed = match work.send(|edge, body, current, previous| {
        let callback = {
            let mut guard = runtime
                .identity
                .lock()
                .map_err(|_| StoreError::Rejected("execution_prepare_unavailable".into()))?;
            let service = guard
                .as_mut()
                .ok_or_else(|| StoreError::Rejected("execution_prepare_unavailable".into()))?;
            service.prepare_execution_world_reentry_v2(current, previous, edge, body)?
        };
        // Actual HTTPS callback is outside the lock; no Native key or SID is
        // in this work. It performs its real limiter/service/Actor five edges.
        let (callback, signed) = callback.send()?;
        {
            let mut guard = runtime
                .identity
                .lock()
                .map_err(|_| StoreError::Rejected("execution_prepare_unavailable".into()))?;
            let service = guard
                .as_mut()
                .ok_or_else(|| StoreError::Rejected("execution_prepare_unavailable".into()))?;
            service.finish_execution_world_reentry_v2(current, &callback, &signed)?;
        }
        Ok(signed)
    }) {
        Ok(v) => v,
        Err(e) => return IdentityReply::error(e),
    };
    let Ok(mut guard) = runtime.identity.lock() else {
        return unavailable();
    };
    let Some(service) = guard.as_mut() else {
        return unavailable();
    };
    match service.finish_execution_funded_v2(completed) {
        Ok(reply) => IdentityReply::ok(reply),
        Err(error) => IdentityReply::error(error),
    }
}
