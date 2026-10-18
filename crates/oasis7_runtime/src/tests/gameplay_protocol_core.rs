use super::super::*;
use super::pos;
use oasis7_agent_api::world_action::ResourceKind;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "gameplay_protocol_policy_tests.rs"]
mod policy_tests;

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[path = "gameplay_protocol_sandbox.rs"]
mod sandbox;
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
use sandbox::GameplayDirectiveSandbox;

fn register_agents(world: &mut World, agent_ids: &[&str]) {
    for (index, agent_id) in agent_ids.iter().enumerate() {
        world.submit_action(Action::RegisterAgent {
            agent_id: (*agent_id).to_string(),
            pos: pos(index as i64, 0),
        });
    }
    world.step().expect("register agents");
}

fn set_agent_resources(world: &mut World, agent_id: &str, electricity: i64, data: i64) {
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Electricity, electricity)
        .expect("set electricity");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Data, data)
        .expect("set data");
}

fn seed_war_ready_resources(world: &mut World, agent_ids: &[&str]) {
    for agent_id in agent_ids {
        set_agent_resources(world, agent_id, 120, 120);
    }
}

fn last_domain_event(world: &World) -> &DomainEvent {
    let event = world.journal().events.last().expect("domain event");
    let WorldEventBody::Domain(domain_event) = &event.body else {
        panic!("expected domain event");
    };
    domain_event
}

fn assert_latest_rule_denied_contains(world: &World, needle: &str) {
    let notes = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) => Some(notes),
            _ => None,
        })
        .expect("rule denied event");
    assert!(
        notes.iter().any(|note| note.contains(needle)),
        "expected reject note containing '{needle}', got {notes:?}"
    );
}

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

fn open_governance_proposal_by(
    world: &mut World,
    proposer_agent_id: &str,
    proposal_key: &str,
    window_ticks: u64,
    quorum_weight: u64,
    pass_threshold_bps: u16,
) {
    world.submit_action(Action::OpenGovernanceProposal {
        proposer_agent_id: proposer_agent_id.to_string(),
        proposal_key: proposal_key.to_string(),
        title: format!("title.{proposal_key}"),
        description: "runtime proposal".to_string(),
        options: vec!["approve".to_string(), "reject".to_string()],
        voting_window_ticks: window_ticks,
        quorum_weight,
        pass_threshold_bps,
    });
    world.step().expect("open governance proposal");
}

fn open_governance_proposal(
    world: &mut World,
    proposal_key: &str,
    window_ticks: u64,
    quorum_weight: u64,
    pass_threshold_bps: u16,
) {
    open_governance_proposal_by(
        world,
        "a",
        proposal_key,
        window_ticks,
        quorum_weight,
        pass_threshold_bps,
    );
}

fn authorize_policy_update(world: &mut World, operator_agent_id: &str, proposal_key: &str) {
    open_governance_proposal_by(world, operator_agent_id, proposal_key, 1, 3, 5_000);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: operator_agent_id.to_string(),
        proposal_key: proposal_key.to_string(),
        option: "approve".to_string(),
        weight: 3,
    });
    world
        .step()
        .expect("cast governance vote for policy authorization");

    for _ in 0..2 {
        let proposal = world
            .state()
            .governance_proposals
            .get(proposal_key)
            .expect("policy authorization proposal exists");
        if proposal.status != GovernanceProposalStatus::Open {
            break;
        }
        world
            .step()
            .expect("advance governance proposal to finalize");
    }

    let proposal = world
        .state()
        .governance_proposals
        .get(proposal_key)
        .expect("policy authorization proposal finalized");
    assert_eq!(proposal.status, GovernanceProposalStatus::Passed);
    assert!(proposal.total_weight_at_finalize >= 3);
}

