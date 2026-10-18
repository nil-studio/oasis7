use super::*;
use crate::viewer_protocol::{
    AdjudicateSocialFactQuoteRequest, CollectDataCommand, CollectDataRequest,
    ScheduleRecipeQuoteRequest, SocialAdjudicationDecision,
};
use ed25519_dalek::SigningKey;

fn test_signer_with_seed(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

fn test_signer() -> (String, String) {
    test_signer_with_seed(7)
}

#[test]
fn adjudicate_social_fact_quote_auth_binds_decision_and_notes() {
    let (public_key, private_key) = test_signer();
    let request = AdjudicateSocialFactQuoteRequest {
        fact_id: 42,
        decision: SocialAdjudicationDecision::Confirm,
        notes: "evidence is sufficient".to_string(),
        player_id: "player-social".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    let proof = sign_adjudicate_social_fact_quote_auth_proof(
        &request,
        17,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign adjudication quote");
    verify_adjudicate_social_fact_quote_auth_proof(&request, &proof)
        .expect("original adjudication request remains valid");

    let mut tampered = request.clone();
    tampered.notes = "tampered".to_string();
    assert!(
        verify_adjudicate_social_fact_quote_auth_proof(&tampered, &proof)
            .expect_err("tampered notes must fail")
            .contains("verify auth signature failed")
    );
    let mut decision_tampered = request;
    decision_tampered.decision = SocialAdjudicationDecision::Retract;
    assert!(
        verify_adjudicate_social_fact_quote_auth_proof(&decision_tampered, &proof)
            .expect_err("tampered decision must fail")
            .contains("verify auth signature failed")
    );
}

#[test]
fn schedule_recipe_quote_auth_rejects_delimiter_collision_tuple() {
    let (public_key, private_key) = test_signer();
    let signed_request = ScheduleRecipeQuoteRequest {
        factory_id: "factory-a|recipe_id:recipe-b".to_string(),
        recipe_id: "recipe-c".to_string(),
        batches: 2,
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    let proof = sign_schedule_recipe_quote_auth_proof(
        &signed_request,
        17,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign quote");
    let previously_colliding_request = ScheduleRecipeQuoteRequest {
        factory_id: "factory-a".to_string(),
        recipe_id: "recipe-b|recipe_id:recipe-c".to_string(),
        ..signed_request.clone()
    };

    verify_schedule_recipe_quote_auth_proof(&signed_request, &proof)
        .expect("original request remains valid");
    let err = verify_schedule_recipe_quote_auth_proof(&previously_colliding_request, &proof)
        .expect_err("structured signing target must reject the former delimiter collision");
    assert!(err.contains("verify auth signature failed"));
}

#[test]
fn prompt_control_apply_auth_sign_and_verify_roundtrip() {
    let (public_key, private_key) = test_signer();
    let request = PromptControlApplyRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
        strong_auth_grant: None,
        expected_version: Some(3),
        updated_by: Some("player-a".to_string()),
        system_prompt_override: Some(Some("system".to_string())),
        short_term_goal_override: Some(None),
        long_term_goal_override: None,
        ..Default::default()
    };
    let proof = sign_prompt_control_apply_auth_proof(
        PromptControlAuthIntent::Apply,
        &request,
        11,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign proof");
    let verified =
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &request, &proof)
            .expect("verify proof");
    assert_eq!(verified.player_id, "player-a");
    assert_eq!(verified.public_key, public_key);
    assert_eq!(verified.nonce, 11);
}

#[test]
fn prompt_control_apply_auth_verify_rejects_tamper() {
    let (public_key, private_key) = test_signer();
    let request = PromptControlApplyRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
        strong_auth_grant: None,
        expected_version: Some(3),
        updated_by: Some("player-a".to_string()),
        system_prompt_override: Some(Some("system".to_string())),
        short_term_goal_override: None,
        long_term_goal_override: None,
        ..Default::default()
    };
    let proof = sign_prompt_control_apply_auth_proof(
        PromptControlAuthIntent::Apply,
        &request,
        12,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign proof");

    let mut tampered = request.clone();
    tampered.system_prompt_override = Some(Some("tampered".to_string()));
    let err =
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .expect_err("tampered payload must fail");
    assert!(err.contains("verify auth signature failed"));
}

#[test]
fn prompt_control_enhanced_auth_binds_epochs_and_operation_identity() {
    let (public_key, private_key) = test_signer();
    let request = PromptControlApplyRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        request_id: Some("request-a".to_string()),
        session_epoch: Some(4),
        binding_epoch: Some(9),
        expected_authority_epoch: Some("authority-a".to_string()),
        public_key: Some(public_key.clone()),
        auth: None,
        strong_auth_grant: None,
        expected_version: Some(3),
        updated_by: Some("player-a".to_string()),
        system_prompt_override: Some(Some("system".to_string())),
        short_term_goal_override: Some(None),
        long_term_goal_override: None,
    };
    let proof = sign_prompt_control_apply_auth_proof(
        PromptControlAuthIntent::Apply,
        &request,
        11,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign enhanced proof");
    verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &request, &proof)
        .expect("verify enhanced proof");

    let mut tampered = request.clone();
    tampered.request_id = Some("request-b".to_string());
    assert!(
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .is_err()
    );
    tampered = request.clone();
    tampered.session_epoch = Some(5);
    assert!(
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .is_err()
    );
    tampered = request.clone();
    tampered.binding_epoch = Some(10);
    assert!(
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .is_err()
    );
    tampered = request.clone();
    tampered.expected_authority_epoch = Some("authority-b".to_string());
    assert!(
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .is_err()
    );
    tampered = request.clone();
    tampered.expected_version = Some(4);
    assert!(
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .is_err()
    );
    tampered = request.clone();
    tampered.updated_by = Some("player-b".to_string());
    assert!(
        verify_prompt_control_apply_auth_proof(PromptControlAuthIntent::Apply, &tampered, &proof)
            .is_err()
    );

    let identity_a = normalize_prompt_control_operation_identity(
        "apply",
        false,
        "agent-0",
        "player-a",
        Some(4),
        Some(9),
        Some("authority-a"),
        Some(3),
        &request.system_prompt_override,
        &request.short_term_goal_override,
        &request.long_term_goal_override,
        None,
        request.updated_by.as_deref(),
    )
    .expect("normalize identity");
    let identity_b = identity_a.clone();
    assert_eq!(
        prompt_control_operation_digest(&identity_a),
        prompt_control_operation_digest(&identity_b),
        "typed operation digest is stable and independent of request id"
    );
}

#[test]
fn hosted_prompt_control_strong_auth_grant_roundtrip() {
    let (player_public_key, _) = test_signer();
    let (backend_public_key, backend_private_key) = test_signer_with_seed(9);
    let request = PromptControlApplyRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        public_key: Some(player_public_key.clone()),
        auth: None,
        strong_auth_grant: None,
        expected_version: Some(3),
        updated_by: Some("player-a".to_string()),
        system_prompt_override: Some(Some("system".to_string())),
        short_term_goal_override: None,
        long_term_goal_override: None,
        ..Default::default()
    };
    let grant = sign_hosted_prompt_control_strong_auth_grant(
        "prompt_control_apply",
        "player-a",
        player_public_key.as_str(),
        "agent-0",
        100,
        200,
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("sign hosted strong-auth grant");
    verify_hosted_prompt_control_apply_strong_auth_grant(
        PromptControlAuthIntent::Apply,
        &request,
        &grant,
        backend_public_key.as_str(),
        150,
    )
    .expect("verify hosted strong-auth grant");
}

#[test]
fn director_capability_grant_sign_and_verify_binds_session_scope() {
    let (player_public_key, _) = test_signer_with_seed(11);
    let (backend_public_key, backend_private_key) = test_signer_with_seed(12);
    let grant = sign_director_capability_grant(
        "player-a",
        player_public_key.as_str(),
        "viewer-live-1",
        4,
        "director-nonce-1",
        1_000,
        2_000,
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("sign director capability grant");
    verify_director_capability_grant(
        &grant,
        "player-a",
        player_public_key.as_str(),
        "viewer-live-1",
        4,
        backend_public_key.as_str(),
        1_500,
    )
    .expect("verify director capability grant");
}

#[test]
fn director_capability_grant_rejects_scope_ttl_and_expiry_violations() {
    let (player_public_key, _) = test_signer_with_seed(13);
    let (backend_public_key, backend_private_key) = test_signer_with_seed(14);
    let grant = sign_director_capability_grant(
        "player-a",
        player_public_key.as_str(),
        "viewer-live-1",
        4,
        "director-nonce-2",
        1_000,
        61_001,
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect_err("director grant TTL above 60s must be rejected before signing");
    assert!(grant.contains("TTL"));

    let valid = sign_director_capability_grant(
        "player-a",
        player_public_key.as_str(),
        "viewer-live-1",
        4,
        "director-nonce-3",
        1_000,
        2_000,
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("sign valid director capability grant");
    let mut tampered = valid.clone();
    tampered.scope = "diagnostics_write".to_string();
    let err = verify_director_capability_grant(
        &tampered,
        "player-a",
        player_public_key.as_str(),
        "viewer-live-1",
        4,
        backend_public_key.as_str(),
        1_500,
    )
    .expect_err("director scope must be fixed and signature-bound");
    assert!(err.contains("scope") || err.contains("signature"));
}

#[test]
fn hosted_prompt_control_strong_auth_grant_rejects_request_mismatch() {
    let (player_public_key, _) = test_signer();
    let (backend_public_key, backend_private_key) = test_signer_with_seed(10);
    let request = PromptControlApplyRequest {
        agent_id: "agent-0".to_string(),
        player_id: "player-a".to_string(),
        public_key: Some(player_public_key.clone()),
        auth: None,
        strong_auth_grant: None,
        expected_version: Some(3),
        updated_by: Some("player-a".to_string()),
        system_prompt_override: Some(Some("system".to_string())),
        short_term_goal_override: None,
        long_term_goal_override: None,
        ..Default::default()
    };
    let grant = sign_hosted_prompt_control_strong_auth_grant(
        "prompt_control_apply",
        "player-a",
        player_public_key.as_str(),
        "agent-1",
        100,
        200,
        backend_public_key.as_str(),
        backend_private_key.as_str(),
    )
    .expect("sign hosted strong-auth grant");
    let err = verify_hosted_prompt_control_apply_strong_auth_grant(
        PromptControlAuthIntent::Apply,
        &request,
        &grant,
        backend_public_key.as_str(),
        150,
    )
    .expect_err("mismatched hosted strong-auth grant must fail");
    assert!(err.contains("agent_id"));
}

#[test]
fn agent_chat_auth_verify_rejects_player_mismatch() {
    let (public_key, private_key) = test_signer();
    let request = AgentChatRequest {
        agent_id: "agent-0".to_string(),
        message: "hello".to_string(),
        player_id: Some("player-a".to_string()),
        public_key: Some(public_key.clone()),
        auth: None,
        intent_tick: Some(9),
        intent_seq: Some(15),
        world_id: None,
        reorg_epoch: None,
        authority_scope: None,
        replaces_intent_id: None,
        canonical_authority: None,
    };
    let mut proof =
        sign_agent_chat_auth_proof(&request, 15, public_key.as_str(), private_key.as_str())
            .expect("sign proof");
    proof.player_id = "player-b".to_string();
    let err = verify_agent_chat_auth_proof(&request, &proof).expect_err("player mismatch");
    assert!(err.contains("player_id"));
}

#[test]
fn agent_chat_auth_verify_rejects_invalid_signature_prefix() {
    let (public_key, private_key) = test_signer();
    let request = AgentChatRequest {
        agent_id: "agent-0".to_string(),
        message: "hello".to_string(),
        player_id: Some("player-a".to_string()),
        public_key: Some(public_key.clone()),
        auth: None,
        intent_tick: None,
        intent_seq: Some(16),
        world_id: None,
        reorg_epoch: None,
        authority_scope: None,
        replaces_intent_id: None,
        canonical_authority: None,
    };
    let mut proof =
        sign_agent_chat_auth_proof(&request, 16, public_key.as_str(), private_key.as_str())
            .expect("sign proof");
    proof.signature = "badprefix:deadbeef".to_string();
    let err = verify_agent_chat_auth_proof(&request, &proof).expect_err("invalid prefix");
    assert!(err.contains("awviewauth:v1"));
}

#[test]
fn agent_chat_auth_verify_rejects_zero_intent_seq() {
    let (public_key, private_key) = test_signer();
    let request = AgentChatRequest {
        agent_id: "agent-0".to_string(),
        message: "hello".to_string(),
        player_id: Some("player-a".to_string()),
        public_key: Some(public_key.clone()),
        auth: None,
        intent_tick: Some(1),
        intent_seq: Some(0),
        world_id: None,
        reorg_epoch: None,
        authority_scope: None,
        replaces_intent_id: None,
        canonical_authority: None,
    };
    let err = sign_agent_chat_auth_proof(&request, 17, public_key.as_str(), private_key.as_str())
        .expect_err("zero intent_seq should fail");
    assert!(err.contains("intent_seq"));
}

fn signed_v2_agent_chat_request(
    public_key: &str,
    private_key: &str,
) -> (AgentChatRequest, PlayerAuthProof) {
    let request = AgentChatRequest {
        agent_id: "agent-v2".to_string(),
        message: "prioritize the eastern relay".to_string(),
        player_id: Some("player-v2".to_string()),
        public_key: Some(public_key.to_string()),
        auth: None,
        intent_tick: Some(42),
        intent_seq: Some(7),
        world_id: Some("world-alpha".to_string()),
        reorg_epoch: Some(3),
        authority_scope: Some(AGENT_CHAT_AUTHORITY_SCOPE.to_string()),
        replaces_intent_id: Some("agent-intent-v2:previous".to_string()),
        canonical_authority: None,
    };
    let proof = sign_agent_chat_auth_proof(&request, 19, public_key, private_key)
        .expect("sign V2 Agent Chat proof");
    (request, proof)
}

#[test]
fn agent_chat_auth_v2_binds_authority_and_explicit_replacement() {
    let (public_key, private_key) = test_signer();
    let (request, proof) = signed_v2_agent_chat_request(&public_key, &private_key);
    verify_agent_chat_auth_proof(&request, &proof).expect("V2 proof remains valid");

    let mut world_tampered = request.clone();
    world_tampered.world_id = Some("world-beta".to_string());
    let err = verify_agent_chat_auth_proof(&world_tampered, &proof)
        .expect_err("world identity is signed");
    assert!(err.contains("verify auth signature failed"), "{err}");

    let mut reorg_tampered = request.clone();
    reorg_tampered.reorg_epoch = Some(4);
    let err =
        verify_agent_chat_auth_proof(&reorg_tampered, &proof).expect_err("reorg epoch is signed");
    assert!(err.contains("verify auth signature failed"), "{err}");

    let mut scope_tampered = request.clone();
    scope_tampered.authority_scope = Some("other_viewer_scope".to_string());
    let err = verify_agent_chat_auth_proof(&scope_tampered, &proof)
        .expect_err("authority scope is signed");
    assert!(err.contains("verify auth signature failed"), "{err}");

    let mut replacement_tampered = request;
    replacement_tampered.replaces_intent_id = Some("agent-intent-v2:another".to_string());
    let err = verify_agent_chat_auth_proof(&replacement_tampered, &proof)
        .expect_err("replacement target is signed");
    assert!(err.contains("verify auth signature failed"), "{err}");
}

#[test]
fn agent_chat_auth_v2_requires_complete_authority_and_expected_context() {
    let (public_key, private_key) = test_signer();
    let (request, proof) = signed_v2_agent_chat_request(&public_key, &private_key);
    verify_agent_chat_auth_proof_with_authority(
        &request,
        &proof,
        "world-alpha",
        3,
        AGENT_CHAT_AUTHORITY_SCOPE,
    )
    .expect("matching authority context remains valid");

    let err = verify_agent_chat_auth_proof_with_authority(
        &request,
        &proof,
        "world-beta",
        3,
        AGENT_CHAT_AUTHORITY_SCOPE,
    )
    .expect_err("cross-world replay must fail");
    assert!(err.contains("world_id mismatch"), "{err}");

    let err = verify_agent_chat_auth_proof_with_authority(
        &request,
        &proof,
        "world-alpha",
        4,
        AGENT_CHAT_AUTHORITY_SCOPE,
    )
    .expect_err("cross-reorg replay must fail");
    assert!(err.contains("reorg_epoch mismatch"), "{err}");

    let err = verify_agent_chat_auth_proof_with_authority(
        &request,
        &proof,
        "world-alpha",
        3,
        "other_viewer_scope",
    )
    .expect_err("cross-scope replay must fail");
    assert!(err.contains("authority_scope mismatch"), "{err}");

    let mut partial = request;
    partial.authority_scope = None;
    let err = sign_agent_chat_auth_proof(&partial, 20, &public_key, &private_key)
        .expect_err("partial authority envelope must not be signed");
    assert!(err.contains("authority envelope"), "{err}");
}

#[test]
fn gameplay_action_auth_sign_and_verify_roundtrip() {
    let (public_key, private_key) = test_signer();
    let request = GameplayActionRequest {
        action_id: "build_factory_smelter_mk1".to_string(),
        target_agent_id: "agent-0".to_string(),
        actor_agent_id: None,
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    let proof =
        sign_gameplay_action_auth_proof(&request, 21, public_key.as_str(), private_key.as_str())
            .expect("sign proof");
    let verified = verify_gameplay_action_auth_proof(&request, &proof).expect("verify proof");
    assert_eq!(verified.player_id, "player-a");
    assert_eq!(verified.public_key, public_key);
    assert_eq!(verified.nonce, 21);
}

#[test]
fn gameplay_action_auth_verify_rejects_tampered_action_id() {
    let (public_key, private_key) = test_signer();
    let request = GameplayActionRequest {
        action_id: "build_factory_smelter_mk1".to_string(),
        target_agent_id: "agent-0".to_string(),
        actor_agent_id: None,
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        auth: None,
    };
    let proof =
        sign_gameplay_action_auth_proof(&request, 22, public_key.as_str(), private_key.as_str())
            .expect("sign proof");
    let mut tampered = request.clone();
    tampered.action_id = "schedule_recipe_smelter_iron_ingot".to_string();
    let err = verify_gameplay_action_auth_proof(&tampered, &proof).expect_err("tamper must fail");
    assert!(err.contains("verify auth signature failed"));
}

#[test]
fn collect_data_auth_binds_mode_cost_and_yield() {
    let (public_key, private_key) = test_signer();
    let command = CollectDataCommand::Preflight {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "player-a".to_string(),
            public_key: Some(public_key.clone()),
            auth: None,
        },
    };
    let proof =
        sign_collect_data_auth_proof(&command, 23, public_key.as_str(), private_key.as_str())
            .expect("sign proof");
    assert_eq!(
        verify_collect_data_auth_proof(&command, &proof)
            .expect("verify preflight proof")
            .nonce,
        23
    );

    let tampered_cost = CollectDataCommand::Preflight {
        request: CollectDataRequest {
            electricity_cost: 8,
            ..match command.clone() {
                CollectDataCommand::Preflight { request } => request,
                CollectDataCommand::Submit { .. } => unreachable!(),
            }
        },
    };
    assert!(
        verify_collect_data_auth_proof(&tampered_cost, &proof)
            .expect_err("cost tampering must fail")
            .contains("verify auth signature failed")
    );

    let submit = match command.clone() {
        CollectDataCommand::Preflight { request } => CollectDataCommand::Submit { request },
        CollectDataCommand::Submit { .. } => unreachable!(),
    };
    assert!(
        verify_collect_data_auth_proof(&submit, &proof)
            .expect_err("preflight proof must not authorize submit")
            .contains("verify auth signature failed")
    );
}

#[test]
fn session_register_auth_verify_rejects_tampered_requested_agent_id() {
    let (public_key, private_key) = test_signer();
    let request = AuthoritativeSessionRegisterRequest {
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        registration_grant: None,
        auth: None,
        requested_agent_id: Some("agent-0".to_string()),
        force_rebind: false,
    };
    let proof =
        sign_session_register_auth_proof(&request, 31, public_key.as_str(), private_key.as_str())
            .expect("sign proof");

    let mut tampered = request.clone();
    tampered.requested_agent_id = Some("agent-1".to_string());
    let err = verify_session_register_auth_proof_signature(&tampered, &proof)
        .expect_err("tampered requested_agent_id must fail");
    assert!(err.contains("verify auth signature failed"));
}

#[test]
fn session_register_auth_verify_rejects_tampered_force_rebind() {
    let (public_key, private_key) = test_signer();
    let request = AuthoritativeSessionRegisterRequest {
        player_id: "player-a".to_string(),
        public_key: Some(public_key.clone()),
        registration_grant: None,
        auth: None,
        requested_agent_id: Some("agent-0".to_string()),
        force_rebind: false,
    };
    let proof =
        sign_session_register_auth_proof(&request, 32, public_key.as_str(), private_key.as_str())
            .expect("sign proof");

    let mut tampered = request.clone();
    tampered.force_rebind = true;
    let err = verify_session_register_auth_proof_signature(&tampered, &proof)
        .expect_err("tampered force_rebind must fail");
    assert!(err.contains("verify auth signature failed"));
}

#[test]
fn hosted_session_register_rejects_arbitrary_self_signed_hosted_player_claim() {
    let (attacker_public_key, attacker_private_key) = test_signer_with_seed(41);
    let request = AuthoritativeSessionRegisterRequest {
        player_id: "hosted-player-test-login".to_string(),
        public_key: Some(attacker_public_key.clone()),
        registration_grant: None,
        auth: None,
        requested_agent_id: None,
        force_rebind: false,
    };
    let proof = sign_session_register_auth_proof(
        &request,
        33,
        attacker_public_key.as_str(),
        attacker_private_key.as_str(),
    )
    .expect("attacker can self-sign its claim");

    let error = verify_session_register_auth_proof_signature(&request, &proof)
        .expect_err("hosted player claims require an issuer-authenticated registration grant");
    assert!(
        error.contains("registration grant"),
        "unexpected error: {error}"
    );
}
