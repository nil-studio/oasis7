use super::*;

impl NodeRuntime {
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
        for action in pending.iter() {
            self.validate_bft_payload_admission(&action.payload_cbor)?;
        }
        Ok(())
    }

    pub(crate) fn validate_bft_proposal_actions_with_hook(
        config: &NodeConfig,
        execution_hook: &Option<Arc<Mutex<Box<dyn NodeExecutionHook>>>>,
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
            governance_auth::preflight_with_hook(execution_hook, action.payload_cbor.as_slice())?;
        }
        validate_replicated_execution_input_actions(actions, height)
            .map_err(|reason| NodeError::Consensus { reason })?;
        validate_consensus_action_root(&compute_consensus_action_root(actions)?, actions)?;
        Ok(())
    }
}
