use crate::formal_read::{
    FormalReadHistoryPageV1, FormalReadProjection, FormalReadReplayCursorV1, FormalReadVerifier,
    VerifiedFormalProjectionV1,
};
use oasis7_client_api::world_service::{
    EventCursor, ProjectionVersion, ReadWorldViewRequest, ReadWorldViewResponse,
};

pub const CONTROLLED_PREREQUISITE_READ_SCOPE: &str = "replicated_durability_prerequisite";

/// Explicit read assurance policy. This is never a formal readiness policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ReadEvidencePolicy {
    Ordinary,
    AllowControlledPrerequisite,
}
impl ReadEvidencePolicy {
    pub(crate) fn accepts(self, scope: Option<&str>) -> bool {
        match scope {
            None => self == Self::Ordinary,
            Some(CONTROLLED_PREREQUISITE_READ_SCOPE) => self == Self::AllowControlledPrerequisite,
            Some(_) => false,
        }
    }
}

/// Authenticated scoped observation. Legacy reads carry service assertions;
/// formal reads additionally hold an independently verified qualified commit
/// and its authorized history anchor. These assurance levels stay distinct.
#[derive(Debug, Clone)]
pub struct VerifiedWorldView<P: FormalReadProjection> {
    response: ReadWorldViewResponse<P>,
    read_authority_identity: Option<String>,
    evidence_policy: ReadEvidencePolicy,
    formal_commit:
        Option<oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1>,
    formal_minimum: Option<oasis7_client_api::world_service::CommitRef>,
    formal_verifier: Option<Box<FormalReadVerifier<P>>>,
    formal_read_anchor:
        Option<oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1>,
    formal_history_page: Option<FormalReadHistoryPageV1>,
}

impl<P: FormalReadProjection> VerifiedWorldView<P> {
    /// Authenticate a service assertion for the exact signed request. Formal
    /// replies still require independently verified proof and are rejected here.
    pub fn verify_service_assertion(
        signed: oasis7_client_api::world_service::SignedServiceResponse<ReadWorldViewResponse<P>>,
        request: &oasis7_client_api::world_service::SignedReadRequest<ReadWorldViewRequest>,
        trusted_service_public_key: &str,
        policy: ReadEvidencePolicy,
    ) -> Result<Self, String> {
        use oasis7_client_api::world_service::{
            VIEW_PATH, request_digest, verify_read_request, verify_service_response,
        };
        verify_read_request(VIEW_PATH, request)?;
        verify_service_response(
            VIEW_PATH,
            &request_digest(VIEW_PATH, request)?,
            &signed,
            trusted_service_public_key,
        )?;
        Self::new_with_evidence_policy(signed.payload, &request.request, policy)
    }
    /// Consume private-constructed qualified evidence for this exact response.
    /// A raw commit selector, checkpoint or service assertion cannot supply it.
    pub fn from_verified_projection(
        response: ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        observation: VerifiedFormalProjectionV1<P>,
    ) -> Result<Self, String> {
        Self::new_formal_verified(response, request, observation)
    }
    pub(crate) fn new(
        response: ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
    ) -> Result<Self, String> {
        Self::new_with_evidence_policy(response, request, ReadEvidencePolicy::Ordinary)
    }

