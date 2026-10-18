//! Shared persistence errors, independent of host archive and filesystem helpers.
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistError {
    Io(String),
    Serde(String),
    SnapshotMismatch {
        expected: usize,
        actual: usize,
    },
    ReplayConflict {
        message: String,
    },
    UnsupportedVersion {
        kind: String,
        version: u32,
        expected: u32,
    },
}

impl From<io::Error> for PersistError {
    fn from(err: io::Error) -> Self {
        PersistError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for PersistError {
    fn from(err: serde_json::Error) -> Self {
        PersistError::Serde(err.to_string())
    }
}
