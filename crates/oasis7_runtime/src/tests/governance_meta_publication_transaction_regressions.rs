use super::pos;
use crate::*;

fn base() -> World {
    let mut world = World::new();
    for id in ["a", "b"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: id.into(),
            pos: pos(0, 0),
        });
        world.step().unwrap();
    }
    world
}

fn fixtures() -> Vec<(&'static str, World, DomainEvent)> {
    let mut world = base();
    let opened = DomainEvent::GovernanceProposalOpened {
        proposer_agent_id: "a".into(),
        proposal_key: "p".into(),
        title: "t".into(),
        description: "d".into(),
        options: vec!["yes".into(), "no".into()],
        voting_window_ticks: 10,
        closes_at: 20,
        quorum_weight: 1,
        pass_threshold_bps: 5_000,
    };
    let opened_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(opened.clone()), None)
        .unwrap();
    let vote = DomainEvent::GovernanceVoteCast {
        voter_agent_id: "b".into(),
        proposal_key: "p".into(),
        option: "yes".into(),
        weight: 1,
    };
    let vote_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(vote.clone()), None)
        .unwrap();
    let finalized = DomainEvent::GovernanceProposalFinalized {
        proposal_key: "p".into(),
        winning_option: Some("yes".into()),
        winning_weight: 1,
        total_weight: 1,
        passed: true,
    };
    let finalized_base = world.clone();
    let spawned = DomainEvent::CrisisSpawned {
        crisis_id: "c".into(),
        kind: "storm".into(),
        severity: 2,
        expires_at: 20,
    };
    let spawned_base = world.clone();
    world
        .append_event_for_test(WorldEventBody::Domain(spawned.clone()), None)
        .unwrap();
    let resolved = DomainEvent::CrisisResolved {
        resolver_agent_id: "a".into(),
        crisis_id: "c".into(),
        strategy: "shield".into(),
        success: true,
        impact: 3,
    };
    let resolved_base = world.clone();
    let timeout = DomainEvent::CrisisTimedOut {
        crisis_id: "c".into(),
        penalty_impact: -2,
    };
    let timeout_base = world.clone();
    let meta = DomainEvent::MetaProgressGranted {
        operator_agent_id: "a".into(),
        target_agent_id: "b".into(),
        track: "builder".into(),
        points: 10,
        achievement_id: Some("first".into()),
    };
    let product = DomainEvent::ProductValidated {
        requester_agent_id: "a".into(),
        module_id: "m".into(),
        stack: MaterialStack::new("gear", 1),
        stack_limit: 10,
        tradable: true,
        quality_levels: vec!["q1".into()],
        notes: vec!["ok".into()],
    };
    vec![
        ("opened", opened_base, opened),
        ("vote", vote_base, vote),
        ("finalized", finalized_base, finalized),
        ("spawned", spawned_base, spawned),
        ("resolved", resolved_base, resolved),
        ("timeout", timeout_base, timeout),
        ("meta", world.clone(), meta),
        ("product", world, product),
    ]
}

fn unchanged(world: &World, before: &World, root: &str) {
    assert_eq!(world.snapshot(), before.snapshot());
    assert_eq!(world.journal(), before.journal());
    assert_eq!(
        world.runtime_backpressure_stats(),
        before.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        before.tick_consensus_records()
    );
    assert_eq!(world.current_state_root_hash().unwrap(), root);
}

#[test]
fn all_eight_raw_events_honor_postprepare_and_retry_replay() {
    let mut bypassed = Vec::new();
    for (i, (name, mut world, event)) in fixtures().into_iter().enumerate() {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        world.fail_next_append_after_publication_prepare_for_test();
        match world.append_event_for_test(
            WorldEventBody::Domain(event.clone()),
            Some(CausedBy::Action(5_500 + i as u64)),
        ) {
            Ok(_) => bypassed.push(name),
            Err(error) => {
                assert!(format!("{error:?}").contains("publication preparation"));
                unchanged(&world, &before, &root);
                let cause = Some(CausedBy::Action(5_510 + i as u64));
                world
                    .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
                    .unwrap();
                assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
                let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
                assert_eq!(replay.snapshot(), world.snapshot());
                assert_eq!(
                    replay.current_state_root_hash().unwrap(),
                    world.current_state_root_hash().unwrap()
                );
            }
        }
    }
    assert!(
        bypassed.is_empty(),
        "events bypassed postprepare failpoint: {bypassed:?}"
    );
}

