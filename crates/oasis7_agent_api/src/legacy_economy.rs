//! Pure legacy lease request and immutable quote contract. No accounting authority.
use serde::{Deserialize, Serialize};
use std::fmt;
pub const COGNITION_LEASE_SCHEMA_VERSION: &str = "cognition-lease.v1";
pub const COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION: &str = "fixed_unit_experimental";
pub const COGNITION_RESOURCE_VERSION_V1: &str = "cognition_units.v1";
const COGNITION_DEFAULT_PURPOSE: &str = "provider_cognition";
const COGNITION_DEFAULT_SCOPE: &str = "agent_turn";
const COGNITION_LEGACY_AUTHORITY_CONTEXT: &str = "legacy-unbound";
const COGNITION_LEGACY_WORLD_BINDING: &str = "legacy-unbound";
const COGNITION_ECONOMY_QUOTE_DOMAIN: &str = "oasis7.cognition.economy.quote.v1";
const COGNITION_ECONOMY_REQUEST_DOMAIN: &str = "oasis7.cognition.economy.request.v1";
const COGNITION_ECONOMY_LEASE_ID_DOMAIN: &str = "oasis7.cognition.economy.lease-id.v1";
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
    crate::legacy_scheduler::digest(domain, value)
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
        self.recompute_digest_with_work_v1(&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
            .expect("lease quote is serializable")
            .text
    }

    pub fn refresh_digest(&mut self) {
        self.quote_digest = self.recompute_digest();
    }

    pub fn validate(&self) -> Result<(), CognitionEconomyError> {
        self.validate_with_work_v1(&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
    }
    pub fn validate_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<(), CognitionEconomyError> {
        for text in [
            &self.schema_version,
            &self.quote_id,
            &self.resource,
            &self.payer_id,
            &self.resource_version,
            &self.purpose,
            &self.scope,
            &self.policy_revision,
            &self.authority_context,
            &self.world_binding,
            &self.quote_digest,
        ] {
            paid::work(source, text.len())?;
        }

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
            || self.quote_digest != self.recompute_digest_with_work_v1(source)?.as_str()
        {
            return Err(CognitionEconomyError::InvalidInput(
                "cognition_quote_invalid",
            ));
        }
        Ok(())
    }
    pub fn recompute_digest_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<PaidLeaseDigestDataV1<A::Charge>, CognitionEconomyError> {
        let _phase = paid::phase(self, source)?;
        let mut value = serde_json::to_value(self).expect("lease quote is serializable");
        value
            .as_object_mut()
            .expect("lease quote is an object")
            .remove("quote_digest");
        paid::digest(COGNITION_ECONOMY_QUOTE_DOMAIN, &value, source)
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
    pub fn new_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        idempotency_key: String,
        account_id: String,
        agent_id: String,
        agent_session_id: String,
        agent_turn_id: String,
        decision_request_id: String,
        request_digest: String,
        mut quote: CognitionLeaseQuoteV1,
        source: &A,
    ) -> Result<Self, CognitionEconomyError> {
        paid::work(source, COGNITION_LEASE_SCHEMA_VERSION.len())?;
        normalize_request_quote_with_work_v1(&mut quote, &account_id, source)?;
        Ok(Self {
            schema_version: COGNITION_LEASE_SCHEMA_VERSION.into(),
            idempotency_key,
            account_id,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
            quote,
        })
    }

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
        normalize_request_quote_with_work_v1(
            &mut quote,
            &account_id,
            &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
        )
        .expect("lease quote is serializable");
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
        self.validate_with_work_v1(&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
    }
    pub fn validate_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<(), CognitionEconomyError> {
        for text in [
            &self.schema_version,
            &self.idempotency_key,
            &self.account_id,
            &self.agent_id,
            &self.agent_session_id,
            &self.agent_turn_id,
            &self.decision_request_id,
            &self.request_digest,
            &self.quote.payer_id,
        ] {
            paid::work(source, text.len())?;
        }

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
        self.quote.validate_with_work_v1(source)
    }

    /// Original fingerprint preimage, shared with metered data consumers.
    /// This readonly tuple is DATA and establishes no lease admission.
    pub fn fingerprint_input_v1(&self) -> (&'static str, impl Serialize + '_) {
        (
            COGNITION_ECONOMY_REQUEST_DOMAIN,
            (
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
    pub fn fingerprint_digest(&self) -> String {
        let (domain, input) = self.fingerprint_input_v1();
        economy_digest(domain, &input)
    }

    pub fn fingerprint_digest_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<PaidLeaseDigestDataV1<A::Charge>, CognitionEconomyError> {
        let (domain, input) = self.fingerprint_input_v1();
        paid::digest(domain, &input, source)
    }
    pub fn derived_lease_id_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<PaidLeaseDigestDataV1<A::Charge>, CognitionEconomyError> {
        let fingerprint = self.fingerprint_digest_with_work_v1(source)?;
        paid::digest(
            COGNITION_ECONOMY_LEASE_ID_DOMAIN,
            &(self.idempotency_key.as_str(), fingerprint.as_str()),
            source,
        )
    }
    pub fn derived_lease_id(&self) -> String {
        economy_digest(
            COGNITION_ECONOMY_LEASE_ID_DOMAIN,
            &(self.idempotency_key.as_str(), self.fingerprint_digest()),
        )
    }
}

#[path = "legacy_economy_paid.rs"]
mod paid;
pub use paid::PaidLeaseDigestDataV1;

pub fn admit_lease_data_with_work_v1<
    T: Serialize + ?Sized,
    A: oasis7_wasm_abi::CapabilityCanonicalWorkV1,
>(
    value: &T,
    source: &A,
) -> Result<Option<A::Charge>, CognitionEconomyError> {
    paid::phase(value, source)
}

fn normalize_request_quote_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
    quote: &mut CognitionLeaseQuoteV1,
    account_id: &str,
    source: &A,
) -> Result<(), CognitionEconomyError> {
    if quote.payer_id.is_empty() {
        paid::work(source, account_id.len())?;
        quote.payer_id = account_id.to_owned();
        let digest = quote.recompute_digest_with_work_v1(source)?;
        paid::work(source, 71)?;
        quote.quote_digest = digest.as_str().to_owned();
    }
    Ok(())
}