fn advance_until_auto_crisis(world: &mut World) -> String {
    for _ in 0..64 {
        world.step().expect("advance for crisis cycle");
        if let Some((crisis_id, _)) = world
            .state()
            .crises
            .iter()
            .find(|(_, crisis)| crisis.status == CrisisStatus::Active)
        {
            return crisis_id.clone();
        }
    }
    panic!("expected an auto crisis to spawn");
}

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-gameplay-{prefix}-{unique}"))
}

#[test]
fn gameplay_actions_emit_action_accepted_before_resolution_event() {
    let mut world = World::new();
    register_agents(&mut world, &["a"]);

    let action_id = world.submit_action(Action::OpenGovernanceProposal {
        proposer_agent_id: "a".to_string(),
        proposal_key: "proposal.action.accepted".to_string(),
        title: "Action Accepted".to_string(),
        description: "verify ack ordering".to_string(),
        options: vec!["approve".to_string(), "reject".to_string()],
        voting_window_ticks: 6,
        quorum_weight: 1,
        pass_threshold_bps: 5_000,
    });
    world.step().expect("open governance proposal");

    let events_for_action = world
        .journal()
        .events
        .iter()
        .filter(|event| matches!(event.caused_by, Some(CausedBy::Action(id)) if id == action_id))
        .collect::<Vec<_>>();
    assert!(
        events_for_action.len() >= 2,
        "expected accepted + resolved events for action, got {}",
        events_for_action.len()
    );

    let WorldEventBody::Domain(DomainEvent::ActionAccepted {
        action_id: accepted_action_id,
        action_kind,
        actor_id,
        eta_ticks,
        ..
    }) = &events_for_action[0].body
    else {
        panic!(
            "expected ActionAccepted as first event, got {:?}",
            events_for_action[0].body
        );
    };
    assert_eq!(*accepted_action_id, action_id);
    assert_eq!(action_kind, "action.gameplay.open_governance_proposal");
    assert_eq!(actor_id, "a");
    assert_eq!(*eta_ticks, 0);
    assert!(matches!(
        events_for_action[1].body,
        WorldEventBody::Domain(DomainEvent::GovernanceProposalOpened { .. })
    ));
}

