use super::social::{agent_owner, electricity_of, first_evidence_event_id, setup_social_kernel};
use super::*;

fn publish_revoke_fixture_fact(kernel: &mut WorldKernel, stake: Option<SocialStake>) -> u64 {
    let evidence_event_id = first_evidence_event_id(kernel);
    kernel.submit_action(Action::PublishSocialFact {
        actor: agent_owner("agent-a"),
        schema_id: "social.reputation.v1".to_string(),
        subject: agent_owner("agent-b"),
        object: Some(agent_owner("agent-c")),
        claim: "agent-b fulfilled delivery contract for agent-c".to_string(),
        confidence_ppm: 875_000,
        evidence_event_ids: vec![evidence_event_id],
        ttl_ticks: Some(12),
        stake,
    });
    let published = kernel.step().expect("publish revoke fixture fact");
    match published.kind {
        WorldEventKind::SocialFactPublished { fact } => fact.fact_id,
        other => panic!("unexpected publish event: {other:?}"),
    }
}

fn declare_revoke_fixture_edge(
    kernel: &mut WorldKernel,
    fact_id: u64,
    ttl_ticks: Option<u64>,
) -> u64 {
    kernel.submit_action(Action::DeclareSocialEdge {
        declarer: agent_owner("agent-a"),
        schema_id: "social.relation.v1".to_string(),
        relation_kind: "trust".to_string(),
        from: agent_owner("agent-b"),
        to: agent_owner("agent-c"),
        weight_bps: 2_000,
        backing_fact_ids: vec![fact_id],
        ttl_ticks,
    });
    let declared = kernel.step().expect("declare revoke fixture edge");
    match declared.kind {
        WorldEventKind::SocialEdgeDeclared { edge } => edge.edge_id,
        other => panic!("unexpected edge event: {other:?}"),
    }
}

fn set_agent_resource(kernel: &mut WorldKernel, agent_id: &str, kind: ResourceKind, amount: i64) {
    let mut snapshot = kernel.kernel_snapshot();
    snapshot
        .model
        .agents
        .get_mut(agent_id)
        .expect("agent exists in snapshot")
        .resources
        .set(kind, amount)
        .expect("set agent resource");
    let journal = kernel.kernel_journal_snapshot();
    *kernel =
        WorldKernel::from_kernel_snapshot(snapshot, journal).expect("rebuild seeded snapshot");
}

fn assert_revoke_quote_matches_apply_rejection(
    kernel: &mut WorldKernel,
    actor: ResourceOwner,
    fact_id: u64,
    reason: &str,
    expected_note: &str,
) {
    let model_before_quote = kernel.model().clone();
    let quoted = kernel
        .quote_revoke_social_fact(&actor, fact_id, reason)
        .expect_err("revoke quote should reject");
    assert_eq!(kernel.model(), &model_before_quote);

    kernel.submit_action(Action::RevokeSocialFact {
        actor,
        fact_id,
        reason: reason.to_string(),
    });
    let applied = kernel.step().expect("revoke rejection event");
    let applied_reason = match applied.kind {
        WorldEventKind::ActionRejected { reason } => reason,
        other => panic!("unexpected revoke event: {other:?}"),
    };
    assert_eq!(quoted, applied_reason);
    match quoted {
        RejectReason::RuleDenied { notes } => assert!(
            notes.iter().any(|note| note.contains(expected_note)),
            "revoke rejection notes: {notes:?}"
        ),
        other => panic!("unexpected revoke rejection: {other:?}"),
    }
    assert_eq!(kernel.model(), &model_before_quote);
}

