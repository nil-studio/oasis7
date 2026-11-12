use super::*;

#[cfg(test)]
#[path = "node_runtime_local_bootstrap_tests.rs"]
mod tests;

impl NodeRuntime {
    pub(super) fn initialize_local_execution_bootstrap(
        &self,
        engine: &mut PosNodeEngine,
    ) -> Result<(), NodeError> {
        let result = self.initialize_local_execution_bootstrap_inner(engine);
        if result.is_err() {
            self.running.store(false, Ordering::SeqCst);
        }
        result
    }

    fn initialize_local_execution_bootstrap_inner(
        &self,
        engine: &mut PosNodeEngine,
    ) -> Result<(), NodeError> {
        if self.local_execution_bootstrap.is_some()
            && self.local_execution_genesis_bootstrap.is_some()
        {
            return Err(NodeError::InvalidConfig {
                reason: "Node cannot combine positive-height and genesis execution bootstrap"
                    .into(),
            });
        }

        if let Some(genesis) = self.local_execution_genesis_bootstrap.as_ref() {
            if self.config.replication.is_some()
                || self.replication_network.is_some()
                || self.config.gossip.is_some()
            {
                return Err(NodeError::Execution {
                    reason: "qualified local genesis bootstrap rejects network recovery and gossip"
                        .into(),
                });
            }
            let hook = self
                .execution_hook
                .as_ref()
                .ok_or_else(|| NodeError::Execution {
                    reason:
                        "qualified local genesis bootstrap requires its validating execution hook"
                            .into(),
                })?;
            let mut hook = hook.lock().map_err(|_| NodeError::Execution {
                reason: "qualified local genesis bootstrap hook lock unavailable".into(),
            })?;
            if !hook.guarded_local_execution() {
                return Err(NodeError::Execution {
                    reason: "qualified local genesis bootstrap requires a guarded local hook"
                        .into(),
                });
            }
            let qualified = hook
                .qualified_local_execution_genesis_bootstrap()
                .map_err(|reason| NodeError::Execution { reason })?
                .ok_or_else(|| NodeError::Execution {
                    reason: "execution hook did not confirm a qualified InitialActivation".into(),
                })?;
            if &qualified != genesis || genesis.world_id() != self.config.world_id {
                return Err(NodeError::Execution {
                    reason: "qualified InitialActivation changed since Node genesis bootstrap"
                        .into(),
                });
            }
            engine.apply_local_execution_genesis_bootstrap(&self.config.world_id, &qualified)?;
            if let Some(saved) = hook
                .pending_local_continuation()
                .map_err(|reason| NodeError::Execution { reason })?
            {
                let boundary = qualified.boundary();
                if saved.predecessor_committed_height != 0
                    || saved.predecessor_execution_height != 0
                    || saved.next_height != saved.context.height
                    || saved.context.height != 1
                    || saved.predecessor_node_block_hash.as_deref()
                        != Some(boundary.consensus_block_hash.as_str())
                    || saved.predecessor_execution_block_hash.as_deref()
                        != Some(boundary.execution_block_hash.as_str())
                    || saved.predecessor_execution_state_root.as_deref()
                        != Some(boundary.execution_state_root.as_str())
                {
                    return Err(NodeError::Execution {
                        reason: "pending first Node continuation differs from qualified genesis"
                            .into(),
                    });
                }
                engine.next_slot = saved.next_slot;
                engine.restore_guarded_local_decision(
                    &saved,
                    &self.config.node_id,
                    &self.config.world_id,
                )?;
                engine.restore_guarded_local_schedule_clock(&saved)?;
                hook.restored_verified_local_continuation(
                    &crate::execution_hook::VerifiedNodeRestoredContinuationV1::from_restored_engine(&saved),
                ).map_err(|reason| NodeError::Execution { reason })?;
            }
            return Ok(());
        }

        if let Some(bootstrap) = self.local_execution_bootstrap.as_ref() {
            engine.apply_local_execution_bootstrap(bootstrap)?;
        }
        if let Some(hook) = self.execution_hook.as_ref() {
            let mut hook = hook.lock().map_err(|_| NodeError::Execution {
                reason: "guarded bootstrap hook lock unavailable".into(),
            })?;
            if hook.guarded_local_execution() {
                if self.config.replication.is_some() || self.replication_network.is_some() {
                    return Err(NodeError::Execution {
                        reason: "guarded local bootstrap rejects ordinary replication recovery"
                            .into(),
                    });
                }
                if let Some(saved) = hook
                    .pending_local_continuation()
                    .map_err(|reason| NodeError::Execution { reason })?
                {
                    if self
                        .local_execution_bootstrap
                        .as_ref()
                        .is_none_or(|b| b.height != saved.predecessor_committed_height)
                        || saved.next_height != saved.context.height
                    {
                        return Err(NodeError::Execution {
                            reason: "guarded original counter boundary changed".into(),
                        });
                    }
                    engine.next_slot = saved.next_slot;
                    engine.restore_guarded_local_decision(
                        &saved,
                        &self.config.node_id,
                        &self.config.world_id,
                    )?;
                    engine.restore_guarded_local_schedule_clock(&saved)?;
                    hook.restored_verified_local_continuation(
                    &crate::execution_hook::VerifiedNodeRestoredContinuationV1::from_restored_engine(&saved),
                ).map_err(|reason| NodeError::Execution { reason })?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_local_execution_bootstrap_after_restore(
        &self,
        engine: &PosNodeEngine,
    ) -> Result<(), NodeError> {
        if let Some(bootstrap) = self.local_execution_bootstrap.as_ref()
            && let Err(err) = engine.validate_local_execution_bootstrap(bootstrap)
        {
            self.running.store(false, Ordering::SeqCst);
            return Err(err);
        }
        Ok(())
    }
}

impl PosNodeEngine {
    /// Re-anchor the inferred logical clock to a continuation only after the
    /// guarded local decision has been fully restored and validated. Without
    /// this floor, a fresh engine starts its inferred clock at slot zero while
    /// `next_slot` may be far ahead, so a restarted single-authority node can
    /// idle forever waiting for a slot it already committed.
    pub(super) fn restore_guarded_local_schedule_clock(
        &mut self,
        saved: &NodeLocalExecutionContinuation,
    ) -> Result<(), NodeError> {
        if self.ticks_per_slot == 0 || self.proposal_tick_phase >= self.ticks_per_slot {
            return Err(NodeError::InvalidConfig {
                reason: "guarded continuation cannot restore an invalid slot clock".into(),
            });
        }
        if saved.context.slot.checked_add(1) != Some(saved.next_slot) {
            return Err(NodeError::Execution {
                reason: "guarded continuation next slot is not the committed slot successor".into(),
            });
        }

        // `context.slot` is the authenticated proposal position. `next_slot`
        // is checked against the exact local successor counter retained with
        // that decision. Do not infer either counter from committed height.
        let clock_slot = saved.context.slot;
        let restored_tick = clock_slot
            .checked_mul(self.ticks_per_slot)
            .and_then(|tick| tick.checked_add(self.proposal_tick_phase))
            .ok_or_else(|| NodeError::InvalidConfig {
                reason: "guarded continuation slot clock overflows logical ticks".into(),
            })?;

        // This runs during runtime initialization, before the worker's first
        // observe_wall_clock_tick call. Keep any configured absolute clock
        // origin intact; the verified continuation only establishes a floor.
        self.last_observed_tick = self.last_observed_tick.max(restored_tick);
        self.last_observed_slot = self.last_observed_slot.max(clock_slot);
        Ok(())
    }
}
