use std::fmt;
use std::io;

use crate::protocol::ProtocolError;

#[derive(Debug)]
pub enum SignerError {
    InvalidInput(String),
    AuthorizationDenied,
    KeyOrAuthorityUnavailable,
    LockBusy,
    IdConflict,
    PersistenceFailed(io::Error),
    CryptoOrBindingInvalid,
    UnsupportedPlatformOrFs,
    RecoveryRequired,
    InstallationDrift,
    BudgetExhausted,
    Protocol(ProtocolError),
}

impl SignerError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) | Self::Protocol(_) => "INVALID_INPUT",
            Self::AuthorizationDenied => "AUTHORIZATION_DENIED",
            Self::KeyOrAuthorityUnavailable => "KEY_OR_AUTHORITY_UNAVAILABLE",
            Self::LockBusy => "LOCK_BUSY",
            Self::IdConflict => "ID_CONFLICT",
            Self::PersistenceFailed(_) => "PERSISTENCE_FAILED",
            Self::CryptoOrBindingInvalid => "CRYPTO_OR_BINDING_INVALID",
            Self::UnsupportedPlatformOrFs => "UNSUPPORTED_PLATFORM_OR_FS",
            Self::RecoveryRequired => "RECOVERY_REQUIRED",
            Self::InstallationDrift => "INSTALLATION_DRIFT",
            Self::BudgetExhausted => "BUDGET_EXHAUSTED",
        }
    }

    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidInput(_) | Self::Protocol(_) => 2,
            Self::AuthorizationDenied => 3,
            Self::KeyOrAuthorityUnavailable => 4,
            Self::LockBusy => 5,
            Self::IdConflict => 6,
            Self::PersistenceFailed(_) => 7,
            Self::CryptoOrBindingInvalid => 8,
            Self::UnsupportedPlatformOrFs => 9,
            Self::RecoveryRequired => 10,
            Self::InstallationDrift => 11,
            Self::BudgetExhausted => 12,
        }
    }
}

impl fmt::Display for SignerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(reason) => write!(formatter, "invalid input: {reason}"),
            Self::AuthorizationDenied => formatter.write_str("authorization denied"),
            Self::KeyOrAuthorityUnavailable => formatter.write_str("key or authority unavailable"),
            Self::LockBusy => formatter.write_str("custody lock is busy"),
            Self::IdConflict => formatter.write_str("request or grant ID conflicts"),
            Self::PersistenceFailed(error) => write!(formatter, "persistence failed: {error}"),
            Self::CryptoOrBindingInvalid => {
                formatter.write_str("cryptographic or payload binding validation failed")
            }
            Self::UnsupportedPlatformOrFs => {
                formatter.write_str("unsupported platform or filesystem")
            }
            Self::RecoveryRequired => formatter.write_str("recovery is required"),
            Self::InstallationDrift => formatter.write_str("installation binding drifted"),
            Self::BudgetExhausted => formatter.write_str("grant request budget is exhausted"),
            Self::Protocol(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for SignerError {}

impl From<ProtocolError> for SignerError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

impl From<io::Error> for SignerError {
    fn from(error: io::Error) -> Self {
        Self::PersistenceFailed(error)
    }
}
