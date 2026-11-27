use super::{
    InstitutionsGovernanceDelta, StagedGovernanceApproval, StagedInstitutionsGovernanceTransition,
};
use crate::world::transition::{
    BorrowedWorldView, DurableCommitConfirmation, PreparedStagedCommit, StagedTransition,
    TransitionContext, TransitionReject,
};
use crate::{
    Action, DomainEvent, GovernanceEvent, GovernanceIdentityStatus, Manifest, ProposalDecision,
    ProposalStatus, World, WorldEventBody,
};
use oasis7_agent_api::world_action::{OrganizationContinuityCommandV1, WorldRuleCommandV1};
use oasis7_world_state::commitment_envelope::WorldCommitmentEnvelope;
use std::collections::{BTreeMap, BTreeSet};

fn local_guardians() -> Vec<String> {
    vec![
        "governance.local.finality.signer.1".to_string(),
        "governance.local.finality.signer.2".to_string(),
    ]
}

fn prepare_staged(
    world: &World,
    body: &WorldEventBody,
) -> Result<PreparedStagedCommit<InstitutionsGovernanceDelta>, TransitionReject> {
    let view = BorrowedWorldView::new(world)?;
    StagedInstitutionsGovernanceTransition::from_body(body)
        .expect("institutions governance transition must classify body")
        .prepare(&view, &TransitionContext::new(view.time(), None))
}

fn install_staged(world: &mut World, body: &WorldEventBody) -> Result<(), TransitionReject> {
    prepare_staged(world, body)?.install_confirmed(world, DurableCommitConfirmation::Confirmed)
}

fn assert_v2_envelope_linked(world: &World) -> WorldCommitmentEnvelope {
    let envelope = world.commitment_envelope_v2().expect("v2 envelope");
    world.verify_commitment_envelope(&envelope).expect("verify v2");
    assert_eq!(
        envelope,
        WorldCommitmentEnvelope::v2_from_snapshot(&world.snapshot()).expect("snapshot v2")
    );
    assert_eq!(
        envelope.governance_sidecar_root,
        Some(world.governance_sidecar_commitment_digest32().expect("sidecar digest"))
    );
    envelope
}

