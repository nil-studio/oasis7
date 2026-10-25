//! Original-request transitions, without signing, dispatch or receipt verification.
use crate::DecimalU64;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Admitted,
    Prepared,
    Dispatching,
    Unknown,
    Confirmed,
    CancelledBeforeDispatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawOperation")]
pub struct Operation {
    request_id: String,
    body_digest: String,
    state: OperationState,
    signed_bytes: Option<Vec<u8>>,
    claimed_epoch: Option<DecimalU64>,
    receipt_digest: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOperation {
    request_id: String,
    body_digest: String,
    state: OperationState,
    signed_bytes: Option<Vec<u8>>,
    claimed_epoch: Option<DecimalU64>,
    receipt_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationError {
    Invalid,
    Conflict,
    InvalidTransition,
}

impl TryFrom<RawOperation> for Operation {
    type Error = &'static str;
    fn try_from(raw: RawOperation) -> Result<Self, Self::Error> {
        let has_bytes = raw
            .signed_bytes
            .as_ref()
            .is_some_and(|bytes| !bytes.is_empty());
        let invalid_bytes = raw.signed_bytes.as_ref().is_some_and(Vec::is_empty);
        let has_receipt = raw
            .receipt_digest
            .as_ref()
            .is_some_and(|value| !value.is_empty());
        let valid_state = match raw.state {
            OperationState::Admitted => {
                raw.signed_bytes.is_none()
                    && raw.claimed_epoch.is_none()
                    && raw.receipt_digest.is_none()
            }
            OperationState::Prepared => {
                has_bytes && raw.claimed_epoch.is_none() && raw.receipt_digest.is_none()
            }
            OperationState::Dispatching | OperationState::Unknown => {
                has_bytes && raw.claimed_epoch.is_some() && raw.receipt_digest.is_none()
            }
            OperationState::Confirmed => has_bytes && raw.claimed_epoch.is_some() && has_receipt,
            OperationState::CancelledBeforeDispatch => {
                raw.claimed_epoch.is_none() && raw.receipt_digest.is_none()
            }
        };
        if raw.request_id.is_empty() || raw.body_digest.is_empty() || invalid_bytes || !valid_state
        {
            return Err("inconsistent persisted operation");
        }
        Ok(Self {
            request_id: raw.request_id,
            body_digest: raw.body_digest,
            state: raw.state,
            signed_bytes: raw.signed_bytes,
            claimed_epoch: raw.claimed_epoch,
            receipt_digest: raw.receipt_digest,
        })
    }
}

impl Operation {
    pub fn admit(request_id: String, body_digest: String) -> Result<Self, OperationError> {
        if request_id.is_empty() || body_digest.is_empty() {
            return Err(OperationError::Invalid);
        }
        Ok(Self {
            request_id,
            body_digest,
            state: OperationState::Admitted,
            signed_bytes: None,
            claimed_epoch: None,
            receipt_digest: None,
        })
    }
    pub fn matches_request(&self, id: &str, digest: &str) -> Result<(), OperationError> {
        if self.request_id == id && self.body_digest == digest {
            Ok(())
        } else {
            Err(OperationError::Conflict)
        }
    }
    pub fn state(&self) -> OperationState {
        self.state
    }
    pub fn original_bytes(&self) -> Option<&[u8]> {
        self.signed_bytes.as_deref()
    }
    pub fn prepare(&mut self, bytes: Vec<u8>) -> Result<(), OperationError> {
        if bytes.is_empty() {
            return Err(OperationError::Invalid);
        }
        if let Some(original) = &self.signed_bytes {
            return if original == &bytes && self.state == OperationState::Prepared {
                Ok(())
            } else {
                Err(OperationError::Conflict)
            };
        }
        if self.state != OperationState::Admitted {
            return Err(OperationError::InvalidTransition);
        }
        self.signed_bytes = Some(bytes);
        self.state = OperationState::Prepared;
        Ok(())
    }
    /// Authority, deadline and fence revalidation is a caller prerequisite.
    pub fn claim(&mut self, epoch: u64) -> Result<(), OperationError> {
        if self.state != OperationState::Prepared {
            return Err(OperationError::InvalidTransition);
        }
        self.claimed_epoch = Some(DecimalU64(epoch));
        self.state = OperationState::Dispatching;
        Ok(())
    }
    pub fn mark_unknown(&mut self) -> Result<(), OperationError> {
        if !matches!(
            self.state,
            OperationState::Dispatching | OperationState::Unknown
        ) {
            return Err(OperationError::InvalidTransition);
        }
        self.state = OperationState::Unknown;
        Ok(())
    }
    /// Call only after verifying the authoritative original-operation receipt.
    pub fn confirm(&mut self, receipt_digest: String) -> Result<(), OperationError> {
        if receipt_digest.is_empty() {
            return Err(OperationError::Invalid);
        }
        if self.state == OperationState::Confirmed {
            return if self.receipt_digest.as_ref() == Some(&receipt_digest) {
                Ok(())
            } else {
                Err(OperationError::Conflict)
            };
        }
        if !matches!(
            self.state,
            OperationState::Dispatching | OperationState::Unknown
        ) {
            return Err(OperationError::InvalidTransition);
        }
        self.receipt_digest = Some(receipt_digest);
        self.state = OperationState::Confirmed;
        Ok(())
    }
    pub fn cancel_before_dispatch(&mut self) -> Result<(), OperationError> {
        if !matches!(
            self.state,
            OperationState::Admitted
                | OperationState::Prepared
                | OperationState::CancelledBeforeDispatch
        ) {
            return Err(OperationError::InvalidTransition);
        }
        self.state = OperationState::CancelledBeforeDispatch;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_never_replaces_original_or_cancels() {
        let mut op = Operation::admit("id".into(), "digest".into()).unwrap();
        assert_eq!(
            op.matches_request("id", "changed"),
            Err(OperationError::Conflict)
        );
        op.prepare(vec![1, 2]).unwrap();
        assert_eq!(op.prepare(vec![2]), Err(OperationError::Conflict));
        op.claim(1).unwrap();
        op.mark_unknown().unwrap();
        assert_eq!(
            op.cancel_before_dispatch(),
            Err(OperationError::InvalidTransition)
        );
        assert_eq!(op.claim(2), Err(OperationError::InvalidTransition));
        assert_eq!(op.original_bytes(), Some([1, 2].as_slice()));
        op.confirm("receipt".into()).unwrap();
        op.confirm("receipt".into()).unwrap();
        assert_eq!(op.confirm("other".into()), Err(OperationError::Conflict));
    }
    #[test]
    fn cancelled_cannot_dispatch() {
        let mut op = Operation::admit("id".into(), "digest".into()).unwrap();
        op.cancel_before_dispatch().unwrap();
        assert_eq!(op.prepare(vec![1]), Err(OperationError::InvalidTransition));
        assert_eq!(op.claim(1), Err(OperationError::InvalidTransition));
    }
}