#[test]
fn gameplay_action_accepted_event_survives_save_and_load() {
    let mut world = World::new();
    register_agents(&mut world, &["a"]);

    let action_id = world.submit_action(Action::OpenEconomicContract {
        creator_agent_id: "a".to_string(),
        contract_id: "contract.ack.persist".to_string(),
        counterparty_agent_id: "a".to_string(),
        fulfillment_kind: EconomicContractFulfillmentKind::AtomicExchange,
        settlement_kind: ResourceKind::Data,
        settlement_amount: 1,
        reputation_stake: 1,
        expires_at: world.state().time.saturating_add(8),
        description: "persist ack event".to_string(),
    });
    world.step().expect("open economic contract");

    let dir = temp_dir("gameplay-action-accepted-persist");
    world.save_to_dir(&dir).expect("save world");
    let restored = World::load_from_dir(&dir).expect("load world");

    let has_action_accepted = restored.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionAccepted {
                action_id: accepted_action_id,
                action_kind,
                actor_id,
                ..
            }) if *accepted_action_id == action_id
                && action_kind == "action.gameplay.open_economic_contract"
                && actor_id == "a"
        )
    });
    assert!(
        has_action_accepted,
        "expected ActionAccepted event to persist through save/load"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn gameplay_protocol_actions_drive_persisted_state() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d"]);

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        members: vec!["b".to_string()],
        charter: "mutual defense".to_string(),
    });
    world.step().expect("form red alliance");

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "c".to_string(),
        alliance_id: "alliance.blue".to_string(),
        members: vec!["d".to_string()],
        charter: "logistics pact".to_string(),
    });
    world.step().expect("form blue alliance");
    seed_war_ready_resources(&mut world, &["a"]);

    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.001".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "control asteroid belt".to_string(),
        intensity: 2,
    });
    world.step().expect("declare war");

    open_governance_proposal(&mut world, "proposal.energy_tax", 4, 2, 5_000);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.energy_tax".to_string(),
        option: "approve".to_string(),
        weight: 3,
    });
    world.step().expect("cast governance vote");

    let crisis_id = advance_until_auto_crisis(&mut world);
    world.submit_action(Action::ResolveCrisis {
        resolver_agent_id: "c".to_string(),
        crisis_id: crisis_id.clone(),
        strategy: "redistribute shield grid".to_string(),
        success: true,
    });
    world.step().expect("resolve crisis");

    world.submit_action(Action::GrantMetaProgress {
        operator_agent_id: "a".to_string(),
        target_agent_id: "b".to_string(),
        track: "campaign".to_string(),
        points: 15,
        achievement_id: Some("first_alliance_win".to_string()),
    });
    world.step().expect("grant meta progress");

    let red = world
        .state()
        .alliances
        .get("alliance.red")
        .expect("red alliance");
    assert_eq!(red.members, vec!["a".to_string(), "b".to_string()]);

    let war = world.state().wars.get("war.001").expect("war record");
    assert_eq!(war.aggressor_alliance_id, "alliance.red");
    assert_eq!(war.defender_alliance_id, "alliance.blue");
    assert!(war.active);

    let governance = world
        .state()
        .governance_votes
        .get("proposal.energy_tax")
        .expect("governance vote state");
    assert_eq!(governance.total_weight, 3);
    assert_eq!(governance.tallies.get("approve"), Some(&3_u64));

    let proposal = world
        .state()
        .governance_proposals
        .get("proposal.energy_tax")
        .expect("governance proposal state");
    assert_eq!(proposal.status, GovernanceProposalStatus::Passed);

    let crisis = world.state().crises.get(&crisis_id).expect("crisis state");
    assert_eq!(crisis.status, CrisisStatus::Resolved);
    assert_eq!(crisis.success, Some(true));
    assert_eq!(crisis.impact, 20);

    let progress = world.state().meta_progress.get("b").expect("meta progress");
    assert_eq!(progress.total_points, 15);
    assert_eq!(progress.track_points.get("campaign"), Some(&15));
    assert_eq!(
        progress.achievements,
        vec!["first_alliance_win".to_string()]
    );

    assert!(matches!(
        last_domain_event(&world),
        DomainEvent::MetaProgressGranted { .. }
    ));
}

#[test]
fn threat_heatmap_tracks_active_war_and_crisis_risk() {
    let mut world = World::new();
    assert!(world.prepare_threat_heatmap().is_empty());
    register_agents(&mut world, &["a", "b", "c", "d"]);

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        members: vec!["b".to_string()],
        charter: "mutual defense".to_string(),
    });
    world.step().expect("form red alliance");
    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "c".to_string(),
        alliance_id: "alliance.blue".to_string(),
        members: vec!["d".to_string()],
        charter: "mutual defense".to_string(),
    });
    world.step().expect("form blue alliance");
    seed_war_ready_resources(&mut world, &["a"]);

    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.threat.001".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "threat-map".to_string(),
        intensity: 3,
    });
    world.step().expect("declare war");

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let heatmap_before_prepare = world.threat_heatmap().clone();
    let prepared_heatmap = world.prepare_threat_heatmap();
    assert_eq!(prepared_heatmap, heatmap_before_prepare);
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert_eq!(world.threat_heatmap(), &heatmap_before_prepare);

    let heatmap = world.threat_heatmap();
    assert!(heatmap.get("alliance:alliance.red").copied().unwrap_or(0) > 0);
    assert!(heatmap.get("alliance:alliance.blue").copied().unwrap_or(0) > 0);
    assert!(heatmap.get("global:war").copied().unwrap_or(0) > 0);

    let crisis_id = advance_until_auto_crisis(&mut world);
    let crisis_kind = world
        .state()
        .crises
        .get(&crisis_id)
        .expect("crisis exists")
        .kind
        .clone();
    let heatmap_after_crisis = world.threat_heatmap();
    assert_eq!(&world.prepare_threat_heatmap(), heatmap_after_crisis);
    assert!(
        heatmap_after_crisis
            .get(format!("crisis:{crisis_kind}").as_str())
            .copied()
            .unwrap_or(0)
            > 0
    );
    assert!(
        heatmap_after_crisis
            .get("global:crisis")
            .copied()
            .unwrap_or(0)
            > 0
    );

    world.submit_action(Action::ResolveCrisis {
        resolver_agent_id: "c".to_string(),
        crisis_id: crisis_id.clone(),
        strategy: "stabilize".to_string(),
        success: true,
    });
    world.step().expect("resolve crisis");
    let heatmap_after_resolution = world.threat_heatmap();
    assert_eq!(&world.prepare_threat_heatmap(), heatmap_after_resolution);
    assert!(
        heatmap_after_resolution
            .get("global:crisis")
            .copied()
            .unwrap_or(0)
            == 0
    );
}

