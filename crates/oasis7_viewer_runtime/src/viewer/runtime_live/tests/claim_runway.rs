use super::*;

const SNAPSHOT_PLAYER_ID: &str = "player-snapshot";

fn bind_agent_for_snapshot(server: &mut ViewerRuntimeLiveServer, agent_id: &str) {
    server
        .llm_sidecar
        .bind_agent_player(
            agent_id,
            SNAPSHOT_PLAYER_ID,
            Some("snapshot-public-key"),
            false,
        )
        .expect("bind snapshot player to agent");
}

#[test]
fn compat_snapshot_quotes_complete_epoch_runway_and_advisory_threshold() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    for (balance, expected_after, expected_runway, expected_warning, expected_action) in [
        (375, 50, 2, false, "compare_candidates_first"),
        (374, 49, 1, true, "wait_or_fund_first"),
    ] {
        let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
            WorldScenario::Minimal,
        ))
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
        server
            .world
            .set_governance_execution_policy(crate::runtime::GovernanceExecutionPolicy {
                epoch_length_ticks: 1,
                ..crate::runtime::GovernanceExecutionPolicy::default()
            })
            .expect("set governance policy");
        server
            .world
            .set_main_token_supply(crate::runtime::MainTokenSupplyState {
                total_supply: balance,
                circulating_supply: balance,
                ..crate::runtime::MainTokenSupplyState::default()
            });
        server
            .world
            .set_main_token_account_balance(primary_agent_id.as_str(), balance, 0)
            .expect("seed main token balance");

        let snapshot = server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID));
        let claim = snapshot
            .player_gameplay
            .as_ref()
            .and_then(|gameplay| gameplay.agent_claim.as_ref())
            .expect("player agent claim snapshot");
        let quote = claim.next_claim_quote.as_ref().expect("next claim quote");
        assert_eq!(quote.total_upfront_amount, 325);
        assert_eq!(quote.upkeep_per_epoch, 25);
        assert_eq!(quote.grace_epochs, 2);
        assert_eq!(quote.eligible_balance_after, expected_after);
        assert_eq!(quote.upkeep_runway_epochs, expected_runway);
        assert_eq!(quote.next_upkeep_due_epoch, Some(claim.current_epoch + 1));
        assert_eq!(
            quote.projected_grace_entry_epoch,
            Some(claim.current_epoch + 1 + expected_runway)
        );
        assert_eq!(quote.low_runway_warning, expected_warning);
        assert_eq!(
            quote.recommended_claim_action.as_deref(),
            Some(expected_action)
        );
        assert_eq!(quote.blocked_reason, None, "warning must remain advisory");
    }
}

#[test]
fn compat_snapshot_exposes_slot_1_claim_choice_quote() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
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
        .expect("player agent claim snapshot");
    let quote = claim.next_claim_quote.as_ref().expect("next claim quote");
    let choice = quote
        .slot_1_claim_choice_quote
        .as_ref()
        .expect("slot-1 quote includes an optional candidate choice package");

    assert_eq!(choice.status, "candidate_rationale_missing");
    assert!(choice.candidates.is_empty());
    assert_eq!(choice.claim_choice_class, "wait_or_fund_first");
    assert_eq!(choice.recommended_claim_action, "wait_or_fund_first");
    assert_eq!(
        choice.fallback_reason.as_deref(),
        Some("candidate_rationale_missing")
    );
}
