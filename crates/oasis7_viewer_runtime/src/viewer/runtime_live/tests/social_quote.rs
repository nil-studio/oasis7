use super::*;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_simulator::{
    ResourceOwner, SocialChallengeState, SocialFactLifecycleState, SocialFactState, SocialStake,
};

fn seed_active_social_backing_fact(server: &mut ViewerRuntimeLiveServer, agent_id: &str) -> u64 {
    let fact_id = 77;
    server
        .seed_model
        .get_or_insert_default()
        .social_facts
        .insert(
            fact_id,
            SocialFactState {
                fact_id,
                actor: ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
                schema_id: "social.relation.v1".to_string(),
                subject: ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
                object: Some(ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                }),
                claim: "agent has cooperative history".to_string(),
                confidence_ppm: 750_000,
                evidence_event_ids: Vec::new(),
                ttl_ticks: None,
                expires_at_tick: None,
                stake: None,
                challenge: None,
                lifecycle: SocialFactLifecycleState::Active,
                created_at_tick: 0,
                updated_at_tick: 0,
            },
        );
    fact_id
}

fn seed_challenged_social_fact(server: &mut ViewerRuntimeLiveServer, agent_id: &str) -> u64 {
    let fact_id = 78;
    server
        .seed_model
        .get_or_insert_default()
        .social_facts
        .insert(
            fact_id,
            SocialFactState {
                fact_id,
                actor: ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
                schema_id: "social.reputation.v1".to_string(),
                subject: ResourceOwner::Agent {
                    agent_id: agent_id.to_string(),
                },
                object: None,
                claim: "agent fulfilled the challenged delivery".to_string(),
                confidence_ppm: 800_000,
                evidence_event_ids: Vec::new(),
                ttl_ticks: Some(12),
                expires_at_tick: None,
                stake: None,
                challenge: Some(SocialChallengeState {
                    challenger: ResourceOwner::Agent {
                        agent_id: agent_id.to_string(),
                    },
                    reason: "needs adjudication".to_string(),
                    stake: None,
                    challenged_at_tick: 0,
                }),
                lifecycle: SocialFactLifecycleState::Challenged,
                created_at_tick: 0,
                updated_at_tick: 0,
            },
        );
    fact_id
}