#[test]
fn declare_war_rejects_initiator_outside_aggressor_alliance() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d"]);

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        members: vec!["b".to_string()],
        charter: "charter.red".to_string(),
    });
    world.step().expect("form red alliance");

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "c".to_string(),
        alliance_id: "alliance.blue".to_string(),
        members: vec!["d".to_string()],
        charter: "charter.blue".to_string(),
    });
    world.step().expect("form blue alliance");
    seed_war_ready_resources(&mut world, &["c"]);

    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "c".to_string(),
        war_id: "war.invalid".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "invalid".to_string(),
        intensity: 1,
    });
    world.step().expect("reject invalid war declare");

    match last_domain_event(&world) {
        DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("is not a member of aggressor alliance"))
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn governance_vote_recast_replaces_previous_tally() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    open_governance_proposal(&mut world, "proposal.runtime", 6, 1, 5_000);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.runtime".to_string(),
        option: "approve".to_string(),
        weight: 3,
    });
    world.step().expect("first vote");

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.runtime".to_string(),
        option: "reject".to_string(),
        weight: 1,
    });
    world.step().expect("recast vote");

    let governance = world
        .state()
        .governance_votes
        .get("proposal.runtime")
        .expect("governance vote state");
    assert_eq!(governance.total_weight, 1);
    assert_eq!(governance.tallies.get("reject"), Some(&1_u64));
    assert!(!governance.tallies.contains_key("approve"));
    assert_eq!(governance.votes_by_agent.len(), 1);
}

#[test]
fn governance_vote_rejects_weight_above_cap() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    open_governance_proposal(&mut world, "proposal.vote_cap", 6, 1, 5_000);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.vote_cap".to_string(),
        option: "approve".to_string(),
        weight: 101,
    });
    world.step().expect("reject vote weight overflow");

    match last_domain_event(&world) {
        DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("vote weight must be <= 100"))
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let governance = world
        .state()
        .governance_votes
        .get("proposal.vote_cap")
        .expect("governance vote state");
    assert_eq!(governance.total_weight, 0);
    assert!(governance.votes_by_agent.is_empty());
    assert!(governance.tallies.is_empty());
}

