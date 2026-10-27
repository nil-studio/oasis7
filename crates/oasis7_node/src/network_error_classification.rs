use oasis7_proto::distributed::DistributedErrorCode;
use oasis7_proto::world_error::WorldError;

use crate::NodeError;

pub(crate) fn replication_network_error_kind_label(code: DistributedErrorCode) -> &'static str {
    match code {
        DistributedErrorCode::ErrNotFound => "not_found",
        DistributedErrorCode::ErrUnsupported => "unsupported",
        DistributedErrorCode::ErrTimeout => "timeout",
        DistributedErrorCode::ErrNotAvailable => "not_available",
        DistributedErrorCode::ErrBusy => "busy",
        DistributedErrorCode::ErrRateLimited => "rate_limited",
        DistributedErrorCode::ErrOverloaded => "overloaded",
        DistributedErrorCode::ErrBadRequest => "bad_request",
        DistributedErrorCode::ErrUnauthorized => "unauthorized",
        DistributedErrorCode::ErrStateMismatch => "state_mismatch",
        DistributedErrorCode::ErrInvalidHash => "invalid_hash",
    }
}

use oasis7_proto::network_failure::{FailureCause, NetworkFailure};

pub(crate) fn replication_network_failure(err: &NodeError) -> NetworkFailure {
    if let NodeError::ReplicationNetwork { failure, .. } = err {
        return (**failure).clone().conservative();
    }
    crate::network_error_legacy::interpret_node_diagnostic(err).conservative()
}

pub(crate) fn replication_network_error_is_availability_gap(err: &NodeError) -> bool {
    replication_network_failure(err).is_connection_gap()
}
pub(crate) fn replication_network_error_is_route_unavailable(err: &NodeError) -> bool {
    matches!(
        replication_network_failure(err).cause,
        FailureCause::MissingHandler
            | FailureCause::ProviderExhausted
            | FailureCause::ProtocolUnavailable
    )
}
pub(crate) fn replication_network_error_mentions_protocol(err: &NodeError, protocol: &str) -> bool {
    replication_network_failure(err).matches_protocol(protocol)
}
pub(crate) fn replication_network_error_is_not_found(err: &NodeError) -> bool {
    replication_network_failure(err).cause == FailureCause::NotFound
}
pub(crate) fn replication_network_error_is_unsupported_protocol(
    err: &NodeError,
    protocol: &str,
) -> bool {
    let facts = replication_network_failure(err);
    matches!(
        facts.cause,
        FailureCause::MissingHandler | FailureCause::ProtocolUnavailable
    ) && facts.matches_protocol(protocol)
}
pub(crate) fn replication_network_error_is_protocol_unavailable(
    err: &NodeError,
    protocol: &str,
) -> bool {
    replication_network_error_is_unsupported_protocol(err, protocol)
}
pub(crate) fn replication_network_error_is_timeout_protocol(
    err: &NodeError,
    protocol: &str,
) -> bool {
    let facts = replication_network_failure(err);
    facts.cause == FailureCause::Timeout && facts.matches_protocol(protocol)
}
pub(crate) fn replication_network_error_is_rate_limited_protocol(
    err: &NodeError,
    protocol: &str,
) -> bool {
    let facts = replication_network_failure(err);
    facts.cause == FailureCause::RateLimited && facts.matches_protocol(protocol)
}
pub(crate) fn replication_network_error_should_keep_timeout_over_provider_gap(
    current: Option<&NodeError>,
    candidate: &NodeError,
    protocol: &str,
) -> bool {
    current.is_some_and(|current| replication_network_error_is_timeout_protocol(current, protocol))
        && (replication_network_error_is_protocol_unavailable(candidate, protocol)
            || replication_network_error_is_availability_gap(candidate))
}

#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub(crate) fn network_world_error_is_retryable_connection_gap(err: &WorldError) -> bool {
    oasis7_net::world_error_is_retryable_connection_gap(err)
}

#[cfg(not(all(feature = "libp2p", not(target_arch = "wasm32"))))]
pub(crate) fn network_world_error_is_retryable_connection_gap(_err: &WorldError) -> bool {
    false
}

#[cfg(all(feature = "libp2p", not(target_arch = "wasm32")))]
pub(crate) fn network_world_error_is_publish_failure(err: &WorldError) -> bool {
    oasis7_net::world_error_is_publish_failure(err)
}

#[cfg(not(all(feature = "libp2p", not(target_arch = "wasm32"))))]
pub(crate) fn network_world_error_is_publish_failure(_err: &WorldError) -> bool {
    false
}

/// Persist facts inside the existing diagnostic string field, without changing snapshots.
pub(crate) fn persisted_network_reason(context: String, err: &NodeError) -> String {
    let facts = replication_network_failure(err);
    format!(
        "{context} [network-failure-v1={}]",
        serde_json::to_string(&facts).expect("network facts serialize")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_protocol_matches_libp2p_command_timeout_shape() {
        let protocol = "/aw/node/replication/fetch-commit/1.0.0";
        let err = NodeError::Replication {
            reason: format!(
                "libp2p command request_to_peer protocol={protocol} timed out after 1500ms"
            ),
        };

        assert!(replication_network_error_is_timeout_protocol(
            &err, protocol
        ));
    }

    #[test]
    fn rate_limited_protocol_matches_structured_response_shape() {
        let protocol = "/aw/node/replication/fetch-blob/1.0.0";
        let err = NodeError::Replication {
            reason: format!(
                "replication network request failed: kind=rate_limited protocol={protocol} detail=fetch-blob response budget exhausted; retry after window reset"
            ),
        };

        assert!(replication_network_error_is_rate_limited_protocol(
            &err, protocol
        ));
    }
}
