//! Runtime-owned cognition economy leases and receipts.
//!
//! The provider may request a quote, but it never owns the balance, lease, or
//! receipt.  This projection is kept beside the existing cognition journal so
//! reserve and terminal transitions can be persisted in the same World
//! snapshot without borrowing scheduler wake capacity as an accounting lane.

use super::super::cognition_recovery::cognition_digest_v1;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fmt;

#[path = "cognition_economy_balances.rs"]
mod balances;
#[path = "cognition_economy_provisioning.rs"]
mod provisioning;
#[path = "cognition_economy_transitions.rs"]
mod transitions;
#[path = "cognition_economy_validation.rs"]
mod validation;

pub use provisioning::{
    CognitionProvisioningEventV1, CognitionProvisioningReceiptV1, CognitionProvisioningRecordV1,
    CognitionProvisioningRequestV1,
};

pub const COGNITION_ECONOMY_SCHEMA_VERSION: &str = "cognition-economy.v1";
pub const COGNITION_LEASE_SCHEMA_VERSION: &str = "cognition-lease.v1";
pub const COGNITION_RECEIPT_SCHEMA_VERSION: &str = "cognition-receipt.v1";
pub const COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION: &str = "fixed_unit_experimental";
pub const COGNITION_RESOURCE_VERSION_V1: &str = "cognition_units.v1";
pub const COGNITION_PROVISIONING_SCHEMA_VERSION: &str = "cognition-provisioning.v1";
pub const COGNITION_PROVISIONING_RECEIPT_SCHEMA_VERSION: &str = "cognition-provisioning-receipt.v1";
pub const COGNITION_PROVISIONING_EVENT_SCHEMA_VERSION: &str = "cognition-provisioning-event.v1";
const COGNITION_DEFAULT_PURPOSE: &str = "provider_cognition";
const COGNITION_DEFAULT_SCOPE: &str = "agent_turn";
const COGNITION_LEGACY_AUTHORITY_CONTEXT: &str = "legacy-unbound";
const COGNITION_LEGACY_WORLD_BINDING: &str = "legacy-unbound";
const COGNITION_ECONOMY_EVENT_SCHEMA_VERSION: &str = "cognition-economy-event.v1";
const COGNITION_ECONOMY_JOURNAL_DOMAIN: &str = "oasis7.cognition.economy.journal-head.v1";
const COGNITION_ECONOMY_EVENT_DOMAIN: &str = "oasis7.cognition.economy.event.v1";
const COGNITION_ECONOMY_QUOTE_DOMAIN: &str = "oasis7.cognition.economy.quote.v1";
const COGNITION_ECONOMY_REQUEST_DOMAIN: &str = "oasis7.cognition.economy.request.v1";
const COGNITION_ECONOMY_LEASE_ID_DOMAIN: &str = "oasis7.cognition.economy.lease-id.v1";
const COGNITION_ECONOMY_OPERATION_DOMAIN: &str = "oasis7.cognition.economy.operation.v1";
const COGNITION_ECONOMY_RECEIPT_ID_DOMAIN: &str = "oasis7.cognition.economy.receipt-id.v1";
const COGNITION_ECONOMY_RECEIPT_DOMAIN: &str = "oasis7.cognition.economy.receipt.v1";
const COGNITION_PROVISIONING_AUTHORITY_DOMAIN: &str =
    "oasis7.cognition.economy.provisioning-authority.v1";
const COGNITION_PROVISIONING_DIGEST_DOMAIN: &str = "oasis7.cognition.economy.provisioning.v1";
const COGNITION_PROVISIONING_RECEIPT_ID_DOMAIN: &str =
    "oasis7.cognition.economy.provisioning-receipt-id.v1";
const COGNITION_PROVISIONING_RECEIPT_DOMAIN: &str =
    "oasis7.cognition.economy.provisioning-receipt.v1";
const COGNITION_PROVISIONING_EVENT_DOMAIN: &str = "oasis7.cognition.economy.provisioning-event.v1";
const COGNITION_PROVISIONING_JOURNAL_DOMAIN: &str =
    "oasis7.cognition.economy.provisioning-journal.v1";
const MAX_IDENTITY_BYTES: usize = 256;