#[test]
fn governance_proposal_finalizes_and_rejects_late_votes() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c"]);
    open_governance_proposal(&mut world, "proposal.finalize", 2, 3, 6_000);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.finalize".to_string(),
        option: "approve".to_string(),
        weight: 2,
    });
    world.step().expect("vote from a");

    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let prepared = world
        .prepared_governance_finalization_events_for_test(world.state().time.saturating_add(1));
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert!(matches!(
        prepared.as_slice(),
        [DomainEvent::GovernanceProposalFinalized {
            proposal_key,
            winning_option,
            winning_weight: 2,
            total_weight: 2,
            passed: false,
        }] if proposal_key == "proposal.finalize"
            && winning_option.as_deref() == Some("approve")
    ));

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "b".to_string(),
        proposal_key: "proposal.finalize".to_string(),
        option: "approve".to_string(),
        weight: 1,
    });
    world.step().expect("vote from b and finalize");

    let proposal = world
        .state()
        .governance_proposals
        .get("proposal.finalize")
        .expect("finalized proposal");
    assert_eq!(proposal.status, GovernanceProposalStatus::Passed);
    assert_eq!(proposal.winning_option.as_deref(), Some("approve"));
    assert_eq!(proposal.total_weight_at_finalize, 3);

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "c".to_string(),
        proposal_key: "proposal.finalize".to_string(),
        option: "reject".to_string(),
        weight: 5,
    });
    world.step().expect("late vote rejected");

    match last_domain_event(&world) {
        DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("proposal is not open"))
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn governance_vote_enforces_identity_snapshot_cap() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    world
        .set_agent_reputation_score("a", 20)
        .expect("set reputation");
    world
        .set_governance_identity_profile("a", 16, 0, GovernanceIdentityStatus::Active)
        .expect("set governance identity profile");

    open_governance_proposal(&mut world, "proposal.identity.snapshot", 6, 1, 5_000);
    let snapshot_cap = world
        .state()
        .governance_proposals
        .get("proposal.identity.snapshot")
        .and_then(|proposal| proposal.vote_weight_snapshot.get("a"))
        .map(|snapshot| snapshot.vote_weight_cap)
        .expect("snapshot cap");
    assert_eq!(snapshot_cap, 6);

    world
        .set_agent_reputation_score("a", 2_000)
        .expect("raise live reputation");
    world
        .set_governance_identity_profile("a", 65_536, 0, GovernanceIdentityStatus::Active)
        .expect("raise live stake");

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.identity.snapshot".to_string(),
        option: "approve".to_string(),
        weight: snapshot_cap.saturating_add(1),
    });
    world
        .step()
        .expect("vote above snapshot cap should be rejected");
    assert_latest_rule_denied_contains(&world, "exceeds snapshot cap");

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.identity.snapshot".to_string(),
        option: "approve".to_string(),
        weight: snapshot_cap,
    });
    world.step().expect("vote at snapshot cap");
    let governance = world
        .state()
        .governance_votes
        .get("proposal.identity.snapshot")
        .expect("governance vote state");
    assert_eq!(governance.total_weight, u64::from(snapshot_cap));
}

#[test]
fn governance_vote_rejects_voter_not_in_snapshot() {
    let mut world = World::new();
    register_agents(&mut world, &["a"]);
    open_governance_proposal(&mut world, "proposal.snapshot.membership", 6, 1, 5_000);

    world.submit_action(Action::RegisterAgent {
        agent_id: "late".to_string(),
        pos: pos(3, 0),
    });
    world.step().expect("register late voter");

    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "late".to_string(),
        proposal_key: "proposal.snapshot.membership".to_string(),
        option: "approve".to_string(),
        weight: 1,
    });
    world.step().expect("late voter should be rejected");
    assert_latest_rule_denied_contains(&world, "not in governance snapshot");
}

#[test]
fn governance_identity_penalty_and_appeal_drive_vote_rights() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);
    world
        .set_governance_identity_profile("a", 25, 0, GovernanceIdentityStatus::Active)
        .expect("set identity profile");
    let penalty_id = world
        .apply_identity_penalty(
            "a",
            "evidence.sybil.vote",
            "suspected sybil voting ring",
            5,
            10,
            "guardian-1",
            local_guardians(),
        )
        .expect("apply identity penalty");

    open_governance_proposal(&mut world, "proposal.penalty.block", 6, 1, 5_000);
    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.penalty.block".to_string(),
        option: "approve".to_string(),
        weight: 1,
    });
    world.step().expect("frozen voter should be rejected");
    assert_latest_rule_denied_contains(&world, "identity is not active");

    world
        .appeal_identity_penalty(penalty_id, "a", "appeal with evidence")
        .expect("appeal penalty");
    world
        .resolve_identity_penalty_appeal(penalty_id, "committee", true, "appeal accepted")
        .expect("resolve appeal");

    open_governance_proposal(&mut world, "proposal.penalty.restore", 6, 1, 5_000);
    world.submit_action(Action::CastGovernanceVote {
        voter_agent_id: "a".to_string(),
        proposal_key: "proposal.penalty.restore".to_string(),
        option: "approve".to_string(),
        weight: 1,
    });
    world.step().expect("restored voter should pass");

    let governance = world
        .state()
        .governance_votes
        .get("proposal.penalty.restore")
        .expect("governance vote state");
    assert_eq!(governance.total_weight, 1);
}

