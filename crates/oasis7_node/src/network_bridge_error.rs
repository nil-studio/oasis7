use super::*;

pub(super) fn network_err(err: WorldError) -> NodeError {
    network_err_with_request_protocol(None, err)
}

pub(super) fn network_err_for_protocol(protocol: &str, err: WorldError) -> NodeError {
    network_err_with_request_protocol(Some(protocol), err)
}

fn network_err_with_request_protocol(protocol: Option<&str>, err: WorldError) -> NodeError {
    let failure = err.network_failure(protocol);
    let rendered = render_network_error(protocol, err.diagnostic_source().clone());
    NodeError::ReplicationNetwork {
        reason: rendered
            .replication_reason()
            .unwrap_or("replication network error")
            .to_owned(),
        failure: Box::new(failure),
    }
}

fn render_network_error(protocol: Option<&str>, err: WorldError) -> NodeError {
    if network_world_error_is_retryable_connection_gap(&err) {
        return NodeError::Replication {
            reason: format!(
                "{REPLICATION_NETWORK_AVAILABILITY_GAP_PREFIX}{}",
                replication_network_error_detail(&err)
            ),
        };
    }
    if network_world_error_is_publish_failure(&err) {
        return NodeError::Replication {
            reason: format!("replication network error: {err:?}"),
        };
    }
    if let WorldError::NetworkRequestFailed { code, message, .. } = &err {
        let protocol = protocol.unwrap_or(message);
        return NodeError::Replication {
            reason: format!(
                "replication network request failed: kind={} protocol={} detail={}",
                replication_network_error_kind_label(*code),
                protocol,
                message
            ),
        };
    }
    if let WorldError::NetworkProtocolUnavailable { .. } = &err {
        return NodeError::Replication {
            reason: format!(
                "{REPLICATION_NETWORK_ROUTE_UNAVAILABLE_PREFIX}{}",
                replication_network_error_detail(&err)
            ),
        };
    }
    NodeError::Replication {
        reason: format!("replication network error: {err:?}"),
    }
}

pub(super) fn world_head_lookup_can_fallback(err: &NodeError) -> bool {
    replication_network_error_is_availability_gap(err)
        || replication_network_error_is_route_unavailable(err)
        || replication_network_error_is_not_found(err)
        || replication_network_error_is_unsupported_protocol(err, REPLICATION_GET_HEAD_PROTOCOL)
        || replication_network_error_is_protocol_unavailable(err, REPLICATION_GET_HEAD_PROTOCOL)
        || replication_network_error_is_timeout_protocol(err, REPLICATION_GET_HEAD_PROTOCOL)
        || replication_network_error_is_libp2p_outbound_timeout(err)
        || {
            let facts = crate::network_error_classification::replication_network_failure(err);
            facts.origin == oasis7_proto::network_failure::FailureOrigin::Dht
                && facts.cause == oasis7_proto::network_failure::FailureCause::Timeout
        }
}

fn replication_network_error_is_libp2p_outbound_timeout(err: &NodeError) -> bool {
    replication_network_error_is_timeout_protocol(err, REPLICATION_GET_HEAD_PROTOCOL)
}

pub(super) fn validate_world_head_world_id(
    world_id: &str,
    head: &WorldHeadAnnounce,
) -> Result<(), NodeError> {
    if head.world_id != world_id {
        return Err(NodeError::Replication {
            reason: format!(
                "world head mismatch: expected={} actual={}",
                world_id, head.world_id
            ),
        });
    }
    Ok(())
}

fn replication_network_error_detail(err: &WorldError) -> &str {
    match err {
        WorldError::NetworkProtocolUnavailable { protocol } => protocol.as_str(),
        WorldError::NetworkRequestFailed { message, .. } => message.as_str(),
        WorldError::DistributedValidationFailed { reason } => reason.as_str(),
        WorldError::BlobNotFound { content_hash } => content_hash.as_str(),
        WorldError::BlobHashMismatch { actual, .. } => actual.as_str(),
        WorldError::BlobHashInvalid { content_hash } => content_hash.as_str(),
        WorldError::Io(message) | WorldError::Serde(message) => message.as_str(),
        WorldError::SignatureKeyInvalid => "invalid signature key",
        WorldError::NetworkFailure { source, .. } => replication_network_error_detail(source),
    }
}

pub(super) fn network_publish_err(err: WorldError) -> NodeError {
    let mut facts = err.network_failure(None);
    facts.origin = oasis7_proto::network_failure::FailureOrigin::Publish;
    network_err(err.with_network_failure(facts.conservative()))
}
