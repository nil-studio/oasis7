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
    #[serde(deserialize_with = "strict_nested")]
    pub prefix_commit: CommitRef,
    pub admitted_at_tick: u64,
    pub writer_public_key: String,
    pub delegation_generation: u64,
    #[serde(deserialize_with = "strict_nested")]
    pub current_runtime_binding: oasis7_agent_api::RuntimeBindingV1,
    #[serde(deserialize_with = "strict_nested")]
    pub current_task_fence: Option<oasis7_agent_api::game::GameTaskFenceV1>,
    #[serde(deserialize_with = "strict_nested")]
    pub read_commit: CommitRef,
}
impl TerminalRecoveryAuthorityV1 {
    /// Shape only. The SDK must additionally authenticate the complete service
    /// envelope and compare the original request, lease and current View watermark.
    pub fn validate(&self, expected: &RequestCorrelation) -> Result<(), ContractError> {
        if self.schema_version != 1 || self.role != TerminalRecoveryRoleV1::TerminalOnly {
            return Err(ContractError("unsupported terminal recovery authority"));
        }
        self.world.validate()?;
        self.prefix_commit.validate()?;
        self.read_commit.validate()?;
        expected.validate()?;
        if &self.prefix_correlation != expected
            || self.world != expected.key.world
            || self.prefix_commit.world != self.world
            || self.read_commit.world != self.world
            || self.current_runtime_binding.world_id != self.world.world_id
            || self.grant_issued_at_tick > self.admitted_at_tick
            || self.admitted_at_tick > self.grant_expires_at_tick
            || self.lease.reserved_amount != 1
            || self.lease.reserved_at_tick > self.admitted_at_tick
            || self.prefix_committed_height != self.prefix_commit.position
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