#[test]
fn crisis_resolve_missing_actor_does_not_leave_legacy_record() {
    let mut world = base();
    let event = DomainEvent::CrisisResolved {
        resolver_agent_id: "missing".into(),
        crisis_id: "legacy".into(),
        strategy: "x".into(),
        success: false,
        impact: -1,
    };
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap_err();
    assert!(format!("{error:?}").contains("AgentNotFound"));
    unchanged(&world, &before, &root);
}

#[test]
fn compatibility_routes_exact_actor_or_none_and_replays() {
    for (_, base, event) in fixtures() {
        let actor = event.agent_id().map(str::to_string);
        let counts = base
            .state()
            .agents
            .iter()
            .map(|(id, c)| (id.clone(), c.mailbox.len()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let snapshot = base.snapshot();
        let mut world = base;
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        for (id, count) in counts {
            assert_eq!(
                world.state().agents[&id].mailbox.len(),
                count + usize::from(actor.as_deref() == Some(id.as_str()))
            );
        }
        let replay = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
        assert_eq!(replay.snapshot(), world.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}

#[test]
fn compatibility_preserves_overwrite_recast_self_and_missing_requester_semantics() {
    let mut world = base();
    let opened = fixtures().remove(0).2;
    world
        .append_event_for_test(WorldEventBody::Domain(opened.clone()), None)
        .unwrap();
    let first_vote = DomainEvent::GovernanceVoteCast {
        voter_agent_id: "b".into(),
        proposal_key: "p".into(),
        option: "yes".into(),
        weight: 1,
    };
    world
        .append_event_for_test(WorldEventBody::Domain(first_vote), None)
        .unwrap();
    let recast = DomainEvent::GovernanceVoteCast {
        voter_agent_id: "b".into(),
        proposal_key: "p".into(),
        option: "no".into(),
        weight: 1,
    };
    world
        .append_event_for_test(WorldEventBody::Domain(recast), None)
        .unwrap();
    let votes = world.state().governance_votes["p"].clone();
    assert_eq!(votes.total_weight, 1);
    assert_eq!(votes.tallies.get("yes"), None);
    assert_eq!(votes.tallies.get("no"), Some(&1));

    let duplicate = DomainEvent::GovernanceProposalOpened {
        proposer_agent_id: "a".into(),
        proposal_key: "p".into(),
        title: "replacement".into(),
        description: "replacement-description".into(),
        options: vec!["new".into()],
        voting_window_ticks: 4,
        closes_at: 50,
        quorum_weight: 2,
        pass_threshold_bps: 6_000,
    };
    world
        .append_event_for_test(WorldEventBody::Domain(duplicate), None)
        .unwrap();
    assert_eq!(world.state().governance_proposals["p"].title, "replacement");
    assert_eq!(world.state().governance_votes["p"], votes);

    let a_mailbox = world.state().agents["a"].mailbox.len();
    let self_meta = DomainEvent::MetaProgressGranted {
        operator_agent_id: "a".into(),
        target_agent_id: "a".into(),
        track: "builder".into(),
        points: 20,
        achievement_id: Some("self".into()),
    };
    world
        .append_event_for_test(WorldEventBody::Domain(self_meta), None)
        .unwrap();
    assert_eq!(world.state().agents["a"].mailbox.len(), a_mailbox + 1);
    let progress = &world.state().meta_progress["a"];
    assert_eq!(progress.total_points, 20);
    assert!(progress.achievements.iter().any(|id| id == "self"));
    assert!(
        progress
            .achievements
            .iter()
            .any(|id| id == "tier.builder.bronze")
    );

    let counts = world
        .state()
        .agents
        .iter()
        .map(|(id, cell)| (id.clone(), cell.mailbox.len()))
        .collect::<std::collections::BTreeMap<_, _>>();
    world
        .append_event_for_test(
            WorldEventBody::Domain(DomainEvent::ProductValidated {
                requester_agent_id: "missing".into(),
                module_id: "m".into(),
                stack: MaterialStack::new("missing-requester-product", 1),
                stack_limit: 1,
                tradable: false,
                quality_levels: vec![],
                notes: vec![],
            }),
            None,
        )
        .unwrap();
    assert_eq!(
        world
            .state()
            .latest_product_validation
            .as_ref()
            .unwrap()
            .product_id,
        "missing-requester-product"
    );
    for (id, count) in counts {
        assert_eq!(world.state().agents[&id].mailbox.len(), count);
    }
}
