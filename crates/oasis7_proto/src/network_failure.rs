//! Internal network facts. This type is deliberately absent from the wire schema.
use crate::{distributed::DistributedErrorCode as Code, world_error::WorldError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FailureOrigin {
    LocalTransport,
    RemoteResponse,
    Dht,
    ProviderRouting,
    Publish,
    DecodeValidation,
    Legacy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FailureEvidence {
    LocalObservation,
    PeerClaim,
    LegacyInterpretation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FailureCause {
    ConnectionGap,
    Timeout,
    MissingHandler,
    NotFound,
    Busy,
    RateLimited,
    Rejected,
    Validation,
    ProviderExhausted,
    ProtocolUnavailable,
    Unavailable,
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkFailure {
    pub origin: FailureOrigin,
    pub evidence: FailureEvidence,
    pub cause: FailureCause,
    pub connection_gap: bool,
    pub requested_protocol: Option<String>,
}
impl NetworkFailure {
    pub fn observed(origin: FailureOrigin, cause: FailureCause, protocol: Option<&str>) -> Self {
        Self {
            origin,
            evidence: FailureEvidence::LocalObservation,
            cause,
            connection_gap: cause == FailureCause::ConnectionGap
                || cause == FailureCause::Timeout && origin == FailureOrigin::LocalTransport,
            requested_protocol: protocol.map(str::to_owned),
        }
    }
    pub fn conservative(mut self) -> Self {
        if self.origin == FailureOrigin::Publish {
            self.cause = FailureCause::Unknown;
        }
        if self.origin == FailureOrigin::DecodeValidation {
            self.cause = FailureCause::Validation;
        }
        if !matches!(
            self.cause,
            FailureCause::ConnectionGap | FailureCause::Timeout
        ) {
            self.connection_gap = false;
        }
        self
    }
    pub fn is_connection_gap(&self) -> bool {
        self.connection_gap
    }
    pub fn matches_protocol(&self, protocol: &str) -> bool {
        self.requested_protocol.as_deref() == Some(protocol)
    }
}

/// Compatibility boundary for pre-fact producers. Never upgrades text to an observation.
pub fn interpret_legacy_world_error(error: &WorldError, protocol: Option<&str>) -> NetworkFailure {
    if let WorldError::NetworkFailure { failure, .. } = error {
        let mut facts = (**failure).clone();
        if facts.requested_protocol.is_none() {
            facts.requested_protocol = protocol.map(str::to_owned);
        }
        return facts;
    }
    let mut facts = NetworkFailure {
        origin: FailureOrigin::Legacy,
        evidence: FailureEvidence::LegacyInterpretation,
        cause: FailureCause::Unknown,
        connection_gap: false,
        requested_protocol: protocol.map(str::to_owned),
    };
    facts.cause = match error {
        WorldError::NetworkRequestFailed { code, message, .. } => {
            facts.origin = FailureOrigin::RemoteResponse;
            // Code is authoritative only for classifying the peer's claim; denial wins.
            match code {
                Code::ErrUnauthorized
                | Code::ErrBadRequest
                | Code::ErrStateMismatch
                | Code::ErrInvalidHash => FailureCause::Rejected,
                Code::ErrNotFound => FailureCause::NotFound,
                Code::ErrTimeout => FailureCause::Timeout,
                Code::ErrRateLimited => FailureCause::RateLimited,
                Code::ErrBusy | Code::ErrOverloaded => FailureCause::Busy,
                Code::ErrUnsupported => {
                    let availability = legacy_availability_cause(message);
                    if availability != FailureCause::Unknown {
                        availability
                    } else if legacy_missing_handler(message) {
                        FailureCause::MissingHandler
                    } else {
                        FailureCause::Rejected
                    }
                }
                Code::ErrNotAvailable => match legacy_availability_cause(message) {
                    FailureCause::Unknown => FailureCause::Unavailable,
                    cause => cause,
                },
            }
        }
        WorldError::NetworkProtocolUnavailable { protocol: detail } => {
            if detail.starts_with("libp2p publish failed topic=") {
                facts.origin = FailureOrigin::Publish;
                FailureCause::Unknown
            } else if detail.contains("must be utf-8") || detail.contains("must be valid") {
                FailureCause::Validation
            } else if legacy_missing_handler(detail) {
                FailureCause::MissingHandler
            } else {
                match legacy_availability_cause(detail) {
                    FailureCause::Unknown => FailureCause::ProtocolUnavailable,
                    cause => cause,
                }
            }
        }
        WorldError::BlobNotFound { .. } => FailureCause::NotFound,
        WorldError::DistributedValidationFailed { .. }
        | WorldError::BlobHashMismatch { .. }
        | WorldError::BlobHashInvalid { .. }
        | WorldError::SignatureKeyInvalid
        | WorldError::Serde(_) => {
            facts.origin = FailureOrigin::DecodeValidation;
            FailureCause::Validation
        }
        WorldError::Io(_) => FailureCause::Unknown,
        WorldError::NetworkFailure { .. } => unreachable!(),
    };
    facts.connection_gap = facts.cause == FailureCause::ConnectionGap
        || facts.cause == FailureCause::Timeout
            && !matches!(
                error,
                WorldError::NetworkRequestFailed {
                    code: Code::ErrTimeout,
                    ..
                }
            );
    facts
}

fn legacy_missing_handler(detail: &str) -> bool {
    detail.starts_with('/')
        && (!detail.chars().any(char::is_whitespace)
            || detail.ends_with(" unsupported by remote") && detail.split_whitespace().count() == 4)
        || detail.starts_with("handler missing")
        || detail.starts_with("libp2p handler missing")
        || detail.starts_with("libp2p-replication handler missing:")
}
fn legacy_availability_cause(detail: &str) -> FailureCause {
    // Only historical producer envelopes are accepted; arbitrary peer prose is not transport evidence.
    if ![
        "request failed: ",
        "libp2p command ",
        "transport dial failed: ",
        "libp2p-replication ",
        "no connected ",
        "no admissible ",
        "no healthy ",
        "peer ",
    ]
    .iter()
    .any(|prefix| detail.starts_with(prefix))
        && detail != "libp2p"
    {
        return FailureCause::Unknown;
    }
    if [
        "ErrUnauthorized",
        "ErrBadRequest",
        "ErrStateMismatch",
        "ErrInvalidHash",
    ]
    .iter()
    .any(|denial| detail.contains(denial))
    {
        return FailureCause::Rejected;
    }
    if detail.contains("request failed: Timeout")
        || detail.starts_with("libp2p command ") && detail.contains(" timed out after ")
    {
        return FailureCause::Timeout;
    }
    if detail == "libp2p"
        || detail.starts_with("transport dial failed: ")
        || [
            "is not connected for protocol",
            "no connected peers for protocol",
            "no admissible connected peers for protocol",
            "no connected providers for protocol",
            "no healthy provider for protocol",
            "no healthy connected providers for protocol",
            "request failed: ConnectionClosed",
            "request failed: DialFailure",
            "UnexpectedEof",
        ]
        .iter()
        .any(|shape| detail.contains(shape))
    {
        FailureCause::ConnectionGap
    } else {
        FailureCause::Unknown
    }
}
