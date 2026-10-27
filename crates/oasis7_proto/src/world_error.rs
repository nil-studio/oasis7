use std::io;

use crate::distributed::DistributedErrorCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    NetworkFailure {
        failure: Box<crate::network_failure::NetworkFailure>,
        source: Box<WorldError>,
    },
    NetworkProtocolUnavailable {
        protocol: String,
    },
    NetworkRequestFailed {
        code: DistributedErrorCode,
        message: String,
        retryable: bool,
    },
    DistributedValidationFailed {
        reason: String,
    },
    BlobNotFound {
        content_hash: String,
    },
    BlobHashMismatch {
        expected: String,
        actual: String,
    },
    BlobHashInvalid {
        content_hash: String,
    },
    SignatureKeyInvalid,
    Io(String),
    Serde(String),
}

impl From<serde_cbor::Error> for WorldError {
    fn from(error: serde_cbor::Error) -> Self {
        WorldError::Serde(error.to_string())
    }
}

impl From<serde_json::Error> for WorldError {
    fn from(error: serde_json::Error) -> Self {
        WorldError::Serde(error.to_string())
    }
}

impl From<io::Error> for WorldError {
    fn from(error: io::Error) -> Self {
        WorldError::Io(error.to_string())
    }
}

impl WorldError {
    pub fn with_network_failure(self, failure: crate::network_failure::NetworkFailure) -> Self {
        let source = match self {
            Self::NetworkFailure { source, .. } => source,
            source => Box::new(source),
        };
        Self::NetworkFailure {
            failure: Box::new(failure),
            source,
        }
    }
    pub fn diagnostic_source(&self) -> &Self {
        match self {
            Self::NetworkFailure { source, .. } => source.diagnostic_source(),
            source => source,
        }
    }
    pub fn network_failure(
        &self,
        protocol: Option<&str>,
    ) -> crate::network_failure::NetworkFailure {
        crate::network_failure::interpret_legacy_world_error(self, protocol).conservative()
    }
}
