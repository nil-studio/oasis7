//! Simulator continuation policy and Runtime projection seams.

use serde::{Deserialize, Serialize};

use super::super::Observation;
use super::super::continuous_agent_harness::{CognitionError, Digest32, h_v1};
use super::GoalSnapshotV1;
use oasis7_agent_harness::ContinuationHarness as HarnessContinuationHarness;

#[cfg(not(target_arch = "wasm32"))]
use oasis7_agent_harness::{
    ContinuationHostProjection,
    ContinuationStatusProjectionV1 as HarnessContinuationStatusProjectionV1, RuntimeAuthority,
};

pub use oasis7_agent_api::{
    ContinuationBudgetV1, ContinuationProposalV1, WakeConditionSubjectV1, WakeConditionV1,
};
pub use oasis7_agent_harness::{
    ContinuationAuthorityContextV1, ContinuationBudgetProgressV1, ContinuationHandle,
    ContinuationInvalidationReason, ContinuationProjectionV1, RuntimeContinuationStatusV1,
};

fn error(code: &'static str, message: impl Into<String>) -> CognitionError {
    CognitionError::new(code, message)
}

/// Current cognition evidence for a target-lane continuation. Historical
/// digest strings alone are not current-state proof; the raw observation is
/// retained until this boundary verifies its canonical digest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContinuationCurrentContextV1 {
    pub observation: Observation,
    pub authority: ContinuationAuthorityContextV1,
}

/// Digest the state that can make a continuation stale while ignoring the
/// simulator clock. Logical ticks are the wake scheduler's trigger, not an
/// observation precondition; including the clock would make every ordinary
/// tick invalidate an otherwise unchanged wait.
pub fn continuation_observation_digest(observation: &Observation) -> String {
    let mut stable = observation.clone();
    stable.time = 0;
    h_v1("oasis7.cognition.observation.v1", &stable).to_string()
}

impl ContinuationCurrentContextV1 {
    pub fn from_observation(
        observation: Observation,
        goal: &GoalSnapshotV1,
        policy_digest: impl Into<String>,
        precondition_digest: impl Into<String>,
    ) -> Self {
        let authority = ContinuationAuthorityContextV1 {
            baseline_observation_digest: continuation_observation_digest(&observation),
            goal_digest: goal.digest.clone(),
            policy_digest: policy_digest.into(),
            precondition_digest: precondition_digest.into(),
        };
        Self {
            observation,
            authority,
        }
    }

