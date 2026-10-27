use super::PosNodeEngine;
use crate::NodeError;

pub(super) const REPLICATION_GAP_SYNC_PROVIDER_ROUTE_BLOCK_RETRY_COOLDOWN_MS: i64 = 30_000;
pub(super) const REPLICATION_GAP_SYNC_FETCH_BLOB_RATE_LIMIT_COOLDOWN_MS: i64 = 60_000;

pub(super) fn replication_gap_sync_provider_blob_route_blocked(err: &NodeError) -> bool {
    if let NodeError::ReplicationNetwork { failure, .. } = err {
        return failure.matches_protocol(crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL)
            && matches!(
                failure.cause,
                oasis7_proto::network_failure::FailureCause::MissingHandler
                    | oasis7_proto::network_failure::FailureCause::ProviderExhausted
                    | oasis7_proto::network_failure::FailureCause::ProtocolUnavailable
                    | oasis7_proto::network_failure::FailureCause::NotFound
            );
    }
    err.replication_reason()
        .is_some_and(replication_gap_sync_provider_blob_route_blocked_reason)
}

pub(super) fn replication_gap_sync_provider_blob_route_blocked_reason(reason: &str) -> bool {
    crate::network_error_legacy::provider_blob_route_blocked_reason(reason)
}

pub(super) fn replication_gap_sync_provider_blob_route_blocked_in_cooldown(
    blocked_height: Option<u64>,
    blocked_reason: Option<&str>,
    blocked_at_ms: Option<i64>,
    next_height: u64,
    now_ms: i64,
) -> bool {
    if blocked_height != Some(next_height)
        || !blocked_reason
            .map(replication_gap_sync_provider_blob_route_blocked_reason)
            .unwrap_or(false)
    {
        return false;
    }
    let Some(blocked_at_ms) = blocked_at_ms else {
        return false;
    };
    now_ms.saturating_sub(blocked_at_ms)
        < REPLICATION_GAP_SYNC_PROVIDER_ROUTE_BLOCK_RETRY_COOLDOWN_MS
}

pub(super) fn replication_gap_sync_fetch_blob_rate_limited(err: &NodeError) -> bool {
    crate::network_bridge::replication_network_error_is_rate_limited_protocol(
        err,
        super::replication::REPLICATION_FETCH_BLOB_PROTOCOL,
    )
}

pub(super) fn record_replication_gap_sync_fetch_blob_rate_limit(
    engine: &mut PosNodeEngine,
    next_height: u64,
    now_ms: i64,
    attempt: usize,
    err: &NodeError,
) -> bool {
    if !replication_gap_sync_fetch_blob_rate_limited(err) {
        return false;
    }
    engine.last_replication_gap_sync_blocked_height = Some(next_height);
    engine.last_replication_gap_sync_blocked_at_ms = Some(now_ms);
    engine.last_replication_gap_sync_blocked_reason = Some(
        crate::network_error_classification::persisted_network_reason(
            format!(
                "replication state sync rate limited at height {next_height}: attempt {attempt}/{} rate limited: {err}",
                super::REPLICATION_GAP_SYNC_MAX_RETRIES_PER_HEIGHT
            ),
            err,
        ),
    );
    true
}

pub(super) fn replication_gap_sync_fetch_blob_rate_limited_in_cooldown(
    blocked_height: Option<u64>,
    blocked_reason: Option<&str>,
    blocked_at_ms: Option<i64>,
    next_height: u64,
    now_ms: i64,
) -> bool {
    if blocked_height != Some(next_height)
        || !blocked_reason
            .map(|reason| {
                replication_gap_sync_fetch_blob_rate_limited(&NodeError::Replication {
                    reason: reason.to_string(),
                })
            })
            .unwrap_or(false)
    {
        return false;
    }
    let Some(blocked_at_ms) = blocked_at_ms else {
        return false;
    };
    now_ms.saturating_sub(blocked_at_ms) < REPLICATION_GAP_SYNC_FETCH_BLOB_RATE_LIMIT_COOLDOWN_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_route_blocked_cooldown_expires_for_retry() {
        let reason = "replication gap sync provider route blocked: gap sync height 1 blob not found for hash abc";
        let blocked_at_ms = 1_000;

        assert!(
            replication_gap_sync_provider_blob_route_blocked_in_cooldown(
                Some(1),
                Some(reason),
                Some(blocked_at_ms),
                1,
                blocked_at_ms + REPLICATION_GAP_SYNC_PROVIDER_ROUTE_BLOCK_RETRY_COOLDOWN_MS - 1
            )
        );
        assert!(
            !replication_gap_sync_provider_blob_route_blocked_in_cooldown(
                Some(1),
                Some(reason),
                Some(blocked_at_ms),
                1,
                blocked_at_ms + REPLICATION_GAP_SYNC_PROVIDER_ROUTE_BLOCK_RETRY_COOLDOWN_MS
            )
        );
        assert!(
            !replication_gap_sync_provider_blob_route_blocked_in_cooldown(
                Some(1),
                Some(reason),
                None,
                1,
                blocked_at_ms
            )
        );
    }

    #[test]
    fn fetch_blob_rate_limit_cooldown_resumes_after_response_window() {
        let reason = "replication network request failed: kind=rate_limited protocol=/aw/node/replication/fetch-blob/1.0.0 detail=retry after window reset";
        let blocked_at_ms = 1_000;

        assert!(replication_gap_sync_fetch_blob_rate_limited_in_cooldown(
            Some(1),
            Some(reason),
            Some(blocked_at_ms),
            1,
            blocked_at_ms + REPLICATION_GAP_SYNC_FETCH_BLOB_RATE_LIMIT_COOLDOWN_MS - 1,
        ));
        assert!(!replication_gap_sync_fetch_blob_rate_limited_in_cooldown(
            Some(1),
            Some(reason),
            Some(blocked_at_ms),
            1,
            blocked_at_ms + REPLICATION_GAP_SYNC_FETCH_BLOB_RATE_LIMIT_COOLDOWN_MS,
        ));
    }
}
