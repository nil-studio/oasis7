use super::snapshot_progress::{SNAPSHOT_PLAYER_ID, bind_agent_for_snapshot};
use super::*;

#[test]
fn compat_snapshot_projects_first_chat_unlock_preview_only_for_zero_liquid_without_starter_oc() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let primary_agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("primary agent");
    bind_agent_for_snapshot(&mut server, primary_agent_id.as_str());

    let snapshot = server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID));
    let claim = snapshot
        .player_gameplay
        .as_ref()
        .and_then(|gameplay| gameplay.agent_claim.as_ref())
        .expect("bound Agent claim overview");
    assert!(server.world.state().agent_claims.is_empty());
    assert!(claim.owned_claims.is_empty());
    let preview = claim
        .first_chat_unlock_preview
        .as_ref()
        .expect("zero-liquid starter gate publishes first-chat unlock preview");
    assert_eq!(
        preview.chat_purpose,
        "Start a first conversation with the Agent you control."
    );
    assert_eq!(
        preview.immediate_playable_help,
        "Ask what the Agent can do next for the current gameplay goal."
    );
    assert_eq!(
        preview.first_question_or_action_hint,
        "Ask: What should we do first?"
    );
    for explanation in [&preview.resource_boundary, &preview.defer_effect] {
        assert!(
            explanation
                .contains("positive liquid OC balance or an existing starter OC claim record")
        );
        assert!(explanation.contains("open even after the balance is spent"));
    }
    assert!(
        preview
            .resource_boundary
            .contains("one-time first-chat unlock")
    );
    assert!(
        preview
            .resource_boundary
            .contains("separate from economic slot-1 claim")
    );
    assert!(preview.defer_effect.starts_with(concat!(
        "Deferring leaves your Agent binding unchanged and creates no economic claim ",
        "or upkeep obligation."
    )));
    assert!(!preview.defer_effect.contains("existing economic claims"));
    assert_eq!(preview.recommended_unlock_action, "claim_starter_oc");

    let mut nonzero_liquid_state = server.world.state().clone();
    nonzero_liquid_state
        .main_token_balances
        .entry(primary_agent_id.clone())
        .or_default()
        .liquid_balance = 1;
    assert!(
        super::super::claim_snapshot::build_player_agent_claim_snapshot(
            &nonzero_liquid_state,
            primary_agent_id.as_str(),
            server
                .world
                .governance_execution_policy()
                .epoch_length_ticks,
        )
        .expect("primary agent claim snapshot")
        .first_chat_unlock_preview
        .is_none()
    );

    let mut claimed_starter_oc_state = server.world.state().clone();
    claimed_starter_oc_state
        .starter_oc_claims
        .insert(primary_agent_id.clone(), Default::default());
    for liquid_balance in [1, 0] {
        claimed_starter_oc_state
            .main_token_balances
            .entry(primary_agent_id.clone())
            .or_default()
            .liquid_balance = liquid_balance;
        assert!(
            super::super::claim_snapshot::build_player_agent_claim_snapshot(
                &claimed_starter_oc_state,
                primary_agent_id.as_str(),
                server
                    .world
                    .governance_execution_policy()
                    .epoch_length_ticks,
            )
            .expect("primary agent claim snapshot")
            .first_chat_unlock_preview
            .is_none(),
            "the starter OC record keeps the gate open after its liquid balance is spent"
        );
    }

    assert!(
        super::super::claim_snapshot::build_player_agent_claim_snapshot(
            server.world.state(),
            "missing-primary-agent",
            server
                .world
                .governance_execution_policy()
                .epoch_length_ticks,
        )
        .is_none()
    );
}

#[test]
fn first_chat_unlock_preview_distinguishes_registration_from_economic_claims() {
    let mut world = crate::runtime::World::new();
    let agent_id = crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID;
    let request = crate::viewer::GameplayActionRequest {
        action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.to_string(),
        target_agent_id: agent_id.to_string(),
        actor_agent_id: None,
        player_id: "first-chat-player".to_string(),
        public_key: None,
        auth: None,
    };
    let action = crate::viewer::build_runtime_action_from_gameplay_request(&request)
        .expect("first-Agent registration action");
    assert!(matches!(
        &action,
        crate::runtime::Action::RegisterAgent { .. }
    ));
    world.submit_action(action);
    world.step().expect("register first Agent");
    assert!(world.state().agent_claims.is_empty());

    let before_claim = super::super::claim_snapshot::build_player_agent_claim_snapshot(
        world.state(),
        agent_id,
        world.governance_execution_policy().epoch_length_ticks,
    )
    .expect("registered Agent snapshot");
    assert_eq!(before_claim.owned_claim_count, 0);
    assert!(before_claim.next_claim_quote.is_some());
    assert!(
        before_claim
            .first_chat_unlock_preview
            .expect("registered Agent first-chat preview")
            .defer_effect
            .contains("creates no economic claim or upkeep obligation")
    );

    world.submit_action(crate::runtime::Action::RegisterAgent {
        agent_id: "first-chat-claim-target".to_string(),
        pos: crate::geometry::GeoPos::new(0, 0, 0),
    });
    world.step().expect("register economic claim target");
    world.set_main_token_supply(crate::runtime::MainTokenSupplyState {
        total_supply: 1_000,
        circulating_supply: 1_000,
        ..crate::runtime::MainTokenSupplyState::default()
    });
    world
        .set_main_token_account_balance_with_restricted(agent_id, 0, 0, 1_000)
        .expect("fund economic claim without unlocking liquid-OC gate");
    world.submit_action(crate::runtime::Action::ClaimAgent {
        claimer_agent_id: agent_id.to_string(),
        target_agent_id: "first-chat-claim-target".to_string(),
    });
    world.step().expect("apply economic claim");
    let actual_claim = world
        .agent_claim("first-chat-claim-target")
        .expect("economic claim exists");
    assert_eq!(actual_claim.claim_owner_id, agent_id);
    assert!(actual_claim.upkeep_per_epoch > 0);
    assert_eq!(world.main_token_liquid_balance(agent_id), 0);

    let owned = super::super::claim_snapshot::build_player_agent_claim_snapshot(
        world.state(),
        agent_id,
        world.governance_execution_policy().epoch_length_ticks,
    )
    .expect("economic owner snapshot");
    assert_eq!(owned.owned_claim_count, 1);
    assert!(
        owned
            .first_chat_unlock_preview
            .expect("zero-liquid economic owner preview")
            .defer_effect
            .starts_with(concat!(
                "Deferring leaves your existing economic claims and their current upkeep ",
                "rules unchanged."
            ))
    );

    let other_agent = super::super::claim_snapshot::build_player_agent_claim_snapshot(
        world.state(),
        "first-chat-claim-target",
        world.governance_execution_policy().epoch_length_ticks,
    )
    .expect("another Agent snapshot");
    assert_eq!(other_agent.owned_claim_count, 0);
    assert!(
        other_agent
            .first_chat_unlock_preview
            .expect("another Agent first-chat preview")
            .defer_effect
            .contains("creates no economic claim or upkeep obligation"),
        "a claim owned by another Agent must not create a claimer upkeep statement"
    );
}
