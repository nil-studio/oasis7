//! Runtime-local presentation ownership, separate from canonical result storage.
use super::*;
use crate::viewer::VerifiedPlayerAuth;
use oasis7_client_api::world_service::{RequestCorrelation, WorldIdentity};
use std::sync::Weak;

#[path = "gameplay_feedback_runtime.rs"]
mod runtime;

const MAX_SCOPED_GAMEPLAY_FEEDBACK_PLAYERS: usize = 256;
const MAX_PENDING_FEEDBACK_RECIPIENTS: usize = 64;

#[derive(Debug, Default)]
pub(super) struct SessionGameplayFeedback {
    generation: u64,
    owner: Option<GameplayFeedbackOwner>,
    presented_player_id: Option<String>,
    pub(super) feedback: Option<PlayerGameplayRecentFeedback>,
}

#[derive(Debug, Clone)]
struct SessionGameplayFeedbackTarget {
    state: Weak<Mutex<SessionGameplayFeedback>>,
    generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GameplayFeedbackOwner {
    player_id: String,
    public_key: String,
    session_epoch: u64,
    world_id: String,
    service_world: Option<WorldIdentity>,
    branch_id: Option<String>,
    reorg_epoch: u64,
    agent_binding: Option<(String, u64)>,
}

#[derive(Debug, Clone)]
pub(super) struct GameplayFeedbackOrigin {
    owner: GameplayFeedbackOwner,
    order: u64,
    runtime_action_id: Option<u64>,
    local_target: Option<SessionGameplayFeedbackTarget>,
    player_scope_authorized: bool,
}

#[derive(Clone)]
pub(super) struct ScopedGameplayFeedback {
    origin: GameplayFeedbackOrigin,
    feedback: PlayerGameplayRecentFeedback,
}

pub(super) struct PendingGameplayFeedback {
    original: GameplayFeedbackOrigin,
    recipients: Vec<SessionGameplayFeedbackTarget>,
}

impl ViewerRuntimeLiveServer {
    fn gameplay_feedback_owner(
        &self,
        player_id: &str,
        public_key: &str,
    ) -> Option<GameplayFeedbackOwner> {
        let session_epoch = self
            .session_policy
            .validate_known_session_key(player_id, public_key)
            .ok()?;
        let commit = self
            .verified_world_view
            .as_ref()
            .map(|view| &view.version().commit);
        let bound_agent = if let Some(config) = &self.config.world_service {
            self.verified_world_view
                .as_ref()
                .filter(|view| view.version().commit.world == config.expected_world)
                .and_then(|view| {
                    crate::world_service::gameplay::resolve_collect_data_collector(
                        &view.projection().state,
                        player_id,
                        public_key,
                    )
                    .ok()
                })
        } else {
            self.llm_sidecar.bound_agent_for_player(player_id)
        };
        Some(GameplayFeedbackOwner {
            player_id: player_id.to_owned(),
            public_key: public_key.to_owned(),
            session_epoch,
            world_id: self.config.world_id.clone(),
            service_world: self
                .config
                .world_service
                .as_ref()
                .map(|config| config.expected_world.clone()),
            branch_id: commit.map(|commit| commit.binding.branch_id.clone()),
            reorg_epoch: commit.map_or(self.reorg_epoch, |commit| commit.binding.reorg_generation),
            agent_binding: bound_agent.map(|agent| {
                (
                    agent.to_owned(),
                    self.prompt_control_authority.binding_epoch(agent),
                )
            }),
        })
    }

    fn gameplay_feedback_owner_is_current(&self, owner: &GameplayFeedbackOwner) -> bool {
        self.gameplay_feedback_owner(&owner.player_id, &owner.public_key)
            .as_ref()
            == Some(owner)
    }

    fn current_local_feedback_owner(
        &self,
        owner: &GameplayFeedbackOwner,
    ) -> Option<GameplayFeedbackOwner> {
        let current = self.gameplay_feedback_owner(&owner.player_id, &owner.public_key)?;
        let mut pinned = owner.clone();
        // A first service request can precede its first committed projection
        // or create its first claim. Pin only previously unknown optional
        // context; known binding/branch, key, epoch and world never migrate.
        if pinned.branch_id.is_none() {
            pinned.branch_id = current.branch_id.clone();
        }
        if pinned.agent_binding.is_none() {
            pinned.agent_binding = current.agent_binding.clone();
        }
        (pinned == current).then_some(current)
    }