    pub fn validate_for_agent(&self, agent_id: &str) -> Result<(), CognitionError> {
        if self.observation.agent_id != agent_id {
            return Err(error(
                "continuation_context_identity_mismatch",
                "current observation does not belong to the continuation agent",
            ));
        }
        self.authority.validate()?;
        for (name, digest) in [
            ("goal_digest", &self.authority.goal_digest),
            ("policy_digest", &self.authority.policy_digest),
            ("precondition_digest", &self.authority.precondition_digest),
        ] {
            if !Digest32::from(digest.as_str()).is_canonical_blake3() {
                return Err(error(
                    "continuation_context_digest_invalid",
                    format!("current {name} is not a canonical BLAKE3-256 digest"),
                ));
            }
        }
        let observed_digest = continuation_observation_digest(&self.observation);
        if self.authority.baseline_observation_digest != observed_digest.as_str() {
            return Err(error(
                "continuation_observation_digest_mismatch",
                "current observation does not match its authoritative digest",
            ));
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct RuntimeContinuationAuthority;

#[cfg(not(target_arch = "wasm32"))]
impl RuntimeAuthority for RuntimeContinuationAuthority {
    type Lease = ();
    type Receipt = ();
    type Continuation = crate::runtime::AgentContinuation;

    fn validate_lease(
        &self,
        _lease: &Self::Lease,
        _request: &oasis7_agent_api::ContinuousAgentRequestContextV1,
    ) -> Result<oasis7_agent_api::CognitionLeaseConsumptionViewV1, CognitionError> {
        Err(error(
            "continuation_authority_unavailable",
            "continuation adapter does not validate leases",
        ))
    }

    fn verify_receipt_readback(
        &self,
        _feedback: &oasis7_agent_api::FeedbackEnvelopeV1,
        _receipt: &Self::Receipt,
        _response_identity: &oasis7_agent_api::ResponseArtifactIdentityV1,
    ) -> Result<oasis7_agent_api::RuntimeReceiptLineageV1, CognitionError> {
        Err(error(
            "continuation_authority_unavailable",
            "continuation adapter does not verify receipt readback",
        ))
    }

    fn verify_memory_receipt(
        &self,
        _receipt: &Self::Receipt,
        _context: Option<&oasis7_agent_harness::MemoryWritePolicyContextV1>,
    ) -> Result<oasis7_agent_api::RuntimeReceiptLineageV1, CognitionError> {
        Err(error(
            "continuation_authority_unavailable",
            "continuation adapter does not verify memory receipts",
        ))
    }

    fn validated_feedback_history(
        &self,
    ) -> Result<Vec<oasis7_agent_harness::FeedbackHistoryProjection>, CognitionError> {
        Err(error(
            "continuation_authority_unavailable",
            "continuation adapter does not read feedback history",
        ))
    }

    fn continuation_projection(
        &self,
        continuation: &Self::Continuation,
    ) -> Result<ContinuationHostProjection, CognitionError> {
        continuation
            .validate_authoritative()
            .map_err(|runtime_error| {
                error(
                    "continuation_runtime_projection_invalid",
                    runtime_error.to_string(),
                )
            })?;
        let continuation_status_digest = continuation
            .continuation_status_digest
            .clone()
            .ok_or_else(|| {
                error(
                    "continuation_runtime_projection_invalid",
                    "validated Runtime continuation is missing its status digest",
                )
            })?;
        Ok(ContinuationHostProjection {
            continuation_proposal_id: continuation.continuation_proposal_id.clone(),
            proposal_digest: continuation.proposal_digest.clone(),
            world_id: continuation.world_id.clone(),
            agent_id: continuation.agent_id.clone(),
            agent_session_id: continuation.agent_session_id.clone(),
            agent_turn_id: continuation.agent_turn_id.clone(),
            decision_request_id: continuation.decision_request_id.clone(),
            origin_turn_id: continuation.origin_turn_id.clone(),
            origin_request_digest: continuation.origin_request_digest.clone(),
            action_or_envelope_digest: continuation.action_or_envelope_digest.clone(),
            precondition_digest: continuation.precondition_digest.clone(),
            valid_until_tick: continuation.valid_until_tick,
            continuation_id: continuation.continuation_id.clone(),
            wake_id: continuation.wake_id.clone(),
            wake_seq: continuation.wake_seq,
            continuation_digest: continuation.continuation_digest(),
            continuation_status_digest,
            status: match continuation.status {
                crate::runtime::ContinuationStatusV1::Scheduled => {
                    HarnessContinuationStatusProjectionV1::Scheduled
                }
                crate::runtime::ContinuationStatusV1::Pending => {
                    HarnessContinuationStatusProjectionV1::Pending
                }
                crate::runtime::ContinuationStatusV1::Waking => {
                    HarnessContinuationStatusProjectionV1::Waking
                }
                crate::runtime::ContinuationStatusV1::Consumed => {
                    HarnessContinuationStatusProjectionV1::Consumed
                }
                crate::runtime::ContinuationStatusV1::Completed => {
                    HarnessContinuationStatusProjectionV1::Completed
                }
                crate::runtime::ContinuationStatusV1::Cancelled => {
                    HarnessContinuationStatusProjectionV1::Cancelled
                }
                crate::runtime::ContinuationStatusV1::Invalidated => {
                    HarnessContinuationStatusProjectionV1::Invalidated
                }
                crate::runtime::ContinuationStatusV1::Expired => {
                    HarnessContinuationStatusProjectionV1::Expired
                }
                crate::runtime::ContinuationStatusV1::Rejected => {
                    HarnessContinuationStatusProjectionV1::Rejected
                }
            },
            terminal_disposition: continuation.terminal_disposition.clone(),
            remaining_budget: ContinuationBudgetV1 {
                unit: continuation.remaining_budget.unit.clone(),
                value: continuation.remaining_budget.value,
            },
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContinuationHarness {
    core: HarnessContinuationHarness,
}

impl ContinuationHarness {
    pub fn submit(
        &mut self,
        proposal: ContinuationProposalV1,
    ) -> Result<ContinuationHandle, CognitionError> {
        self.core.submit(proposal)
    }

    /// Production continuation admission. The old `submit` method remains a
    /// compatibility fixture lane; production callers must provide the
    /// current Runtime-derived context through this seam.
    pub fn submit_with_context(
        &mut self,
        proposal: ContinuationProposalV1,
        context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, CognitionError> {
        self.core.submit_with_context(proposal, context)
    }

    /// Consume exactly one logical wake delivery (or a bounded number of
    /// units for a tick-based contract). Duplicate delivery of the same wake
    /// is idempotent and cannot debit the chain twice.
    pub fn consume_wake(
        &mut self,
        handle: &mut ContinuationHandle,
        wake_id: &str,
        units: u64,
    ) -> Result<ContinuationBudgetProgressV1, CognitionError> {
        self.core.consume_wake(handle, wake_id, units)
    }

    pub fn consume_runtime_status(
        &mut self,
        handle: ContinuationHandle,
        runtime: RuntimeContinuationStatusV1,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        let _ = (handle, runtime);
        Err(error(
            "continuation_runtime_status_unverified",
            "legacy Runtime status lacks an authoritative continuation projection",
        ))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn consume_runtime_projection(
        &mut self,
        handle: ContinuationHandle,
        runtime: &crate::runtime::AgentContinuation,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        self.core
            .consume_authoritative_projection(handle, runtime, &RuntimeContinuationAuthority)
    }

    /// Runtime projection consumer for the production wake path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn consume_runtime_projection_with_context(
        &mut self,
        handle: ContinuationHandle,
        runtime: &crate::runtime::AgentContinuation,
        context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        self.core.consume_authoritative_projection_with_context(
            handle,
            runtime,
            context,
            &RuntimeContinuationAuthority,
        )
    }

    /// Consume a Runtime-ready wake only after rechecking the current
    /// observation/goal/policy/precondition context and reconciling the
    /// Runtime-owned budget transition.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn advance_ready_wake(
        &mut self,
        handle: ContinuationHandle,
        runtime: &crate::runtime::AgentContinuation,
        context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, CognitionError> {
        let mut projected = self.core.advance_ready_wake(
            handle,
            runtime,
            context,
            &RuntimeContinuationAuthority,
        )?;
        if runtime.status == crate::runtime::ContinuationStatusV1::Consumed {
            projected.active = false;
        }
        Ok(projected)
    }

    pub fn invalidate(
        &mut self,
        handle: ContinuationHandle,
        reason: ContinuationInvalidationReason,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        self.core.invalidate(handle, reason)
    }

    pub fn active_count(&self) -> usize {
        self.core.active_count()
    }
}
