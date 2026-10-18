use super::*;
use crate::world_service::{SignedReadRequest, VIEW_PATH, authority};
use oasis7_client_api::world_service::{ReadWorldViewRequest, WORLD_SERVICE_CONTRACT_VERSION};
use oasis7_proto::viewer::CanonicalAgentOwnerFenceV1;

pub(super) type PreparedOwnerRead = (
    SignedReadRequest<ReadWorldViewRequest>,
    Result<crate::world_service::verified_view::VerifiedWorldView, String>,
);

#[derive(Clone)]
pub(super) struct OwnerReadContext {
    pub request: ReadWorldViewRequest,
    pub fence: Option<CanonicalAgentOwnerFenceV1>,
    pub expected: OwnerReadExpected,
}

#[derive(Clone, PartialEq, serde::Serialize)]
pub(super) struct OwnerReadExpected {
    agent_id: String,
    player_id: String,
    public_key: String,
    world_id: String,
    branch_id: String,
    reorg_epoch: u64,
}
impl OwnerReadExpected {
    fn matches(&self, view: &oasis7_proto::viewer::CanonicalAgentChatViewV1) -> bool {
        self.agent_id == view.agent_id
            && self.player_id == view.player_id
            && self.public_key == view.public_key
            && self.world_id == view.world_id
            && self.branch_id == view.canonical_authority.branch_id
            && self.reorg_epoch == view.reorg_epoch
            && view.canonical_authority.agent_identity_generation > 0
            && view.authority_scope == "player_agent_chat"
    }
}

impl ViewerRuntimeLiveServer {
    pub(super) fn prepare_shared_owner_read(
        shared: &std::sync::Arc<std::sync::Mutex<Self>>,
        request: &ViewerRequest,
        session: &RuntimeLiveSession,
    ) -> Result<Option<PreparedOwnerRead>, ViewerRuntimeLiveServerError> {
        let ViewerRequest::CanonicalAgentOwnerRead { request } = request else {
            return Ok(None);
        };
        let signed: SignedReadRequest<ReadWorldViewRequest> =
            serde_json::from_value(request.clone())
                .map_err(|error| ViewerRuntimeLiveServerError::Serde(error.to_string()))?;
        let client = {
            let server = lock_shared_server(shared)?;
            authority::verify_read_request(VIEW_PATH, &signed)
                .map_err(ViewerRuntimeLiveServerError::Init)?;
            let context = session.owner_read_context.as_ref().ok_or_else(|| {
                ViewerRuntimeLiveServerError::Init("owner read context required".into())
            })?;
            let current = server.current_owner_read_context(&context.expected.agent_id, session)?;
            if current.expected != context.expected
                || current.fence != context.fence
                || signed.request != context.request
                || signed.subject_public_key != context.expected.public_key
            {
                return Err(ViewerRuntimeLiveServerError::Init(
                    "owner read stale or foreign original request".into(),
                ));
            }
            server.world_service_client()?.ok_or_else(|| {
                ViewerRuntimeLiveServerError::Init("owner read requires WorldService".into())
            })?
        };
        let result = client
            .read_owner_view(signed.clone())
            .map_err(|error| error.to_string());
        Ok(Some((signed, result)))
    }

    pub(super) fn invalidate_owner_read_session(&self, session: &mut RuntimeLiveSession) {
        let valid = if let Some(view) = session.owner_read_view.as_ref() {
            self.current_owner_read_context(&view.agent_id, session)
                .is_ok_and(|current| {
                    current.expected.matches(view)
                        && current.request.min_commit.as_ref().is_none_or(|minimum| {
                            session.owner_read_commit.as_ref().is_some_and(|commit| {
                                commit.satisfies_minimum(minimum).unwrap_or(false)
                            })
                        })
                        && current
                            .fence
                            .as_ref()
                            .is_none_or(|fence| fence == &CanonicalAgentOwnerFenceV1::from(view))
                })
        } else if let Some(context) = session.owner_read_context.as_ref() {
            self.current_owner_read_context(&context.expected.agent_id, session)
                .is_ok_and(|current| {
                    current.expected == context.expected && current.fence == context.fence
                })
        } else {
            true
        };
        if !valid {
            session.owner_read_view = None;
            session.owner_read_commit = None;
            session.owner_read_context = None;
        }
    }