#[test]
fn social_fact_impact_quote_previews_revoke_with_active_and_inactive_backing_edges() {
    let mut kernel = setup_social_kernel();
    let fact_id = publish_revoke_fixture_fact(&mut kernel, None);
    let active_edge_id = declare_revoke_fixture_edge(&mut kernel, fact_id, None);
    let inactive_edge_id = declare_revoke_fixture_edge(&mut kernel, fact_id, Some(1));

    kernel.submit_action(Action::DebugGrantResource {
        owner: agent_owner("agent-a"),
        kind: ResourceKind::Data,
        amount: 1,
    });
    let _ = kernel.step().expect("advance edge ttl");
    assert!(
        kernel
            .model()
            .social_edges
            .get(&active_edge_id)
            .is_some_and(SocialEdgeState::is_active)
    );
    assert!(
        !kernel
            .model()
            .social_edges
            .get(&inactive_edge_id)
            .is_some_and(SocialEdgeState::is_active)
    );

    let journal_len_before_quote = kernel.journal().len();
    let model_before_quote = kernel.model().clone();
    let quote = kernel
        .quote_revoke_social_fact(
            &agent_owner("agent-a"),
            fact_id,
            "withdraw obsolete delivery evidence",
        )
        .expect("revoke quote");
    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(kernel.model(), &model_before_quote);
    assert_eq!(
        kernel
            .quote_revoke_social_fact(
                &agent_owner("agent-a"),
                fact_id,
                "withdraw obsolete delivery evidence",
            )
            .expect("repeat revoke quote"),
        quote
    );

    assert_eq!(quote.actor_id, "agent-a");
    assert_eq!(quote.action_kind, "revoke_fact");
    assert_eq!(quote.schema_id, "social.reputation.v1");
    assert_eq!(quote.subject_id.as_deref(), Some("agent-b"));
    assert_eq!(quote.object_id.as_deref(), Some("agent-c"));
    assert!(quote.claim_summary.contains("fulfilled delivery contract"));
    assert_eq!(quote.confidence_ppm, Some(875_000));
    assert_eq!(quote.ttl_ticks, Some(12));
    assert_eq!(quote.stake_at_risk, 0);
    for relationship in [
        "schema:social.reputation.v1".to_string(),
        "subject:agent-b".to_string(),
        "object:agent-c".to_string(),
        format!("edge:{active_edge_id}"),
        format!("edge:{inactive_edge_id}"),
    ] {
        assert!(quote.affected_relationships.contains(&relationship));
    }
    for surface in ["social_fact_ledger", "reputation", "relationship"] {
        assert!(
            quote
                .affected_social_surfaces
                .contains(&surface.to_string())
        );
    }
    assert_eq!(
        quote.cooperation_opportunity_delta,
        "withdraws_relationship_support"
    );
    assert_eq!(quote.blacklist_or_dispute_risk, "relationship_withdrawal");
    assert_eq!(
        quote.governance_or_claim_relevance,
        "withdraws_evidence_backed_claim"
    );
    assert_eq!(quote.recommended_social_action, "revoke_fact");
    assert!(
        quote
            .why_this_action_matters
            .contains(&format!("active backing edges [{active_edge_id}]"))
    );
    assert!(
        quote
            .why_this_action_matters
            .contains(&format!("already inactive edges [{inactive_edge_id}]"))
    );
}

#[test]
fn social_fact_impact_quote_previews_revoke_returns_both_stakes_with_zero_risk() {
    let mut kernel = setup_social_kernel();
    let fact_id = publish_revoke_fixture_fact(
        &mut kernel,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 30,
        }),
    );
    kernel.submit_action(Action::ChallengeSocialFact {
        challenger: agent_owner("agent-c"),
        fact_id,
        reason: "delivery evidence no longer supports the relationship".to_string(),
        stake: Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 20,
        }),
    });
    assert!(matches!(
        kernel.step().expect("challenge revoke fixture fact").kind,
        WorldEventKind::SocialFactChallenged { .. }
    ));

    let journal_len_before_quote = kernel.journal().len();
    let model_before_quote = kernel.model().clone();
    let publisher_power_before_quote = electricity_of(&kernel, "agent-a");
    let challenger_power_before_quote = electricity_of(&kernel, "agent-c");
    let quote = kernel
        .quote_revoke_social_fact(
            &agent_owner("agent-a"),
            fact_id,
            "withdraw obsolete delivery evidence",
        )
        .expect("revoke quote");

    assert_eq!(kernel.journal().len(), journal_len_before_quote);
    assert_eq!(kernel.model(), &model_before_quote);
    assert_eq!(
        electricity_of(&kernel, "agent-a"),
        publisher_power_before_quote
    );
    assert_eq!(
        electricity_of(&kernel, "agent-c"),
        challenger_power_before_quote
    );
    assert_eq!(quote.stake_at_risk, 0);
    assert!(
        quote
            .why_this_action_matters
            .contains("Publisher stake return: 30")
    );
    assert!(
        quote
            .why_this_action_matters
            .contains("challenger stake return: 20")
    );
    assert_eq!(
        kernel
            .quote_revoke_social_fact(
                &agent_owner("agent-a"),
                fact_id,
                "withdraw obsolete delivery evidence",
            )
            .expect("repeat revoke quote"),
        quote
    );

    kernel.submit_action(Action::RevokeSocialFact {
        actor: agent_owner("agent-a"),
        fact_id,
        reason: "withdraw obsolete delivery evidence".to_string(),
    });
    assert!(matches!(
        kernel.step().expect("revoke challenged fact").kind,
        WorldEventKind::SocialFactRevoked { .. }
    ));
    assert_eq!(electricity_of(&kernel, "agent-a"), 1_000);
    assert_eq!(electricity_of(&kernel, "agent-c"), 1_000);
    let fact = kernel
        .model()
        .social_facts
        .get(&fact_id)
        .expect("revoked fact remains tracked");
    assert_eq!(fact.lifecycle, SocialFactLifecycleState::Revoked);
    assert!(fact.stake.is_none());
    assert!(
        fact.challenge
            .as_ref()
            .expect("challenge remains tracked")
            .stake
            .is_none()
    );
}