    pub(super) fn new_gameplay_feedback_origin(
        &mut self,
        verified: &VerifiedPlayerAuth,
    ) -> Option<GameplayFeedbackOrigin> {
        let owner = self.gameplay_feedback_owner(&verified.player_id, &verified.public_key)?;
        self.next_gameplay_feedback_order = self.next_gameplay_feedback_order.saturating_add(1);
        Some(GameplayFeedbackOrigin {
            owner,
            order: self.next_gameplay_feedback_order,
            runtime_action_id: None,
            local_target: None,
            player_scope_authorized: false,
        })
    }

    pub(super) fn gameplay_request_feedback_origin(
        &mut self,
        request: &ViewerRequest,
    ) -> Option<GameplayFeedbackOrigin> {
        let verified = match request {
            ViewerRequest::GameplayAction { request } => request.auth.as_ref().and_then(|auth| {
                crate::viewer::verify_gameplay_action_auth_proof(request, auth).ok()
            }),
            ViewerRequest::CollectData { command } => {
                let (crate::viewer::CollectDataCommand::Preflight { request }
                | crate::viewer::CollectDataCommand::Submit { request }) = command;
                request.auth.as_ref().and_then(|auth| {
                    crate::viewer::verify_collect_data_auth_proof(command, auth).ok()
                })
            }
            ViewerRequest::AgentChat { request } => request
                .auth
                .as_ref()
                .and_then(|auth| crate::viewer::verify_agent_chat_auth_proof(request, auth).ok()),
            ViewerRequest::PromptControl { command } => match command.as_ref() {
                crate::viewer::PromptControlCommand::Apply { request } => {
                    request.auth.as_ref().and_then(|auth| {
                        crate::viewer::verify_prompt_control_apply_auth_proof(
                            crate::viewer::PromptControlAuthIntent::Apply,
                            request,
                            auth,
                        )
                        .ok()
                    })
                }
                crate::viewer::PromptControlCommand::Rollback { request } => {
                    request.auth.as_ref().and_then(|auth| {
                        crate::viewer::verify_prompt_control_rollback_auth_proof(request, auth).ok()
                    })
                }
                _ => None,
            },
            _ => None,
        }?;
        // A prepared HTTP submission may finish after another connection has
        // changed this player's key/binding. Keep its original admission fence.
        if let Some(prepared) = &self.prepared_world_service_submission
            && let Some(origin) = &prepared.feedback_origin
        {
            return Some(origin.clone());
        }
        self.new_gameplay_feedback_origin(&verified)
    }

    pub(super) fn begin_gameplay_feedback_request(
        &self,
        session: &mut RuntimeLiveSession,
        mut origin: Option<GameplayFeedbackOrigin>,
    ) -> Option<GameplayFeedbackOrigin> {
        if let Ok(mut local) = session.local_gameplay_feedback.lock() {
            local.generation = local.generation.saturating_add(1);
            local.owner = origin.as_ref().map(|origin| origin.owner.clone());
            local.presented_player_id = session.current_player_id.clone();
            local.feedback = None;
            if let Some(origin) = &mut origin {
                origin.local_target = Some(SessionGameplayFeedbackTarget {
                    state: Arc::downgrade(&session.local_gameplay_feedback),
                    generation: local.generation,
                });
            }
        }
        origin
    }

    pub(super) fn gameplay_feedback_session_matches(
        &self,
        session: &RuntimeLiveSession,
        origin: Option<&GameplayFeedbackOrigin>,
    ) -> bool {
        origin.is_some_and(|origin| {
            session.gameplay_feedback_owner.as_ref() == Some(&origin.owner)
                && session.current_player_id.as_deref() == Some(origin.owner.player_id.as_str())
                && self.gameplay_feedback_owner_is_current(&origin.owner)
        })
    }

    pub(super) fn clear_session_gameplay_feedback(&self, session: &mut RuntimeLiveSession) {
        session.gameplay_feedback_owner = None;
        if let Ok(mut local) = session.local_gameplay_feedback.lock() {
            local.generation = local.generation.saturating_add(1);
            local.owner = None;
            local.presented_player_id = session.current_player_id.clone();
            local.feedback = None;
        }
    }

    pub(super) fn set_gameplay_feedback_session_player(
        &self,
        session: &mut RuntimeLiveSession,
        player_id: String,
    ) {
        if session
            .gameplay_feedback_owner
            .as_ref()
            .is_some_and(|owner| owner.player_id != player_id)
        {
            session.gameplay_feedback_owner = None;
        }
        session.current_player_id = Some(player_id);
        if let Ok(mut local) = session.local_gameplay_feedback.lock() {
            local.presented_player_id = session.current_player_id.clone();
        }
    }

    // Call only after the request's existing authentication nonce has been
    // consumed. Signature/known-key checks alone do not authorize private reads.
    pub(super) fn authorize_active_gameplay_feedback(&mut self) {
        self.active_gameplay_feedback_authorized = true;
    }

