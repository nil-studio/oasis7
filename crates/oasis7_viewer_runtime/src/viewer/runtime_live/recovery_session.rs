use super::authoritative::compute_runtime_snapshot_hash;
use super::session_policy::RuntimeRecoveryCursor;
use super::*;
use oasis7_simulator::WorldEventKind;

pub(super) struct RuntimeSessionMutationSnapshot {
    session_policy: RuntimeSessionPolicy,
    session_revoke_metadata: BTreeMap<(String, String), RuntimeSessionRevokeMetadata>,
    agent_player_bindings: BTreeMap<String, String>,
    player_agent_bindings: BTreeMap<String, String>,
    agent_public_key_bindings: BTreeMap<String, String>,
    player_auth_last_nonce: BTreeMap<String, u64>,
    player_chat_intent_acks:
        BTreeMap<(String, String, u64), super::control_plane::RuntimeChatIntentAckRecord>,
    primary_intents: BTreeMap<String, super::control_plane::RuntimePrimaryIntent>,
    binding_epoch_by_agent: BTreeMap<String, u64>,
    pending_virtual_events: VecDeque<WorldEvent>,
}

impl ViewerRuntimeLiveServer {
    pub(super) fn plan_player_session_agent_binding(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
        allow_player_rebind: bool,
    ) -> Result<RuntimePlayerBindingPlan, String> {
        if allow_player_rebind {
            if !self.world.state().agents.contains_key(agent_id) {
                return Err(format!("agent not found: {agent_id}"));
            }
        } else {
            control_plane::ensure_agent_player_binding_target_runtime(
                &self.world,
                &self.llm_sidecar,
                agent_id,
                player_id,
                public_key,
            )
            .map_err(|err| err.message)?;
        }
        self.llm_sidecar.plan_agent_player_binding(
            agent_id,
            player_id,
            public_key,
            allow_player_rebind,
        )
    }

    pub(super) fn restore_persisted_session_side_effects(
        llm_sidecar: &mut RuntimeLlmSidecar,
        persisted: &super::recovery_receipt::RuntimePersistedSessionSideEffects,
    ) {
        llm_sidecar.agent_player_bindings = persisted.agent_player_bindings.clone();
        llm_sidecar.player_agent_bindings = persisted.player_agent_bindings.clone();
        llm_sidecar.agent_public_key_bindings = persisted.agent_public_key_bindings.clone();
        llm_sidecar.player_auth_last_nonce = persisted.player_auth_last_nonce.clone();
        llm_sidecar.player_chat_intent_acks = persisted
            .player_chat_intent_acks
            .iter()
            .map(|entry| {
                (
                    (
                        entry.player_id.clone(),
                        entry.agent_id.clone(),
                        entry.intent_seq,
                    ),
                    entry.record.clone(),
                )
            })
            .collect();
        llm_sidecar.primary_intents = persisted.primary_intents.clone();
    }

    pub(super) fn persisted_session_side_effects(
        &self,
    ) -> super::recovery_receipt::RuntimePersistedSessionSideEffects {
        super::recovery_receipt::RuntimePersistedSessionSideEffects {
            agent_player_bindings: self.llm_sidecar.agent_player_bindings.clone(),
            player_agent_bindings: self.llm_sidecar.player_agent_bindings.clone(),
            agent_public_key_bindings: self.llm_sidecar.agent_public_key_bindings.clone(),
            player_auth_last_nonce: self.llm_sidecar.player_auth_last_nonce.clone(),
            player_chat_intent_acks: self
                .llm_sidecar
                .player_chat_intent_acks
                .iter()
                .map(|((player_id, agent_id, intent_seq), record)| {
                    super::recovery_receipt::RuntimePersistedChatIntentAck {
                        player_id: player_id.clone(),
                        agent_id: agent_id.clone(),
                        intent_seq: *intent_seq,
                        record: record.clone(),
                    }
                })
                .collect(),
            primary_intents: self.llm_sidecar.primary_intents.clone(),
            pending_virtual_events: self.pending_virtual_events.clone(),
        }
    }

    pub(super) fn session_mutation_snapshot(&self) -> RuntimeSessionMutationSnapshot {
        RuntimeSessionMutationSnapshot {
            session_policy: self.session_policy.clone(),
            session_revoke_metadata: self.session_revoke_metadata.clone(),
            agent_player_bindings: self.llm_sidecar.agent_player_bindings.clone(),
            player_agent_bindings: self.llm_sidecar.player_agent_bindings.clone(),
            agent_public_key_bindings: self.llm_sidecar.agent_public_key_bindings.clone(),
            player_auth_last_nonce: self.llm_sidecar.player_auth_last_nonce.clone(),
            player_chat_intent_acks: self.llm_sidecar.player_chat_intent_acks.clone(),
            primary_intents: self.llm_sidecar.primary_intents.clone(),
            binding_epoch_by_agent: self.prompt_control_authority.binding_epoch_by_agent.clone(),
            pending_virtual_events: self.pending_virtual_events.clone(),
        }
    }

