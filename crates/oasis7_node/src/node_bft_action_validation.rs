use super::*;
use crate::consensus_support::release_action_payload_bytes;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;

pub(super) fn reject_conflicting_queue_id(
    pending: &[NodeConsensusAction],
    action: &NodeConsensusAction,
) -> Result<bool, NodeError> {
    let mut found_exact = false;
    for existing in pending
        .iter()
        .filter(|existing| existing.action_id == action.action_id)
    {
        if existing != action {
            return Err(NodeError::Consensus {
                reason: format!(
                    "conflicting BFT queued action for action_id={}",
                    action.action_id
                ),
            });
        }
        found_exact = true;
    }
    Ok(found_exact)
}

impl NodeRuntime {
    /// Apply the selected profile's stateful admission before generic payload
    /// authentication and the legacy-only governance preflight.
    pub(super) fn validate_selected_consensus_payload(
        &self,
        payload: &[u8],
    ) -> Result<(), NodeError> {
        if self.bft_config.is_some() {
            self.validate_bft_payload_admission(payload)?;
        }
        if self.bft_v2_config.is_some() {
            self.validate_bft_v2_payload_admission(payload)?;
        }
        if self.bft_v3_config.is_some() {
            self.validate_bft_v3_payload_admission(payload)?;
        }
        validate_consensus_action_payload_auth(&self.config, payload)?;
        // BFT V2 admission above is stateful: its execution hook validates the
        // signed governance action against the exact verified V2 parent cursor.
        // The legacy preflight uses the ordinary governance path and can reject
        // a valid V2 transition under a different, non-BFT parent projection.
        if self.bft_v2_config.is_none() && self.bft_v3_config.is_none() {
            governance_auth::preflight(self, payload)?;
        }
        Ok(())
    }

    pub(crate) fn validate_bft_payload_admission(&self, payload: &[u8]) -> Result<(), NodeError> {
        let hook = self
            .execution_hook
            .as_ref()
            .ok_or_else(|| NodeError::Consensus {
                reason: "BFT action admission requires an execution hook".into(),
            })?;
        hook.lock()
            .map_err(|_| NodeError::Consensus {
                reason: "BFT execution hook lock poisoned".into(),
            })?
            .validate_bft_action_payload(payload)
            .map_err(|reason| NodeError::Consensus { reason })
    }

    pub(crate) fn validate_bft_pending_admission(&self) -> Result<(), NodeError> {
        let pending = self
            .pending_consensus_actions
            .lock()
            .map_err(|_| NodeError::Consensus {
                reason: "BFT pending action queue lock poisoned".into(),
            })?;
        let mut by_id = BTreeMap::<u64, &NodeConsensusAction>::new();
        for action in pending.iter() {
            if let Some(previous) = by_id.get(&action.action_id) {
                if *previous != action {
                    return Err(NodeError::Consensus {
                        reason: format!(
                            "conflicting preexisting BFT queued actions for action_id={}",
                            action.action_id
                        ),
                    });
                }
                continue;
            }
            by_id.insert(action.action_id, action);
            self.validate_bft_payload_admission(&action.payload_cbor)?;
        }
        Ok(())
    }

    pub(crate) fn canonicalize_bft_local_actions(
        pending: &mut Vec<NodeConsensusAction>,
        pending_bytes: &AtomicUsize,
        hook: &dyn NodeExecutionHook,
    ) -> Result<(Vec<NodeConsensusAction>, Vec<u64>), NodeError> {
        let mut ordered = pending.clone();
        ordered.sort_by_key(|action| action.action_id);
        for pair in ordered.windows(2) {
            if pair[0].action_id == pair[1].action_id && pair[0] != pair[1] {
                return Err(NodeError::Consensus {
                    reason: format!(
                        "conflicting preexisting BFT queued actions for action_id={}",
                        pair[0].action_id
                    ),
                });
            }
        }

        let mut selected = Vec::with_capacity(ordered.len());
        let mut rejected = Vec::new();
        let mut released_bytes = 0usize;
        let mut highwater = BTreeMap::<String, u64>::new();
        for action in ordered {
            if selected
                .last()
                .is_some_and(|previous: &NodeConsensusAction| {
                    previous.action_id == action.action_id
                })
            {
                released_bytes = released_bytes
                    .checked_add(action.payload_cbor.len())
                    .ok_or_else(|| NodeError::Consensus {
                        reason: "BFT local action byte accounting overflow".into(),
                    })?;
                continue;
            }
            if crate::node_runtime_core::retain_governance_action_for_proposal(
                &action,
                Some(hook),
                &mut highwater,
            ) {
                selected.push(action);
            } else {
                released_bytes = released_bytes
                    .checked_add(action.payload_cbor.len())
                    .ok_or_else(|| NodeError::Consensus {
                        reason: "BFT local action byte accounting overflow".into(),
                    })?;
                rejected.push(action.action_id);
            }
        }
        *pending = selected.clone();
        release_action_payload_bytes(pending_bytes, released_bytes);
        Ok((selected, rejected))
    }