    pub(super) fn finish_gameplay_feedback_request(
        &mut self,
        session: &mut RuntimeLiveSession,
    ) -> Option<GameplayFeedbackOrigin> {
        let mut origin = self.active_gameplay_feedback_origin.take();
        let authorized = std::mem::take(&mut self.active_gameplay_feedback_authorized);
        if let Some(origin) = &mut origin {
            origin.player_scope_authorized =
                authorized && self.gameplay_feedback_owner_is_current(&origin.owner);
        }
        if authorized
            && let Some(origin) = origin
                .as_ref()
                .filter(|origin| self.gameplay_feedback_owner_is_current(&origin.owner))
        {
            self.set_gameplay_feedback_session_player(session, origin.owner.player_id.clone());
            session.gameplay_feedback_owner = Some(origin.owner.clone());
            if let Ok(mut local) = session.local_gameplay_feedback.lock() {
                local.feedback = None;
            }
        }
        origin
    }

    pub(super) fn set_verified_player_gameplay_feedback(
        &mut self,
        verified: &VerifiedPlayerAuth,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        self.set_verified_player_gameplay_feedback_for_action(verified, None, feedback);
    }

    pub(super) fn set_verified_player_gameplay_feedback_for_action(
        &mut self,
        verified: &VerifiedPlayerAuth,
        runtime_action_id: Option<u64>,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        let previous = self.active_gameplay_feedback_origin.clone();
        let origin = previous
            .clone()
            .filter(|origin| {
                origin.owner.player_id == verified.player_id
                    && origin.owner.public_key == verified.public_key
                    && self.gameplay_feedback_owner_is_current(&origin.owner)
            })
            .or_else(|| self.new_gameplay_feedback_origin(verified));
        if let Some(mut origin) = origin {
            origin.runtime_action_id = runtime_action_id;
            if let Some(previous) = previous {
                origin.local_target = previous.local_target;
            }
            if self.active_gameplay_feedback_origin.is_some() {
                self.active_gameplay_feedback_origin = Some(origin.clone());
            }
            self.set_request_gameplay_feedback(origin, feedback);
        }
    }

    pub(super) fn set_request_gameplay_feedback(
        &mut self,
        origin: GameplayFeedbackOrigin,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        if self.config.world_service.is_none() || self.active_gameplay_feedback_authorized {
            self.set_scoped_gameplay_feedback(origin.clone(), feedback.clone());
            // Fresh legacy Prompt/Chat successes emit here after their nonce
            // checks. Their replay paths do not emit or grant historical reads.
            self.set_local_request_gameplay_feedback(&origin, feedback);
        } else {
            // WorldService submit is replayable by original correlation. It
            // cannot upgrade an unauthenticated connection to a private reader.
            self.set_local_request_gameplay_feedback(&origin, feedback);
        }
    }

    fn set_local_request_gameplay_feedback(
        &self,
        origin: &GameplayFeedbackOrigin,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        if self.current_local_feedback_owner(&origin.owner).is_none() {
            return;
        }
        if let Some(target) = &origin.local_target
            && let Some(state) = target.state.upgrade()
            && let Ok(mut local) = state.lock()
            && local.generation == target.generation
            && let Some(owner) = local
                .owner
                .as_ref()
                .and_then(|owner| self.current_local_feedback_owner(owner))
        {
            local.owner = Some(owner);
            local.feedback = Some(feedback);
        }
    }

    pub(super) fn set_scoped_gameplay_feedback(
        &mut self,
        origin: GameplayFeedbackOrigin,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        if !self.gameplay_feedback_owner_is_current(&origin.owner) {
            return;
        }
        if self
            .scoped_gameplay_feedback
            .get(&origin.owner.player_id)
            .is_some_and(|current| {
                current.origin.owner == origin.owner && current.origin.order > origin.order
            })
        {
            return;
        }
        self.scoped_gameplay_feedback.insert(
            origin.owner.player_id.clone(),
            ScopedGameplayFeedback { origin, feedback },
        );
        self.prune_scoped_gameplay_feedback();
    }

    pub(super) fn prune_scoped_gameplay_feedback(&mut self) {
        let stale: Vec<_> = self
            .scoped_gameplay_feedback
            .iter()
            .filter(|(_, record)| !self.gameplay_feedback_owner_is_current(&record.origin.owner))
            .map(|(player, _)| player.clone())
            .collect();
        for player in stale {
            self.scoped_gameplay_feedback.remove(&player);
        }
        while self.scoped_gameplay_feedback.len() > MAX_SCOPED_GAMEPLAY_FEEDBACK_PLAYERS {
            let oldest = self
                .scoped_gameplay_feedback
                .iter()
                .min_by_key(|(_, record)| record.origin.order)
                .map(|(player, _)| player.clone());
            if let Some(player) = oldest {
                self.scoped_gameplay_feedback.remove(&player);
            }
        }
    }

