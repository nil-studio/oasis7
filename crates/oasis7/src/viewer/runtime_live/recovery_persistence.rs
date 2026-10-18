use super::*;
use sha2::{Digest, Sha256};

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeRecoveryFaultInjection {
    ReconciliationDrift,
    CursorCompute,
    AckEncode,
}

impl ViewerRuntimeLiveServer {
    fn apply_committed_authoritative_recovery_generation(
        &mut self,
        committed: crate::runtime::CommittedAuthoritativeRecoveryGeneration,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let generation: super::recovery_receipt::RuntimeAuthoritativeRecoveryGeneration =
            serde_json::from_slice(&committed.recovery_metadata)
                .map_err(|err| ViewerRuntimeLiveServerError::Serde(err.to_string()))?;
        if generation.ack.reorg_epoch != generation.reorg_epoch {
            return Err(ViewerRuntimeLiveServerError::Init(
                "authoritative recovery fenced generation epoch mismatch".to_string(),
            ));
        }
        self.world = crate::runtime_host::install_runtime_capabilities(committed.world);
        self.authoritative_batches = generation.authoritative_batches;
        self.next_authoritative_batch_id = generation.next_authoritative_batch_id;
        self.authoritative_challenges = generation.authoritative_challenges;
        self.next_authoritative_challenge_id = generation.next_authoritative_challenge_id;
        self.stable_checkpoints = generation.stable_checkpoints;
        self.reorg_epoch = generation.reorg_epoch;
        self.session_policy = generation.session_policy;
        self.session_revoke_metadata = generation
            .session_revoke_metadata
            .into_iter()
            .map(|entry| {
                (
                    session_revoke_metadata_key(
                        entry.player_id.as_str(),
                        entry.session_pubkey.as_str(),
                    ),
                    entry.metadata,
                )
            })
            .collect();
        self.rollback_readiness = generation.rollback_readiness;
        self.consumed_strict_audit_nonces = generation.consumed_strict_audit_nonces;
        self.runtime_action_players = generation.runtime_action_players;
        self.consumed_rollback_operator_nonces = generation.consumed_rollback_operator_nonces;
        self.llm_sidecar.agent_player_bindings =
            generation.session_side_effects.agent_player_bindings;
        self.llm_sidecar.player_agent_bindings =
            generation.session_side_effects.player_agent_bindings;
        self.llm_sidecar.agent_public_key_bindings =
            generation.session_side_effects.agent_public_key_bindings;
        self.llm_sidecar.player_auth_last_nonce =
            generation.session_side_effects.player_auth_last_nonce;
        self.llm_sidecar.player_chat_intent_acks = generation
            .session_side_effects
            .player_chat_intent_acks
            .into_iter()
            .map(|entry| {
                (
                    (entry.player_id, entry.agent_id, entry.intent_seq),
                    entry.record,
                )
            })
            .collect();
        self.llm_sidecar.primary_intents = generation.session_side_effects.primary_intents;
        self.pending_virtual_events = generation.session_side_effects.pending_virtual_events;
        self.rebuild_settlement_ranking_gate();
        Ok(())
    }

    pub(super) fn commit_authoritative_recovery_envelope(
        &mut self,
        candidate_world: &RuntimeWorld,
        recovery_metadata: &[u8],
    ) -> Result<
        crate::runtime::AuthoritativeRecoveryCommitStatus,
        crate::runtime::AuthoritativeRecoveryCommitError,
    > {
        let recovery_dir = self.authoritative_recovery_dir().ok_or_else(|| {
            crate::runtime::AuthoritativeRecoveryCommitError::StatusUnknown {
                reason: "authoritative recovery commit has no durable recovery directory"
                    .to_string(),
            }
        })?;
        let recovery_metadata_hash = hex::encode(Sha256::digest(recovery_metadata));
        match candidate_world
            .commit_authoritative_recovery_generation(&recovery_dir, recovery_metadata)
        {
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::Committed(committed)) => {
                let generation_id = committed.generation_id.clone();
                if let Err(error) =
                    self.apply_committed_authoritative_recovery_generation(committed)
                {
                    self.authoritative_recovery_write_fence = Some(recovery_metadata_hash);
                    return Err(
                        crate::runtime::AuthoritativeRecoveryCommitError::StatusUnknown {
                            reason: format!(
                                "committed authoritative recovery envelope could not be applied: {error:?}"
                            ),
                        },
                    );
                }
                Ok(
                    crate::runtime::AuthoritativeRecoveryCommitStatus::Committed(
                        crate::runtime::CommittedAuthoritativeRecoveryGeneration {
                            generation_id,
                            world: self.world.clone(),
                            recovery_metadata: recovery_metadata.to_vec(),
                        },
                    ),
                )
            }
            Ok(status @ crate::runtime::AuthoritativeRecoveryCommitStatus::NotCommitted { .. }) => {
                Ok(status)
            }
            Err(error @ crate::runtime::AuthoritativeRecoveryCommitError::StatusUnknown { .. }) => {
                self.authoritative_recovery_write_fence = Some(recovery_metadata_hash);
                Err(error)
            }
        }
    }

    pub(super) fn resolve_authoritative_recovery_write_fence(
        &mut self,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        let Some(expected_hash) = self.authoritative_recovery_write_fence.clone() else {
            return Ok(());
        };
        let Some(recovery_dir) = self.authoritative_recovery_dir() else {
            return Err(ViewerRuntimeLiveServerError::Init(
                "authoritative recovery write fence has no durable recovery directory".to_string(),
            ));
        };
        match crate::runtime::World::readback_authoritative_recovery_generation(
            &recovery_dir,
            expected_hash.as_str(),
        ) {
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::Committed(committed)) => {
                self.apply_committed_authoritative_recovery_generation(committed)?;
                self.authoritative_recovery_write_fence = None;
            }
            Ok(crate::runtime::AuthoritativeRecoveryCommitStatus::NotCommitted { .. }) => {
                self.authoritative_recovery_write_fence = None;
            }
            Err(crate::runtime::AuthoritativeRecoveryCommitError::StatusUnknown { .. }) => {}
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn set_recovery_fault_injection(
        &mut self,
        fault: Option<RuntimeRecoveryFaultInjection>,
    ) {
        self.recovery_fault_injection = fault;
    }

    #[cfg(test)]
    pub(super) fn set_authoritative_recovery_dir_override(
        &mut self,
        dir: Option<std::path::PathBuf>,
    ) {
        self.authoritative_recovery_dir_override = dir;
    }

    pub(super) fn authoritative_recovery_dir(&self) -> Option<std::path::PathBuf> {
        #[cfg(test)]
        if let Some(dir) = self.authoritative_recovery_dir_override.as_ref() {
            return Some(dir.clone());
        }
        self.config
            .generated_world_dir
            .as_deref()
            .map(|dir| dir.join("runtime-live-authoritative-recovery"))
    }
}