#[test]
fn social_fact_impact_quote_rejects_publisher_stake_return_overflow_atomically() {
    let mut kernel = setup_social_kernel();
    let fact_id = publish_revoke_fixture_fact(
        &mut kernel,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 30,
        }),
    );
    set_agent_resource(&mut kernel, "agent-a", ResourceKind::Electricity, i64::MAX);

    let model_before_quote = kernel.model().clone();
    let quoted = kernel
        .quote_revoke_social_fact(
            &agent_owner("agent-a"),
            fact_id,
            "withdraw obsolete delivery evidence",
        )
        .expect_err("publisher stake return overflow must reject quote");
    assert_eq!(quoted, RejectReason::InvalidAmount { amount: 30 });
    assert_eq!(kernel.model(), &model_before_quote);

    kernel.submit_action(Action::RevokeSocialFact {
        actor: agent_owner("agent-a"),
        fact_id,
        reason: "withdraw obsolete delivery evidence".to_string(),
    });
    let applied_reason = match kernel.step().expect("revoke rejection event").kind {
        WorldEventKind::ActionRejected { reason } => reason,
        other => panic!("unexpected revoke event: {other:?}"),
    };
    assert_eq!(applied_reason, quoted);
    assert_eq!(electricity_of(&kernel, "agent-a"), i64::MAX);
    assert_eq!(
        kernel
            .model()
            .social_facts
            .get(&fact_id)
            .expect("fact remains tracked")
            .stake,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 30,
        })
    );
}

#[test]
fn social_fact_impact_quote_rejects_challenger_stake_return_overflow_atomically() {
    let mut kernel = setup_social_kernel();
    let fact_id = publish_revoke_fixture_fact(
        &mut kernel,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 30,
        }),
    );
    kernel.submit_action(Action::ChallengeSocialFact {
        challenger: agent_owner("agent-c"),
        fact_id,
        reason: "delivery evidence no longer supports the relationship".to_string(),
        stake: Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 20,
        }),
    });
    assert!(matches!(
        kernel.step().expect("challenge revoke fixture fact").kind,
        WorldEventKind::SocialFactChallenged { .. }
    ));
    set_agent_resource(&mut kernel, "agent-c", ResourceKind::Electricity, i64::MAX);

    let model_before_quote = kernel.model().clone();
    let quoted = kernel
        .quote_revoke_social_fact(
            &agent_owner("agent-a"),
            fact_id,
            "withdraw obsolete delivery evidence",
        )
        .expect_err("challenger stake return overflow must reject quote");
    assert_eq!(quoted, RejectReason::InvalidAmount { amount: 20 });
    assert_eq!(kernel.model(), &model_before_quote);

    kernel.submit_action(Action::RevokeSocialFact {
        actor: agent_owner("agent-a"),
        fact_id,
        reason: "withdraw obsolete delivery evidence".to_string(),
    });
    let applied_reason = match kernel.step().expect("revoke rejection event").kind {
        WorldEventKind::ActionRejected { reason } => reason,
        other => panic!("unexpected revoke event: {other:?}"),
    };
    assert_eq!(applied_reason, quoted);
    assert_eq!(electricity_of(&kernel, "agent-a"), 970);
    assert_eq!(electricity_of(&kernel, "agent-c"), i64::MAX);
    let fact = kernel
        .model()
        .social_facts
        .get(&fact_id)
        .expect("fact remains tracked");
    assert_eq!(
        fact.stake,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 30,
        })
    );
    assert_eq!(
        fact.challenge
            .as_ref()
            .expect("challenge remains tracked")
            .stake,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 20,
        })
    );
}

