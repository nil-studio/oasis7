//! Persisted lease DTO and its schema validation; accounting is owned by Runtime.
pub use oasis7_agent_api::CognitionLeaseStatusV1;
pub use oasis7_agent_api::legacy_economy::{
    COGNITION_LEASE_SCHEMA_VERSION, CognitionEconomyError, CognitionLeaseQuoteV1,
    CognitionLeaseRequestV1,
};
use serde::{Deserialize, Serialize};
const MAX_IDENTITY_BYTES: usize = 256;
fn bounded_identity(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_IDENTITY_BYTES
        && !value.chars().any(char::is_control)
}

fn valid_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Durable lease identity and its immutable quote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionLeaseV1 {
    pub schema_version: String,
    pub lease_id: String,
    pub idempotency_key: String,
    pub account_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub quote: CognitionLeaseQuoteV1,
    pub reserved_amount: u64,
    pub settled_amount: u64,
    #[serde(default)]
    pub released_amount: u64,
    pub refunded_amount: u64,
    #[serde(default)]
    pub compensated_amount: u64,
    #[serde(default)]
    pub net_amount: u64,
    pub status: CognitionLeaseStatusV1,
    pub reserved_at_tick: u64,
    #[serde(default)]
    pub closed_at_tick: Option<u64>,
    #[serde(default)]
    pub receipt_id: Option<String>,
}

/// Stable short name for the durable lease record.
pub type CognitionLease = CognitionLeaseV1;

impl CognitionLeaseV1 {
    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_LEASE_SCHEMA_VERSION
            || !bounded_identity(&self.lease_id)
            || !bounded_identity(&self.idempotency_key)
            || !bounded_identity(&self.account_id)
            || !bounded_identity(&self.agent_id)
            || !bounded_identity(&self.agent_session_id)
            || !bounded_identity(&self.agent_turn_id)
            || !bounded_identity(&self.decision_request_id)
            || !bounded_identity(&self.request_digest)
            || self.quote.payer_id != self.account_id
            || self.reserved_amount != self.quote.amount
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_lease_invalid",
            ));
        }
        self.quote.validate()?;
        let request = CognitionLeaseRequestV1::new(
            self.idempotency_key.clone(),
            self.account_id.clone(),
            self.agent_id.clone(),
            self.agent_session_id.clone(),
            self.agent_turn_id.clone(),
            self.decision_request_id.clone(),
            self.request_digest.clone(),
            self.quote.clone(),
        );
        if request.derived_lease_id() != self.lease_id {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_lease_identity_invalid",
            ));
        }
        let legacy_release_shape = self.status == CognitionLeaseStatusV1::Released
            && self.released_amount == 0
            && self.refunded_amount == self.reserved_amount
            && self.compensated_amount == 0
            && self.net_amount == 0;
        let legacy_refund_shape = self.status == CognitionLeaseStatusV1::Refunded
            && self.released_amount == 0
            && self.compensated_amount == 0
            && self.refunded_amount == self.reserved_amount
            && self.net_amount == 0;
        if self.settled_amount > self.reserved_amount
            || self.refunded_amount > self.reserved_amount
            || (self.status == CognitionLeaseStatusV1::Reserved
                && (self.settled_amount != 0
                    || self.refunded_amount != 0
                    || self.released_amount != 0
                    || self.compensated_amount != 0
                    || self.net_amount != 0
                    || self.closed_at_tick.is_some()))
            || (matches!(
                self.status,
                CognitionLeaseStatusV1::Released | CognitionLeaseStatusV1::Expired
            ) && (self.settled_amount != 0
                || (!legacy_release_shape
                    && (self.refunded_amount != 0
                        || self.released_amount != self.reserved_amount))
                || self.compensated_amount != 0
                || self.net_amount != 0))
            || (self.status == CognitionLeaseStatusV1::Refunded
                && (!legacy_refund_shape
                    && (self.settled_amount == 0
                        || self.released_amount != 0
                        || self.compensated_amount == 0
                        || self.compensated_amount > self.settled_amount
                        || self
                            .reserved_amount
                            .checked_sub(self.settled_amount)
                            .and_then(|remaining| remaining.checked_add(self.compensated_amount))
                            != Some(self.refunded_amount)
                        || self.settled_amount.checked_sub(self.compensated_amount)
                            != Some(self.net_amount))))
            || (self.status != CognitionLeaseStatusV1::Reserved
                && (self.closed_at_tick.is_none()
                    || self
                        .receipt_id
                        .as_deref()
                        .is_none_or(|id| !valid_digest(id))))
            || (self.status == CognitionLeaseStatusV1::Settled
                && (self.released_amount != 0
                    || self.compensated_amount != 0
                    || (self.net_amount != 0 && self.net_amount != self.settled_amount)))
            || (self.status == CognitionLeaseStatusV1::Settled
                && self.settled_amount.checked_add(self.refunded_amount)
                    != Some(self.reserved_amount))
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_lease_state_invalid",
            ));
        }
        Ok(())
    }
}