    pub(super) fn record_session_gameplay_rejection(
        &mut self,
        session: &mut RuntimeLiveSession,
        error: &GameplayActionError,
        origin: Option<GameplayFeedbackOrigin>,
    ) {
        let feedback = Self::gameplay_action_rejection_feedback(error);
        if let Some(origin) = origin {
            if origin.player_scope_authorized {
                self.set_scoped_gameplay_feedback(origin, feedback);
            } else {
                self.set_local_request_gameplay_feedback(&origin, feedback);
            }
            return;
        }
        // This connection may show its own authentication error without
        // writing any player's feedback or replacing a world-wide fault.
        if let Ok(mut local) = session.local_gameplay_feedback.lock() {
            local.feedback = Some(feedback);
        }
    }

    pub(super) fn pending_gameplay_feedback_origin(
        &mut self,
        correlation: &RequestCorrelation,
        origin: Option<GameplayFeedbackOrigin>,
    ) {
        if let Some(mut origin) = origin
            && let Ok(key) = crate::world_service::correlation::key_digest(&correlation.key)
        {
            // A retry retains the first correlation fence. A fresh reader may
            // have acquired newer optional context since that first request;
            // in that case only this request's local sink can receive its result.
            origin.player_scope_authorized = self.active_gameplay_feedback_authorized
                && self.gameplay_feedback_owner_is_current(&origin.owner)
                && self
                    .scoped_gameplay_feedback
                    .get(&origin.owner.player_id)
                    .is_some_and(|current| {
                        current.origin.owner == origin.owner && current.origin.order == origin.order
                    });
            let target = origin.local_target.take();
            let record = self
                .pending_gameplay_feedback
                .entry((key, correlation.payload_digest.clone()))
                .or_insert_with(|| PendingGameplayFeedback {
                    original: origin.clone(),
                    recipients: Vec::new(),
                });
            record.original.player_scope_authorized |= origin.player_scope_authorized;
            record.recipients.retain(|existing| {
                let Some(state) = existing.state.upgrade() else {
                    return false;
                };
                let current = state
                    .lock()
                    .ok()
                    .is_some_and(|local| local.generation == existing.generation);
                current
                    && target
                        .as_ref()
                        .is_none_or(|target| !Weak::ptr_eq(&target.state, &existing.state))
            });
            if !origin.player_scope_authorized
                && let Some(target) = target
            {
                if record.recipients.len() == MAX_PENDING_FEEDBACK_RECIPIENTS {
                    record.recipients.remove(0);
                }
                record.recipients.push(target);
            }
        }
    }

    pub(super) fn remember_pending_world_service_gameplay(
        &mut self,
        correlation: RequestCorrelation,
        payload: crate::world_service::WorldServicePayloadV1,
    ) {
        if !self
            .pending_world_service_gameplay
            .iter()
            .any(|(current, _)| current == &correlation)
        {
            self.pending_world_service_gameplay
                .push((correlation, payload));
        }
    }

    pub(super) fn reuse_pending_gameplay_feedback_origin(
        &self,
        correlation: &RequestCorrelation,
        mut candidate: Option<GameplayFeedbackOrigin>,
    ) -> Option<GameplayFeedbackOrigin> {
        if let Ok(key) = crate::world_service::correlation::key_digest(&correlation.key)
            && let Some(record) = self
                .pending_gameplay_feedback
                .get(&(key, correlation.payload_digest.clone()))
            && let Some(candidate) = &mut candidate
        {
            // Retrying the same signed request keeps its first ordering and
            // authority fence, while each live connection retains its own sink.
            candidate.owner = record.original.owner.clone();
            candidate.order = record.original.order;
        }
        candidate
    }

    pub(super) fn complete_pending_gameplay_feedback(
        &mut self,
        correlation: &RequestCorrelation,
        feedback: PlayerGameplayRecentFeedback,
    ) {
        let Ok(key) = crate::world_service::correlation::key_digest(&correlation.key) else {
            return;
        };
        if let Some(record) = self
            .pending_gameplay_feedback
            .remove(&(key, correlation.payload_digest.clone()))
        {
            if record.original.player_scope_authorized
                && self
                    .scoped_gameplay_feedback
                    .get(&record.original.owner.player_id)
                    .is_some_and(|current| {
                        current.origin.owner == record.original.owner
                            && current.origin.order == record.original.order
                    })
            {
                self.set_scoped_gameplay_feedback(record.original.clone(), feedback.clone());
            }
            for target in record.recipients {
                let mut origin = record.original.clone();
                origin.local_target = Some(target);
                self.set_local_request_gameplay_feedback(&origin, feedback.clone());
            }
        }
        // A missing original owner is not a public/world notification. The
        // canonical result remains available through the existing signed lookup.
    }

