use super::*;

fn agent_owner(agent_id: &str) -> ResourceOwner {
    ResourceOwner::Agent {
        agent_id: agent_id.to_string(),
    }
}

fn setup_social_kernel() -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-social".to_string(),
        name: "social-hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    for agent_id in ["agent-a", "agent-b", "agent-c"] {
        kernel.submit_action(Action::RegisterAgent {
            agent_id: agent_id.to_string(),
            location_id: "loc-social".to_string(),
        });
    }
    kernel.step_until_empty();

    for agent_id in ["agent-a", "agent-b", "agent-c"] {
        seed_owner_resource(
            &mut kernel,
            agent_owner(agent_id),
            ResourceKind::Electricity,
            1_000,
        );
    }
    kernel
}

fn first_evidence_event_id(kernel: &WorldKernel) -> WorldEventId {
    kernel
        .journal()
        .first()
        .map(|event| event.id)
        .expect("seed event id")
}

#[test]
fn social_replay_from_snapshot_keeps_adjudicated_and_declared_state() {
    let mut kernel = setup_social_kernel();
    let snapshot = kernel.kernel_snapshot();
    let evidence_event_id = first_evidence_event_id(&kernel);

    kernel.submit_action(Action::PublishSocialFact {
        actor: agent_owner("agent-a"),
        schema_id: "social.reputation.v1".to_string(),
        subject: agent_owner("agent-b"),
        object: None,
        claim: "agent-b reliably delivers requested outcomes".to_string(),
        confidence_ppm: 850_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: None,
        stake: Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 25,
        }),
    });
    let publish = kernel.step().expect("publish");
    let fact_id = match publish.kind {
        WorldEventKind::SocialFactPublished { fact } => fact.fact_id,
        other => panic!("unexpected publish event: {other:?}"),
    };

    kernel.submit_action(Action::ChallengeSocialFact {
        challenger: agent_owner("agent-c"),
        fact_id,
        reason: "need stronger corroboration".to_string(),
        stake: Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 10,
        }),
    });
    let _ = kernel.step().expect("challenge");

    kernel.submit_action(Action::AdjudicateSocialFact {
        adjudicator: agent_owner("agent-a"),
        fact_id,
        decision: SocialAdjudicationDecision::Confirm,
        notes: "cross-check passed".to_string(),
    });
    let _ = kernel.step().expect("adjudicate");

    kernel.submit_action(Action::DeclareSocialEdge {
        declarer: agent_owner("agent-a"),
        schema_id: "social.reputation.v1".to_string(),
        relation_kind: "trust".to_string(),
        from: agent_owner("agent-a"),
        to: agent_owner("agent-b"),
        weight_bps: 3_000,
        backing_fact_ids: vec![fact_id],
        ttl_ticks: None,
    });
    let _ = kernel.step().expect("declare");

    let journal = kernel.kernel_journal_snapshot();
    let replayed = WorldKernel::replay_from_kernel_snapshot(snapshot, journal).expect("replay");
    assert_eq!(replayed.model(), kernel.model());
}

#[test]
fn social_replay_rejects_non_publisher_adjudicator() {
    let mut kernel = setup_social_kernel();
    let snapshot = kernel.kernel_snapshot();
    let evidence_event_id = first_evidence_event_id(&kernel);

    kernel.submit_action(Action::PublishSocialFact {
        actor: agent_owner("agent-a"),
        schema_id: "social.reputation.v1".to_string(),
        subject: agent_owner("agent-b"),
        object: Some(agent_owner("agent-c")),
        claim: "agent-b reliably delivers requested outcomes".to_string(),
        confidence_ppm: 850_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: None,
        stake: None,
    });
    let publish = kernel.step().expect("publish");
    let fact_id = match publish.kind {
        WorldEventKind::SocialFactPublished { fact } => fact.fact_id,
        other => panic!("unexpected publish event: {other:?}"),
    };
    kernel.submit_action(Action::ChallengeSocialFact {
        challenger: agent_owner("agent-c"),
        fact_id,
        reason: "need stronger corroboration".to_string(),
        stake: None,
    });
    let _ = kernel.step().expect("challenge");
    kernel.submit_action(Action::AdjudicateSocialFact {
        adjudicator: agent_owner("agent-a"),
        fact_id,
        decision: SocialAdjudicationDecision::Confirm,
        notes: "cross-check passed".to_string(),
    });
    let _ = kernel.step().expect("adjudicate");

    let mut tampered_journal = kernel.kernel_journal_snapshot();
    for event in &mut tampered_journal.events {
        if let WorldEventKind::SocialFactAdjudicated { adjudicator, .. } = &mut event.kind {
            *adjudicator = agent_owner("agent-b");
        }
    }
    let error = WorldKernel::replay_from_kernel_snapshot(snapshot, tampered_journal)
        .expect_err("replay must reject non-publisher adjudicator");
    assert!(format!("{error:?}").contains("social adjudicator is not the fact publisher"));
}

