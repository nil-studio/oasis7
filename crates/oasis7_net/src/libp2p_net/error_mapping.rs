use crate::error::WorldError;
use oasis7_proto::distributed::{DistributedErrorCode, ErrorResponse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Libp2pAvailabilityClass {
    MissingHandler,
    BadRequest,
    RetryableGap,
    Overloaded,
    Other,
}

pub(super) fn error_response_from_world_error(err: &WorldError) -> ErrorResponse {
    match err.diagnostic_source() {
        WorldError::NetworkRequestFailed {
            code,
            message,
            retryable,
        } => ErrorResponse {
            code: match code {
                DistributedErrorCode::ErrOverloaded => DistributedErrorCode::ErrBusy,
                code => *code,
            },
            message: message.clone(),
            retryable: *retryable,
        },
        WorldError::NetworkProtocolUnavailable { protocol } => {
            ErrorResponse::from_code(protocol_unavailable_code(protocol), protocol.clone())
        }
        WorldError::DistributedValidationFailed { reason } => {
            ErrorResponse::from_code(DistributedErrorCode::ErrBadRequest, reason.clone())
        }
        WorldError::BlobNotFound { content_hash } => ErrorResponse::from_code(
            DistributedErrorCode::ErrNotFound,
            format!("blob not found: {content_hash}"),
        ),
        WorldError::BlobHashMismatch { expected, actual } => ErrorResponse::from_code(
            DistributedErrorCode::ErrStateMismatch,
            format!("blob hash mismatch expected={expected} actual={actual}"),
        ),
        WorldError::BlobHashInvalid { content_hash } => ErrorResponse::from_code(
            DistributedErrorCode::ErrInvalidHash,
            format!("blob hash invalid: {content_hash}"),
        ),
        WorldError::SignatureKeyInvalid => ErrorResponse::from_code(
            DistributedErrorCode::ErrUnauthorized,
            "invalid signature key",
        ),
        WorldError::NetworkFailure { .. } => unreachable!("diagnostic source is unwrapped"),
        WorldError::Io(message) => {
            ErrorResponse::from_code(DistributedErrorCode::ErrNotAvailable, message.clone())
        }
        WorldError::Serde(message) => {
            ErrorResponse::from_code(DistributedErrorCode::ErrBadRequest, message.clone())
        }
    }
}

fn protocol_unavailable_code(protocol: &str) -> DistributedErrorCode {
    match classify_protocol_unavailable(protocol) {
        Libp2pAvailabilityClass::MissingHandler | Libp2pAvailabilityClass::Other => {
            DistributedErrorCode::ErrUnsupported
        }
        Libp2pAvailabilityClass::Overloaded => DistributedErrorCode::ErrOverloaded,
        Libp2pAvailabilityClass::BadRequest => DistributedErrorCode::ErrBadRequest,
        Libp2pAvailabilityClass::RetryableGap => DistributedErrorCode::ErrNotAvailable,
    }
}

pub fn classify_world_error_availability(err: &WorldError) -> Libp2pAvailabilityClass {
    use oasis7_proto::network_failure::FailureCause;
    let facts = err.network_failure(None);
    if facts.is_connection_gap() {
        return Libp2pAvailabilityClass::RetryableGap;
    }
    match facts.cause {
        FailureCause::MissingHandler => Libp2pAvailabilityClass::MissingHandler,

        FailureCause::RateLimited | FailureCause::Busy => Libp2pAvailabilityClass::Overloaded,
        FailureCause::Validation => Libp2pAvailabilityClass::BadRequest,
        _ => Libp2pAvailabilityClass::Other,
    }
}

pub fn world_error_is_retryable_connection_gap(err: &WorldError) -> bool {
    matches!(
        classify_world_error_availability(err),
        Libp2pAvailabilityClass::RetryableGap
    )
}

pub fn world_error_is_missing_handler(err: &WorldError) -> bool {
    matches!(
        classify_world_error_availability(err),
        Libp2pAvailabilityClass::MissingHandler
    )
}

pub fn world_error_is_publish_failure(err: &WorldError) -> bool {
    err.network_failure(None).origin == oasis7_proto::network_failure::FailureOrigin::Publish
}

fn classify_protocol_unavailable(protocol: &str) -> Libp2pAvailabilityClass {
    if protocol.contains("handler missing") || protocol.starts_with('/') {
        Libp2pAvailabilityClass::MissingHandler
    } else if protocol_unavailable_is_bad_request(protocol) {
        Libp2pAvailabilityClass::BadRequest
    } else if protocol_unavailable_is_retryable_gap(protocol) {
        Libp2pAvailabilityClass::RetryableGap
    } else {
        Libp2pAvailabilityClass::Other
    }
}

fn protocol_unavailable_is_bad_request(protocol: &str) -> bool {
    protocol.contains("must be utf-8") || protocol.contains("must be valid")
}

fn protocol_unavailable_is_retryable_gap(protocol: &str) -> bool {
    protocol == "libp2p"
        || protocol.starts_with("transport dial failed: ")
        || protocol.contains("is not connected for protocol")
        || protocol.contains("no connected peers for protocol")
        || protocol.contains("no admissible connected peers for protocol")
        || protocol.contains("no connected providers for protocol")
        || protocol.contains("no healthy provider for protocol")
        || protocol.contains("no healthy connected providers for protocol")
        || protocol.contains("request failed: ConnectionClosed")
        || protocol.contains("request failed: DialFailure")
        || protocol.contains("request failed: Timeout")
        || protocol.contains("UnexpectedEof")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn denial_code_cannot_claim_a_retryable_connection_gap_in_its_message() {
        for code in [
            DistributedErrorCode::ErrUnauthorized,
            DistributedErrorCode::ErrBadRequest,
            DistributedErrorCode::ErrStateMismatch,
            DistributedErrorCode::ErrInvalidHash,
        ] {
            let err = WorldError::NetworkRequestFailed {
                code,
                message: "request failed: Timeout protocol=/aw/node/replication/get-head/1.0.0"
                    .into(),
                retryable: true,
            };
            assert!(!world_error_is_retryable_connection_gap(&err), "{code:?}");
            assert_eq!(error_response_from_world_error(&err).code, code);
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    #[expect(
        clippy::enum_variant_names,
        reason = "Legacy v1 error-code names are part of the persisted wire contract"
    )]
    enum LegacyV1DistributedErrorCode {
        ErrNotFound,
        ErrBadRequest,
        ErrInvalidHash,
        ErrStateMismatch,
        ErrUnsupported,
        ErrUnauthorized,
        ErrBusy,
        ErrRateLimited,
        ErrTimeout,
        ErrNotAvailable,
    }

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    struct LegacyV1ErrorResponse {
        code: LegacyV1DistributedErrorCode,
        message: String,
        retryable: bool,
    }

    #[test]
    fn overload_response_remains_decodable_by_legacy_v1_peers_as_busy() {
        let current_error = WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrOverloaded,
            message: "response worker overloaded; retry shortly".to_string(),
            retryable: true,
        };
        let current_response = error_response_from_world_error(&current_error);
        let payload = crate::util::to_canonical_cbor(&current_response)
            .expect("serialize current overload response");

        let legacy_response: LegacyV1ErrorResponse = serde_cbor::from_slice(&payload)
            .expect("legacy v1 peer must decode overload response as ERR_BUSY");
        assert_eq!(legacy_response.code, LegacyV1DistributedErrorCode::ErrBusy);
        assert_eq!(legacy_response.message, current_response.message);
        assert!(legacy_response.retryable);
    }

    #[test]
    fn world_error_availability_classifies_retryable_request_failures() {
        let err = WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrNotAvailable,
            message: "libp2p-replication no connected peers for protocol /aw/node/replication/ping"
                .to_string(),
            retryable: true,
        };
        assert_eq!(
            classify_world_error_availability(&err),
            Libp2pAvailabilityClass::RetryableGap
        );
        assert!(world_error_is_retryable_connection_gap(&err));

        let generic_retryable = WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrNotAvailable,
            message: "remote temporarily unavailable".to_string(),
            retryable: true,
        };
        assert_eq!(
            classify_world_error_availability(&generic_retryable),
            Libp2pAvailabilityClass::Other
        );
        assert!(!world_error_is_retryable_connection_gap(&generic_retryable));
    }

    #[test]
    fn network_protocol_unavailable_handler_missing_maps_to_unsupported() {
        let response = error_response_from_world_error(&WorldError::NetworkProtocolUnavailable {
            protocol: "/aw/node/replication/ping".to_string(),
        });
        assert_eq!(response.code, DistributedErrorCode::ErrUnsupported);
        assert!(!response.retryable);
        assert!(world_error_is_missing_handler(
            &WorldError::NetworkProtocolUnavailable {
                protocol: "/aw/node/replication/ping".to_string(),
            }
        ));
    }

    #[test]
    fn network_protocol_unavailable_availability_gap_maps_to_not_available() {
        let response = error_response_from_world_error(&WorldError::NetworkProtocolUnavailable {
            protocol: "no connected peers for protocol /aw/node/replication/ping".to_string(),
        });
        assert_eq!(response.code, DistributedErrorCode::ErrNotAvailable);
        assert!(response.retryable);
    }

    #[test]
    fn network_protocol_unavailable_request_failure_with_context_stays_retryable() {
        let err = WorldError::NetworkProtocolUnavailable {
            protocol: "request failed: Timeout protocol=/aw/node/replication/fetch-blob/1.0.0 peer=12D3KooWTest".to_string(),
        };
        assert_eq!(
            classify_world_error_availability(&err),
            Libp2pAvailabilityClass::RetryableGap
        );
        assert!(world_error_is_retryable_connection_gap(&err));

        let response = error_response_from_world_error(&err);
        assert_eq!(response.code, DistributedErrorCode::ErrNotAvailable);
        assert!(response.retryable);
    }

    #[test]
    fn network_protocol_unavailable_no_admissible_peers_maps_to_not_available() {
        let response = error_response_from_world_error(&WorldError::NetworkProtocolUnavailable {
            protocol:
                "libp2p-replication no admissible connected peers for protocol /aw/node/replication/ping"
                    .to_string(),
        });
        assert_eq!(response.code, DistributedErrorCode::ErrNotAvailable);
        assert!(response.retryable);
    }

    #[test]
    fn network_protocol_unavailable_no_healthy_provider_maps_to_not_available() {
        let response = error_response_from_world_error(&WorldError::NetworkProtocolUnavailable {
            protocol:
                "libp2p-replication no healthy connected providers for protocol /aw/node/replication/ping"
                    .to_string(),
        });
        assert_eq!(response.code, DistributedErrorCode::ErrNotAvailable);
        assert!(response.retryable);
    }

    #[test]
    fn network_protocol_unavailable_bad_request_maps_to_bad_request() {
        let response = error_response_from_world_error(&WorldError::NetworkProtocolUnavailable {
            protocol: "cached peer record payload must be utf-8".to_string(),
        });
        assert_eq!(response.code, DistributedErrorCode::ErrBadRequest);
        assert!(!response.retryable);
    }

    #[test]
    fn publish_failure_classifies_as_other_availability() {
        let err = WorldError::NetworkProtocolUnavailable {
            protocol: "libp2p publish failed topic=aw.publish.fail: InsufficientPeers".to_string(),
        };
        assert_eq!(
            classify_world_error_availability(&err),
            Libp2pAvailabilityClass::Other
        );
        assert!(!world_error_is_retryable_connection_gap(&err));
        assert!(!world_error_is_missing_handler(&err));
    }
}