    fn current_owner_read_context(
        &self,
        agent: &str,
        session: &RuntimeLiveSession,
    ) -> Result<OwnerReadContext, ViewerRuntimeLiveServerError> {
        let fail = |message: &str| ViewerRuntimeLiveServerError::Init(message.into());
        let verified = self
            .verified_world_view
            .as_ref()
            .ok_or_else(|| fail("verified owner read context unavailable"))?;
        if agent.trim().is_empty() || !verified.projection().state.agents.contains_key(agent) {
            return Err(fail("owner read Agent unavailable"));
        }
        let player = session
            .current_player_id
            .as_deref()
            .ok_or_else(|| fail("authenticated owner read session required"))?;
        let key = self
            .session_policy
            .active_session_public_key(player)
            .ok_or_else(|| fail("owner read session key unavailable"))?;
        self.session_policy
            .validate_known_session_key(player, key)
            .map_err(|_| fail("owner read session key mismatch"))?;
        let client = self
            .world_service_client()?
            .ok_or_else(|| fail("owner read requires WorldService"))?;
        if client.config().expected_world != verified.version().commit.world {
            return Err(fail("owner read current world mismatch"));
        }
        let expected = OwnerReadExpected {
            agent_id: agent.into(),
            player_id: player.into(),
            public_key: key.into(),
            world_id: verified.version().commit.world.world_id.clone(),
            branch_id: verified.version().commit.binding.branch_id.clone(),
            reorg_epoch: verified.version().commit.binding.reorg_generation,
        };
        let fence = verified
            .projection()
            .canonical_agent_owner
            .as_ref()
            .filter(|fence| fence.agent_id == agent)
            .cloned();
        if fence.as_ref().is_some_and(|fence| {
            fence.player_id != player
                || fence.public_key != key
                || fence.world_id != expected.world_id
                || fence.reorg_epoch != expected.reorg_epoch
                || fence.canonical_authority.branch_id != expected.branch_id
        }) {
            return Err(fail("owner read current owner fence mismatch"));
        }
        Ok(OwnerReadContext {
            request: client
                .prepare_read_view_request(ReadWorldViewRequest {
                    observation_nonce: None,
                    formal_anchor: None,
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    world: client.config().expected_world.clone(),
                    scope_id: format!("agent:{agent}"),
                    min_commit: Some(verified.version().commit.clone()),
                    fixed_commit: None,
                    deadline_unix_ms: None,
                })
                .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))?,
            fence,
            expected,
        })
    }
    pub(super) fn handle_owner_read_context(
        &mut self,
        agent: String,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        session.owner_read_context = None;
        session.owner_read_view = None;
        session.owner_read_commit = None;
        let context = self.current_owner_read_context(&agent, session)?;
        let signing = authority::signing_bytes(VIEW_PATH, &context.request)
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        send_response(
            writer,
            &ViewerResponse::CanonicalAgentOwnerReadContext {
                context: serde_json::json!({"request":context.request,"fence":context.fence,"expected":context.expected,"signing_domain":VIEW_PATH,"signing_bytes_hex":hex::encode(signing)}),
            },
        )?;
        session.owner_read_context = Some(context);
        Ok(())
    }
    pub(super) fn handle_owner_read(
        &mut self,
        value: serde_json::Value,
        session: &mut RuntimeLiveSession,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), ViewerRuntimeLiveServerError> {
        session.owner_read_view = None;
        session.owner_read_commit = None;
        let original: SignedReadRequest<ReadWorldViewRequest> = serde_json::from_value(value)
            .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))?;
        authority::verify_read_request(VIEW_PATH, &original)
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        let context = session.owner_read_context.take().ok_or_else(|| {
            ViewerRuntimeLiveServerError::Init("owner read session context required".into())
        })?;
        let current = self.current_owner_read_context(&context.expected.agent_id, session)?;
        if context.expected != current.expected
            || context.fence != current.fence
            || context.request != original.request
            || original.subject_public_key != context.expected.public_key
        {
            return Err(ViewerRuntimeLiveServerError::Init(
                "owner read stale or foreign session request".into(),
            ));
        }
        let client = self.world_service_client()?.ok_or_else(|| {
            ViewerRuntimeLiveServerError::Init("owner read requires WorldService".into())
        })?;
        let digest = authority::request_digest(VIEW_PATH, &original)
            .map_err(ViewerRuntimeLiveServerError::Init)?;
        let echo = serde_json::to_value(&original)
            .map_err(|error| ViewerRuntimeLiveServerError::Serde(error.to_string()))?;
        let verified = if let Some(prepared) = self.prepared_owner_read.take() {
            let (prepared, result) = *prepared;
            if prepared != original {
                return Err(ViewerRuntimeLiveServerError::Init(
                    "prepared owner read proof mismatch".into(),
                ));
            }
            result.map_err(ViewerRuntimeLiveServerError::Init)?
        } else {
            client
                .read_owner_view(original)
                .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))?
        };
        if let Some(minimum) = current.request.min_commit.as_ref()
            && !verified
                .version()
                .commit
                .satisfies_minimum(minimum)
                .map_err(|error| ViewerRuntimeLiveServerError::Init(error.to_string()))?
        {
            return Err(ViewerRuntimeLiveServerError::Init(
                "owner read response fell behind current verified commit".into(),
            ));
        }
        let view = verified
            .projection()
            .canonical_agent_chat
            .as_ref()
            .ok_or_else(|| {
                ViewerRuntimeLiveServerError::Init("owner authenticated goal view missing".into())
            })?;
        if !current.expected.matches(view)
            || current
                .fence
                .as_ref()
                .is_some_and(|fence| fence != &CanonicalAgentOwnerFenceV1::from(view))
        {
            return Err(ViewerRuntimeLiveServerError::Init(
                "owner read response identity fence mismatch".into(),
            ));
        }
        session.owner_read_view = Some(view.clone());
        session.owner_read_commit = Some(verified.version().commit.clone());
        // Internal AgentHost delivery is separate from the shared public World.
        self.llm_sidecar.canonical_owner_goal = Some(view.clone());
        self.llm_sidecar.canonical_owner_goal_commit = Some(verified.version().commit.clone());
        self.configure_service_provider();
        send_response(
            writer,
            &ViewerResponse::CanonicalAgentOwnerView {
                version: serde_json::to_value(verified.version())
                    .map_err(|error| ViewerRuntimeLiveServerError::Serde(error.to_string()))?,
                request_digest: digest,
                request: echo,
                view: view.clone(),
            },
        )
    }
}