#[test]
fn crisis_cycle_spawns_and_times_out_if_unresolved() {
    let mut world = World::new();
    let snapshot_before_spawn_prepare = world.snapshot();
    let journal_before_spawn_prepare = world.journal().clone();
    let prepared_spawn = world.prepared_crisis_auto_spawn_event_for_test(8);
    assert_eq!(world.snapshot(), snapshot_before_spawn_prepare);
    assert_eq!(world.journal(), &journal_before_spawn_prepare);
    assert!(matches!(
        prepared_spawn,
        Some(DomainEvent::CrisisSpawned {
            crisis_id,
            kind,
            severity: 2,
            expires_at: 16,
        }) if crisis_id == "crisis.auto.8" && kind == "solar_storm"
    ));

    register_agents(&mut world, &["a"]);
    let crisis_id = advance_until_auto_crisis(&mut world);

    let expires_at = world
        .state()
        .crises
        .get(&crisis_id)
        .expect("active crisis")
        .expires_at;
    let snapshot_before_timeout_prepare = world.snapshot();
    let journal_before_timeout_prepare = world.journal().clone();
    let prepared_timeouts = world.prepared_crisis_timeout_events_for_test(expires_at);
    assert_eq!(world.snapshot(), snapshot_before_timeout_prepare);
    assert_eq!(world.journal(), &journal_before_timeout_prepare);
    assert!(matches!(
        prepared_timeouts.as_slice(),
        [DomainEvent::CrisisTimedOut {
            crisis_id: prepared_crisis_id,
            penalty_impact,
        }] if prepared_crisis_id.as_str() == crisis_id.as_str() && *penalty_impact < 0
    ));

    while world.state().time <= expires_at {
        world.step().expect("advance to crisis timeout");
    }

    let crisis = world
        .state()
        .crises
        .get(&crisis_id)
        .expect("timed out crisis");
    assert_eq!(crisis.status, CrisisStatus::TimedOut);
    assert_eq!(crisis.success, Some(false));
    assert!(crisis.impact < 0);
    assert!(matches!(
        last_domain_event(&world),
        DomainEvent::CrisisTimedOut { .. }
    ));
}

#[test]
fn war_auto_concludes_after_duration() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d"]);

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        members: vec!["b".to_string()],
        charter: "charter.red".to_string(),
    });
    world.step().expect("form red alliance");

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "c".to_string(),
        alliance_id: "alliance.blue".to_string(),
        members: vec!["d".to_string()],
        charter: "charter.blue".to_string(),
    });
    world.step().expect("form blue alliance");
    seed_war_ready_resources(&mut world, &["a", "b", "c", "d"]);

    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.auto".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "hold position".to_string(),
        intensity: 2,
    });
    world.step().expect("declare war");

    let active_war = world.state().wars.get("war.auto").expect("active war");
    let due_at = active_war
        .declared_at
        .saturating_add(active_war.max_duration_ticks.max(1));
    let snapshot_before_prepare = world.snapshot();
    let journal_before_prepare = world.journal().clone();
    let prepared = world.prepared_next_due_war_event_for_test(due_at);
    assert_eq!(world.snapshot(), snapshot_before_prepare);
    assert_eq!(world.journal(), &journal_before_prepare);
    assert!(matches!(
        prepared,
        Some(DomainEvent::WarConcluded {
            war_id,
            winner_alliance_id,
            loser_alliance_id,
            participant_outcomes,
            ..
        }) if war_id == "war.auto"
            && winner_alliance_id == "alliance.red"
            && loser_alliance_id == "alliance.blue"
            && !participant_outcomes.is_empty()
    ));

    for _ in 0..12 {
        world.step().expect("advance war lifecycle");
    }

    let war = world.state().wars.get("war.auto").expect("war state");
    assert!(!war.active);
    assert_eq!(war.winner_alliance_id.as_deref(), Some("alliance.red"));
    assert_eq!(war.loser_alliance_id.as_deref(), Some("alliance.blue"));
    assert!(!war.participant_outcomes.is_empty());
    assert!(war.concluded_at.is_some());
    assert!(
        war.settlement_summary
            .as_deref()
            .unwrap_or_default()
            .contains("auto settlement")
    );
}