#[test]
fn social_fact_impact_quote_rejects_aggregate_same_owner_stake_return_overflow() {
    let mut kernel = setup_social_kernel();
    let fact_id = publish_revoke_fixture_fact(
        &mut kernel,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 30,
        }),
    );
    kernel.submit_action(Action::ChallengeSocialFact {
        challenger: agent_owner("agent-a"),
        fact_id,
        reason: "publisher challenges its own claim for recovery testing".to_string(),
        stake: Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 20,
        }),
    });
    assert!(matches!(
        kernel.step().expect("same-owner challenge").kind,
        WorldEventKind::SocialFactChallenged { .. }
    ));
    set_agent_resource(
        &mut kernel,
        "agent-a",
        ResourceKind::Electricity,
        i64::MAX - 49,
    );

    let model_before_quote = kernel.model().clone();
    let quoted = kernel
        .quote_revoke_social_fact(
            &agent_owner("agent-a"),
            fact_id,
            "withdraw obsolete delivery evidence",
        )
        .expect_err("aggregate same-owner stake return overflow must reject quote");
    assert_eq!(quoted, RejectReason::InvalidAmount { amount: 20 });
    assert_eq!(kernel.model(), &model_before_quote);

    kernel.submit_action(Action::RevokeSocialFact {
        actor: agent_owner("agent-a"),
        fact_id,
        reason: "withdraw obsolete delivery evidence".to_string(),
    });
    let applied_reason = match kernel.step().expect("revoke rejection event").kind {
        WorldEventKind::ActionRejected { reason } => reason,
        other => panic!("unexpected revoke event: {other:?}"),
    };
    assert_eq!(applied_reason, quoted);
    assert_eq!(electricity_of(&kernel, "agent-a"), i64::MAX - 49);
    let fact = kernel
        .model()
        .social_facts
        .get(&fact_id)
        .expect("fact remains tracked");
    assert!(fact.stake.is_some());
    assert_eq!(
        fact.challenge
            .as_ref()
            .expect("challenge remains tracked")
            .stake,
        Some(SocialStake {
            kind: ResourceKind::Electricity,
            amount: 20,
        })
    );
}

#[test]
fn social_fact_impact_quote_previews_revoke_matches_execution_rejections() {
    let mut kernel = setup_social_kernel();
    let fact_id = publish_revoke_fixture_fact(&mut kernel, None);

    assert_revoke_quote_matches_apply_rejection(
        &mut kernel,
        agent_owner("agent-a"),
        fact_id,
        "   ",
        "social revoke reason cannot be empty",
    );
    assert_revoke_quote_matches_apply_rejection(
        &mut kernel,
        agent_owner("agent-a"),
        fact_id + 1,
        "missing fact",
        "social fact not found",
    );
    assert_revoke_quote_matches_apply_rejection(
        &mut kernel,
        agent_owner("agent-b"),
        fact_id,
        "publisher is not actor",
        "can only be revoked by publisher",
    );

    kernel.submit_action(Action::RevokeSocialFact {
        actor: agent_owner("agent-a"),
        fact_id,
        reason: "withdraw obsolete delivery evidence".to_string(),
    });
    assert!(matches!(
        kernel.step().expect("revoke fact for terminal parity").kind,
        WorldEventKind::SocialFactRevoked { .. }
    ));
    assert_revoke_quote_matches_apply_rejection(
        &mut kernel,
        agent_owner("agent-a"),
        fact_id,
        "revoke again",
        "cannot be revoked in state",
    );
}