#[test]
fn social_replay_rejects_adjudication_without_challenge() {
    let mut kernel = setup_social_kernel();
    let snapshot = kernel.kernel_snapshot();
    let evidence_event_id = first_evidence_event_id(&kernel);

    kernel.submit_action(Action::PublishSocialFact {
        actor: agent_owner("agent-a"),
        schema_id: "social.reputation.v1".to_string(),
        subject: agent_owner("agent-b"),
        object: None,
        claim: "agent-b reliably delivers requested outcomes".to_string(),
        confidence_ppm: 850_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: None,
        stake: None,
    });
    let publish = kernel.step().expect("publish");
    let fact_id = match publish.kind {
        WorldEventKind::SocialFactPublished { fact } => fact.fact_id,
        other => panic!("unexpected publish event: {other:?}"),
    };

    let mut tampered_journal = kernel.kernel_journal_snapshot();
    tampered_journal.events.push(WorldEvent {
        id: tampered_journal.events.last().expect("publish event").id + 1,
        time: kernel.time(),
        kind: WorldEventKind::SocialFactAdjudicated {
            fact_id,
            adjudicator: agent_owner("agent-a"),
            decision: SocialAdjudicationDecision::Confirm,
            notes: "malformed unchallenged adjudication".to_string(),
            adjudicated_at_tick: kernel.time(),
        },
    });

    let error = WorldKernel::replay_from_kernel_snapshot(snapshot, tampered_journal)
        .expect_err("replay must reject adjudication without challenge");
    assert!(format!("{error:?}").contains("cannot be adjudicated without challenge"));
}

#[test]
fn social_replay_from_snapshot_keeps_expired_fact_and_edge() {
    let mut kernel = setup_social_kernel();
    let snapshot = kernel.kernel_snapshot();
    let evidence_event_id = first_evidence_event_id(&kernel);

    kernel.submit_action(Action::PublishSocialFact {
        actor: agent_owner("agent-a"),
        schema_id: "social.relation.v1".to_string(),
        subject: agent_owner("agent-a"),
        object: Some(agent_owner("agent-b")),
        claim: "agent-a and agent-b maintained stable collaboration".to_string(),
        confidence_ppm: 780_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: Some(2),
        stake: None,
    });
    let publish = kernel.step().expect("publish");
    let fact_id = match publish.kind {
        WorldEventKind::SocialFactPublished { fact } => fact.fact_id,
        other => panic!("unexpected publish event: {other:?}"),
    };

    kernel.submit_action(Action::DeclareSocialEdge {
        declarer: agent_owner("agent-a"),
        schema_id: "social.relation.v1".to_string(),
        relation_kind: "cooperate".to_string(),
        from: agent_owner("agent-a"),
        to: agent_owner("agent-b"),
        weight_bps: 1_500,
        backing_fact_ids: vec![fact_id],
        ttl_ticks: None,
    });
    let _ = kernel.step().expect("declare");

    kernel.submit_action(Action::DebugGrantResource {
        owner: agent_owner("agent-a"),
        kind: ResourceKind::Data,
        amount: 1,
    });
    let _ = kernel.step().expect("advance");

    assert!(kernel.journal().iter().any(|event| {
        matches!(
            &event.kind,
            WorldEventKind::SocialFactExpired { fact_id: value, .. } if *value == fact_id
        )
    }));
    assert!(
        kernel
            .journal()
            .iter()
            .any(|event| { matches!(&event.kind, WorldEventKind::SocialEdgeExpired { .. }) })
    );

    let journal = kernel.kernel_journal_snapshot();
    let replayed = WorldKernel::replay_from_kernel_snapshot(snapshot, journal).expect("replay");
    assert_eq!(replayed.model(), kernel.model());
}

#[test]
fn social_replay_from_snapshot_keeps_revoked_fact_state() {
    let mut kernel = setup_social_kernel();
    let snapshot = kernel.kernel_snapshot();
    let evidence_event_id = first_evidence_event_id(&kernel);

    kernel.submit_action(Action::PublishSocialFact {
        actor: agent_owner("agent-a"),
        schema_id: "social.reputation.v1".to_string(),
        subject: agent_owner("agent-b"),
        object: None,
        claim: "agent-b resolved incidents quickly".to_string(),
        confidence_ppm: 720_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: None,
        stake: Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 17,
        }),
    });
    let publish = kernel.step().expect("publish");
    let fact_id = match publish.kind {
        WorldEventKind::SocialFactPublished { fact } => fact.fact_id,
        other => panic!("unexpected publish event: {other:?}"),
    };

    kernel.submit_action(Action::RevokeSocialFact {
        actor: agent_owner("agent-a"),
        fact_id,
        reason: "publisher replaced by corrected statement".to_string(),
    });
    let _ = kernel.step().expect("revoke");

    let journal = kernel.kernel_journal_snapshot();
    let replayed = WorldKernel::replay_from_kernel_snapshot(snapshot, journal).expect("replay");
    assert_eq!(replayed.model(), kernel.model());
}