#[test]
fn alliance_join_leave_dissolve_lifecycle_updates_state() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c"]);

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.alpha".to_string(),
        members: vec!["b".to_string()],
        charter: "alpha charter".to_string(),
    });
    world.step().expect("form alliance");

    world.submit_action(Action::JoinAlliance {
        operator_agent_id: "a".to_string(),
        alliance_id: "alliance.alpha".to_string(),
        member_agent_id: "c".to_string(),
    });
    world.step().expect("join alliance");
    let alliance = world
        .state()
        .alliances
        .get("alliance.alpha")
        .expect("alliance after join");
    assert_eq!(
        alliance.members,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );

    world.submit_action(Action::LeaveAlliance {
        operator_agent_id: "a".to_string(),
        alliance_id: "alliance.alpha".to_string(),
        member_agent_id: "c".to_string(),
    });
    world.step().expect("leave alliance");
    let alliance = world
        .state()
        .alliances
        .get("alliance.alpha")
        .expect("alliance after leave");
    assert_eq!(alliance.members, vec!["a".to_string(), "b".to_string()]);

    world.submit_action(Action::DissolveAlliance {
        operator_agent_id: "a".to_string(),
        alliance_id: "alliance.alpha".to_string(),
        reason: "merge into coalition".to_string(),
    });
    world.step().expect("dissolve alliance");
    assert!(!world.state().alliances.contains_key("alliance.alpha"));
    assert!(matches!(
        last_domain_event(&world),
        DomainEvent::AllianceDissolved { .. }
    ));
}

