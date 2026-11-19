//! Runtime-owned durable continuation lifecycle and transitions.
use super::cognition::is_canonical_identifier;
use super::cognition_scheduler::SchedulerWakeV1;
pub use oasis7_agent_api::legacy_scheduler::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
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
    pub(crate) fn validate_identity_for_schema_v1(
        &self,
        schema: &str,
    ) -> Result<(), WakeConditionError> {
        let bounded = |value: &str| is_canonical_identifier(value, MAX_ID_BYTES);
        let terminal = matches!(
            self.status,
            ContinuationStatusV1::Completed
                | ContinuationStatusV1::Cancelled
                | ContinuationStatusV1::Invalidated
                | ContinuationStatusV1::Expired
                | ContinuationStatusV1::Rejected
        );
        if self.schema_version != schema
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
        Ok(())
    }

    pub fn validate_authoritative(&self) -> Result<(), WakeConditionError> {
        match self
            .validate_authoritative_with_work_v1(&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
        {
            Ok(result) => result,
            Err(_) => unreachable!("legacy continuation work cannot fail"),
        }
    }
    pub fn validate_authoritative_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<Result<(), WakeConditionError>, oasis7_wasm_abi::CanonicalCborStreamError> {
        source.charge_work(1, 0)?;
        let identity_bytes = [
            self.schema_version.as_str(),
            self.continuation_id.as_str(),
            self.wake_id.as_str(),
            self.world_id.as_str(),
            self.branch_id.as_str(),
            self.finality_status.as_str(),
            self.runtime_manifest_hash.as_str(),
            self.agent_id.as_str(),
            self.agent_session_id.as_str(),
            self.agent_turn_id.as_str(),
            self.decision_request_id.as_str(),
            self.origin_turn_id.as_str(),
            self.origin_request_digest.as_str(),
            self.continuation_proposal_id.as_str(),
            self.proposal_digest.as_str(),
            self.precondition_digest.as_str(),
            self.remaining_budget.unit.as_str(),
        ]
        .into_iter()
        .chain(
            [
                self.finality_block_hash.as_deref(),
                self.action_or_envelope_digest.as_deref(),
                self.terminal_disposition.as_deref(),
            ]
            .into_iter()
            .flatten(),
        );
        for text in identity_bytes {
            for part in text.as_bytes().chunks(8192) {
                source.charge_work(1 + 4 * part.len() as u64, part.len() as u64)?;
            }
        }
        if let Err(error) = self.validate_identity_for_schema_v1(CONTINUATION_SCHEMA) {
            return Ok(Err(error));
        }
        let terminal = matches!(
            self.status,
            ContinuationStatusV1::Completed
                | ContinuationStatusV1::Cancelled
                | ContinuationStatusV1::Invalidated
                | ContinuationStatusV1::Expired
                | ContinuationStatusV1::Rejected
        );
        if let Err(error) =
            WakeConditionValidator::validate_with_work_v1(self.wake_conditions.as_slice(), source)?
        {
            return Ok(Err(error));
        }
        if (terminal && self.terminal_disposition.is_none())
            || (!terminal && self.terminal_disposition.is_some())
        {
            return Ok(Err(WakeConditionError::new("recovery_pending")));
        }
        let digest = self.status_digest_with_work_v1(source)?;
        source.charge_work(
            1,
            self.continuation_status_digest
                .as_ref()
                .map_or(0, String::len) as u64
                + digest.as_str().len() as u64,
        )?;
        if self.continuation_status_digest.as_deref() != Some(digest.as_str()) {
            return Ok(Err(WakeConditionError::new("recovery_pending")));
        }
        Ok(Ok(()))
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

    /// Borrow the unique original v1 status projection. This is DATA only;
    /// callers cannot replace any field or a validation predicate.
    pub fn status_digest_input_v1(&self) -> impl Serialize + '_ {
        ContinuationStatusDigestInput {
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
        }
    }
    pub fn status_digest_with_work_v1<A: oasis7_wasm_abi::CapabilityCanonicalWorkV1>(
        &self,
        source: &A,
    ) -> Result<
        oasis7_agent_api::world_cognition::PaidCognitionDigestDataV1<A::Charge>,
        oasis7_wasm_abi::CanonicalCborStreamError,
    > {
        oasis7_agent_api::world_cognition::cognition_digest_with_work_v1(
            CONTINUATION_STATUS_DIGEST_DOMAIN,
            &self.status_digest_input_v1(),
            source,
        )
    }
    pub fn status_digest(&self) -> String {
        h_v1(
            CONTINUATION_STATUS_DIGEST_DOMAIN,
            &self.status_digest_input_v1(),
        )
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

#[path = "cognition_iws_continuation_v2.rs"]
mod iws_v2;
pub use iws_v2::*;
