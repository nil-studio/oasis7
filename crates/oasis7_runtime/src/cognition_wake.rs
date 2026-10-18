//! Runtime-owned durable continuation lifecycle and transitions.
use serde::{Deserialize, Serialize};
use serde_json::json;
use super::cognition::is_canonical_identifier;
use super::cognition_scheduler::SchedulerWakeV1;
pub use oasis7_agent_api::legacy_scheduler::*;
const MAX_ID_BYTES: usize = 128;
const CONTINUATION_SCHEMA: &str = "agent-continuation.v1";
const CONTINUATION_STATUS_DIGEST_DOMAIN: &str = "oasis7.cognition.continuation-status.v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionBudgetConsumptionV1 {
    pub continuation_id: String,
    pub wake_id: String,
    pub consumed: u64,
    pub remaining_budget: ContinuationBudgetV1,
    pub status: ContinuationStatusV1,
    pub continuation_status_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionWakeHandoffResultV1 {
    pub wake: SchedulerWakeV1,
    pub continuation: AgentContinuation,
    #[serde(default)]
    pub replanned_continuation: Option<AgentContinuation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentContinuation {
    pub schema_version: String,
    pub continuation_id: String,
    pub wake_id: String,
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default)]
    pub finality_block_hash: Option<String>,
    pub finality_status: String,
    pub reorg_epoch: u64,
    pub runtime_manifest_hash: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub origin_turn_id: String,
    pub origin_request_digest: String,
    pub continuation_proposal_id: String,
    pub proposal_digest: String,
    #[serde(default)]
    pub action_or_envelope_digest: Option<String>,
    pub wake_conditions: Vec<WakeConditionV1>,
    #[serde(default)]
    pub next_wake_tick: Option<u64>,
    pub remaining_budget: ContinuationBudgetV1,
    #[serde(default)]
    pub valid_until_tick: Option<u64>,
    pub precondition_digest: String,
    pub wake_seq: u64,
    /// The committed logical tick at which this status projection was
    /// produced.  It is Runtime-owned and participates in the status digest.
    #[serde(default)]
    pub logical_tick: u64,
    pub status: ContinuationStatusV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation_status_digest: Option<String>,
    #[serde(default)]
    pub terminal_disposition: Option<String>,
}

impl AgentContinuation {
    pub fn refresh_status_digest(&mut self) {
        self.continuation_status_digest = Some(self.status_digest());
    }

    /// Validate a complete Runtime-owned continuation projection.  The
    /// legacy persisted shape may omit `continuation_status_digest`, but a
    /// projection crossing into the simulator must carry the Runtime-issued
    /// digest and match the canonical status fields exactly.
    pub fn validate_authoritative(&self) -> Result<(), WakeConditionError> {
        let bounded = |value: &str| is_canonical_identifier(value, MAX_ID_BYTES);
        let terminal = matches!(
            self.status,
            ContinuationStatusV1::Completed
                | ContinuationStatusV1::Cancelled
                | ContinuationStatusV1::Invalidated
                | ContinuationStatusV1::Expired
                | ContinuationStatusV1::Rejected
        );
        if self.schema_version != CONTINUATION_SCHEMA
            || !bounded(&self.continuation_id)
            || !bounded(&self.wake_id)
            || !bounded(&self.world_id)
            || !bounded(&self.branch_id)
            || !bounded(&self.finality_status)
            || !bounded(&self.runtime_manifest_hash)
            || !bounded(&self.agent_id)
            || !bounded(&self.agent_session_id)
            || !bounded(&self.agent_turn_id)
            || !bounded(&self.decision_request_id)
            || !bounded(&self.origin_turn_id)
            || !bounded(&self.origin_request_digest)
            || !bounded(&self.continuation_proposal_id)
            || !bounded(&self.proposal_digest)
            || !bounded(&self.precondition_digest)
            || self
                .finality_block_hash
                .as_deref()
                .is_some_and(|value| !bounded(value))
            || self
                .action_or_envelope_digest
                .as_deref()
                .is_some_and(|value| !bounded(value))
            || self
                .terminal_disposition
                .as_deref()
                .is_some_and(|value| !bounded(value))
            || (self.remaining_budget.value == 0 && !terminal)
            || !matches!(self.remaining_budget.unit.as_str(), "steps" | "ticks")
            || !crate::cognition::finality_binding_is_legal(
                &self.finality_status,
                self.finality_block_hash.as_deref(),
            )
        {
            return Err(WakeConditionError::new("recovery_pending"));
        }
        WakeConditionValidator::validate(self.wake_conditions.as_slice())?;
        if (terminal && self.terminal_disposition.is_none())
            || (!terminal && self.terminal_disposition.is_some())
            || self.continuation_status_digest.as_deref() != Some(self.status_digest().as_str())
        {
            return Err(WakeConditionError::new("recovery_pending"));
        }
        Ok(())
    }