    pub(super) fn scoped_feedback_for_player(
        &self,
        player_id: Option<&str>,
    ) -> Option<PlayerGameplayRecentFeedback> {
        let record = self.scoped_gameplay_feedback.get(player_id?)?;
        self.gameplay_feedback_owner_is_current(&record.origin.owner)
            .then(|| record.feedback.clone())
    }

    pub(super) fn compat_snapshot_for_session(
        &mut self,
        session: &mut RuntimeLiveSession,
    ) -> WorldSnapshot {
        self.prune_scoped_gameplay_feedback();
        if session
            .gameplay_feedback_owner
            .as_ref()
            .is_some_and(|owner| {
                session.current_player_id.as_deref() != Some(owner.player_id.as_str())
                    || !self.gameplay_feedback_owner_is_current(owner)
            })
        {
            session.gameplay_feedback_owner = None;
        }
        let private = session
            .gameplay_feedback_owner
            .as_ref()
            .and_then(|owner| {
                self.scoped_gameplay_feedback
                    .get(&owner.player_id)
                    .filter(|record| record.origin.owner == *owner)
            })
            .map(|record| record.feedback.clone());
        let local = session
            .local_gameplay_feedback
            .lock()
            .ok()
            .and_then(|mut local| {
                let owner = local
                    .owner
                    .as_ref()
                    .and_then(|owner| self.current_local_feedback_owner(owner));
                if local.presented_player_id != session.current_player_id
                    || (local.owner.is_some() && owner.is_none())
                {
                    local.generation = local.generation.saturating_add(1);
                    local.owner = None;
                    local.presented_player_id = session.current_player_id.clone();
                    local.feedback = None;
                } else {
                    local.owner = owner;
                }
                local.feedback.clone()
            });
        let feedback = local.or(private);
        self.compat_snapshot_with_feedback(session.current_player_id.as_deref(), feedback.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_client_api::world_service::RequestKey;

    fn server_and_subject() -> (ViewerRuntimeLiveServer, VerifiedPlayerAuth) {
        let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
            WorldScenario::Minimal,
        ))
        .unwrap();
        server
            .session_policy
            .register_session("owner", "key-a")
            .unwrap();
        (
            server,
            VerifiedPlayerAuth {
                player_id: "owner".into(),
                public_key: "key-a".into(),
                nonce: 1,
                hosted_registration_nonce: None,
            },
        )
    }

    fn correlation(id: &str) -> RequestCorrelation {
        RequestCorrelation {
            key: RequestKey {
                world: WorldIdentity {
                    world_id: "world".into(),
                    genesis_digest: "genesis".into(),
                },
                verified_subject: "key-a".into(),
                operation_domain: "gameplay".into(),
                nonce_scope: "player_auth".into(),
                request_id_or_nonce: id.into(),
            },
            payload_digest: format!("digest-{id}"),
        }
    }

    fn feedback(stage: &str) -> PlayerGameplayRecentFeedback {
        ViewerRuntimeLiveServer::make_player_gameplay_feedback(
            "world_service_intent",
            stage,
            "request disposition",
            None,
            None,
            None,
            None,
            0,
            0,
        )
    }