#[test]
fn staged_governance_proposal_lifecycle_links_v2_sidecar_digest_and_fences_failures() {
    let mut world = World::new();
    let env_initial = assert_v2_envelope_linked(&world);
    let proposal_id = 1;
    let next_manifest = Manifest {
        version: world.manifest().version.saturating_add(1),
        content: serde_json::json!({"staged_proposal": true}),
    };
    let next_hash = crate::util::hash_json(&next_manifest).expect("manifest hash");
    let proposed_body = WorldEventBody::Governance(GovernanceEvent::Proposed {
        proposal_id,
        author: "alice".into(),
        base_manifest_hash: world.current_manifest_hash().expect("base hash"),
        manifest: next_manifest,
        patch: None,
    });

    // StatusUnknown and BaseHeadStale fence without mutating sidecar root.
    let pending_unknown = prepare_staged(&world, &proposed_body).expect("prepare");
    assert!(matches!(
        pending_unknown.install_confirmed(
            &mut world,
            DurableCommitConfirmation::StatusUnknown { reason: "lost ack".into() }
        ),
        Err(TransitionReject::StatusUnknownFenced { .. })
    ));
    assert_eq!(assert_v2_envelope_linked(&world), env_initial);

    let stale_commit = prepare_staged(&world, &proposed_body).expect("prepare stale");
    install_staged(&mut world, &proposed_body).expect("install proposed");
    let env_proposed = assert_v2_envelope_linked(&world);
    assert_ne!(env_proposed.governance_sidecar_root, env_initial.governance_sidecar_root);
    assert_ne!(env_proposed.envelope_digest().unwrap(), env_initial.envelope_digest().unwrap());
    assert!(matches!(
        stale_commit.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed),
        Err(TransitionReject::BaseHeadStale)
    ));

    let shadow_body = WorldEventBody::Governance(GovernanceEvent::ShadowReport {
        proposal_id,
        manifest_hash: next_hash.clone(),
    });
    install_staged(&mut world, &shadow_body).expect("install shadow");
    let env_shadowed = assert_v2_envelope_linked(&world);
    assert_ne!(env_shadowed.governance_sidecar_root, env_proposed.governance_sidecar_root);

    let approved_ev = GovernanceEvent::Approved {
        proposal_id,
        approver: "council".into(),
        decision: ProposalDecision::Approve,
    };
    let queued_ev = GovernanceEvent::Queued {
        proposal_id,
        manifest_hash: next_hash,
        queued_at_tick: world.state.time,
        not_before_tick: world.state.time,
        activate_epoch: 1,
        timelock_ticks: 0,
    };
    let approval_commit = {
        let view = BorrowedWorldView::new(&world).expect("view");
        StagedGovernanceApproval { approved: &approved_ev, queued: Some(&queued_ev) }
            .prepare(&view, &TransitionContext::new(view.time(), None))
            .expect("prepare approval")
    };
    approval_commit.install_confirmed(&mut world, DurableCommitConfirmation::Confirmed).unwrap();
    let env_approved = assert_v2_envelope_linked(&world);
    assert_ne!(env_approved.governance_sidecar_root, env_shadowed.governance_sidecar_root);

    let veto_body = WorldEventBody::Governance(GovernanceEvent::EmergencyVetoed {
        proposal_id,
        initiator: "guardian-1".into(),
        reason: "veto".into(),
        threshold: 2,
        signer_node_ids: local_guardians(),
    });
    install_staged(&mut world, &veto_body).expect("install veto");
    let env_vetoed = assert_v2_envelope_linked(&world);
    assert_ne!(env_vetoed.governance_sidecar_root, env_approved.governance_sidecar_root);
    assert!(matches!(
        world.proposals.get(&proposal_id).map(|p| &p.status),
        Some(ProposalStatus::Rejected { .. })
    ));

    let mut snapshot = world.governance_effective_finality_epoch_snapshot(9);
    world.normalize_governance_finality_epoch_snapshot(&mut snapshot).expect("normalize snapshot");
    let snapshot_set = WorldEventBody::Governance(GovernanceEvent::FinalityEpochSnapshotSet {
        snapshot: snapshot.clone(),
        previous: None,
    });
    install_staged(&mut world, &snapshot_set).expect("install epoch snapshot");
    let env_epoch_set = assert_v2_envelope_linked(&world);
    assert_ne!(env_epoch_set.governance_sidecar_root, env_vetoed.governance_sidecar_root);

    let snapshot_removed = GovernanceEvent::FinalityEpochSnapshotRemoved { epoch_id: 9, snapshot };
    let transition = StagedInstitutionsGovernanceTransition::ProposalState(&snapshot_removed);
    world.append_staged_institutions_governance_event(transition, None).expect("remove epoch snapshot");
    assert_ne!(assert_v2_envelope_linked(&world).governance_sidecar_root, env_epoch_set.governance_sidecar_root);
}

#[test]
fn staged_identity_penalty_and_emergency_brake_sidecars_update_v2_sidecar_root() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "penalized-agent".into(),
        pos: crate::tests::pos(0, 0),
    });
    world.step().expect("register agent");
    world
        .set_governance_identity_profile("penalized-agent", 100, 0, GovernanceIdentityStatus::Active)
        .expect("set profile");
    let mut prev_root = assert_v2_envelope_linked(&world).governance_sidecar_root;

    let events = [
        GovernanceEvent::EmergencyBrakeActivated {
            initiator: "guardian-1".into(),
            reason: "incident".into(),
            active_until_tick: world.state.time + 5,
            threshold: 2,
            signer_node_ids: local_guardians(),
        },
        GovernanceEvent::EmergencyBrakeReleased {
            initiator: "guardian-2".into(),
            reason: "resolved".into(),
            threshold: 2,
            signer_node_ids: local_guardians(),
        },
        GovernanceEvent::IdentityPenaltyApplied {
            penalty_id: 1,
            target_agent_id: "penalized-agent".into(),
            evidence_hash: "sha256:evidence-1".into(),
            initiator: "guardian-1".into(),
            reason: "misconduct".into(),
            slash_stake: 25,
            appeal_deadline_tick: world.state.time + 10,
            threshold: 2,
            signer_node_ids: local_guardians(),
        },
        GovernanceEvent::IdentityPenaltyAppealed {
            penalty_id: 1,
            appellant: "penalized-agent".into(),
            reason: "counter-proof".into(),
        },
        GovernanceEvent::IdentityPenaltyResolved {
            penalty_id: 1,
            resolver: "guardian-2".into(),
            accepted: true,
            reason: "restored".into(),
        },
    ];

    for event in events {
        install_staged(&mut world, &WorldEventBody::Governance(event)).expect("install sidecar");
        let next_root = assert_v2_envelope_linked(&world).governance_sidecar_root;
        assert_ne!(next_root, prev_root);
        prev_root = next_root;
    }
    assert_eq!(world.governance_emergency_brake_until_tick(), None);
}

