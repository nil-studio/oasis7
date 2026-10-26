//! Bounded adapters for old Node diagnostics and persisted blocked_reason snapshots.
//! Text interpreted here is never a local transport observation.
use crate::{
    NodeError,
    network_bridge::{
        REPLICATION_NETWORK_AVAILABILITY_GAP_PREFIX, REPLICATION_NETWORK_ROUTE_UNAVAILABLE_PREFIX,
    },
};
pub(crate) fn legacy_is_availability_gap(err: &NodeError) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    reason.starts_with(REPLICATION_NETWORK_AVAILABILITY_GAP_PREFIX)
}

pub(crate) fn legacy_mentions_protocol(err: &NodeError, protocol: &str) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    mentions_exact_protocol(reason, protocol)
}

pub(crate) fn legacy_is_not_found(err: &NodeError) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    network_request_reason_has_kind(reason, "not_found")
        || (reason.contains("NetworkRequestFailed") && reason.contains("ErrNotFound"))
}

pub(crate) fn legacy_is_unsupported_protocol(err: &NodeError, protocol: &str) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    (network_request_reason_has_kind(reason, "unsupported")
        && network_request_reason_detail_mentions_protocol(reason, protocol))
        || (reason.contains("NetworkRequestFailed")
            && reason.contains("ErrUnsupported")
            && reason
                .split_once("message: \"")
                .and_then(|(_, tail)| tail.split_once("\", retryable:"))
                .is_some_and(|(message, _)| legacy_missing_handler_detail(message, protocol)))
}

pub(crate) fn legacy_is_protocol_unavailable(err: &NodeError, protocol: &str) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    (reason.starts_with(REPLICATION_NETWORK_ROUTE_UNAVAILABLE_PREFIX)
        && mentions_exact_protocol(reason, protocol))
        || reason.contains("NetworkProtocolUnavailable")
            && (reason.contains("handler missing") || mentions_exact_protocol(reason, protocol))
}

pub(crate) fn legacy_is_timeout_protocol(err: &NodeError, protocol: &str) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    mentions_exact_protocol(reason, protocol)
        && (network_request_reason_has_kind(reason, "timeout")
            || reason.contains("request failed: Timeout")
            || reason.contains("timed out"))
}

pub(crate) fn legacy_is_rate_limited_protocol(err: &NodeError, protocol: &str) -> bool {
    let NodeError::Replication { reason } = err else {
        return false;
    };
    (network_request_reason_has_kind(reason, "rate_limited")
        && network_request_reason_has_protocol(reason, protocol))
        || (reason.contains("NetworkRequestFailed")
            && reason.contains("ErrRateLimited")
            && mentions_exact_protocol(reason, protocol))
}

fn network_request_reason_has_kind(reason: &str, kind: &str) -> bool {
    reason
        .split_whitespace()
        .any(|field| field.strip_prefix("kind=") == Some(kind))
}

fn legacy_missing_handler_detail(detail: &str, protocol: &str) -> bool {
    detail == protocol
        || detail == format!("{protocol} unsupported by remote")
        || detail == format!("handler missing: {protocol}")
        || detail == format!("libp2p-replication handler missing: {protocol}")
}
fn network_request_reason_detail_mentions_protocol(reason: &str, protocol: &str) -> bool {
    reason
        .split_once(" detail=")
        .is_some_and(|(_, detail)| legacy_missing_handler_detail(detail, protocol))
}

fn network_request_reason_has_protocol(reason: &str, protocol: &str) -> bool {
    reason
        .split_whitespace()
        .any(|field| field.strip_prefix("protocol=") == Some(protocol))
}

