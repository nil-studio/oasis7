use super::*;
use oasis7_simulator::{Action as SimulatorAction, ActionSubmitter, WorldEventKind};

impl NodeRuntimeExecutionDriver {
    pub(super) fn apply_simulator_actions(
        &mut self,
        context: &NodeExecutionCommitContext,
        simulator_actions: &[(SimulatorAction, ActionSubmitter)],
    ) -> Result<
        (
            Option<ExecutionSimulatorMirrorRecord>,
            SimulatorMirrorCommitObservation,
        ),
        String,
    > {
        let height = context.height;
        if simulator_actions.is_empty() {
            return Ok((None, SimulatorMirrorCommitObservation::default()));
        }

        let mut rejected_action_count = 0_usize;
        for (action, submitter) in simulator_actions {
            match submitter {
                ActionSubmitter::System => {
                    self.simulator_mirror
                        .submit_action_from_system(action.clone());
                }
                ActionSubmitter::Agent { agent_id } => {
                    self.simulator_mirror
                        .submit_action_from_agent(agent_id.clone(), action.clone());
                }
                ActionSubmitter::Player { player_id } => {
                    self.simulator_mirror
                        .submit_action_from_player(player_id.clone(), action.clone());
                }
            }

            let event = self.simulator_mirror.step().ok_or_else(|| {
                format!(
                    "execution driver simulator mirror step produced no event at height={height}"
                )
            })?;
            if matches!(event.kind, WorldEventKind::ActionRejected { .. }) {
                rejected_action_count = rejected_action_count.saturating_add(1);
            }
        }

        let resource_commit_hash = execution_resource_commit_hash(&context.world_id, height);
        let resource_context = ChainResourceDerivationContext {
            world_id: context.world_id.as_str(),
            chain_id: context.world_id.as_str(),
            genesis_ref: None,
            created_at_height: 0,
            manifest_height: context.height,
            commit_block_hash: Some(resource_commit_hash.as_str()),
            tick: self.simulator_mirror.time(),
        };
        let snapshot_value = self
            .simulator_mirror
            .snapshot_with_chain_resource_context(resource_context);
        let journal_value = self.simulator_mirror.journal_snapshot();
        let snapshot_bytes = super::super::to_cbor(snapshot_value)?;
        let journal_bytes = super::super::to_cbor(journal_value)?;

        let snapshot_ref = self
            .execution_store
            .put_bytes(snapshot_bytes.as_slice())
            .map_err(|err| {
                format!(
                    "execution driver simulator CAS snapshot put failed: {:?}",
                    err
                )
            })?;
        let journal_ref = self
            .execution_store
            .put_bytes(journal_bytes.as_slice())
            .map_err(|err| {
                format!(
                    "execution driver simulator CAS journal put failed: {:?}",
                    err
                )
            })?;
        let state_root = blake3_hex(snapshot_bytes.as_slice());

        let observation = SimulatorMirrorCommitObservation {
            action_count: simulator_actions.len(),
            rejected_action_count,
            snapshot_bytes: snapshot_bytes.len(),
            journal_bytes: journal_bytes.len(),
        };

        Ok((
            Some(ExecutionSimulatorMirrorRecord {
                action_count: simulator_actions.len(),
                rejected_action_count,
                journal_len: self.simulator_mirror.journal().len(),
                snapshot_ref,
                journal_ref,
                state_root,
            }),
            observation,
        ))
    }
}
