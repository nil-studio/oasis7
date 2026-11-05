//! Ordinary DevLocal service assertion for closing one historical request only.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalRecoveryRoleV1 {
    TerminalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalLeaseIdentityV1 {
    pub lease_id: String,
    pub idempotency_key: String,
    pub account_id: String,
    pub quote_digest: String,
    pub reserved_amount: u64,
    pub reserved_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalPrefixReceiptV1 {
    pub dispatched: bool,
    pub request_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentAuthenticatedJournalV1 {
    pub prefix_event_id: u64,
    pub prefix_event_time: u64,
    pub prefix_receipt: TerminalPrefixReceiptV1,
    #[serde(deserialize_with = "strict_nested")]
    pub reserve_correlation: RequestCorrelation,
    pub reserve_full_result_digest: String,
    pub reserve_receipt_digest: String,
    pub reserve_committed_height: u64,
    pub reserve_event_id: u64,
    pub reserve_event_time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalRecoveryAuthorityV1 {
    pub schema_version: u16,
    pub role: TerminalRecoveryRoleV1,
    #[serde(deserialize_with = "strict_nested")]
    pub world: WorldIdentity,
    pub agent_id: String,
    pub owner_binding: String,
    pub identity_generation: u64,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub grant_id: String,
    pub grant_issued_at_tick: u64,
    pub grant_expires_at_tick: u64,
    pub lease: TerminalLeaseIdentityV1,
    #[serde(deserialize_with = "strict_nested")]
    pub prefix_correlation: RequestCorrelation,
    pub prefix_result_digest: String,
    pub prefix_action_id: u64,
    pub prefix_deadline_unix_ms: Option<u64>,
    pub prefix_committed_height: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "strict_nested"
    )]
    pub prefix_commit: Option<CommitRef>,
    pub admitted_at_tick: u64,
    pub writer_public_key: String,
    pub delegation_generation: u64,
    #[serde(deserialize_with = "strict_nested")]
    pub current_runtime_binding: oasis7_agent_api::RuntimeBindingV1,
    #[serde(deserialize_with = "strict_nested")]
    pub current_task_fence: Option<oasis7_agent_api::game::GameTaskFenceV1>,
    #[serde(deserialize_with = "strict_nested")]
    pub read_commit: CommitRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_authenticated_journal: Option<CurrentAuthenticatedJournalV1>,
}
impl TerminalRecoveryAuthorityV1 {
    /// Shape only. The SDK must additionally authenticate the complete service
    /// envelope and compare the original request, lease and current View watermark.
    pub fn validate(&self, expected: &RequestCorrelation) -> Result<(), ContractError> {
        if self.role != TerminalRecoveryRoleV1::TerminalOnly {
            return Err(ContractError("unsupported terminal recovery authority"));
        }
        self.world.validate()?;
        self.read_commit.validate()?;
        expected.validate()?;
        match (
            self.schema_version,
            &self.prefix_commit,
            &self.current_authenticated_journal,
        ) {
            (1, Some(commit), None) => {
                commit.validate()?;
                if commit.world != self.world || commit.position != self.prefix_committed_height {
                    return Err(ContractError("terminal historical commit mismatch"));
                }
            }
            (2, None, Some(evidence)) => {
                evidence.reserve_correlation.validate()?;
                if evidence.prefix_event_id == 0
                    || evidence.reserve_event_id == 0
                    || evidence.reserve_event_id >= evidence.prefix_event_id
                    || evidence.prefix_event_time != self.admitted_at_tick
                    || evidence.reserve_event_time > evidence.prefix_event_time
                    || evidence.reserve_event_time != self.lease.reserved_at_tick
                    || evidence.reserve_committed_height == 0
                    || self.prefix_committed_height == 0
                    || evidence.reserve_event_time < self.grant_issued_at_tick
                    || evidence.reserve_event_time > self.grant_expires_at_tick
                    || evidence.reserve_committed_height > self.prefix_committed_height
                    || self.prefix_committed_height > self.read_commit.position
                    || evidence.reserve_correlation.key.world != self.world
                    || evidence.reserve_correlation.key.operation_domain
                        != expected.key.operation_domain
                    || evidence.reserve_correlation.key.nonce_scope != expected.key.nonce_scope
                    || evidence.reserve_correlation.key.verified_subject
                        != expected.key.verified_subject
                    || !evidence.prefix_receipt.dispatched
                    || evidence.prefix_receipt.request_digest != self.request_digest
                    || self.prefix_deadline_unix_ms.is_some()
                {
                    return Err(ContractError("terminal current Journal evidence mismatch"));
                }
                required(&evidence.reserve_full_result_digest)?;
                required(&evidence.reserve_receipt_digest)?;
            }
            _ => {
                return Err(ContractError(
                    "unsupported or mixed terminal recovery evidence",
                ));
            }
        }
        if &self.prefix_correlation != expected
            || self.world != expected.key.world
            || self.read_commit.world != self.world
            || self.current_runtime_binding.world_id != self.world.world_id
            || self.grant_issued_at_tick > self.admitted_at_tick
            || self.admitted_at_tick > self.grant_expires_at_tick
            || self.lease.reserved_amount != 1
            || self.lease.reserved_at_tick > self.admitted_at_tick
            || self.transport_attempt == 0
        {
            return Err(ContractError(
                "terminal recovery identity or admission mismatch",
            ));
        }
        for value in [
            &self.agent_id,
            &self.owner_binding,
            &self.agent_session_id,
            &self.agent_turn_id,
            &self.decision_request_id,
            &self.request_digest,
            &self.grant_id,
            &self.lease.lease_id,
            &self.lease.idempotency_key,
            &self.lease.account_id,
            &self.lease.quote_digest,
            &self.prefix_result_digest,
            &self.writer_public_key,
        ] {
            required(value)?;
        }
        Ok(())
    }
}

// Strict only inside the new proof. Existing public DTO readers keep their
// compatibility rules, while every nested signed proof value must preserve
// exactly the fields emitted by its canonical typed serializer.
fn strict_nested<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Serialize,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let decoded: T = serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
    let encoded = serde_json::to_value(&decoded).map_err(serde::de::Error::custom)?;
    if encoded != value {
        return Err(serde::de::Error::custom(
            "noncanonical or unknown nested terminal proof field",
        ));
    }
    Ok(decoded)
}