fn default_resource_version() -> String {
    COGNITION_RESOURCE_VERSION_V1.to_string()
}

fn default_purpose() -> String {
    COGNITION_DEFAULT_PURPOSE.to_string()
}

fn default_scope() -> String {
    COGNITION_DEFAULT_SCOPE.to_string()
}

fn default_policy_revision() -> String {
    COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION.to_string()
}

fn default_authority_context() -> String {
    COGNITION_LEGACY_AUTHORITY_CONTEXT.to_string()
}

fn default_world_binding() -> String {
    COGNITION_LEGACY_WORLD_BINDING.to_string()
}

fn default_provisioning_head_digest() -> String {
    economy_digest(
        COGNITION_PROVISIONING_JOURNAL_DOMAIN,
        &(0_u64, Vec::<CognitionProvisioningEventV1>::new()),
    )
}

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

fn economy_digest<T: Serialize>(domain: &str, value: &T) -> String {
    cognition_digest_v1(domain, value)
}

/// Stable error categories emitted by the runtime economy boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CognitionEconomyError {
    InvalidInput(&'static str),
    InvalidState(&'static str),
    Conflict(&'static str),
    InsufficientBalance {
        account_id: String,
        resource: String,
        requested: u64,
        available: u64,
    },
    LeaseNotFound(String),
}

impl CognitionEconomyError {
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidInput(code) | Self::InvalidState(code) | Self::Conflict(code) => code,
            Self::InsufficientBalance { .. } => "cognition_insufficient_balance",
            Self::LeaseNotFound(_) => "cognition_lease_not_found",
        }
    }
}

impl fmt::Display for CognitionEconomyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(code) | Self::InvalidState(code) | Self::Conflict(code) => {
                formatter.write_str(code)
            }
            Self::InsufficientBalance {
                account_id,
                resource,
                requested,
                available,
            } => write!(
                formatter,
                "cognition_insufficient_balance account={account_id} resource={resource} requested={requested} available={available}"
            ),
            Self::LeaseNotFound(lease_id) => {
                write!(formatter, "cognition_lease_not_found lease_id={lease_id}")
            }
        }
    }
}

impl std::error::Error for CognitionEconomyError {}

/// A host-issued immutable integer-resource quote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionLeaseQuoteV1 {
    pub schema_version: String,
    pub quote_id: String,
    /// Runtime payer identity.  The legacy constructor leaves this empty so
    /// the request constructor can bind the existing account identity.
    #[serde(default)]
    pub payer_id: String,
    pub resource: String,
    #[serde(default = "default_resource_version")]
    pub resource_version: String,
    pub amount: u64,
    #[serde(default = "default_purpose")]
    pub purpose: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_policy_revision")]
    pub policy_revision: String,
    #[serde(default = "default_authority_context")]
    pub authority_context: String,
    #[serde(default = "default_world_binding")]
    pub world_binding: String,
    #[serde(default)]
    pub valid_until_tick: Option<u64>,
    pub quote_digest: String,
}

/// Short compatibility name for callers that treat a quote independently of
/// the lease request.
pub type CognitionQuoteV1 = CognitionLeaseQuoteV1;

impl CognitionLeaseQuoteV1 {
    pub fn new(quote_id: impl Into<String>, resource: impl Into<String>, amount: u64) -> Self {
        let resource = resource.into();
        let mut quote = Self {
            schema_version: COGNITION_LEASE_SCHEMA_VERSION.to_string(),
            quote_id: quote_id.into(),
            payer_id: String::new(),
            resource: resource.clone(),
            resource_version: format!("{resource}.v1"),
            amount,
            purpose: COGNITION_DEFAULT_PURPOSE.to_string(),
            scope: COGNITION_DEFAULT_SCOPE.to_string(),
            policy_revision: COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION.to_string(),
            authority_context: COGNITION_LEGACY_AUTHORITY_CONTEXT.to_string(),
            world_binding: COGNITION_LEGACY_WORLD_BINDING.to_string(),
            valid_until_tick: None,
            quote_digest: String::new(),
        };
        quote.refresh_digest();
        quote
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_authority(
        mut self,
        payer_id: impl Into<String>,
        resource_version: impl Into<String>,
        purpose: impl Into<String>,
        scope: impl Into<String>,
        policy_revision: impl Into<String>,
        authority_context: impl Into<String>,
        world_binding: impl Into<String>,
    ) -> Self {
        self.payer_id = payer_id.into();
        self.resource_version = resource_version.into();
        self.purpose = purpose.into();
        self.scope = scope.into();
        self.policy_revision = policy_revision.into();
        self.authority_context = authority_context.into();
        self.world_binding = world_binding.into();
        self.refresh_digest();
        self
    }

    pub fn with_valid_until_tick(mut self, tick: u64) -> Self {
        self.valid_until_tick = Some(tick);
        self.refresh_digest();
        self
    }

    pub fn recompute_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("lease quote is serializable");
        value
            .as_object_mut()
            .expect("lease quote is an object")
            .remove("quote_digest");
        economy_digest(COGNITION_ECONOMY_QUOTE_DOMAIN, &value)
    }