pub(crate) fn interpret_node_diagnostic(
    err: &NodeError,
) -> oasis7_proto::network_failure::NetworkFailure {
    use oasis7_proto::network_failure::{
        FailureCause, FailureEvidence, FailureOrigin, NetworkFailure,
    };
    let mut facts = NetworkFailure {
        origin: FailureOrigin::Legacy,
        evidence: FailureEvidence::LegacyInterpretation,
        cause: FailureCause::Unknown,
        connection_gap: false,
        requested_protocol: None,
    };
    let Some(reason) = err.replication_reason() else {
        return facts;
    };
    // The final versioned suffix is the current writer's fact record. Context is diagnostic.
    if reason.contains(" [network-failure-") {
        if let Some((_, encoded)) = reason.rsplit_once(" [network-failure-v1=")
            && let Some(encoded) = encoded.strip_suffix(']')
            && let Ok(mut restored) = serde_json::from_str::<NetworkFailure>(encoded)
        {
            restored.evidence = FailureEvidence::LegacyInterpretation;
            return restored.conservative();
        }
        return facts;
    }
    // Persisted legacy diagnostics may contain nested peer claims. Denials win there too.
    if [
        "kind=unauthorized",
        "kind=bad_request",
        "kind=state_mismatch",
        "kind=invalid_hash",
        "ErrUnauthorized",
        "ErrBadRequest",
        "ErrStateMismatch",
        "ErrInvalidHash",
    ]
    .iter()
    .any(|denial| reason.contains(denial))
    {
        facts.cause = FailureCause::Rejected;
        return facts;
    }
    let protocols = [
        crate::replication::REPLICATION_GET_HEAD_PROTOCOL,
        crate::replication::REPLICATION_FETCH_COMMIT_PROTOCOL,
        crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL,
    ];
    for protocol in protocols {
        if legacy_mentions_protocol(err, protocol) {
            facts.requested_protocol = Some(protocol.to_owned());
        }
        if legacy_is_timeout_protocol(err, protocol) {
            facts.cause = FailureCause::Timeout;
            facts.connection_gap = legacy_is_availability_gap(err);
            return facts;
        }
        if legacy_is_rate_limited_protocol(err, protocol) {
            facts.cause = FailureCause::RateLimited;
            return facts;
        }
        if legacy_is_unsupported_protocol(err, protocol)
            || legacy_is_protocol_unavailable(err, protocol)
        {
            facts.cause = FailureCause::MissingHandler;
            return facts;
        }
    }
    facts.cause = if legacy_is_availability_gap(err) {
        FailureCause::ConnectionGap
    } else if network_request_reason_has_kind(reason, "timeout") {
        FailureCause::Timeout
    } else if network_request_reason_has_kind(reason, "not_available") {
        FailureCause::Unavailable
    } else if network_request_reason_has_kind(reason, "busy") {
        FailureCause::Busy
    } else if network_request_reason_has_kind(reason, "rate_limited") {
        FailureCause::RateLimited
    } else if legacy_is_not_found(err) {
        FailureCause::NotFound
    } else if reason.starts_with("blob fetch routes exhausted without response")
        || reason.starts_with("blob fetch provider routes exhausted without response")
    {
        FailureCause::ProviderExhausted
    } else if reason.starts_with(REPLICATION_NETWORK_ROUTE_UNAVAILABLE_PREFIX) {
        FailureCause::ProtocolUnavailable
    } else {
        FailureCause::Unknown
    };
    facts.connection_gap = facts.cause == FailureCause::ConnectionGap;
    facts
}
pub(crate) fn provider_blob_route_blocked_reason(reason: &str) -> bool {
    let err = NodeError::Replication {
        reason: reason.to_owned(),
    };
    let facts = interpret_node_diagnostic(&err);
    if facts.evidence == oasis7_proto::network_failure::FailureEvidence::LegacyInterpretation
        && facts.matches_protocol(crate::replication::REPLICATION_FETCH_BLOB_PROTOCOL)
        && matches!(
            facts.cause,
            oasis7_proto::network_failure::FailureCause::MissingHandler
                | oasis7_proto::network_failure::FailureCause::ProviderExhausted
                | oasis7_proto::network_failure::FailureCause::ProtocolUnavailable
                | oasis7_proto::network_failure::FailureCause::NotFound
        )
    {
        return true;
    }
    if reason.contains(" [network-failure-")
        || facts.cause == oasis7_proto::network_failure::FailureCause::Rejected
    {
        return false;
    }
    reason.contains("gap sync height ")
        && (reason.contains(" blob not found for hash ")
            || reason.contains("blob fetch provider routes exhausted")
            || reason.contains(REPLICATION_NETWORK_ROUTE_UNAVAILABLE_PREFIX))
}

fn mentions_exact_protocol(reason: &str, protocol: &str) -> bool {
    reason.match_indices(protocol).any(|(offset, _)| {
        let before = &reason[..offset];
        let after = &reason[offset + protocol.len()..];
        before
            .chars()
            .next_back()
            .is_none_or(|ch| ch.is_whitespace() || matches!(ch, '=' | '\"'))
            && after
                .chars()
                .next()
                .is_none_or(|ch| ch.is_whitespace() || matches!(ch, '\"' | ',' | '}'))
    })
}

/// Old local domain errors carry a precise blob hash in their established diagnostic shape.
/// Callers must not pass ReplicationNetwork diagnostics through this boundary.
pub(crate) fn checkpoint_blob_missing_reason(reason: &str) -> Option<String> {
    if reason.contains("execution checkpoint blob not found hash=") {
        return Some(reason.to_owned());
    }
    if reason.contains("gap sync height ") && reason.contains(" blob not found for hash ") {
        let content_hash = reason
            .rsplit_once(" hash ")
            .map(|(_, content_hash)| content_hash)
            .unwrap_or("unknown");
        return Some(format!(
            "execution checkpoint blob not found hash={content_hash}"
        ));
    }
    None
}
