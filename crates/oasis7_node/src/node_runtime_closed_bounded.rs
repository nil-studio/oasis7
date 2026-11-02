//! Closed-profile bounded local admission. Ordinary snapshot/queue APIs retain
//! their existing semantics, and the shared full validation path is unchanged.
use super::*;
impl NodeRuntime {
    /// Actual local status only, not a task/World/Current authorization. The
    /// caller must still apply its complete configured readiness predicate.
    pub fn try_closed_task_mutation_status(&self) -> Result<(bool, bool), NodeError> {
        let state = self.state.try_lock().map_err(|_| NodeError::Consensus {
            reason: "closed task Node state busy or poisoned".into(),
        })?;
        Ok((
            self.running.load(Ordering::SeqCst),
            state.last_error.is_some(),
        ))
    }
    /// Submit one canonical closed task original. No alternate player, no
    /// ordinary/BFT fallback, and no queue byte reservation before lock success.
    pub fn try_submit_closed_task_mutation_payload(
        &self,
        action_id: u64,
        payload_cbor: Vec<u8>,
    ) -> Result<(), NodeError> {
        if !is_closed_task_catalog(&payload_cbor) || payload_cbor.len() > 64 * 1024 {
            return Err(NodeError::Consensus {
                reason: "closed task mutation profile required within 64 KiB".into(),
            });
        }
        if self.bft_config.is_some() || self.bft_v2_config.is_some() || self.bft_v3_config.is_some()
        {
            return Err(NodeError::Consensus {
                reason: "closed task mutation requires non-BFT Formal begun profile".into(),
            });
        }
        // Identical player/reserved-id/size/auth/Owner/qualified-hook preflight
        // helper used by ordinary submit. It also keeps the closed hook bounded.
        let action = self.validated_player_consensus_action(
            self.config.player_id.clone(),
            action_id,
            payload_cbor,
        )?;
        let mut pending =
            self.pending_consensus_actions
                .try_lock()
                .map_err(|_| NodeError::Consensus {
                    reason: "closed task pending queue busy or poisoned".into(),
                })?;
        if pending.len() >= self.config.max_pending_consensus_actions {
            return Err(NodeError::Consensus {
                reason: format!(
                    "pending consensus actions queue saturated: len={} limit={}",
                    pending.len(),
                    self.config.max_pending_consensus_actions
                ),
            });
        }
        reserve_action_payload_bytes(
            &self.pending_consensus_action_queue_bytes,
            self.config.max_pending_consensus_action_queue_bytes,
            action.payload_cbor.len(),
        )?;
        pending.push(action);
        Ok(())
    }
}