#[test]
fn war_declaration_requires_mobilization_resources_and_conclusion_applies_outcomes() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b", "c", "d", "e"]);

    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        members: vec!["b".to_string()],
        charter: "charter.red".to_string(),
    });
    world.step().expect("form red alliance");
    world.submit_action(Action::FormAlliance {
        proposer_agent_id: "c".to_string(),
        alliance_id: "alliance.blue".to_string(),
        members: vec!["d".to_string()],
        charter: "charter.blue".to_string(),
    });
    world.step().expect("form blue alliance");

    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.reject.cost".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "no budget".to_string(),
        intensity: 3,
    });
    world
        .step()
        .expect("reject war for insufficient mobilization");
    match last_domain_event(&world) {
        DomainEvent::ActionRejected {
            reason:
                RejectReason::InsufficientResource {
                    kind: ResourceKind::Electricity,
                    ..
                },
            ..
        } => {}
        other => panic!("unexpected event: {other:?}"),
    }

    set_agent_resources(&mut world, "a", 120, 120);
    set_agent_resources(&mut world, "b", 80, 60);
    set_agent_resources(&mut world, "c", 90, 70);
    set_agent_resources(&mut world, "d", 60, 50);
    let before_a_electricity = world
        .agent_resource_balance("a", ResourceKind::Electricity)
        .expect("a electricity before war");
    let before_a_data = world
        .agent_resource_balance("a", ResourceKind::Data)
        .expect("a data before war");

    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "a".to_string(),
        war_id: "war.outcome".to_string(),
        aggressor_alliance_id: "alliance.red".to_string(),
        defender_alliance_id: "alliance.blue".to_string(),
        objective: "hold belt".to_string(),
        intensity: 3,
    });
    world.step().expect("declare war with mobilization");
    let war = world.state().wars.get("war.outcome").expect("war state");
    assert_eq!(war.declared_mobilization_electricity_cost, 24);
    assert_eq!(war.declared_mobilization_data_cost, 17);
    let after_a_electricity = world
        .agent_resource_balance("a", ResourceKind::Electricity)
        .expect("a electricity after declare");
    let after_a_data = world
        .agent_resource_balance("a", ResourceKind::Data)
        .expect("a data after declare");
    assert_eq!(
        before_a_electricity - after_a_electricity,
        war.declared_mobilization_electricity_cost
    );
    assert_eq!(
        before_a_data - after_a_data,
        war.declared_mobilization_data_cost
    );

    world.submit_action(Action::JoinAlliance {
        operator_agent_id: "a".to_string(),
        alliance_id: "alliance.red".to_string(),
        member_agent_id: "e".to_string(),
    });
    world.step().expect("join during active war rejected");
    match last_domain_event(&world) {
        DomainEvent::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
            ..
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("cannot change members"))
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    for _ in 0..12 {
        world.step().expect("advance war lifecycle");
    }
    let war = world
        .state()
        .wars
        .get("war.outcome")
        .expect("war after conclude");
    assert!(!war.active);
    assert!(
        war.participant_outcomes
            .iter()
            .any(|item| item.electricity_delta < 0)
    );
    assert!(
        war.participant_outcomes
            .iter()
            .any(|item| item.electricity_delta > 0)
    );
    assert!(
        war.participant_outcomes
            .iter()
            .any(|item| item.reputation_delta != 0)
    );
    let total_electricity_delta: i64 = war
        .participant_outcomes
        .iter()
        .map(|item| item.electricity_delta)
        .sum();
    let total_data_delta: i64 = war
        .participant_outcomes
        .iter()
        .map(|item| item.data_delta)
        .sum();
    assert_eq!(total_electricity_delta, 0);
    assert_eq!(total_data_delta, 0);
}

#[test]
fn meta_progress_unlocks_track_tiers() {
    let mut world = World::new();
    register_agents(&mut world, &["a", "b"]);

    world.submit_action(Action::GrantMetaProgress {
        operator_agent_id: "a".to_string(),
        target_agent_id: "b".to_string(),
        track: "campaign".to_string(),
        points: 20,
        achievement_id: None,
    });
    world.step().expect("grant bronze points");

    world.submit_action(Action::GrantMetaProgress {
        operator_agent_id: "a".to_string(),
        target_agent_id: "b".to_string(),
        track: "campaign".to_string(),
        points: 30,
        achievement_id: None,
    });
    world.step().expect("grant silver points");

    world.submit_action(Action::GrantMetaProgress {
        operator_agent_id: "a".to_string(),
        target_agent_id: "b".to_string(),
        track: "campaign".to_string(),
        points: 50,
        achievement_id: None,
    });
    world.step().expect("grant gold points");

    let progress = world.state().meta_progress.get("b").expect("meta progress");
    assert_eq!(progress.track_points.get("campaign"), Some(&100));
    let tiers = progress
        .unlocked_tiers
        .get("campaign")
        .expect("campaign tiers");
    assert!(tiers.iter().any(|tier| tier == "bronze"));
    assert!(tiers.iter().any(|tier| tier == "silver"));
    assert!(tiers.iter().any(|tier| tier == "gold"));
    assert!(
        progress
            .achievements
            .iter()
            .any(|value| value == "tier.campaign.bronze")
    );
    assert!(
        progress
            .achievements
            .iter()
            .any(|value| value == "tier.campaign.silver")
    );
    assert!(
        progress
            .achievements
            .iter()
            .any(|value| value == "tier.campaign.gold")
    );
}