    /// Derive the Runtime-owned continuation context identity.  Keeping this
    /// method on the durable projection prevents adapters from introducing a
    /// competing local digest algorithm.
    pub fn continuation_digest(&self) -> String {
        h_v1(
            "oasis7.cognition.continuation-context.v1",
            &json!({
                "continuation_proposal_id": self.continuation_proposal_id,
                "proposal_digest": self.proposal_digest,
                "continuation_id": self.continuation_id,
                "wake_id": self.wake_id,
                "wake_seq": self.wake_seq,
                "world_id": self.world_id,
                "branch_id": self.branch_id,
                "finality_epoch": self.finality_epoch,
                "finality_block_hash": self.finality_block_hash,
                "finality_status": self.finality_status,
                "reorg_epoch": self.reorg_epoch,
                "runtime_manifest_hash": self.runtime_manifest_hash,
                "remaining_budget": self.remaining_budget,
                "valid_until_tick": self.valid_until_tick,
                "status": self.status,
                "continuation_status_digest": self.continuation_status_digest,
                "terminal_disposition": self.terminal_disposition,
            }),
        )
    }

    pub fn status_digest(&self) -> String {
        let payload = ContinuationStatusDigestInput {
            continuation_id: &self.continuation_id,
            wake_id: &self.wake_id,
            wake_seq: self.wake_seq,
            from_status: None,
            to_status: self.status,
            logical_tick: self.logical_tick,
            world_id: &self.world_id,
            branch_id: &self.branch_id,
            finality_epoch: self.finality_epoch,
            finality_block_hash: self.finality_block_hash.as_deref(),
            finality_status: &self.finality_status,
            reorg_epoch: self.reorg_epoch,
            proposal_digest: &self.proposal_digest,
            terminal_disposition: self.terminal_disposition.as_deref(),
        };
        h_v1(CONTINUATION_STATUS_DIGEST_DOMAIN, &payload)
    }
}

#[derive(Debug, Serialize)]
struct ContinuationStatusDigestInput<'a> {
    continuation_id: &'a str,
    wake_id: &'a str,
    wake_seq: u64,
    from_status: Option<ContinuationStatusV1>,
    to_status: ContinuationStatusV1,
    logical_tick: u64,
    world_id: &'a str,
    branch_id: &'a str,
    finality_epoch: u64,
    finality_block_hash: Option<&'a str>,
    finality_status: &'a str,
    reorg_epoch: u64,
    proposal_digest: &'a str,
    terminal_disposition: Option<&'a str>,
}

fn h_v1<T: Serialize>(domain: &str, payload: &T) -> String {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(domain, payload))
        .expect("cognition identity payload must be canonicalizable");
    format!("blake3:{}", blake3::hash(&bytes))
}

pub struct ContinuationTransition;