    pub(crate) fn new_with_evidence_policy(
        response: ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        evidence_policy: ReadEvidencePolicy,
    ) -> Result<Self, String> {
        if response.formal_profile.is_some() || response.formal_proof.is_some() {
            return Err(
                "formal advertised reads require configured external proof verification".into(),
            );
        }
        if !evidence_policy.accepts(response.execution_evidence_scope.as_deref()) {
            return Err(
                "read execution evidence scope is unknown or not explicitly allowed".into(),
            );
        }
        Self::validate_contents(&response, request)?;
        Ok(Self {
            response,
            read_authority_identity: None,
            evidence_policy,
            formal_commit: None,
            formal_minimum: None,
            formal_verifier: None,
            formal_read_anchor: None,
            formal_history_page: None,
        })
    }
    pub(crate) fn validate_contents(
        response: &ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
    ) -> Result<(), String> {
        P::validate_contents(response, request)
    }
    pub(crate) fn new_formal_verified(
        mut response: ReadWorldViewResponse<P>,
        request: &ReadWorldViewRequest,
        observation: VerifiedFormalProjectionV1<P>,
    ) -> Result<Self, String> {
        observation.matches(&response, request)?;
        let profile = response.formal_profile.take();
        let sidecar = response.formal_proof.take();
        let mut shape_request = request.clone();
        shape_request.min_commit = None;
        let mut verified = Self::new(response, &shape_request)?;
        verified.response.formal_profile = profile;
        verified.response.formal_proof = sidecar;
        verified.formal_commit = Some(observation.commit().clone());
        verified.formal_minimum = request.min_commit.clone();
        verified.formal_verifier = Some(Box::new(observation.verifier().clone()));
        verified.formal_read_anchor = Some(observation.read_anchor().clone());
        verified.formal_history_page = observation.history_page().cloned();
        Ok(verified)
    }
    pub(crate) fn formal_verifier(&self) -> Option<&FormalReadVerifier<P>> {
        self.formal_verifier.as_deref()
    }
    /// Bounded history page accepted by the formal verifier for this view.
    /// The page is useful only when replayed from the configured external pin.
    pub fn formal_history_page(&self) -> Option<&FormalReadHistoryPageV1> {
        self.formal_history_page.as_ref()
    }
    /// Consistency cursor derived from this view's private verifier state.
    /// It is not a standalone trust anchor.
    pub fn formal_replay_cursor(&self) -> Result<Option<FormalReadReplayCursorV1>, String> {
        self.formal_verifier
            .as_deref()
            .map(FormalReadVerifier::replay_cursor)
            .transpose()
    }
    pub fn formal_commit(
        &self,
    ) -> Option<&oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1> {
        self.formal_commit.as_ref()
    }
    /// Exact prior projection ancestry certified during this formal read.
    /// Neither a larger height nor a service assertion supplies this relation.
    /// True only when this authenticated view certifies the exact prior view
    /// under the same configured read authority. Heights alone do not suffice.
    pub fn certifies_successor_of(&self, previous: &Self) -> bool {
        self.formally_satisfies_view(previous)
    }
    pub(crate) fn formally_satisfies_view(&self, previous: &Self) -> bool {
        self.formal_commit.is_some()
            && previous.formal_commit.is_some()
            && self.read_authority_identity.is_some()
            && self.read_authority_identity == previous.read_authority_identity
            && (self.version().commit == previous.version().commit
                || self.formal_minimum.as_ref() == Some(&previous.version().commit))
    }
    pub(crate) fn formal_minimum_matches(
        &self,
        minimum: &oasis7_client_api::world_service::CommitRef,
    ) -> bool {
        self.formal_commit.is_some()
            && (&self.version().commit == minimum || self.formal_minimum.as_ref() == Some(minimum))
    }
    pub fn formal_read_anchor(
        &self,
    ) -> Option<&oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalReadAnchorV1>
    {
        self.formal_read_anchor.as_ref()
    }
    pub(crate) fn bind_read_authority(mut self, identity: String) -> Self {
        self.read_authority_identity = Some(identity);
        self
    }
    /// Whether this authenticated view was obtained by this exact configured
    /// reader identity. The token itself cannot establish view authority.
    pub fn belongs_to(&self, reader: &crate::remote_world_service::ReadAuthorityId) -> bool {
        self.read_authority_matches(&reader.0)
    }
    pub(crate) fn read_authority_matches(&self, identity: &str) -> bool {
        self.read_authority_identity.as_deref() == Some(identity)
    }
    pub fn execution_evidence_scope(&self) -> Option<&str> {
        self.response.execution_evidence_scope.as_deref()
    }
    pub fn evidence_policy(&self) -> ReadEvidencePolicy {
        self.evidence_policy
    }
    pub fn version(&self) -> &ProjectionVersion {
        &self.response.version
    }
    pub fn continuation(&self) -> &EventCursor {
        &self.response.continuation
    }
    pub fn logical_tick(&self) -> u64 {
        self.response.logical_tick
    }
    pub fn projection(&self) -> &P {
        &self.response.view
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_client_api::world_service_feedback::{
        WorldServiceFeedbackHistory, WorldServiceFeedbackReplayRecord,
    };
    type WorldServiceProjection = TestProjection;
    type VerifiedWorldView = super::VerifiedWorldView<TestProjection>;
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
    struct TestState {
        time: u64,
    }
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
    struct TestProjection {
        state: TestState,
        runtime_binding: Option<oasis7_agent_api::RuntimeBindingV1>,
        feedback_history: Option<WorldServiceFeedbackHistory>,
    }
    impl FormalReadProjection for TestProjection {
        fn validate_contents(
            response: &ReadWorldViewResponse<Self>,
            request: &ReadWorldViewRequest,
        ) -> Result<(), String> {
            response.validate(request).map_err(|e| e.to_string())?;
            if response.view.state.time != response.logical_tick {
                return Err("projection tick differs from declared logical tick".into());
            }
            let agent = request.scope_id.strip_prefix("agent:");
            if response.view.feedback_history.as_ref().is_some_and(|h| {
                Some(h.agent_id.as_str()) != agent
                    || h.records
                        .iter()
                        .any(|r| Some(r.feedback.agent_subject.as_str()) != agent)
            }) {
                return Err("Agent observation differs from authorized visibility scope".into());
            }
            Ok(())
        }
    }
    use oasis7_client_api::world_service::{
        CommitRef, ExecutionBinding, WORLD_SERVICE_CONTRACT_VERSION, WorldIdentity,
    };

    fn feedback_history(agent_id: &str, subject: &str) -> WorldServiceFeedbackHistory {
        WorldServiceFeedbackHistory {
            agent_id: agent_id.into(),
            records: vec![WorldServiceFeedbackReplayRecord {
                feedback: oasis7_agent_api::FeedbackEnvelopeV1 {
                    feedback_id: "feedback".into(),
                    feedback_seq: 1,
                    agent_subject: subject.into(),
                    agent_session_id: "session".into(),
                    agent_turn_id: "turn".into(),
                    decision_request_id: "request".into(),
                    candidate_action_id: None,
                    runtime_receipt_id: None,
                    status: "rejected".into(),
                    request_digest: oasis7_agent_api::Digest32(format!(
                        "blake3:{}",
                        "a".repeat(64)
                    )),
                    reject_reason: Some("stale_base".into()),
                    provenance: "runtime_authoritative".into(),
                },
                original_envelope_digest: format!("blake3:{}", "b".repeat(64)),
                delivery_state: "pending".into(),
                receipt_lineage: None,
            }],
        }
    }

    fn fixture() -> (
        ReadWorldViewRequest,
        ReadWorldViewResponse<WorldServiceProjection>,
    ) {
        let logical_tick = 0;
        let identity = WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        };
        let commit = CommitRef {
            world: identity.clone(),
            binding: ExecutionBinding {
                provider_world_id: "provider".into(),
                branch_id: "branch".into(),
                finality_ref: "finality".into(),
                reorg_generation: 0,
                governing_manifest_ref: "manifest".into(),
                authority_generation: 1,
                permission_generation: 1,
            },
            position: 7,
            execution_block_hash: "block".into(),
            state_root_ref: "root".into(),
        };
        let request = ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: identity,
            scope_id: "public".into(),
            min_commit: Some(commit.clone()),
            fixed_commit: None,
            deadline_unix_ms: None,
        };
        let projection = TestProjection {
            runtime_binding: None,
            ..Default::default()
        };
        let response = ReadWorldViewResponse {
            formal_profile: None,
            formal_proof: None,
            execution_evidence_scope: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            version: ProjectionVersion {
                commit: commit.clone(),
                projection_revision: "projection".into(),
                visibility_scope: "public".into(),
            },
            logical_tick,
            continuation: EventCursor {
                stream_id: "events".into(),
                scope_id: "public".into(),
                era: 0,
                sequence: 3,
                commit,
            },
            view: projection,
        };
        (request, response)
    }

    #[test]
    fn absent_scope_preserves_original_view_cbor_signing_bytes() {
        #[derive(serde::Serialize)]
        struct OriginalView<'a> {
            contract_version: u32,
            version: &'a ProjectionVersion,
            logical_tick: u64,
            continuation: &'a EventCursor,
            view: &'a WorldServiceProjection,
        }
        let (_, response) = fixture();
        let original = OriginalView {
            contract_version: response.contract_version,
            version: &response.version,
            logical_tick: response.logical_tick,
            continuation: &response.continuation,
            view: &response.view,
        };
        assert_eq!(
            serde_cbor::to_vec(&response).unwrap(),
            serde_cbor::to_vec(&original).unwrap()
        );
    }

    #[test]
    fn prerequisite_read_requires_explicit_policy_and_rejects_unknown_scopes() {
        let (request, mut response) = fixture();
        assert!(
            serde_json::to_value(&response)
                .unwrap()
                .get("execution_evidence_scope")
                .is_none()
        );
        assert!(
            VerifiedWorldView::new_with_evidence_policy(
                response.clone(),
                &request,
                ReadEvidencePolicy::AllowControlledPrerequisite,
            )
            .is_err()
        );
        response.execution_evidence_scope = Some(CONTROLLED_PREREQUISITE_READ_SCOPE.into());
        assert!(VerifiedWorldView::new(response.clone(), &request).is_err());
        let verified = VerifiedWorldView::new_with_evidence_policy(
            response.clone(),
            &request,
            ReadEvidencePolicy::AllowControlledPrerequisite,
        )
        .unwrap();
        assert_eq!(
            verified.execution_evidence_scope(),
            Some(CONTROLLED_PREREQUISITE_READ_SCOPE)
        );
        assert_eq!(
            verified.evidence_policy(),
            ReadEvidencePolicy::AllowControlledPrerequisite
        );
        response.execution_evidence_scope = Some("formal_commit".into());
        assert!(
            VerifiedWorldView::new_with_evidence_policy(
                response,
                &request,
                ReadEvidencePolicy::AllowControlledPrerequisite,
            )
            .is_err()
        );
    }

    #[test]
    fn view_rejects_tick_cursor_world_and_generation_drift() {
        let (request, response) = fixture();
        assert!(VerifiedWorldView::new(response.clone(), &request).is_ok());
        let mut tick = response.clone();
        tick.view.state.time += 1;
        assert!(VerifiedWorldView::new(tick, &request).is_err());
        let mut cursor = response.clone();
        cursor.continuation.commit.position += 1;
        assert!(VerifiedWorldView::new(cursor, &request).is_err());
        let mut world = response.clone();
        world.version.commit.world.genesis_digest = "other-genesis".into();
        world.continuation.commit = world.version.commit.clone();
        assert!(VerifiedWorldView::new(world, &request).is_err());
        let mut generation = response;
        generation.version.commit.binding.permission_generation += 1;
        generation.continuation.commit = generation.version.commit.clone();
        assert!(VerifiedWorldView::new(generation, &request).is_err());
    }

    #[test]
    fn verified_view_binds_feedback_history_to_agent_scope_and_hides_it_from_public_scope() {
        let (public_request, mut public_response) = fixture();
        public_response.view.feedback_history = Some(feedback_history("agent-a", "agent-a"));
        assert!(
            VerifiedWorldView::new(public_response, &public_request).is_err(),
            "public projection must not expose private agent feedback history"
        );

        let (mut agent_request, mut agent_response) = fixture();
        agent_request.scope_id = "agent:agent-a".into();
        agent_response.version.visibility_scope = agent_request.scope_id.clone();
        agent_response.continuation.scope_id = agent_request.scope_id.clone();

        agent_response.view.feedback_history = Some(feedback_history("agent-b", "agent-b"));
        assert!(
            VerifiedWorldView::new(agent_response.clone(), &agent_request).is_err(),
            "history owner must match the requested agent"
        );

        agent_response.view.feedback_history = Some(feedback_history("agent-a", "agent-b"));
        assert!(
            VerifiedWorldView::new(agent_response.clone(), &agent_request).is_err(),
            "every feedback subject must match the requested agent"
        );

        agent_response.view.feedback_history = Some(feedback_history("agent-a", "agent-a"));
        assert!(VerifiedWorldView::new(agent_response, &agent_request).is_ok());
    }
}