    pub fn refresh_digest(&mut self) {
        self.quote_digest = self.recompute_digest();
    }

    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_LEASE_SCHEMA_VERSION
            || !bounded_identity(&self.quote_id)
            || !bounded_identity(&self.resource)
            || (!self.payer_id.is_empty() && !bounded_identity(&self.payer_id))
            || !bounded_identity(&self.resource_version)
            || self.amount == 0
            || !bounded_identity(&self.purpose)
            || !bounded_identity(&self.scope)
            || self.policy_revision != COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION
            || !bounded_identity(&self.authority_context)
            || !bounded_identity(&self.world_binding)
            || !valid_digest(&self.quote_digest)
            || self.quote_digest != self.recompute_digest()
        {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_quote_invalid",
            ));
        }
        Ok(())
    }
}

/// Correlation supplied by an admission adapter.  Runtime derives lease and
/// receipt identities from this value and the immutable quote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionLeaseRequestV1 {
    pub schema_version: String,
    pub idempotency_key: String,
    pub account_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub quote: CognitionLeaseQuoteV1,
}

impl CognitionLeaseRequestV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        idempotency_key: impl Into<String>,
        account_id: impl Into<String>,
        agent_id: impl Into<String>,
        agent_session_id: impl Into<String>,
        agent_turn_id: impl Into<String>,
        decision_request_id: impl Into<String>,
        request_digest: impl Into<String>,
        mut quote: CognitionLeaseQuoteV1,
    ) -> Self {
        let account_id = account_id.into();
        if quote.payer_id.is_empty() {
            quote.payer_id = account_id.clone();
            quote.refresh_digest();
        }
        Self {
            schema_version: COGNITION_LEASE_SCHEMA_VERSION.to_string(),
            idempotency_key: idempotency_key.into(),
            account_id,
            agent_id: agent_id.into(),
            agent_session_id: agent_session_id.into(),
            agent_turn_id: agent_turn_id.into(),
            decision_request_id: decision_request_id.into(),
            request_digest: request_digest.into(),
            quote,
        }
    }

    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        if self.schema_version != COGNITION_LEASE_SCHEMA_VERSION
            || !bounded_identity(&self.idempotency_key)
            || !bounded_identity(&self.account_id)
            || !bounded_identity(&self.agent_id)
            || !bounded_identity(&self.agent_session_id)
            || !bounded_identity(&self.agent_turn_id)
            || !bounded_identity(&self.decision_request_id)
            || !bounded_identity(&self.request_digest)
            || !bounded_identity(&self.quote.payer_id)
            || self.quote.payer_id != self.account_id
        {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_lease_request_invalid",
            ));
        }
        self.quote.validate()
    }

    fn fingerprint_digest(&self) -> String {
        economy_digest(
            COGNITION_ECONOMY_REQUEST_DOMAIN,
            &(
                &self.schema_version,
                &self.account_id,
                &self.agent_id,
                &self.agent_session_id,
                &self.agent_turn_id,
                &self.decision_request_id,
                &self.request_digest,
                &self.quote,
            ),
        )
    }

    pub fn derived_lease_id(&self) -> String {
        economy_digest(
            COGNITION_ECONOMY_LEASE_ID_DOMAIN,
            &(self.idempotency_key.as_str(), self.fingerprint_digest()),
        )
    }
}