#[test]
fn staged_organization_continuity_write_links_v2_commitment_envelope() {
    let mut world = World::new();
    world.chain_resource_manifest.world_id = "world-1".into();
    world.chain_resource_manifest.chain_id = "chain-1".into();
    world.chain_resource_manifest.genesis_ref = Some("genesis-1".into());
    for (agent_id, player_id) in [
        ("owner", "player-owner"),
        ("primary", "player-primary"),
        ("recipient", "player-recipient"),
    ] {
        world.submit_action(Action::RegisterAgent { agent_id: agent_id.into(), pos: crate::tests::pos(1, 1) });
        world.state.starter_oc_claims.insert(
            agent_id.into(),
            crate::gameplay_state::StarterOcClaimState {
                agent_id: agent_id.into(),
                player_id: player_id.into(),
                public_key: Some(format!("key-{agent_id}")),
                claimed_at: 1,
                ..Default::default()
            },
        );
    }
    world.step().expect("register organization agents");
    let charter = crate::world_rules::OrganizationContinuityCharterV1 {
        schema: "organization-continuity-v1".into(),
        designated_successors: vec!["primary".into()],
        neutral_maintainers: vec!["recipient".into()],
        appointment_authorities: BTreeSet::from(["owner".into()]),
        remedy_adjudicators: BTreeSet::from(["recipient".into()]),
        activity_source_policy: crate::world_rules::ORGANIZATION_ACTIVITY_SOURCE_POLICY_V1.into(),
        residual_share_bps: BTreeMap::from([("owner".into(), 5_000), ("primary".into(), 5_000)]),
    };
    let charter_json = serde_json::to_string(&charter).expect("serialize charter");
    let expected_charter_hash = crate::util::sha256_hex(charter_json.as_bytes());
    let alliance = DomainEvent::AllianceFormed {
        proposer_agent_id: "owner".into(),
        alliance_id: "org-1".into(),
        members: vec!["owner".into(), "primary".into()],
        charter: charter_json,
    };
    world.append_event_for_test(WorldEventBody::Domain(alliance), None).expect("form alliance");

    let command = WorldRuleCommandV1::OrganizationContinuity {
        command: OrganizationContinuityCommandV1::Enable {
            organization_id: "org-1".into(),
            expected_charter_hash,
            governance_proposal_key: "enable-org-1".into(),
            designated_successors: charter.designated_successors,
            neutral_maintainers: charter.neutral_maintainers,
            appointment_authorities: charter.appointment_authorities,
            remedy_adjudicators: charter.remedy_adjudicators,
            residual_share_bps: charter.residual_share_bps,
        },
    };
    let description = oasis7_agent_api::world_action::world_rule_proposal_description(&command)
        .expect("proposal description");
    world.state.governance_proposals.insert(
        "enable-org-1".into(),
        crate::GovernanceProposalState {
            proposal_key: "enable-org-1".into(),
            proposer_agent_id: "owner".into(),
            title: "enable org".into(),
            description,
            options: vec!["yes".into(), "no".into()],
            voting_window_ticks: 10,
            quorum_weight: 1,
            pass_threshold_bps: 5_000,
            opened_at: 0,
            closes_at: 10,
            status: crate::GovernanceProposalStatus::Passed,
            finalized_at: Some(1),
            winning_option: Some("yes".into()),
            winning_weight: 1,
            total_weight_at_finalize: 1,
            snapshot_at_tick: 0,
            vote_weight_snapshot: BTreeMap::new(),
        },
    );

    let env_before = assert_v2_envelope_linked(&world);
    let org_enable_body = WorldEventBody::Domain(DomainEvent::WorldRuleCommitted {
        action_id: 0,
        actor_id: "owner".into(),
        request_id: "org-enable-staged-1".into(),
        command,
        governance_epoch: world.current_governance_epoch(),
        world_binding: world.world_rule_binding(),
    });
    install_staged(&mut world, &org_enable_body).expect("install organization enable");

    let env_after = assert_v2_envelope_linked(&world);
    assert_ne!(env_after.business_state_root, env_before.business_state_root);
    assert_eq!(env_after.governance_sidecar_root, env_before.governance_sidecar_root);
    assert!(world.state.world_rules.as_ref().is_some_and(|r| r.organization_continuity.registrations.contains_key("org-1")));
}