    pub(crate) fn canonicalize_bft_v2_local_actions(
        pending: &mut Vec<NodeConsensusAction>,
        pending_bytes: &AtomicUsize,
        hook: &mut dyn NodeExecutionHook,
        schedule: &oasis7_consensus::tendermint_v2::BftHeightProposerScheduleV2,
    ) -> Result<(Vec<NodeConsensusAction>, Vec<u64>), NodeError> {
        let mut ordered = pending.clone();
        ordered.sort_by_key(|action| action.action_id);
        for pair in ordered.windows(2) {
            if pair[0].action_id == pair[1].action_id && pair[0] != pair[1] {
                return Err(NodeError::Consensus {
                    reason: format!(
                        "conflicting preexisting BFT queued actions for action_id={}",
                        pair[0].action_id
                    ),
                });
            }
        }

        // Check the cursor even when the queue is empty. Candidate-by-candidate
        // validation below may discard a stale action, but must not disguise a
        // broken execution cursor as an empty proposal.
        hook.validate_bft_v2_action_batch(schedule, &[])
            .map_err(|reason| NodeError::Consensus { reason })?;

        let mut selected = Vec::with_capacity(ordered.len());
        let mut rejected = Vec::new();
        let mut released_bytes = 0usize;
        for action in ordered {
            if selected
                .last()
                .is_some_and(|previous: &NodeConsensusAction| {
                    previous.action_id == action.action_id
                })
            {
                released_bytes = released_bytes
                    .checked_add(action.payload_cbor.len())
                    .ok_or_else(|| NodeError::Consensus {
                        reason: "BFT V2 local action byte accounting overflow".into(),
                    })?;
                continue;
            }

            let mut candidate = selected.clone();
            candidate.push(action.clone());
            if hook
                .validate_bft_v2_action_batch(schedule, &candidate)
                .is_ok()
            {
                selected.push(action);
            } else {
                released_bytes = released_bytes
                    .checked_add(action.payload_cbor.len())
                    .ok_or_else(|| NodeError::Consensus {
                        reason: "BFT V2 local action byte accounting overflow".into(),
                    })?;
                rejected.push(action.action_id);
            }
        }
        *pending = selected.clone();
        release_action_payload_bytes(pending_bytes, released_bytes);
        Ok((selected, rejected))
    }

    pub(crate) fn validate_bft_proposal_actions_with_execution_hook(
        config: &NodeConfig,
        execution_hook: &mut dyn NodeExecutionHook,
        actions: &[NodeConsensusAction],
        height: u64,
    ) -> Result<(), NodeError> {
        if actions.len() > config.max_engine_pending_consensus_actions {
            return Err(NodeError::Consensus {
                reason: "BFT proposal has more actions than the configured consensus bound".into(),
            });
        }
        for action in actions {
            action.validate().map_err(|error| NodeError::Consensus {
                reason: error.reason,
            })?;
            if action.payload_cbor.len() > config.max_consensus_action_payload_bytes {
                return Err(NodeError::Consensus {
                    reason: format!(
                        "BFT action payload exceeds configured limit action_id={} bytes={} limit={}",
                        action.action_id,
                        action.payload_cbor.len(),
                        config.max_consensus_action_payload_bytes
                    ),
                });
            }
            validate_consensus_action_payload_auth(config, action.payload_cbor.as_slice())?;
            execution_hook
                .validate_bft_action_payload(action.payload_cbor.as_slice())
                .map_err(|reason| NodeError::Consensus { reason })?;
            super::preflight_with_execution_hook(execution_hook, action.payload_cbor.as_slice())?;
        }
        validate_replicated_execution_input_actions(actions, height)
            .map_err(|reason| NodeError::Consensus { reason })?;
        validate_consensus_action_root(&compute_consensus_action_root(actions)?, actions)?;
        Ok(())
    }
}