/// Shared labels for the Runtime-owned lease state machine. The shared DTO
/// carries no transition authority; all accounting remains in this module.
pub use oasis7_agent_api::CognitionLeaseStatusV1;

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

/// Immutable economic receipt for one terminal operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionReceiptV1 {
    pub schema_version: String,
    pub receipt_id: String,
    pub receipt_digest: String,
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
    /// Canonical operation name: reserve, settle, release, expire, or refund.
    #[serde(default)]
    pub operation: String,
    pub consumed_amount: u64,
    #[serde(default)]
    pub released_amount: u64,
    pub refunded_amount: u64,
    #[serde(default)]
    pub net_amount: u64,
    #[serde(default)]
    pub parent_receipt_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    pub status: CognitionLeaseStatusV1,
    pub issued_at_tick: u64,
}

/// Stable short name for the durable economic receipt.
pub type CognitionReceipt = CognitionReceiptV1;

impl CognitionReceiptV1 {
    pub fn recompute_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("cognition receipt is serializable");
        value
            .as_object_mut()
            .expect("cognition receipt is an object")
            .remove("receipt_digest");
        economy_digest(COGNITION_ECONOMY_RECEIPT_DOMAIN, &value)
    }

    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        let legacy_operation = self.operation.is_empty();
        if self.schema_version != COGNITION_RECEIPT_SCHEMA_VERSION
            || !bounded_identity(&self.receipt_id)
            || !valid_digest(&self.receipt_id)
            || !valid_digest(&self.receipt_digest)
            || !bounded_identity(&self.lease_id)
            || !bounded_identity(&self.idempotency_key)
            || !bounded_identity(&self.account_id)
            || !bounded_identity(&self.agent_id)
            || !bounded_identity(&self.agent_session_id)
            || !bounded_identity(&self.agent_turn_id)
            || !bounded_identity(&self.decision_request_id)
            || !bounded_identity(&self.request_digest)
            || self.quote.payer_id != self.account_id
            || self.reserved_amount == 0
            || (!self.operation.is_empty()
                && !matches!(
                    self.operation.as_str(),
                    "reserve" | "settle" | "release" | "expire" | "refund"
                ))
            || self.consumed_amount > self.reserved_amount
            || self.released_amount > self.reserved_amount
            || self.refunded_amount > self.reserved_amount
            || (self.status == CognitionLeaseStatusV1::Reserved && self.operation != "reserve")
            || (self.status == CognitionLeaseStatusV1::Settled
                && self.consumed_amount.checked_add(self.refunded_amount)
                    != Some(self.reserved_amount))
            || (self.status == CognitionLeaseStatusV1::Released
                && ((!legacy_operation
                    && (self.consumed_amount != 0
                        || self.released_amount != self.reserved_amount
                        || self.refunded_amount != 0
                        || self.net_amount != 0))
                    || (legacy_operation
                        && (self.consumed_amount != 0
                            || self.released_amount != 0
                            || self.refunded_amount != self.reserved_amount
                            || self.net_amount != 0))))
            || (self.status == CognitionLeaseStatusV1::Expired
                && (self.consumed_amount != 0
                    || self.released_amount != self.reserved_amount
                    || self.refunded_amount != 0
                    || self.net_amount != 0))
            || (self.status == CognitionLeaseStatusV1::Refunded
                && (!legacy_operation
                    && (self.consumed_amount == 0
                        || self.released_amount != 0
                        || self.refunded_amount == 0
                        || self.refunded_amount > self.consumed_amount
                        || self.consumed_amount.checked_sub(self.refunded_amount)
                            != Some(self.net_amount))))
            || (self.operation == "reserve"
                && (self.status != CognitionLeaseStatusV1::Reserved
                    || self.consumed_amount != 0
                    || self.released_amount != 0
                    || self.refunded_amount != 0
                    || self.net_amount != 0
                    || self.parent_receipt_id.is_some()
                    || self.reason.is_some()))
            || (self.operation == "settle"
                && (self.status != CognitionLeaseStatusV1::Settled
                    || self.consumed_amount == 0
                    || self.released_amount != 0
                    || self.net_amount != self.consumed_amount
                    || self.parent_receipt_id.is_some()
                    || self.reason.is_some()))
            || (matches!(self.operation.as_str(), "release" | "expire")
                && (self.status
                    != if self.operation == "release" {
                        CognitionLeaseStatusV1::Released
                    } else {
                        CognitionLeaseStatusV1::Expired
                    }
                    || self.consumed_amount != 0
                    || self.released_amount != self.reserved_amount
                    || self.refunded_amount != 0
                    || self.net_amount != 0
                    || self.parent_receipt_id.is_some()))
            || (self.operation == "refund"
                && (self.status != CognitionLeaseStatusV1::Refunded
                    || self.consumed_amount == 0
                    || self.released_amount != 0
                    || self.refunded_amount == 0
                    || self.refunded_amount > self.consumed_amount
                    || self.consumed_amount.checked_sub(self.refunded_amount)
                        != Some(self.net_amount)
                    || self
                        .parent_receipt_id
                        .as_deref()
                        .is_none_or(|id| !valid_digest(id))
                    || self
                        .reason
                        .as_deref()
                        .is_none_or(|reason| !bounded_identity(reason))))
            || self.receipt_digest != self.recompute_digest()
        {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_receipt_invalid",
            ));
        }
        self.quote.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CognitionResourceBalanceV1 {
    pub available: u64,
    pub reserved: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionEconomyIdempotencyRecordV1 {
    pub request_fingerprint: String,
    pub lease_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionEconomyOperationRecordV1 {
    pub operation_key: String,
    pub operation_digest: String,
    pub lease_id: String,
    pub receipt_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionEconomyEventV1 {
    pub schema_version: String,
    pub journal_seq: u64,
    pub parent_event_digest: String,
    pub event_kind: String,
    pub lease_id: String,
    pub operation_key: String,
    pub idempotency_key: String,
    pub resource: String,
    pub reserved_amount: u64,
    pub consumed_amount: u64,
    #[serde(default)]
    pub released_amount: u64,
    pub refunded_amount: u64,
    #[serde(default)]
    pub net_amount: u64,
    #[serde(default)]
    pub parent_receipt_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    pub status: CognitionLeaseStatusV1,
    pub receipt_id: String,
    pub event_digest: String,
}

impl CognitionEconomyEventV1 {
    fn recompute_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("economy event is serializable");
        value
            .as_object_mut()
            .expect("economy event is an object")
            .remove("event_digest");
        economy_digest(COGNITION_ECONOMY_EVENT_DOMAIN, &value)
    }
}

/// The complete durable cognition economy projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionEconomyStateV1 {
    pub schema_version: String,
    #[serde(default)]
    pub balances: BTreeMap<String, BTreeMap<String, CognitionResourceBalanceV1>>,
    /// Versioned balances for Runtime-authorized provisioning bindings. The
    /// legacy `balances` map remains readable for snapshots written before
    /// authority-bound provisioning existed.
    #[serde(default)]
    pub provisioned_balances: BTreeMap<String, BTreeMap<String, CognitionResourceBalanceV1>>,
    /// Durable location of each lease's reserved amount. This keeps terminal
    /// transitions on the generation/reorg balance that admitted the lease.
    #[serde(default)]
    pub lease_binding_keys: BTreeMap<String, String>,
    /// Immutable one-time allowances installed by Runtime authority before a
    /// provider turn.  The map key is the authority-supplied provision id.
    #[serde(default)]
    pub provisions: BTreeMap<String, CognitionProvisioningRecordV1>,
    #[serde(default)]
    pub provision_receipts: BTreeMap<String, CognitionProvisioningReceiptV1>,
    #[serde(default)]
    pub provision_journal: Vec<CognitionProvisioningEventV1>,
    #[serde(default)]
    pub provision_head_seq: u64,
    #[serde(default = "default_provisioning_head_digest")]
    pub provision_head_digest: String,
    #[serde(default)]
    pub leases: BTreeMap<String, CognitionLeaseV1>,
    #[serde(default)]
    pub receipts: BTreeMap<String, CognitionReceiptV1>,
    #[serde(default)]
    pub idempotency: BTreeMap<String, CognitionEconomyIdempotencyRecordV1>,
    #[serde(default)]
    pub operations: BTreeMap<String, CognitionEconomyOperationRecordV1>,
    #[serde(default)]
    pub journal: Vec<CognitionEconomyEventV1>,
    #[serde(default)]
    pub head_seq: u64,
    #[serde(default)]
    pub head_digest: String,
}

pub type CognitionEconomyV1 = CognitionEconomyStateV1;

impl Default for CognitionEconomyStateV1 {
    fn default() -> Self {
        Self {
            schema_version: COGNITION_ECONOMY_SCHEMA_VERSION.to_string(),
            balances: BTreeMap::new(),
            provisioned_balances: BTreeMap::new(),
            lease_binding_keys: BTreeMap::new(),
            provisions: BTreeMap::new(),
            provision_receipts: BTreeMap::new(),
            provision_journal: Vec::new(),
            provision_head_seq: 0,
            provision_head_digest: default_provisioning_head_digest(),
            leases: BTreeMap::new(),
            receipts: BTreeMap::new(),
            idempotency: BTreeMap::new(),
            operations: BTreeMap::new(),
            journal: Vec::new(),
            head_seq: 0,
            head_digest: economy_digest(
                COGNITION_ECONOMY_JOURNAL_DOMAIN,
                &(0_u64, Vec::<CognitionEconomyEventV1>::new()),
            ),
        }
    }
}

impl CognitionEconomyStateV1 {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a validated typed projection from persisted cognition JSON.
    pub fn from_snapshot_json(value: JsonValue) -> Result<Self, CognitionEconomyError> {
        let state: Self = serde_json::from_value(value).map_err(|_| {
            CognitionEconomyError::InvalidState("cognition_economy_projection_invalid")
        })?;
        state.validate()?;
        Ok(state)
    }

    pub fn snapshot_json(&self) -> Result<JsonValue, CognitionEconomyError> {
        self.validate()?;
        serde_json::to_value(self).map_err(|_| {
            CognitionEconomyError::InvalidState("cognition_economy_projection_serialize_failed")
        })
    }

    /// Explicit bootstrap/credit operation.  It never refills implicitly and
    /// cannot overwrite an account while an active reservation exists.
    pub fn set_resource_balance(
        &mut self,
        account_id: impl Into<String>,
        resource: impl Into<String>,
        available: u64,
    ) -> Result<(), CognitionEconomyError> {
        let mut next = self.clone();
        next.set_resource_balance_inner(account_id.into(), resource.into(), available)?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn reserve(
        &mut self,
        request: CognitionLeaseRequestV1,
        tick: u64,
    ) -> Result<CognitionLeaseV1, CognitionEconomyError> {
        request.validate()?;
        let mut next = self.clone();
        let lease = next.reserve_inner(request, tick, None)?;
        next.validate()?;
        *self = next;
        Ok(lease)
    }

    /// Reserve against one exact Runtime provisioning binding. The binding is
    /// supplied by `World`, which derives it from the current capability
    /// identity and runtime world/reorg tuple.
    pub(crate) fn reserve_for_binding(
        &mut self,
        request: CognitionLeaseRequestV1,
        binding_key: impl Into<String>,
        tick: u64,
    ) -> Result<CognitionLeaseV1, CognitionEconomyError> {
        request.validate()?;
        let mut next = self.clone();
        let binding_key = binding_key.into();
        let lease = next.reserve_inner(request, tick, Some(binding_key))?;
        next.validate()?;
        *self = next;
        Ok(lease)
    }

    pub fn settle(
        &mut self,
        lease_id: &str,
        consumed_amount: u64,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let mut next = self.clone();
        let receipt = next.settle_inner(lease_id, consumed_amount, tick)?;
        next.validate()?;
        *self = next;
        Ok(receipt)
    }

    pub fn release(
        &mut self,
        lease_id: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let mut next = self.clone();
        let receipt = next.release_inner(lease_id, tick)?;
        next.validate()?;
        *self = next;
        Ok(receipt)
    }

    pub fn refund(
        &mut self,
        lease_id: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let refund_key = operation_key(lease_id, "refund");
        if let Some(existing) = self.operations.get(&refund_key) {
            return self.receipts.get(&existing.receipt_id).cloned().ok_or(
                CognitionEconomyError::InvalidState("cognition_operation_receipt_missing"),
            );
        }
        let parent_receipt_id = self
            .leases
            .get(lease_id)
            .and_then(|lease| lease.receipt_id.as_deref())
            .filter(|receipt_id| {
                self.receipts
                    .get(*receipt_id)
                    .is_some_and(|receipt| receipt.operation == "settle")
            })
            .or_else(|| {
                self.operations
                    .get(&operation_key(lease_id, "settle"))
                    .map(|record| record.receipt_id.as_str())
            })
            .unwrap_or_default()
            .to_string();
        let amount = self
            .leases
            .get(lease_id)
            .map(|lease| lease.settled_amount)
            .unwrap_or_default();
        self.refund_settled(
            lease_id,
            amount,
            parent_receipt_id.as_str(),
            "runtime_compatibility_refund",
            tick,
        )
    }

    /// Runtime-authorized compensating refund for a settled lease. The parent
    /// settlement receipt and reason are immutable audit inputs; the amount
    /// cannot exceed the settled usage and the operation is terminal.
    pub fn refund_settled(
        &mut self,
        lease_id: &str,
        refunded_amount: u64,
        parent_receipt_id: &str,
        reason: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let mut next = self.clone();
        let receipt =
            next.refund_inner(lease_id, refunded_amount, parent_receipt_id, reason, tick)?;
        next.validate()?;
        *self = next;
        Ok(receipt)
    }

    pub fn expire(
        &mut self,
        lease_id: &str,
        tick: u64,
    ) -> Result<CognitionReceiptV1, CognitionEconomyError> {
        let mut next = self.clone();
        let receipt = next.expire_inner(lease_id, tick)?;
        next.validate()?;
        *self = next;
        Ok(receipt)
    }

    fn set_resource_balance_inner(
        &mut self,
        account_id: String,
        resource: String,
        available: u64,
    ) -> Result<(), CognitionEconomyError> {
        if !bounded_identity(&account_id) || !bounded_identity(&resource) {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_balance_identity_invalid",
            ));
        }
        if self.reserved_balance(account_id.as_str(), resource.as_str()) != 0 {
            return Err(CognitionEconomyError::InvalidState(
                "cognition_balance_has_active_reservation",
            ));
        }
        if self.provisions.values().any(|provision| {
            provision.request.account_id == account_id && provision.request.resource == resource
        }) {
            return Err(CognitionEconomyError::Conflict(
                "cognition_provisioning_balance_immutable",
            ));
        }
        self.balances.entry(account_id).or_default().insert(
            resource,
            CognitionResourceBalanceV1 {
                available,
                reserved: 0,
            },
        );
        Ok(())
    }

    fn reserve_inner(
        &mut self,
        request: CognitionLeaseRequestV1,
        tick: u64,
        binding_key: Option<String>,
    ) -> Result<CognitionLeaseV1, CognitionEconomyError> {
        let fingerprint = request.fingerprint_digest();
        if let Some(existing) = self.idempotency.get(&request.idempotency_key) {
            if existing.request_fingerprint != fingerprint {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_idempotency_conflict",
                ));
            }
            if let Some(binding_key) = binding_key.as_deref()
                && self
                    .lease_binding_keys
                    .get(&existing.lease_id)
                    .is_some_and(|existing_binding| existing_binding != binding_key)
            {
                return Err(CognitionEconomyError::Conflict(
                    "cognition_provisioning_binding_conflict",
                ));
            }
            return self.leases.get(&existing.lease_id).cloned().ok_or(
                CognitionEconomyError::InvalidState("cognition_idempotency_lease_missing"),
            );
        }
        if request
            .quote
            .valid_until_tick
            .is_some_and(|expires| tick > expires)
        {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_quote_expired",
            ));
        }
        if let Some(binding_key) = binding_key.as_deref() {
            self.migrate_legacy_balance_for_binding(binding_key)?;
        }
        let lease_id = request.derived_lease_id();
        if let Some(binding_key) = binding_key.as_deref() {
            let _ = self.provisioned_balance_mut(
                binding_key,
                request.account_id.as_str(),
                request.quote.resource.as_str(),
            )?;
        }
        let balance = if let Some(binding_key) = binding_key.as_deref() {
            self.provisioned_balances
                .get_mut(binding_key)
                .and_then(|resources| resources.get_mut(&request.quote.resource))
                .expect("validated provisioning balance exists")
        } else {
            self.balance_mut(&request.account_id, &request.quote.resource)
        };
        if balance.available < request.quote.amount {
            return Err(CognitionEconomyError::InsufficientBalance {
                account_id: request.account_id,
                resource: request.quote.resource,
                requested: request.quote.amount,
                available: balance.available,
            });
        }
        balance.available -= request.quote.amount;
        balance.reserved = balance.reserved.checked_add(request.quote.amount).ok_or(
            CognitionEconomyError::InvalidState("cognition_balance_reservation_overflow"),
        )?;
        let lease = CognitionLeaseV1 {
            schema_version: COGNITION_LEASE_SCHEMA_VERSION.to_string(),
            lease_id: lease_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
            account_id: request.account_id.clone(),
            agent_id: request.agent_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            request_digest: request.request_digest.clone(),
            quote: request.quote.clone(),
            reserved_amount: request.quote.amount,
            settled_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            compensated_amount: 0,
            net_amount: 0,
            status: CognitionLeaseStatusV1::Reserved,
            reserved_at_tick: tick,
            closed_at_tick: None,
            receipt_id: None,
        };
        self.leases.insert(lease_id.clone(), lease.clone());
        if let Some(binding_key) = binding_key {
            self.lease_binding_keys
                .insert(lease_id.clone(), binding_key);
        }
        self.idempotency.insert(
            request.idempotency_key.clone(),
            CognitionEconomyIdempotencyRecordV1 {
                request_fingerprint: fingerprint,
                lease_id: lease_id.clone(),
            },
        );
        let reserve_operation_key = operation_key(&lease_id, "reserve");
        let reserve_receipt_id = economy_digest(
            COGNITION_ECONOMY_RECEIPT_ID_DOMAIN,
            &(lease.lease_id.as_str(), reserve_operation_key.as_str()),
        );
        let mut reserve_receipt = CognitionReceiptV1 {
            schema_version: COGNITION_RECEIPT_SCHEMA_VERSION.to_string(),
            receipt_id: reserve_receipt_id.clone(),
            receipt_digest: String::new(),
            lease_id: lease.lease_id.clone(),
            idempotency_key: lease.idempotency_key.clone(),
            account_id: lease.account_id.clone(),
            agent_id: lease.agent_id.clone(),
            agent_session_id: lease.agent_session_id.clone(),
            agent_turn_id: lease.agent_turn_id.clone(),
            decision_request_id: lease.decision_request_id.clone(),
            request_digest: lease.request_digest.clone(),
            quote: lease.quote.clone(),
            reserved_amount: lease.reserved_amount,
            operation: "reserve".to_string(),
            consumed_amount: 0,
            released_amount: 0,
            refunded_amount: 0,
            net_amount: 0,
            parent_receipt_id: None,
            reason: None,
            status: CognitionLeaseStatusV1::Reserved,
            issued_at_tick: tick,
        };
        reserve_receipt.receipt_digest = reserve_receipt.recompute_digest();
        reserve_receipt.validate()?;
        let mut lease = lease;
        lease.receipt_id = Some(reserve_receipt_id.clone());
        self.leases.insert(lease_id.clone(), lease.clone());
        self.receipts
            .insert(reserve_receipt_id.clone(), reserve_receipt);
        self.operations.insert(
            reserve_operation_key.clone(),
            CognitionEconomyOperationRecordV1 {
                operation_key: reserve_operation_key.clone(),
                operation_digest: economy_digest(
                    COGNITION_ECONOMY_OPERATION_DOMAIN,
                    &reserve_operation_key,
                ),
                lease_id: lease_id.clone(),
                receipt_id: reserve_receipt_id.clone(),
            },
        );
        self.append_event(
            "reserve",
            &lease,
            &reserve_operation_key,
            &reserve_receipt_id,
            0,
            0,
            0,
            0,
            None,
            None,
        )?;
        Ok(lease)
    }
}

fn operation_key(lease_id: &str, operation: &str) -> String {
    economy_digest(COGNITION_ECONOMY_OPERATION_DOMAIN, &(lease_id, operation))
}