    #[test]
    fn delayed_result_reaches_only_original_live_connection_generation() {
        let (mut server, verified) = server_and_subject();
        let mut sender = RuntimeLiveSession::new();
        let mut other = RuntimeLiveSession::new();
        let origin = server.new_gameplay_feedback_origin(&verified);
        let origin = server
            .begin_gameplay_feedback_request(&mut sender, origin)
            .unwrap();
        server.set_local_request_gameplay_feedback(&origin, feedback("pending"));
        server.pending_gameplay_feedback_origin(&correlation("first"), Some(origin));
        server.complete_pending_gameplay_feedback(&correlation("first"), feedback("committed"));
        assert_eq!(
            sender
                .local_gameplay_feedback
                .lock()
                .unwrap()
                .feedback
                .as_ref()
                .unwrap()
                .stage,
            "committed"
        );
        assert!(
            other
                .local_gameplay_feedback
                .lock()
                .unwrap()
                .feedback
                .is_none()
        );
        assert!(
            sender.gameplay_feedback_owner.is_none(),
            "request result does not grant historical reads"
        );
        assert!(server.scoped_gameplay_feedback.is_empty());
        assert!(server.latest_player_gameplay_feedback.is_none());

        let origin = server.new_gameplay_feedback_origin(&verified);
        let origin = server
            .begin_gameplay_feedback_request(&mut sender, origin)
            .unwrap();
        server.pending_gameplay_feedback_origin(&correlation("obsolete"), Some(origin));
        server.begin_gameplay_feedback_request(&mut sender, None);
        server.record_session_gameplay_rejection(
            &mut sender,
            &GameplayActionError {
                code: "auth_proof_required".into(),
                message: "new request".into(),
                action_id: None,
                target_agent_id: None,
            },
            None,
        );
        server.complete_pending_gameplay_feedback(&correlation("obsolete"), feedback("committed"));
        assert_eq!(
            sender
                .local_gameplay_feedback
                .lock()
                .unwrap()
                .feedback
                .as_ref()
                .unwrap()
                .stage,
            "rejected"
        );

        let origin = server.new_gameplay_feedback_origin(&verified);
        let origin = server
            .begin_gameplay_feedback_request(&mut other, origin)
            .unwrap();
        let weak = Arc::downgrade(&other.local_gameplay_feedback);
        server.pending_gameplay_feedback_origin(&correlation("disconnected"), Some(origin));
        drop(other);
        assert!(
            weak.upgrade().is_none(),
            "pending result does not keep a disconnected session alive"
        );
        server.complete_pending_gameplay_feedback(
            &correlation("disconnected"),
            feedback("committed"),
        );
        assert!(server.pending_gameplay_feedback.is_empty());
        assert!(server.latest_player_gameplay_feedback.is_none());
    }

    #[test]
    fn delayed_player_result_cannot_replace_newer_request_or_cross_an_authority_fence() {
        for change in [
            "newer_request",
            "rotate",
            "revoke",
            "rebind",
            "reorg",
            "world",
        ] {
            let (mut server, verified) = server_and_subject();
            server
                .llm_sidecar
                .bind_agent_player("agent", "owner", Some("key-a"), true)
                .unwrap();
            let origin = server.new_gameplay_feedback_origin(&verified).unwrap();
            server.set_scoped_gameplay_feedback(origin.clone(), feedback("pending"));
            server.active_gameplay_feedback_authorized = true;
            server.pending_gameplay_feedback_origin(&correlation("original"), Some(origin));
            server.active_gameplay_feedback_authorized = false;
            match change {
                "newer_request" => {
                    let newer = server.new_gameplay_feedback_origin(&verified).unwrap();
                    server.set_scoped_gameplay_feedback(newer, feedback("newer"));
                }
                "rotate" => {
                    server
                        .session_policy
                        .rotate_session("owner", "key-a", "key-b")
                        .unwrap();
                }
                "revoke" => {
                    server
                        .session_policy
                        .revoke_session("owner", Some("key-a"))
                        .unwrap();
                }
                "rebind" => {
                    server
                        .prompt_control_authority
                        .advance_binding_epoch("agent");
                }
                "reorg" => {
                    server.reorg_epoch += 1;
                }
                "world" => {
                    server.config.world_id = "other-world".into();
                }
                _ => unreachable!(),
            }
            server.complete_pending_gameplay_feedback(
                &correlation("original"),
                feedback("committed"),
            );
            let current = server.scoped_feedback_for_player(Some("owner"));
            if change == "newer_request" {
                assert_eq!(current.unwrap().stage, "newer");
            } else {
                assert!(current.is_none(), "stale feedback survived {change}");
            }
            assert!(server.latest_player_gameplay_feedback.is_none());
        }
    }

    #[test]
    fn local_pending_result_is_cancelled_by_identity_change_or_rebind() {
        for rebind in [false, true] {
            let (mut server, verified) = server_and_subject();
            server
                .llm_sidecar
                .bind_agent_player("agent", "owner", Some("key-a"), true)
                .unwrap();
            let mut session = RuntimeLiveSession::new();
            let origin = server.new_gameplay_feedback_origin(&verified);
            let origin = server.begin_gameplay_feedback_request(&mut session, origin);
            server.pending_gameplay_feedback_origin(&correlation("pending"), origin);
            if rebind {
                server
                    .prompt_control_authority
                    .advance_binding_epoch("agent");
            } else {
                session.current_player_id = Some("other-owner".into());
                server.clear_session_gameplay_feedback(&mut session);
            }
            server
                .complete_pending_gameplay_feedback(&correlation("pending"), feedback("committed"));
            assert!(
                session
                    .local_gameplay_feedback
                    .lock()
                    .unwrap()
                    .feedback
                    .is_none()
            );
            assert!(server.scoped_gameplay_feedback.is_empty());
        }
    }