impl ContinuationTransition {
    pub fn validate(
        from: ContinuationStatusV1,
        to: ContinuationStatusV1,
    ) -> Result<(), WakeConditionError> {
        let allowed = match from {
            ContinuationStatusV1::Scheduled => matches!(
                to,
                ContinuationStatusV1::Pending
                    | ContinuationStatusV1::Waking
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Pending => matches!(
                to,
                ContinuationStatusV1::Scheduled
                    | ContinuationStatusV1::Waking
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Waking => matches!(
                to,
                ContinuationStatusV1::Consumed
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Consumed => matches!(
                to,
                ContinuationStatusV1::Scheduled
                    | ContinuationStatusV1::Completed
                    | ContinuationStatusV1::Cancelled
                    | ContinuationStatusV1::Invalidated
                    | ContinuationStatusV1::Expired
                    | ContinuationStatusV1::Rejected
            ),
            ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected => false,
        };
        if allowed {
            Ok(())
        } else {
            Err(WakeConditionError::new("recovery_pending"))
        }
    }

    pub fn apply(
        continuation: &mut AgentContinuation,
        to: ContinuationStatusV1,
    ) -> Result<(), WakeConditionError> {
        Self::validate(continuation.status, to)?;
        continuation.status = to;
        if !matches!(
            to,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
                | ContinuationStatusV1::Consumed
        ) {
            continuation.terminal_disposition = Some(
                match to {
                    ContinuationStatusV1::Cancelled => "cancelled",
                    ContinuationStatusV1::Invalidated => "reorg_invalidated",
                    ContinuationStatusV1::Expired => "expired",
                    ContinuationStatusV1::Rejected => "rejected",
                    ContinuationStatusV1::Completed => "completed",
                    _ => "",
                }
                .to_string(),
            );
        }
        continuation.refresh_status_digest();
        Ok(())
    }

    pub fn apply_at_tick(
        continuation: &mut AgentContinuation,
        to: ContinuationStatusV1,
        logical_tick: u64,
    ) -> Result<(), WakeConditionError> {
        Self::validate(continuation.status, to)?;
        continuation.logical_tick = logical_tick;
        continuation.status = to;
        if !matches!(
            to,
            ContinuationStatusV1::Scheduled
                | ContinuationStatusV1::Pending
                | ContinuationStatusV1::Waking
                | ContinuationStatusV1::Consumed
        ) {
            continuation.terminal_disposition = Some(
                match to {
                    ContinuationStatusV1::Cancelled => "cancelled",
                    ContinuationStatusV1::Invalidated => "reorg_invalidated",
                    ContinuationStatusV1::Expired => "expired",
                    ContinuationStatusV1::Rejected => "rejected",
                    ContinuationStatusV1::Completed => "completed",
                    _ => "",
                }
                .to_string(),
            );
        }
        continuation.refresh_status_digest();
        Ok(())
    }

    pub fn invalidate_for_reorg(
        continuation: &mut AgentContinuation,
        reorg_epoch: u64,
    ) -> Result<ContinuationReorgReport, WakeConditionError> {
        Self::validate(continuation.status, ContinuationStatusV1::Invalidated)?;
        continuation.reorg_epoch = reorg_epoch;
        continuation.status = ContinuationStatusV1::Invalidated;
        continuation.terminal_disposition = Some("reorg_invalidated".to_string());
        continuation.refresh_status_digest();
        Ok(ContinuationReorgReport {
            terminal_disposition: "reorg_invalidated".to_string(),
            provider_invocation_count: 0,
            effect_count: 0,
            receipt_count: 0,
        })
    }

    pub fn invalidate_for_reorg_at_tick(
        continuation: &mut AgentContinuation,
        reorg_epoch: u64,
        logical_tick: u64,
    ) -> Result<ContinuationReorgReport, WakeConditionError> {
        Self::validate(continuation.status, ContinuationStatusV1::Invalidated)?;
        continuation.reorg_epoch = reorg_epoch;
        continuation.logical_tick = logical_tick;
        continuation.status = ContinuationStatusV1::Invalidated;
        continuation.terminal_disposition = Some("reorg_invalidated".to_string());
        continuation.refresh_status_digest();
        Ok(ContinuationReorgReport {
            terminal_disposition: "reorg_invalidated".to_string(),
            provider_invocation_count: 0,
            effect_count: 0,
            receipt_count: 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContinuationReorgReport {
    pub terminal_disposition: String,
    pub provider_invocation_count: u64,
    pub effect_count: u64,
    pub receipt_count: u64,
}
