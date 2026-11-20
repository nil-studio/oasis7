//! Portable continuation accounting and current-context gates.
//!
//! Durable continuation objects remain Runtime-owned. The host callback
//! validates the complete durable object and translates it into the internal,
//! non-serialized projection below; this module owns only bounded proposal
//! correlation and idempotent wake accounting.

use std::collections::BTreeMap;

use oasis7_agent_api::{CognitionError, ContinuationBudgetV1, ContinuationProposalV1, h_v1};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::RuntimeAuthority;

fn error(code: &'static str, message: impl Into<String>) -> CognitionError {
    CognitionError::new(code, message)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationAuthorityContextV1 {
    pub baseline_observation_digest: String,
    pub goal_digest: String,
    pub policy_digest: String,
    pub precondition_digest: String,
}

impl ContinuationAuthorityContextV1 {
    pub fn validate(&self) -> Result<(), CognitionError> {
        for (name, value) in [
            (
                "baseline_observation_digest",
                &self.baseline_observation_digest,
            ),
            ("goal_digest", &self.goal_digest),
            ("policy_digest", &self.policy_digest),
            ("precondition_digest", &self.precondition_digest),
        ] {
            if value.trim().is_empty() || value.len() > 512 {
                return Err(error(
                    "continuation_context_invalid",
                    format!("{name} is required and bounded"),
                ));
            }
        }
        Ok(())
    }

    pub fn validate_proposal(
        &self,
        proposal: &ContinuationProposalV1,
    ) -> Result<(), CognitionError> {
        self.validate()?;
        if self.baseline_observation_digest != proposal.baseline_observation_digest
            || self.goal_digest != proposal.goal_digest
            || self.policy_digest != proposal.policy_digest
            || self.precondition_digest != proposal.precondition_digest
        {
            return Err(error(
                "continuation_context_stale",
                "continuation lineage no longer matches the authoritative cognition context",
            ));
        }
        Ok(())
    }
}

/// Runtime's fully validated continuation state reduced to the fields the
/// Harness needs for correlation and wake accounting. This is an in-process
/// callback result, not a serializable/persisted API DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationHostProjection {
    pub continuation_proposal_id: String,
    pub proposal_digest: String,
    pub world_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub origin_turn_id: String,
    pub origin_request_digest: String,
    pub action_or_envelope_digest: Option<String>,
    pub precondition_digest: String,
    pub valid_until_tick: Option<u64>,
    pub continuation_id: String,
    pub wake_id: String,
    pub wake_seq: u64,
    pub continuation_digest: String,
    pub continuation_status_digest: String,
    pub status: ContinuationStatusProjectionV1,
    pub terminal_disposition: Option<String>,
    pub remaining_budget: ContinuationBudgetV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationStatusProjectionV1 {
    Scheduled,
    Pending,
    Waking,
    Consumed,
    Completed,
    Cancelled,
    Invalidated,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationBudgetProgressV1 {
    pub chain_id: String,
    pub wake_id: String,
    pub unit: String,
    pub consumed: u64,
    pub remaining: u64,
    pub exhausted: bool,
    pub duplicate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_disposition: Option<String>,
}

/// Legacy status-only continuation DTO rejected unless accompanied by an
/// authoritative host continuation projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeContinuationStatusV1 {
    pub status: String,
    #[serde(default)]
    pub terminal_disposition: Option<String>,
    pub continuation_id: String,
    pub wake_id: String,
    pub wake_seq: u64,
    pub continuation_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationHandle {
    pub proposal: ContinuationProposalV1,
    #[serde(default)]
    pub chain_id: String,
    pub continuation_id: String,
    pub wake_id: String,
    pub wake_seq: u64,
    pub continuation_digest: String,
    #[serde(default)]
    pub continuation_status_digest: String,
    pub status: String,
    #[serde(default)]
    pub terminal_disposition: Option<String>,
    pub active: bool,
    pub provenance: String,
    pub world_effect: bool,
    pub provider_invocation_count: u64,
    #[serde(default)]
    pub remaining_budget: ContinuationBudgetV1,
    #[serde(default)]
    pub consumed_budget: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationInvalidationReason {
    ObservationChanged,
    GoalChanged,
    PolicyChanged,
    PreconditionChanged,
    BudgetExhausted,
    Rejected,
    Stale,
    Timeout,
    Expired,
    Reorg,
    Cancelled,
}

impl ContinuationInvalidationReason {
    fn status(self) -> &'static str {
        match self {
            Self::ObservationChanged
            | Self::GoalChanged
            | Self::PolicyChanged
            | Self::PreconditionChanged
            | Self::Stale
            | Self::Reorg => "invalidated",
            Self::Rejected => "rejected",
            Self::BudgetExhausted | Self::Timeout | Self::Expired => "expired",
            Self::Cancelled => "cancelled",
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::ObservationChanged => "observation_changed",
            Self::GoalChanged => "goal_changed",
            Self::PolicyChanged => "policy_changed",
            Self::PreconditionChanged => "precondition_changed",
            Self::BudgetExhausted => "budget_exhausted",
            Self::Rejected => "rejected",
            Self::Stale => "stale",
            Self::Timeout => "timeout",
            Self::Expired => "expired",
            Self::Reorg => "reorg",
            Self::Cancelled => "cancelled",
        }
    }
}

pub type ContinuationProjectionV1 = ContinuationHandle;

#[derive(Debug, Clone, Default)]
pub struct ContinuationHarness {
    active: BTreeMap<String, ContinuationProposalV1>,
    chains: BTreeMap<String, ContinuationChainState>,
    deliveries: BTreeMap<(String, String), ContinuationBudgetProgressV1>,
}

#[derive(Debug, Clone)]
struct ContinuationChainState {
    unit: String,
    remaining: u64,
    consumed: u64,
    terminal_disposition: Option<String>,
}

struct ContinuationHandleState {
    chain_id: String,
    status: String,
    active: bool,
    provenance: String,
    continuation_id: String,
    wake_id: String,
    wake_seq: u64,
    continuation_digest: String,
    continuation_status_digest: String,
    terminal_disposition: Option<String>,
}

impl ContinuationHandleState {
    fn scheduled(chain_id: String) -> Self {
        Self {
            chain_id,
            status: "scheduled".into(),
            active: true,
            provenance: "harness_policy".into(),
            continuation_id: String::new(),
            wake_id: String::new(),
            wake_seq: 0,
            continuation_digest: String::new(),
            continuation_status_digest: String::new(),
            terminal_disposition: None,
        }
    }
}

impl ContinuationHarness {
    fn chain_id(proposal: &ContinuationProposalV1) -> String {
        h_v1(
            "oasis7.cognition.continuation-chain.v1",
            &json!({
                "world_id": proposal.world_id,
                "agent_id": proposal.agent_id,
                "origin_turn_id": proposal.origin_turn_id,
                "origin_request_digest": proposal.origin_request_digest,
            }),
        )
        .to_string()
    }

    fn handle_for(
        &self,
        proposal: ContinuationProposalV1,
        state: ContinuationHandleState,
    ) -> ContinuationHandle {
        let (remaining_budget, consumed_budget) = self
            .chains
            .get(&state.chain_id)
            .map(|state| {
                (
                    ContinuationBudgetV1 {
                        unit: state.unit.clone(),
                        value: state.remaining,
                    },
                    state.consumed,
                )
            })
            .unwrap_or_else(|| (proposal.remaining_budget.clone(), 0));
        ContinuationHandle {
            proposal,
            chain_id: state.chain_id,
            continuation_id: state.continuation_id,
            wake_id: state.wake_id,
            wake_seq: state.wake_seq,
            continuation_digest: state.continuation_digest,
            continuation_status_digest: state.continuation_status_digest,
            status: state.status,
            terminal_disposition: state.terminal_disposition,
            active: state.active,
            provenance: state.provenance,
            world_effect: false,
            provider_invocation_count: 0,
            remaining_budget,
            consumed_budget,
        }
    }

    fn submit_inner(
        &mut self,
        proposal: ContinuationProposalV1,
        context: Option<&ContinuationAuthorityContextV1>,
    ) -> Result<ContinuationHandle, CognitionError> {
        proposal.validate()?;
        if let Some(context) = context {
            context.validate_proposal(&proposal)?;
        }
        let chain_id = Self::chain_id(&proposal);
        if let Some(existing) = self.active.get(&proposal.continuation_proposal_id) {
            if existing.proposal_digest != proposal.proposal_digest {
                return Err(error(
                    "continuation_duplicate",
                    "continuation proposal id was reused with a different digest",
                ));
            }
            return Ok(self.handle_for(
                existing.clone(),
                ContinuationHandleState::scheduled(chain_id),
            ));
        }
        if let Some(state) = self.chains.get(&chain_id) {
            if state.unit != proposal.remaining_budget.unit {
                return Err(error(
                    "continuation_budget_unit_mismatch",
                    "continuation chain cannot change budget units",
                ));
            }
            if state.remaining == 0 {
                return Err(error(
                    "continuation_budget_exhausted",
                    "continuation chain has no remaining budget",
                ));
            }
            if state.terminal_disposition.is_some() {
                return Err(error(
                    "continuation_terminal",
                    "continuation chain has already reached a terminal disposition",
                ));
            }
            if proposal.remaining_budget.value > state.remaining {
                return Err(error(
                    "continuation_budget_increase",
                    "continuation proposal budget exceeds the chain remainder",
                ));
            }
        } else {
            self.chains.insert(
                chain_id.clone(),
                ContinuationChainState {
                    unit: proposal.remaining_budget.unit.clone(),
                    remaining: proposal.remaining_budget.value,
                    consumed: 0,
                    terminal_disposition: None,
                },
            );
        }
        if self.active.values().any(|existing| {
            Self::chain_id(existing) == chain_id
                && existing.continuation_proposal_id != proposal.continuation_proposal_id
        }) {
            return Err(error(
                "continuation_active",
                "a continuation in this cognition chain is already active",
            ));
        }
        self.active
            .insert(proposal.continuation_proposal_id.clone(), proposal.clone());
        Ok(self.handle_for(proposal, ContinuationHandleState::scheduled(chain_id)))
    }

    pub fn submit(
        &mut self,
        proposal: ContinuationProposalV1,
    ) -> Result<ContinuationHandle, CognitionError> {
        self.submit_inner(proposal, None)
    }

    pub fn submit_with_context(
        &mut self,
        proposal: ContinuationProposalV1,
        context: &ContinuationAuthorityContextV1,
    ) -> Result<ContinuationHandle, CognitionError> {
        self.submit_inner(proposal, Some(context))
    }

    pub fn consume_wake(
        &mut self,
        handle: &mut ContinuationHandle,
        wake_id: &str,
        units: u64,
    ) -> Result<ContinuationBudgetProgressV1, CognitionError> {
        let chain_id = handle.chain_id.clone();
        if chain_id.is_empty() || wake_id.trim().is_empty() {
            return Err(error(
                "continuation_delivery_invalid",
                "a continuation chain and wake identity are required",
            ));
        }
        if let Some(previous) = self
            .deliveries
            .get(&(chain_id.clone(), wake_id.to_string()))
            .cloned()
        {
            handle.remaining_budget = ContinuationBudgetV1 {
                unit: previous.unit.clone(),
                value: previous.remaining,
            };
            handle.consumed_budget = previous.consumed;
            if previous.exhausted {
                handle.active = false;
                handle.status = "expired".to_string();
                handle.terminal_disposition = previous.terminal_disposition.clone();
            }
            let mut duplicate = previous;
            duplicate.duplicate = true;
            return Ok(duplicate);
        }
        if !handle.active
            || !self
                .active
                .contains_key(&handle.proposal.continuation_proposal_id)
        {
            return Err(error(
                "continuation_unknown",
                "continuation handle is not active",
            ));
        }
        if units == 0 {
            return Err(error(
                "continuation_budget_invalid",
                "continuation consumption must be positive",
            ));
        }
        let state = self
            .chains
            .get_mut(&chain_id)
            .ok_or_else(|| error("continuation_unknown", "continuation chain is not active"))?;
        if units > state.remaining {
            return Err(error(
                "continuation_budget_exhausted",
                "continuation delivery exceeds the remaining chain budget",
            ));
        }
        state.remaining -= units;
        state.consumed = state.consumed.saturating_add(units);
        let exhausted = state.remaining == 0;
        if exhausted {
            state.terminal_disposition = Some("budget_exhausted".to_string());
        }
        let progress = ContinuationBudgetProgressV1 {
            chain_id: chain_id.clone(),
            wake_id: wake_id.to_string(),
            unit: state.unit.clone(),
            consumed: state.consumed,
            remaining: state.remaining,
            exhausted,
            duplicate: false,
            terminal_disposition: exhausted.then(|| "budget_exhausted".to_string()),
        };
        self.deliveries
            .insert((chain_id, wake_id.to_string()), progress.clone());
        handle.remaining_budget = ContinuationBudgetV1 {
            unit: progress.unit.clone(),
            value: progress.remaining,
        };
        handle.consumed_budget = progress.consumed;
        if exhausted {
            handle.active = false;
            handle.status = "expired".to_string();
            handle.terminal_disposition = progress.terminal_disposition.clone();
            self.active
                .remove(&handle.proposal.continuation_proposal_id);
        }
        Ok(progress)
    }

    pub fn consume_runtime_status(
        &mut self,
        handle: ContinuationHandle,
        _runtime: ContinuationStatusProjectionV1,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        let _ = handle;
        Err(error(
            "continuation_runtime_status_unverified",
            "status alone is not an authoritative continuation projection",
        ))
    }

    pub fn consume_authoritative_projection<A: RuntimeAuthority>(
        &mut self,
        handle: ContinuationHandle,
        continuation: &A::Continuation,
        authority: &A,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        let projection = authority.continuation_projection(continuation)?;
        self.consume_verified_projection(handle, projection)
    }

    pub fn consume_authoritative_projection_with_context<A: RuntimeAuthority>(
        &mut self,
        handle: ContinuationHandle,
        continuation: &A::Continuation,
        context: &ContinuationAuthorityContextV1,
        authority: &A,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        let proposal = self.active_proposal(&handle)?;
        context.validate_proposal(&proposal)?;
        self.consume_authoritative_projection(handle, continuation, authority)
    }

    fn active_proposal(
        &self,
        handle: &ContinuationHandle,
    ) -> Result<ContinuationProposalV1, CognitionError> {
        self.active
            .get(&handle.proposal.continuation_proposal_id)
            .filter(|candidate| candidate.proposal_digest == handle.proposal.proposal_digest)
            .cloned()
            .ok_or_else(|| error("continuation_unknown", "continuation handle is not active"))
    }

    fn reconcile_runtime_budget(
        &mut self,
        proposal: &ContinuationProposalV1,
        runtime: &ContinuationHostProjection,
    ) -> Result<(String, ContinuationBudgetV1, u64), CognitionError> {
        let chain_id = Self::chain_id(proposal);
        let delivery_key = (chain_id.clone(), runtime.wake_id.clone());
        if let Some(previous) = self.deliveries.get(&delivery_key) {
            if runtime.remaining_budget.unit != previous.unit
                || runtime.remaining_budget.value != previous.remaining
            {
                return Err(error(
                    "continuation_budget_replay_mismatch",
                    "a Runtime wake was replayed with a different remaining budget",
                ));
            }
            return Ok((
                chain_id,
                ContinuationBudgetV1 {
                    unit: previous.unit.clone(),
                    value: previous.remaining,
                },
                previous.consumed,
            ));
        }
        let (remaining, consumed, progress) = {
            let state = self
                .chains
                .get_mut(&chain_id)
                .ok_or_else(|| error("continuation_unknown", "continuation chain is not active"))?;
            if runtime.remaining_budget.unit != state.unit {
                return Err(error(
                    "continuation_budget_unit_mismatch",
                    "Runtime changed the continuation budget unit",
                ));
            }
            if runtime.remaining_budget.value > state.remaining {
                return Err(error(
                    "continuation_budget_increase",
                    "Runtime increased the remaining continuation budget",
                ));
            }
            let delta = state.remaining - runtime.remaining_budget.value;
            state.remaining = runtime.remaining_budget.value;
            state.consumed = state.consumed.saturating_add(delta);
            let exhausted = state.remaining == 0;
            let progress = (delta > 0).then(|| ContinuationBudgetProgressV1 {
                chain_id: chain_id.clone(),
                wake_id: runtime.wake_id.clone(),
                unit: state.unit.clone(),
                consumed: state.consumed,
                remaining: state.remaining,
                exhausted,
                duplicate: false,
                terminal_disposition: if exhausted {
                    runtime
                        .terminal_disposition
                        .clone()
                        .or_else(|| Some("budget_exhausted".to_string()))
                } else {
                    None
                },
            });
            (state.remaining, state.consumed, progress)
        };
        if let Some(progress) = progress {
            self.deliveries.insert(delivery_key, progress);
        }
        Ok((
            chain_id,
            ContinuationBudgetV1 {
                unit: runtime.remaining_budget.unit.clone(),
                value: remaining,
            },
            consumed,
        ))
    }

    fn consume_verified_projection(
        &mut self,
        handle: ContinuationHandle,
        runtime: ContinuationHostProjection,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        let proposal = self.active_proposal(&handle)?;
        if runtime.continuation_proposal_id != proposal.continuation_proposal_id
            || runtime.proposal_digest != proposal.proposal_digest
            || runtime.world_id != proposal.world_id
            || runtime.agent_id != proposal.agent_id
            || runtime.agent_session_id != proposal.agent_session_id
            || runtime.agent_turn_id != proposal.agent_turn_id
            || runtime.decision_request_id != proposal.decision_request_id
            || runtime.origin_turn_id != proposal.origin_turn_id
            || runtime.origin_request_digest != proposal.origin_request_digest
            || runtime.action_or_envelope_digest != proposal.action_or_envelope_digest
            || runtime.precondition_digest != proposal.precondition_digest
            || runtime.valid_until_tick != proposal.valid_until_tick
        {
            return Err(error(
                "continuation_runtime_correlation_mismatch",
                "Runtime continuation projection does not match the proposal lineage",
            ));
        }
        if runtime.continuation_id.trim().is_empty()
            || runtime.wake_id.trim().is_empty()
            || runtime.continuation_digest.trim().is_empty()
            || runtime.continuation_status_digest.trim().is_empty()
            || runtime.wake_seq == 0
        {
            return Err(error(
                "continuation_runtime_projection_invalid",
                "verified Runtime projection must carry continuation and wake identity",
            ));
        }
        let (chain_id, remaining_budget, consumed_budget) =
            self.reconcile_runtime_budget(&proposal, &runtime)?;
        let (status, active) = match runtime.status {
            ContinuationStatusProjectionV1::Scheduled => ("scheduled", true),
            ContinuationStatusProjectionV1::Pending => ("pending", true),
            ContinuationStatusProjectionV1::Waking => ("waking", true),
            ContinuationStatusProjectionV1::Consumed => ("consumed", true),
            ContinuationStatusProjectionV1::Completed => ("completed", false),
            ContinuationStatusProjectionV1::Cancelled => ("cancelled", false),
            ContinuationStatusProjectionV1::Invalidated => ("invalidated", false),
            ContinuationStatusProjectionV1::Expired => ("expired", false),
            ContinuationStatusProjectionV1::Rejected => ("rejected", false),
        };
        if !active {
            if let Some(state) = self.chains.get_mut(&chain_id) {
                state.terminal_disposition = runtime
                    .terminal_disposition
                    .clone()
                    .or_else(|| Some(status.to_string()));
            }
            self.active.remove(&proposal.continuation_proposal_id);
        }
        Ok(ContinuationHandle {
            proposal,
            chain_id,
            continuation_id: runtime.continuation_id,
            wake_id: runtime.wake_id,
            wake_seq: runtime.wake_seq,
            continuation_digest: runtime.continuation_digest,
            continuation_status_digest: runtime.continuation_status_digest,
            status: status.to_string(),
            terminal_disposition: runtime.terminal_disposition,
            active,
            provenance: "runtime_authoritative".to_string(),
            world_effect: false,
            provider_invocation_count: 0,
            remaining_budget,
            consumed_budget,
        })
    }

    pub fn advance_ready_wake<A: RuntimeAuthority>(
        &mut self,
        handle: ContinuationHandle,
        continuation: &A::Continuation,
        context: &ContinuationAuthorityContextV1,
        authority: &A,
    ) -> Result<ContinuationHandle, CognitionError> {
        let proposal = self.active_proposal(&handle)?;
        context.validate_proposal(&proposal)?;
        let runtime = authority.continuation_projection(continuation)?;
        if !matches!(
            runtime.status,
            ContinuationStatusProjectionV1::Consumed
                | ContinuationStatusProjectionV1::Completed
                | ContinuationStatusProjectionV1::Cancelled
                | ContinuationStatusProjectionV1::Invalidated
                | ContinuationStatusProjectionV1::Expired
                | ContinuationStatusProjectionV1::Rejected
        ) {
            return Err(error(
                "continuation_wake_not_ready",
                "a pending or waking continuation cannot advance to the next cognition step",
            ));
        }
        if runtime.status == ContinuationStatusProjectionV1::Consumed {
            let chain_id = Self::chain_id(&proposal);
            let delivery_key = (chain_id.clone(), runtime.wake_id.clone());
            let prior = self.deliveries.get(&delivery_key);
            let chain_remaining = self
                .chains
                .get(&chain_id)
                .ok_or_else(|| error("continuation_unknown", "continuation chain is not active"))?
                .remaining;
            if prior.is_none() && runtime.remaining_budget.value == chain_remaining {
                return Err(error(
                    "continuation_budget_not_consumed",
                    "a consumed wake must carry a lower remaining budget",
                ));
            }
        }
        let consumed = self.consume_verified_projection(handle, runtime)?;
        if consumed.status == "consumed" {
            self.active
                .remove(&consumed.proposal.continuation_proposal_id);
        }
        Ok(consumed)
    }

    pub fn invalidate(
        &mut self,
        handle: ContinuationHandle,
        reason: ContinuationInvalidationReason,
    ) -> Result<ContinuationProjectionV1, CognitionError> {
        let proposal = self.active_proposal(&handle)?;
        self.active
            .remove(&handle.proposal.continuation_proposal_id);
        let chain_id = Self::chain_id(&proposal);
        if let Some(state) = self.chains.get_mut(&chain_id) {
            state.terminal_disposition = Some(reason.reason().to_string());
        }
        let (remaining_budget, consumed_budget) = self
            .chains
            .get(&chain_id)
            .map(|state| {
                (
                    ContinuationBudgetV1 {
                        unit: state.unit.clone(),
                        value: state.remaining,
                    },
                    state.consumed,
                )
            })
            .unwrap_or_else(|| (proposal.remaining_budget.clone(), 0));
        Ok(ContinuationHandle {
            proposal,
            chain_id,
            continuation_id: String::new(),
            wake_id: String::new(),
            wake_seq: 0,
            continuation_digest: String::new(),
            continuation_status_digest: String::new(),
            status: reason.status().to_string(),
            terminal_disposition: Some(reason.reason().to_string()),
            active: false,
            provenance: "harness_policy".to_string(),
            world_effect: false,
            provider_invocation_count: 0,
            remaining_budget,
            consumed_budget,
        })
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> ContinuationProposalV1 {
        let mut proposal = ContinuationProposalV1 {
            schema_version: 1,
            continuation_proposal_id: "proposal-1".into(),
            world_id: "world-continuation-fixture".into(),
            agent_id: "agent-continuation-1".into(),
            agent_session_id: "session-continuation-1".into(),
            agent_turn_id: "turn-continuation-1".into(),
            decision_request_id: "request-continuation-1".into(),
            origin_turn_id: "turn-continuation-1".into(),
            origin_request_digest:
                "blake3:2222222222222222222222222222222222222222222222222222222222222222".into(),
            action_or_plan_kind: "wait_for_receipt".into(),
            action_or_envelope_digest: None,
            remaining_budget: ContinuationBudgetV1 {
                unit: "steps".into(),
                value: 2,
            },
            baseline_observation_digest:
                "blake3:3333333333333333333333333333333333333333333333333333333333333333".into(),
            goal_digest: "blake3:4444444444444444444444444444444444444444444444444444444444444444"
                .into(),
            policy_digest:
                "blake3:5555555555555555555555555555555555555555555555555555555555555555".into(),
            policy_revision: 3,
            precondition_summary: "receipt pending".into(),
            precondition_digest:
                "blake3:6666666666666666666666666666666666666666666666666666666666666666".into(),
            wake_conditions: vec![oasis7_agent_api::WakeConditionV1 {
                schema_version: "wake-condition.v1".into(),
                kind: "receipt_linked".into(),
                logical_tick: None,
                event_digest: None,
                receipt_id: Some(
                    "blake3:9999999999999999999999999999999999999999999999999999999999999999"
                        .into(),
                ),
                subject: None,
                path_or_rule: None,
                operator: None,
                expected_value_bytes: None,
            }],
            valid_until_tick: Some(100),
            source: "harness".into(),
            proposal_digest: String::new(),
        };
        proposal.proposal_digest = proposal.proposal_digest().unwrap().to_string();
        proposal
    }

    #[test]
    fn wake_delivery_is_idempotent_and_never_increases_budget() {
        let proposal = proposal();
        assert_eq!(
            proposal.proposal_digest,
            "blake3:a2390a5908bd4568a1ebd77d3e82c876556d8e93697ea60890ba3137d9adb8ea"
        );
        let wire = serde_json::to_value(&proposal).unwrap();
        assert!(wire["action_or_envelope_digest"].is_null());
        let mut harness = ContinuationHarness::default();
        let mut handle = harness.submit(proposal).unwrap();
        let first = harness.consume_wake(&mut handle, "wake-1", 1).unwrap();
        let duplicate = harness.consume_wake(&mut handle, "wake-1", 1).unwrap();
        assert_eq!(first.remaining, 1);
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.remaining, 1);
        assert_eq!(handle.consumed_budget, 1);
    }

    #[test]
    fn stale_current_context_blocks_proposal_admission() {
        let mut harness = ContinuationHarness::default();
        let mut current = ContinuationAuthorityContextV1 {
            baseline_observation_digest:
                "blake3:3333333333333333333333333333333333333333333333333333333333333333".into(),
            goal_digest: "blake3:4444444444444444444444444444444444444444444444444444444444444444"
                .into(),
            policy_digest:
                "blake3:5555555555555555555555555555555555555555555555555555555555555555".into(),
            precondition_digest:
                "blake3:6666666666666666666666666666666666666666666666666666666666666666".into(),
        };
        assert!(harness.submit_with_context(proposal(), &current).is_ok());
        current.goal_digest =
            "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        let changed = ContinuationAuthorityContextV1 {
            baseline_observation_digest: current.baseline_observation_digest,
            goal_digest: current.goal_digest,
            policy_digest: current.policy_digest,
            precondition_digest: current.precondition_digest,
        };
        let rejected = harness
            .submit_with_context(proposal(), &changed)
            .unwrap_err();
        assert_eq!(rejected.code(), "continuation_context_stale");
    }

    #[test]
    fn host_projection_is_not_serializable_or_constructible_from_runtime_status_only() {
        let mut harness = ContinuationHarness::default();
        let handle = harness.submit(proposal()).unwrap();
        assert_eq!(harness.active_count(), 1);
        assert_eq!(
            harness
                .consume_runtime_status(handle, ContinuationStatusProjectionV1::Consumed)
                .unwrap_err()
                .code(),
            "continuation_runtime_status_unverified"
        );
    }
}