fn signed_adjudicate_social_fact_quote_request(
    fact_id: u64,
    decision: crate::viewer::SocialAdjudicationDecision,
    notes: &str,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::AdjudicateSocialFactQuoteRequest {
    let mut request = crate::viewer::AdjudicateSocialFactQuoteRequest {
        fact_id,
        decision,
        notes: notes.to_string(),
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_adjudicate_social_fact_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign adjudicate social fact quote auth"),
    );
    request
}

fn seed_revoke_social_fact(
    server: &mut ViewerRuntimeLiveServer,
    fact_id: u64,
    publisher_agent_id: &str,
    subject_agent_id: &str,
    object_agent_id: &str,
) -> u64 {
    server
        .seed_model
        .get_or_insert_default()
        .social_facts
        .insert(
            fact_id,
            SocialFactState {
                fact_id,
                actor: ResourceOwner::Agent {
                    agent_id: publisher_agent_id.to_string(),
                },
                schema_id: "social.reputation.relationship.v1".to_string(),
                subject: ResourceOwner::Agent {
                    agent_id: subject_agent_id.to_string(),
                },
                object: Some(ResourceOwner::Agent {
                    agent_id: object_agent_id.to_string(),
                }),
                claim: "agent-b fulfilled delivery contract for agent-c".to_string(),
                confidence_ppm: 875_000,
                evidence_event_ids: Vec::new(),
                ttl_ticks: Some(12),
                expires_at_tick: None,
                stake: Some(SocialStake {
                    kind: ResourceKind::Electricity,
                    amount: 30,
                }),
                challenge: None,
                lifecycle: SocialFactLifecycleState::Active,
                created_at_tick: 0,
                updated_at_tick: 0,
            },
        );
    fact_id
}

fn signed_revoke_social_fact_quote_request(
    fact_id: u64,
    reason: &str,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::RevokeSocialFactQuoteRequest {
    let mut request = crate::viewer::RevokeSocialFactQuoteRequest {
        fact_id,
        reason: reason.to_string(),
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_revoke_social_fact_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign revoke social fact quote auth"),
    );
    request
}

#[test]
fn runtime_adjudicate_social_fact_quote_requires_auth_proof() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let error = server
        .handle_adjudicate_social_fact_quote(crate::viewer::AdjudicateSocialFactQuoteRequest {
            fact_id: 78,
            decision: crate::viewer::SocialAdjudicationDecision::Confirm,
            notes: "evidence is sufficient".to_string(),
            player_id: "player-social-adjudication".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("unsigned adjudication quote must fail");
    assert_eq!(error.code, "auth_proof_required");
    assert_eq!(
        error.action_id.as_deref(),
        Some("quote_adjudicate_social_fact")
    );
}

#[test]
fn runtime_adjudicate_social_fact_quote_rejects_unknown_session() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let (public_key, private_key) = test_signer(101);
    let error = server
        .handle_adjudicate_social_fact_quote(signed_adjudicate_social_fact_quote_request(
            78,
            crate::viewer::SocialAdjudicationDecision::Confirm,
            "evidence is sufficient",
            "unknown-player-social-adjudication",
            3301,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("unknown session must fail closed");
    assert_eq!(error.code, "session_not_found");
}

#[test]
fn runtime_adjudicate_social_fact_quote_is_signed_deterministic_non_mutating_and_serializable() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let fact_id = seed_challenged_social_fact(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(102);
    register_runtime_session(
        &mut server,
        "player-social-adjudication",
        Some(agent_id.as_str()),
        3302,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let request = signed_adjudicate_social_fact_quote_request(
        fact_id,
        crate::viewer::SocialAdjudicationDecision::Confirm,
        "evidence is sufficient",
        "player-social-adjudication",
        3303,
        public_key.as_str(),
        private_key.as_str(),
    );
    let quote = server
        .handle_adjudicate_social_fact_quote(request.clone())
        .expect("signed adjudication quote");
    assert_eq!(quote.actor_id, agent_id);
    assert_eq!(quote.action_kind, "adjudicate_fact");
    assert_eq!(quote.schema_id, "social.reputation.v1");
    assert_eq!(quote.subject_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(quote.stake_at_risk, 0);
    assert_eq!(quote.ttl_ticks, Some(12));
    assert_eq!(quote.cooperation_opportunity_delta, "confirmed");
    assert_eq!(quote.recommended_social_action, "confirm_fact");
    assert_eq!(quote.blacklist_or_dispute_risk, "adjudication_settlement");
    assert!(quote.why_this_action_matters.contains("stake pool"));
    assert_eq!(
        server
            .handle_adjudicate_social_fact_quote(request.clone())
            .expect("repeat signed adjudication quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);

    let request_json =
        serde_json::to_value(crate::viewer::ViewerRequest::QuoteAdjudicateSocialFact { request })
            .expect("adjudication quote request serializes");
    let reparsed: crate::viewer::ViewerRequest =
        serde_json::from_value(request_json.clone()).expect("adjudication quote request parses");
    assert_eq!(
        serde_json::to_value(reparsed).expect("re-serialize request"),
        request_json
    );
    let response_json = serde_json::to_value(
        crate::viewer::ViewerResponse::AdjudicateSocialFactQuotePreflight { quote },
    )
    .expect("adjudication quote response serializes");
    assert_eq!(
        response_json["type"],
        "adjudicate_social_fact_quote_preflight"
    );
    assert!(
        response_json["quote"]
            .get("recommended_social_action")
            .is_some()
    );
}

fn signed_declare_social_edge_quote_request(
    agent_id: &str,
    backing_fact_id: u64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::DeclareSocialEdgeQuoteRequest {
    let mut request = crate::viewer::DeclareSocialEdgeQuoteRequest {
        schema_id: "social.relation.v1".to_string(),
        relation_kind: "trust".to_string(),
        from_agent_id: agent_id.to_string(),
        to_agent_id: agent_id.to_string(),
        weight_bps: 2_000,
        backing_fact_ids: vec![backing_fact_id],
        ttl_ticks: Some(12),
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_declare_social_edge_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign declare social edge quote auth"),
    );
    request
}

fn signed_publish_social_fact_quote_request(
    agent_id: &str,
    evidence_event_id: u64,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::PublishSocialFactQuoteRequest {
    let mut request = crate::viewer::PublishSocialFactQuoteRequest {
        schema_id: "social.reputation.v1".to_string(),
        subject_agent_id: agent_id.to_string(),
        object_agent_id: Some(agent_id.to_string()),
        claim: "agent fulfilled the evidence-backed delivery".to_string(),
        confidence_ppm: 875_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: Some(12),
        stake: None,
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_publish_social_fact_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign publish social fact quote auth"),
    );
    request
}

#[test]
fn runtime_declare_social_edge_quote_requires_auth_proof() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let err = server
        .handle_declare_social_edge_quote(crate::viewer::DeclareSocialEdgeQuoteRequest {
            schema_id: "social.relation.v1".to_string(),
            relation_kind: "trust".to_string(),
            from_agent_id: "agent-a".to_string(),
            to_agent_id: "agent-b".to_string(),
            weight_bps: 2_000,
            backing_fact_ids: vec![77],
            ttl_ticks: Some(12),
            player_id: "player-social-quote".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("unsigned declare social edge quote must fail");

    assert_eq!(err.code, "auth_proof_required");
    assert_eq!(err.action_id.as_deref(), Some("quote_declare_social_edge"));
}

#[test]
fn runtime_declare_social_edge_quote_is_signed_deterministic_non_mutating_and_player_readable() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let backing_fact_id = seed_active_social_backing_fact(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(95);
    register_runtime_session(
        &mut server,
        "player-social-quote",
        Some(agent_id.as_str()),
        2795,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let request = signed_declare_social_edge_quote_request(
        agent_id.as_str(),
        backing_fact_id,
        "player-social-quote",
        2796,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_declare_social_edge_quote(request.clone())
        .expect("signed declare social edge quote");
    assert_eq!(quote.actor_id, agent_id);
    assert_eq!(quote.action_kind, "declare_social_edge");
    assert_eq!(quote.schema_id, request.schema_id);
    assert_eq!(quote.subject_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(quote.object_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(quote.claim_summary, "trust");
    assert_eq!(quote.confidence_ppm, None);
    assert_eq!(quote.stake_at_risk, 0);
    assert_eq!(quote.ttl_ticks, Some(12));
    assert!(
        quote
            .affected_relationships
            .contains(&"schema:social.relation.v1".to_string())
    );
    assert!(
        quote
            .affected_social_surfaces
            .contains(&"reputation".to_string())
    );
    assert_eq!(quote.recommended_social_action, "declare_edge");
    assert!(quote.why_this_action_matters.contains("trust"));
    assert_eq!(
        server
            .handle_declare_social_edge_quote(request)
            .expect("repeat signed declare social edge quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_publish_social_fact_quote_is_signed_deterministic_non_mutating_and_player_readable() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let evidence_event_id = server
        .world
        .journal()
        .events
        .first()
        .expect("seed runtime evidence event")
        .id;
    let (public_key, private_key) = test_signer(96);
    register_runtime_session(
        &mut server,
        "player-publish-social-fact-quote",
        Some(agent_id.as_str()),
        2896,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let request = signed_publish_social_fact_quote_request(
        agent_id.as_str(),
        evidence_event_id,
        "player-publish-social-fact-quote",
        2897,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_publish_social_fact_quote(request.clone())
        .expect("signed publish social fact quote");
    assert_eq!(quote.actor_id, agent_id);
    assert_eq!(quote.action_kind, "publish_fact");
    assert_eq!(quote.schema_id, request.schema_id);
    assert_eq!(quote.subject_id.as_deref(), Some(agent_id.as_str()));
    assert_eq!(quote.object_id.as_deref(), Some(agent_id.as_str()));
    assert!(quote.claim_summary.contains("evidence-backed delivery"));
    assert_eq!(quote.confidence_ppm, Some(875_000));
    assert_eq!(quote.stake_at_risk, 0);
    assert_eq!(quote.ttl_ticks, Some(12));
    assert!(
        quote
            .affected_social_surfaces
            .contains(&"reputation".to_string())
    );
    assert_eq!(quote.cooperation_opportunity_delta, "positive");
    assert_eq!(quote.blacklist_or_dispute_risk, "challengeable_claim");
    assert_eq!(quote.governance_or_claim_relevance, "evidence_backed_claim");
    assert_eq!(quote.recommended_social_action, "publish_fact");
    assert!(quote.why_this_action_matters.contains(agent_id.as_str()));
    assert_eq!(
        server
            .handle_publish_social_fact_quote(request)
            .expect("repeat signed publish social fact quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_publish_social_fact_quote_rejects_missing_runtime_evidence_without_mutation() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(97);
    register_runtime_session(
        &mut server,
        "player-publish-social-fact-missing-evidence",
        Some(agent_id.as_str()),
        2898,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let err = server
        .handle_publish_social_fact_quote(signed_publish_social_fact_quote_request(
            agent_id.as_str(),
            999_999,
            "player-publish-social-fact-missing-evidence",
            2899,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("missing runtime evidence must reject publish social fact quote");

    assert_eq!(err.code, "publish_social_fact_quote_rejected");
    assert_eq!(err.action_id.as_deref(), Some("quote_publish_social_fact"));
    assert!(
        err.message
            .contains("social evidence event missing: 999999")
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_revoke_social_fact_quote_requires_auth_proof() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let err = server
        .handle_revoke_social_fact_quote(crate::viewer::RevokeSocialFactQuoteRequest {
            fact_id: 88,
            reason: "withdraw obsolete delivery evidence".to_string(),
            player_id: "player-revoke-social-fact-quote".to_string(),
            public_key: None,
            auth: None,
        })
        .expect_err("unsigned revoke social fact quote must fail");

    assert_eq!(err.code, "auth_proof_required");
    assert_eq!(err.action_id.as_deref(), Some("quote_revoke_social_fact"));
}

#[test]
fn runtime_revoke_social_fact_quote_is_signed_deterministic_non_mutating_and_authoritative() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let mut agent_ids = server.world.state().agents.keys().cloned();
    let publisher_agent_id = agent_ids.next().expect("publisher Agent");
    let subject_agent_id = agent_ids.next().expect("subject Agent");
    let fact_id = seed_revoke_social_fact(
        &mut server,
        88,
        publisher_agent_id.as_str(),
        subject_agent_id.as_str(),
        publisher_agent_id.as_str(),
    );
    let (public_key, private_key) = test_signer(103);
    register_runtime_session(
        &mut server,
        "player-revoke-social-fact-quote",
        Some(publisher_agent_id.as_str()),
        3498,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let request = signed_revoke_social_fact_quote_request(
        fact_id,
        "withdraw obsolete delivery evidence",
        "player-revoke-social-fact-quote",
        3499,
        public_key.as_str(),
        private_key.as_str(),
    );
    let mut tampered_reason = request.clone();
    tampered_reason.reason = "withdraw a different reason".to_string();
    let tampered_error = server
        .handle_revoke_social_fact_quote(tampered_reason)
        .expect_err("reason tampering must reject revoke social fact quote");
    assert_eq!(tampered_error.code, "auth_signature_invalid");
    assert_eq!(
        tampered_error.action_id.as_deref(),
        Some("quote_revoke_social_fact")
    );
    assert_eq!(server.world.state(), &state_before);

    let quote = server
        .handle_revoke_social_fact_quote(request.clone())
        .expect("signed revoke social fact quote");
    assert_eq!(quote.actor_id, publisher_agent_id);
    assert_eq!(quote.action_kind, "revoke_fact");
    assert_eq!(quote.schema_id, "social.reputation.relationship.v1");
    assert_eq!(quote.subject_id.as_deref(), Some(subject_agent_id.as_str()));
    assert_eq!(
        quote.object_id.as_deref(),
        Some(publisher_agent_id.as_str())
    );
    assert!(quote.claim_summary.contains("fulfilled delivery contract"));
    assert_eq!(quote.confidence_ppm, Some(875_000));
    assert_eq!(quote.stake_at_risk, 0);
    assert_eq!(quote.ttl_ticks, Some(12));
    assert!(
        quote
            .affected_relationships
            .iter()
            .any(|relationship| relationship == "schema:social.reputation.relationship.v1")
    );
    assert!(
        quote
            .affected_relationships
            .iter()
            .any(|relationship| relationship.contains(subject_agent_id.as_str()))
    );
    assert!(
        quote
            .affected_social_surfaces
            .contains(&"social_fact_ledger".to_string())
    );
    assert!(
        quote
            .affected_social_surfaces
            .contains(&"relationship".to_string())
    );
    assert!(quote.cooperation_opportunity_delta.contains("withdraw"));
    assert!(quote.blacklist_or_dispute_risk.contains("withdraw"));
    assert!(quote.governance_or_claim_relevance.contains("withdraw"));
    assert_eq!(quote.recommended_social_action, "revoke_fact");
    assert!(
        quote
            .why_this_action_matters
            .contains("withdraw obsolete delivery evidence")
    );
    assert!(
        quote
            .why_this_action_matters
            .contains("Publisher stake return")
    );
    assert_eq!(
        server
            .handle_revoke_social_fact_quote(request)
            .expect("repeat signed revoke social fact quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_revoke_social_fact_quote_rejects_actor_session_mismatch_structured_error() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let mut agent_ids = server.world.state().agents.keys().cloned();
    let session_agent_id = agent_ids.next().expect("session Agent");
    let publisher_agent_id = agent_ids.next().expect("publisher Agent");
    let fact_id = seed_revoke_social_fact(
        &mut server,
        89,
        publisher_agent_id.as_str(),
        session_agent_id.as_str(),
        publisher_agent_id.as_str(),
    );
    let (public_key, private_key) = test_signer(104);
    register_runtime_session(
        &mut server,
        "player-revoke-social-fact-mismatch",
        Some(session_agent_id.as_str()),
        3598,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let err = server
        .handle_revoke_social_fact_quote(signed_revoke_social_fact_quote_request(
            fact_id,
            "withdraw with the wrong publisher session",
            "player-revoke-social-fact-mismatch",
            3599,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect_err("session actor mismatch must reject revoke social fact quote");

    assert_eq!(err.code, "revoke_social_fact_quote_rejected");
    assert_eq!(err.action_id.as_deref(), Some("quote_revoke_social_fact"));
    assert_eq!(
        err.target_agent_id.as_deref(),
        Some(session_agent_id.as_str())
    );
    assert!(err.message.contains("can only be revoked by publisher"));
    assert_eq!(server.world.state(), &state_before);
}

fn signed_social_contact_quote_request(
    contact_purpose: &str,
    first_contact_class: crate::viewer::FirstContactClass,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::SocialContactQuoteRequest {
    signed_social_contact_quote_request_for_candidate(
        contact_purpose,
        first_contact_class,
        "candidate-agent",
        player_id,
        nonce,
        public_key_hex,
        private_key_hex,
    )
}

fn signed_social_contact_quote_request_for_candidate(
    contact_purpose: &str,
    first_contact_class: crate::viewer::FirstContactClass,
    candidate_agent_id: &str,
    player_id: &str,
    nonce: u64,
    public_key_hex: &str,
    private_key_hex: &str,
) -> crate::viewer::SocialContactQuoteRequest {
    let mut request = crate::viewer::SocialContactQuoteRequest {
        contact_purpose: contact_purpose.to_string(),
        first_contact_class,
        candidate_agent_id: candidate_agent_id.to_string(),
        player_id: player_id.to_string(),
        public_key: Some(public_key_hex.to_string()),
        auth: None,
    };
    request.auth = Some(
        crate::viewer::sign_social_contact_quote_auth_proof(
            &request,
            nonce,
            public_key_hex,
            private_key_hex,
        )
        .expect("sign social contact quote auth"),
    );
    request
}

#[test]
fn runtime_social_contact_quote_defers_known_candidate_without_authoritative_capacity_evidence() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let mut agent_ids = server.world.state().agents.keys().cloned();
    let actor_id = agent_ids.next().expect("actor Agent");
    let candidate_id = agent_ids.next().expect("candidate Agent");
    let (public_key, private_key) = test_signer(102);
    register_runtime_session(
        &mut server,
        "player-social-contact-known-candidate",
        Some(actor_id.as_str()),
        3398,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let request = signed_social_contact_quote_request_for_candidate(
        "exchange local route information",
        crate::viewer::FirstContactClass::InformationExchange,
        candidate_id.as_str(),
        "player-social-contact-known-candidate",
        3399,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_social_contact_quote(request.clone())
        .expect("known candidate without evidence returns a defer quote");
    assert_eq!(
        quote.first_contact_class,
        crate::viewer::FirstContactClass::DeferContact
    );
    assert!(quote.expected_mutual_value.contains("No reciprocal value"));
    assert!(quote.defer_reason.contains("no authoritative reciprocal"));
    assert!(quote.solo_lane_preserved);
    assert_eq!(
        server
            .handle_social_contact_quote(request)
            .expect("repeat known-candidate quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_social_contact_quote_requires_auth_and_binds_candidate_without_mutation() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(101);
    register_runtime_session(
        &mut server,
        "player-social-contact-auth",
        Some(agent_id.as_str()),
        3298,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let err = server
        .handle_social_contact_quote(crate::viewer::SocialContactQuoteRequest {
            contact_purpose: "exchange local route information".to_string(),
            first_contact_class: crate::viewer::FirstContactClass::InformationExchange,
            candidate_agent_id: agent_id.clone(),
            player_id: "player-social-contact-auth".to_string(),
            public_key: Some(public_key.clone()),
            auth: None,
        })
        .expect_err("missing auth must reject first contact");
    assert_eq!(err.code, "auth_proof_required");
    assert_eq!(server.world.state(), &state_before);

    let mut tampered = signed_social_contact_quote_request(
        "exchange local route information",
        crate::viewer::FirstContactClass::InformationExchange,
        "player-social-contact-auth",
        3299,
        public_key.as_str(),
        private_key.as_str(),
    );
    tampered.candidate_agent_id = agent_id;
    let err = server
        .handle_social_contact_quote(tampered)
        .expect_err("candidate tampering must reject first contact");
    assert_eq!(err.code, "auth_signature_invalid");
    assert_eq!(server.world.state(), &state_before);

    let mut tampered_purpose = signed_social_contact_quote_request(
        "exchange local route information",
        crate::viewer::FirstContactClass::InformationExchange,
        "player-social-contact-auth",
        3300,
        public_key.as_str(),
        private_key.as_str(),
    );
    tampered_purpose.contact_purpose = "commit to a long-term supply contract".to_string();
    assert_eq!(
        server
            .handle_social_contact_quote(tampered_purpose)
            .expect_err("purpose tampering must reject first contact")
            .code,
        "auth_signature_invalid"
    );
    let mut tampered_class = signed_social_contact_quote_request(
        "exchange local route information",
        crate::viewer::FirstContactClass::InformationExchange,
        "player-social-contact-auth",
        3301,
        public_key.as_str(),
        private_key.as_str(),
    );
    tampered_class.first_contact_class = crate::viewer::FirstContactClass::MutualAid;
    assert_eq!(
        server
            .handle_social_contact_quote(tampered_class)
            .expect_err("class tampering must reject first contact")
            .code,
        "auth_signature_invalid"
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_social_contact_quote_is_authenticated_deterministic_non_mutating_and_has_only_readable_fields()
 {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(98);
    register_runtime_session(
        &mut server,
        "player-social-contact-quote",
        Some(agent_id.as_str()),
        2998,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let request = signed_social_contact_quote_request(
        "sell a small surplus locally",
        crate::viewer::FirstContactClass::TradeOrService,
        "player-social-contact-quote",
        2999,
        public_key.as_str(),
        private_key.as_str(),
    );

    let quote = server
        .handle_social_contact_quote(request.clone())
        .expect("signed social contact quote");
    assert_eq!(
        quote.first_contact_class,
        crate::viewer::FirstContactClass::DeferContact
    );
    assert_eq!(quote.contact_purpose, request.contact_purpose);
    assert_eq!(request.candidate_agent_id, "candidate-agent");
    assert!(quote.expected_mutual_value.contains("No reciprocal value"));
    assert!(!quote.risk_or_commitment.is_empty());
    assert!(quote.solo_lane_preserved);
    assert!(!quote.recommended_contact_action.is_empty());
    assert!(!quote.defer_reason.is_empty());
    let quote_json = serde_json::to_value(&quote).expect("serialize social contact quote");
    assert_eq!(
        quote_json
            .as_object()
            .expect("social contact quote object")
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "contact_purpose",
            "expected_mutual_value",
            "first_contact_class",
            "risk_or_commitment",
            "solo_lane_preserved",
            "recommended_contact_action",
            "defer_reason",
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(
        server
            .handle_social_contact_quote(request)
            .expect("repeat signed social contact quote"),
        quote
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_social_contact_quote_uses_closed_classes_and_explicitly_defers_ineligible_contact() {
    assert_eq!(
        serde_json::to_string(&crate::viewer::FirstContactClass::TradeOrService)
            .expect("serialize trade class"),
        "\"trade_or_service\""
    );
    assert_eq!(
        serde_json::to_string(&crate::viewer::FirstContactClass::MutualAid)
            .expect("serialize mutual aid class"),
        "\"mutual_aid\""
    );
    assert_eq!(
        serde_json::to_string(&crate::viewer::FirstContactClass::InformationExchange)
            .expect("serialize information exchange class"),
        "\"information_exchange\""
    );
    assert_eq!(
        serde_json::to_string(&crate::viewer::FirstContactClass::DeferContact)
            .expect("serialize defer class"),
        "\"defer_contact\""
    );
    assert_eq!(
        serde_json::to_string(&crate::viewer::FirstContactClass::OrganizationEscalation)
            .expect("serialize escalation class"),
        "\"organization_escalation\""
    );
    assert!(
        serde_json::from_str::<crate::viewer::FirstContactClass>("\"unbounded_diplomacy\"")
            .is_err()
    );

    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(99);
    register_runtime_session(
        &mut server,
        "player-social-contact-defer",
        Some(agent_id.as_str()),
        3098,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();
    let quote = server
        .handle_social_contact_quote(signed_social_contact_quote_request(
            "",
            crate::viewer::FirstContactClass::TradeOrService,
            "player-social-contact-defer",
            3099,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("ineligible first contact returns an explicit defer quote");

    assert_eq!(
        quote.first_contact_class,
        crate::viewer::FirstContactClass::DeferContact
    );
    assert!(!quote.defer_reason.is_empty());
    assert!(quote.solo_lane_preserved);
    assert_ne!(
        quote.first_contact_class,
        crate::viewer::FirstContactClass::OrganizationEscalation
    );
    assert_eq!(server.world.state(), &state_before);
}

#[test]
fn runtime_social_contact_quote_preserves_low_commitment_classes_and_defers_escalation() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(100);
    register_runtime_session(
        &mut server,
        "player-social-contact-classes",
        Some(agent_id.as_str()),
        3198,
        public_key.as_str(),
        private_key.as_str(),
    );
    let state_before = server.world.state().clone();

    for (nonce, class) in [
        (3199, crate::viewer::FirstContactClass::MutualAid),
        (3200, crate::viewer::FirstContactClass::InformationExchange),
    ] {
        let quote = server
            .handle_social_contact_quote(signed_social_contact_quote_request(
                "remove the current local blocker",
                class.clone(),
                "player-social-contact-classes",
                nonce,
                public_key.as_str(),
                private_key.as_str(),
            ))
            .expect("low-commitment first contact quote");
        assert_eq!(
            quote.first_contact_class,
            crate::viewer::FirstContactClass::DeferContact
        );
        assert!(!quote.defer_reason.is_empty());
        assert!(quote.solo_lane_preserved);
    }

    let escalation = server
        .handle_social_contact_quote(signed_social_contact_quote_request(
            "join a regional organization",
            crate::viewer::FirstContactClass::OrganizationEscalation,
            "player-social-contact-classes",
            3201,
            public_key.as_str(),
            private_key.as_str(),
        ))
        .expect("organization escalation is deferred from first contact");
    assert_eq!(
        escalation.first_contact_class,
        crate::viewer::FirstContactClass::DeferContact
    );
    assert!(!escalation.defer_reason.is_empty());
    assert!(escalation.solo_lane_preserved);
    assert_eq!(server.world.state(), &state_before);
}