    #[test]
    fn scoped_feedback_cache_is_bounded_and_removes_invalid_owners() {
        let (mut server, _) = server_and_subject();
        for index in 0..=MAX_SCOPED_GAMEPLAY_FEEDBACK_PLAYERS {
            let player = format!("owner-{index}");
            server
                .session_policy
                .register_session(&player, "key-a")
                .unwrap();
            let verified = VerifiedPlayerAuth {
                player_id: player,
                public_key: "key-a".into(),
                nonce: 1,
                hosted_registration_nonce: None,
            };
            let origin = server.new_gameplay_feedback_origin(&verified).unwrap();
            server.set_scoped_gameplay_feedback(origin, feedback("queued"));
        }
        assert_eq!(
            server.scoped_gameplay_feedback.len(),
            MAX_SCOPED_GAMEPLAY_FEEDBACK_PLAYERS
        );
        assert!(server.scoped_feedback_for_player(Some("owner-0")).is_none());
        server
            .session_policy
            .revoke_session("owner-1", Some("key-a"))
            .unwrap();
        server.prune_scoped_gameplay_feedback();
        assert!(!server.scoped_gameplay_feedback.contains_key("owner-1"));
    }

    #[test]
    fn repeated_pending_correlation_keeps_each_connection_and_original_order() {
        let (mut server, verified) = server_and_subject();
        let mut first = RuntimeLiveSession::new();
        let mut second = RuntimeLiveSession::new();
        let original = server.new_gameplay_feedback_origin(&verified).unwrap();
        let original_order = original.order;
        let original = server.begin_gameplay_feedback_request(&mut first, Some(original));
        server.pending_gameplay_feedback_origin(&correlation("same"), original);
        let retry = server.new_gameplay_feedback_origin(&verified);
        let retry = server.begin_gameplay_feedback_request(&mut second, retry);
        let retry = server
            .reuse_pending_gameplay_feedback_origin(&correlation("same"), retry)
            .unwrap();
        assert_eq!(retry.order, original_order);
        server.pending_gameplay_feedback_origin(&correlation("same"), Some(retry));
        server.complete_pending_gameplay_feedback(&correlation("same"), feedback("committed"));
        for session in [&first, &second] {
            assert_eq!(
                session
                    .local_gameplay_feedback
                    .lock()
                    .unwrap()
                    .feedback
                    .as_ref()
                    .unwrap()
                    .stage,
                "committed"
            );
            assert!(session.gameplay_feedback_owner.is_none());
        }
        assert!(server.scoped_gameplay_feedback.is_empty());
    }

    #[test]
    fn authorized_retry_of_initial_unknown_owner_keeps_its_local_terminal_recipient() {
        let (mut server, verified) = server_and_subject();
        let key = correlation("initially-unbound");
        let mut session = RuntimeLiveSession::new();
        let origin = server.new_gameplay_feedback_origin(&verified);
        let origin = server.begin_gameplay_feedback_request(&mut session, origin);
        server.pending_gameplay_feedback_origin(&key, origin);

        server
            .llm_sidecar
            .bind_agent_player("agent", "owner", Some("key-a"), true)
            .unwrap();
        let fresh = server.new_gameplay_feedback_origin(&verified);
        server.active_gameplay_feedback_origin =
            server.begin_gameplay_feedback_request(&mut session, fresh);
        server.authorize_active_gameplay_feedback();
        server.finish_gameplay_feedback_request(&mut session);
        assert!(session.gameplay_feedback_owner.is_some());

        let candidate = server.new_gameplay_feedback_origin(&verified);
        assert!(server.gameplay_feedback_session_matches(&session, candidate.as_ref()));
        let candidate = server.begin_gameplay_feedback_request(&mut session, candidate);
        let original = server
            .reuse_pending_gameplay_feedback_origin(&key, candidate)
            .unwrap();
        assert!(
            original.owner.agent_binding.is_none(),
            "first fence stays immutable"
        );
        server.active_gameplay_feedback_authorized = true;
        server.active_gameplay_feedback_origin = Some(original.clone());
        server.set_request_gameplay_feedback(original.clone(), feedback("pending"));
        server.pending_gameplay_feedback_origin(&key, Some(original));
        server.finish_gameplay_feedback_request(&mut session);
        assert!(
            server.scoped_gameplay_feedback.is_empty(),
            "no historical-read widening"
        );
        assert_eq!(
            server
                .pending_gameplay_feedback
                .values()
                .next()
                .unwrap()
                .recipients
                .len(),
            1
        );
        server.complete_pending_gameplay_feedback(&key, feedback("committed"));
        assert_eq!(
            session
                .local_gameplay_feedback
                .lock()
                .unwrap()
                .feedback
                .as_ref()
                .unwrap()
                .stage,
            "committed"
        );
    }