    pub(super) fn restore_session_mutation_snapshot(
        &mut self,
        snapshot: RuntimeSessionMutationSnapshot,
    ) {
        self.session_policy = snapshot.session_policy;
        self.session_revoke_metadata = snapshot.session_revoke_metadata;
        self.llm_sidecar.agent_player_bindings = snapshot.agent_player_bindings;
        self.llm_sidecar.player_agent_bindings = snapshot.player_agent_bindings;
        self.llm_sidecar.agent_public_key_bindings = snapshot.agent_public_key_bindings;
        self.llm_sidecar.player_auth_last_nonce = snapshot.player_auth_last_nonce;
        self.llm_sidecar.player_chat_intent_acks = snapshot.player_chat_intent_acks;
        self.llm_sidecar.primary_intents = snapshot.primary_intents;
        self.prompt_control_authority.binding_epoch_by_agent = snapshot.binding_epoch_by_agent;
        self.pending_virtual_events = snapshot.pending_virtual_events;
    }

    pub(super) fn current_recovery_cursor(
        &self,
    ) -> Result<RuntimeRecoveryCursor, ViewerRuntimeLiveServerError> {
        if let Some(view) = &self.verified_world_view {
            return Ok(RuntimeRecoveryCursor {
                snapshot_hash: view.version().commit.state_root_ref.clone(),
                snapshot_height: view.version().commit.position,
                log_cursor: view.continuation().sequence,
                stable_batch_id: None,
            });
        }
        let snapshot_hash = compute_runtime_snapshot_hash(&self.world.snapshot())?;
        Ok(RuntimeRecoveryCursor {
            snapshot_hash,
            snapshot_height: self.world.state().time,
            log_cursor: latest_runtime_event_seq(&self.world),
            stable_batch_id: self
                .stable_checkpoints
                .back()
                .map(|entry| entry.batch_id.clone()),
        })
    }

    pub(super) fn clear_player_auth_runtime_state(&mut self, player_id: &str) {
        self.llm_sidecar
            .player_auth_last_nonce
            .remove(player_id.trim());
        self.llm_sidecar
            .clear_chat_intent_acks_for_player(player_id.trim());
    }

    pub(super) fn apply_session_revoke_binding(&mut self, player_id: &str, _revoked_pubkey: &str) {
        if let Some(event) = self.llm_sidecar.clear_player_binding(player_id) {
            if let WorldEventKind::AgentPlayerUnbound { agent_id, .. } = &event {
                self.prompt_control_authority
                    .advance_binding_epoch(agent_id.as_str());
            }
            self.enqueue_virtual_event(event);
        }
    }

    pub(super) fn apply_session_rotate_binding(
        &mut self,
        player_id: &str,
        old_pubkey: &str,
        new_pubkey: &str,
    ) {
        let affected_agents = self
            .llm_sidecar
            .agent_player_bindings
            .iter()
            .filter(|(_, bound_player)| *bound_player == player_id)
            .map(|(agent_id, _)| agent_id.clone())
            .collect::<Vec<_>>();
        for agent_id in affected_agents {
            let should_replace = self
                .llm_sidecar
                .agent_public_key_bindings
                .get(agent_id.as_str())
                .is_none_or(|bound| bound == old_pubkey);
            if should_replace {
                self.llm_sidecar
                    .agent_public_key_bindings
                    .insert(agent_id.clone(), new_pubkey.to_string());
                self.prompt_control_authority
                    .advance_binding_epoch(agent_id.as_str());
            }
        }
    }

    pub(super) fn record_session_revoke_metadata(
        &mut self,
        player_id: &str,
        session_pubkey: &str,
        metadata: RuntimeSessionRevokeMetadata,
    ) {
        self.session_revoke_metadata.insert(
            session_revoke_metadata_key(player_id, session_pubkey),
            metadata,
        );
    }

    #[cfg(test)]
    pub(super) fn bind_player_session_agent(
        &mut self,
        agent_id: &str,
        player_id: &str,
        public_key: Option<&str>,
        allow_player_rebind: bool,
    ) -> Result<(), String> {
        if allow_player_rebind {
            if !self.world.state().agents.contains_key(agent_id) {
                return Err(format!("agent not found: {agent_id}"));
            }
        } else {
            control_plane::ensure_agent_player_binding_target_runtime(
                &self.world,
                &self.llm_sidecar,
                agent_id,
                player_id,
                public_key,
            )
            .map_err(|err| err.message)?;
        }
        for event in self.llm_sidecar.bind_agent_player(
            agent_id,
            player_id,
            public_key,
            allow_player_rebind,
        )? {
            self.enqueue_virtual_event(event);
        }
        Ok(())
    }
}
