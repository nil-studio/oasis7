//! Semantic failures of the formal read service, independent of wire errors.
use oasis7_formal_protocol::controlled_authority::formal::FormalError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormalReadError {
    RequestRejected(String),
    GrantRejected(String),
    NotReady(String),
    HistoryUnavailable(String),
    Internal(String),
    Integrity(String),
}

pub type FormalReadResult<T> = Result<T, FormalReadError>;

impl std::fmt::Display for FormalReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let diagnostic = match self {
            Self::RequestRejected(message)
            | Self::GrantRejected(message)
            | Self::NotReady(message)
            | Self::HistoryUnavailable(message)
            | Self::Internal(message)
            | Self::Integrity(message) => message,
        };
        f.write_str(diagnostic)
    }
}
impl std::error::Error for FormalReadError {}

impl From<FormalError> for FormalReadError {
    fn from(error: FormalError) -> Self {
        match error {
            FormalError::Io(message) => Self::Internal(message),
            FormalError::HistoryUnavailable(message) => Self::HistoryUnavailable(message),
            other => Self::Integrity(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_absence_is_distinct_from_invalid_diagnostics() {
        let diagnostic = "HistoryUnavailable: not ready".to_owned();
        assert_eq!(
            FormalReadError::from(FormalError::HistoryUnavailable(diagnostic.clone())),
            FormalReadError::HistoryUnavailable(diagnostic.clone())
        );
        assert_eq!(
            FormalReadError::from(FormalError::Invalid(diagnostic.clone())),
            FormalReadError::Integrity(format!("Invalid({diagnostic:?})"))
        );
        assert_eq!(
            FormalReadError::from(FormalError::Io(diagnostic.clone())),
            FormalReadError::Internal(diagnostic)
        );
    }
}
