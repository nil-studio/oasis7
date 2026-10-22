//! Reauthorize queued governance commands immediately before local proposal.
use super::*;

impl PosNodeEngine {
    pub(super) fn discard_invalid_governance_actions(
        &mut self,
        hook: Option<&dyn NodeExecutionHook>,
    ) {
        let mut highwater = BTreeMap::<String, u64>::new();
        let mut released_bytes = 0usize;
        self.pending_consensus_actions.retain(|_, action| {
            let keep =
                match crate::node_runtime_core::governance_payload_nonce(&action.payload_cbor) {
                    Ok(None) => true,
                    Err(_) => false,
                    Ok(Some((controller, nonce))) => {
                        let authorized = hook.is_some_and(|hook| {
                            hook.validate_governance_action_payload(&action.payload_cbor)
                                .is_ok()
                        });
                        let previous = highwater.entry(controller).or_default();
                        if authorized && nonce > *previous {
                            *previous = nonce;
                            true
                        } else {
                            false
                        }
                    }
                };
            if !keep {
                released_bytes = released_bytes.saturating_add(action.payload_cbor.len());
            }
            keep
        });
        release_action_payload_bytes(&self.pending_consensus_action_queue_bytes, released_bytes);
    }
}

#[cfg(test)]
#[path = "node_engine_governance_auth_tests.rs"]
mod tests;