    #[test]
    fn changing_ack_player_clears_private_reader_even_without_a_snapshot_subscription() {
        let (mut server, verified) = server_and_subject();
        let mut session = RuntimeLiveSession::new();
        let origin = server.new_gameplay_feedback_origin(&verified);
        server.active_gameplay_feedback_origin =
            server.begin_gameplay_feedback_request(&mut session, origin);
        server.authorize_active_gameplay_feedback();
        server.finish_gameplay_feedback_request(&mut session);
        assert!(session.subscribed.is_empty());
        let previous = server.new_gameplay_feedback_origin(&verified);
        assert!(server.gameplay_feedback_session_matches(&session, previous.as_ref()));
        server.set_gameplay_feedback_session_player(&mut session, "other-owner".into());
        assert!(session.gameplay_feedback_owner.is_none());
        assert!(!server.gameplay_feedback_session_matches(&session, previous.as_ref()));
    }

    #[test]
    fn pending_recipients_prune_retries_and_disconnects_without_replacing_original_fence() {
        let (mut server, verified) = server_and_subject();
        let key = correlation("bounded");
        let mut session = RuntimeLiveSession::new();
        let original = server.new_gameplay_feedback_origin(&verified).unwrap();
        let order = original.order;
        let original = server.begin_gameplay_feedback_request(&mut session, Some(original));
        server.pending_gameplay_feedback_origin(&key, original);
        for _ in 0..128 {
            let origin = server.new_gameplay_feedback_origin(&verified);
            let origin = server.begin_gameplay_feedback_request(&mut session, origin);
            let origin = server.reuse_pending_gameplay_feedback_origin(&key, origin);
            server.pending_gameplay_feedback_origin(&key, origin);
        }
        let record = server.pending_gameplay_feedback.values().next().unwrap();
        assert_eq!(record.original.order, order);
        assert_eq!(record.recipients.len(), 1);
        drop(session);
        let mut sessions = Vec::new();
        for _ in 0..=MAX_PENDING_FEEDBACK_RECIPIENTS {
            let mut session = RuntimeLiveSession::new();
            let origin = server.new_gameplay_feedback_origin(&verified);
            let origin = server.begin_gameplay_feedback_request(&mut session, origin);
            let origin = server.reuse_pending_gameplay_feedback_origin(&key, origin);
            server.pending_gameplay_feedback_origin(&key, origin);
            sessions.push(session);
        }
        let record = server.pending_gameplay_feedback.values().next().unwrap();
        assert_eq!(record.original.order, order);
        assert_eq!(record.recipients.len(), MAX_PENDING_FEEDBACK_RECIPIENTS);
        let payload = crate::world_service::WorldServicePayloadV1::GameplayJson(vec![]);
        server.remember_pending_world_service_gameplay(key.clone(), payload.clone());
        server.remember_pending_world_service_gameplay(key.clone(), payload);
        assert_eq!(server.pending_world_service_gameplay.len(), 1);
        server.complete_pending_gameplay_feedback(&key, feedback("committed"));
        assert_eq!(
            sessions
                .last()
                .unwrap()
                .local_gameplay_feedback
                .lock()
                .unwrap()
                .feedback
                .as_ref()
                .unwrap()
                .stage,
            "committed"
        );
        assert!(server.pending_gameplay_feedback.is_empty());
    }

    #[test]
    fn evicted_player_feedback_is_not_recreated_by_an_old_terminal_result() {
        let (mut server, verified) = server_and_subject();
        let original = server.new_gameplay_feedback_origin(&verified).unwrap();
        server.set_scoped_gameplay_feedback(original.clone(), feedback("pending"));
        server.active_gameplay_feedback_authorized = true;
        server.pending_gameplay_feedback_origin(&correlation("evicted"), Some(original));
        server.active_gameplay_feedback_authorized = false;
        for index in 0..MAX_SCOPED_GAMEPLAY_FEEDBACK_PLAYERS {
            let player = format!("replacement-{index}");
            server
                .session_policy
                .register_session(&player, "key-a")
                .unwrap();
            let verified = VerifiedPlayerAuth {
                player_id: player,
                public_key: "key-a".into(),
                nonce: 1,
                hosted_registration_nonce: None,
            };
            let origin = server.new_gameplay_feedback_origin(&verified).unwrap();
            server.set_scoped_gameplay_feedback(origin, feedback("newer"));
        }
        assert!(server.scoped_feedback_for_player(Some("owner")).is_none());
        server
            .session_policy
            .revoke_session("replacement-0", Some("key-a"))
            .unwrap();
        server.prune_scoped_gameplay_feedback();
        server.complete_pending_gameplay_feedback(&correlation("evicted"), feedback("committed"));
        assert!(server.scoped_feedback_for_player(Some("owner")).is_none());
    }
}
