use super::runtime_loop::PendingResponse;
use super::{WorldError, push_bounded_clone};
use libp2p::{PeerId, request_response};
use oasis7_proto::{
    distributed::DistributedErrorCode,
    network_failure::{FailureCause, FailureOrigin, NetworkFailure},
};
use std::sync::{Arc, Mutex};
pub(super) fn fail_pending_request(
    pending_response: PendingResponse,
    peer: PeerId,
    error: request_response::OutboundFailure,
    event_errors: &Arc<Mutex<Vec<String>>>,
    max_error_messages: usize,
) {
    let protocol = pending_response.protocol;
    let message = format!("request failed: {error:?} protocol={protocol} peer={peer}");
    push_bounded_clone(
        event_errors,
        message.clone(),
        max_error_messages,
        "lock errors",
    );
    let _ = pending_response.response.send(Err({
        let err = request_failure_world_error(error, message);
        let facts = err.network_failure(Some(&protocol));
        err.with_network_failure(facts)
    }));
}

pub(super) fn request_failure_world_error(
    error: request_response::OutboundFailure,
    message: String,
) -> WorldError {
    let cause = match &error {
        request_response::OutboundFailure::Timeout => FailureCause::Timeout,
        request_response::OutboundFailure::ConnectionClosed
        | request_response::OutboundFailure::DialFailure => FailureCause::ConnectionGap,
        request_response::OutboundFailure::Io(error)
            if error.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
            FailureCause::ConnectionGap
        }
        request_response::OutboundFailure::UnsupportedProtocols => FailureCause::MissingHandler,
        request_response::OutboundFailure::Io(_) => FailureCause::Unknown,
    };
    let source = if matches!(cause, FailureCause::ConnectionGap | FailureCause::Timeout) {
        WorldError::NetworkRequestFailed {
            code: DistributedErrorCode::ErrNotAvailable,
            message: format!("{message}: {error:?}"),
            retryable: true,
        }
    } else {
        WorldError::NetworkProtocolUnavailable { protocol: message }
    };
    source.with_network_failure(NetworkFailure::observed(
        FailureOrigin::LocalTransport,
        cause,
        None,
    ))
}

#[cfg(test)]
mod request_failure_tests {
    use super::*;

    #[test]
    fn cancelled_response_and_unexpected_eof_are_retryable_transport_failures() {
        for failure in [
            request_response::OutboundFailure::ConnectionClosed,
            request_response::OutboundFailure::Io(std::io::Error::from(
                std::io::ErrorKind::UnexpectedEof,
            )),
        ] {
            let error = request_failure_world_error(failure, "request failed".to_string());
            assert!(super::super::error_mapping::world_error_is_retryable_connection_gap(&error));
            assert!(matches!(
                error.diagnostic_source(),
                WorldError::NetworkRequestFailed {
                    code: DistributedErrorCode::ErrNotAvailable,
                    retryable: true,
                    ..
                }
            ));
        }
    }
}

#[cfg(test)]
mod typed_outbound_tests {
    use super::*;
    #[test]
    fn actual_outbound_variants_survive_diagnostic_changes() {
        for (outbound, expected) in [
            (
                request_response::OutboundFailure::Timeout,
                FailureCause::Timeout,
            ),
            (
                request_response::OutboundFailure::UnsupportedProtocols,
                FailureCause::MissingHandler,
            ),
            (
                request_response::OutboundFailure::DialFailure,
                FailureCause::ConnectionGap,
            ),
            (
                request_response::OutboundFailure::Io(std::io::Error::from(
                    std::io::ErrorKind::PermissionDenied,
                )),
                FailureCause::Unknown,
            ),
        ] {
            let err =
                request_failure_world_error(outbound, "diagnostic has no cause spelling".into());
            let facts = err.network_failure(None);
            assert_eq!(facts.cause, expected);
            assert_eq!(facts.origin, FailureOrigin::LocalTransport);
            assert_eq!(
                facts.evidence,
                oasis7_proto::network_failure::FailureEvidence::LocalObservation
            );
        }
    }
}
