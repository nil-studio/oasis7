use super::*;

fn custody_return_fixture(handoff_expiry: u64) -> WorldState {
    use crate::world_rules::{
        AgentIndustrialOrderV1, AgentIndustrialPhaseV1, AgentManufacturingQuoteV1,
        ORGANIZATION_CONTINUITY_POLICY_VERSION_V1, OrganizationAvailabilityEvidenceStatusV1,
        OrganizationAvailabilityIntervalV1, OrganizationCaseOriginV1, OrganizationContinuityCaseV1,
        OrganizationContinuityPhaseV1, OrganizationDispositionStageV1, OrganizationManifestAssetV1,
        OrganizationRegistrationV1, OrganizationSuccessorCandidateV1,
    };

    let mut state = organization_fixture();
    state.agent_claims.clear();
    state
        .material_ledgers
        .insert(MaterialLedgerId::agent("primary"), BTreeMap::new());
    for (agent_id, player_id) in [
        ("owner", "player-owner"),
        ("primary", "player-primary"),
        ("recipient", "player-recipient"),
    ] {
        state.starter_oc_claims.insert(
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

    let scope_refs = BTreeSet::from(["trust:one".to_owned(), "trust:two".to_owned()]);
    let registration = OrganizationRegistrationV1 {
        organization_id: "org".into(),
        founder_id: "owner".into(),
        policy_version: ORGANIZATION_CONTINUITY_POLICY_VERSION_V1,
        enabled_at_world_tick: 0,
        last_accounted_world_tick: 10,
        activity_evidence_complete_through_world_tick: 10,
        activity_evidence_paused_reason: None,
        availability_paused_reason: None,
        world_binding: WORLD_BINDING.into(),
        charter_hash: "charter-hash".into(),
        registration_receipt: "registration".into(),
        manifest_root: "manifest".into(),
        manifest_asset_refs: scope_refs.clone(),
        designated_successors: vec!["primary".into()],
        neutral_maintainers: vec![],
        appointment_authorities: BTreeSet::from(["owner".into()]),
        remedy_adjudicators: BTreeSet::from(["recipient".into()]),
        residual_share_bps: BTreeMap::from([("owner".into(), 10_000)]),
        controller_bindings: ["owner", "primary", "recipient"]
            .into_iter()
            .map(|agent_id| {
                let owner = state.resolved_agent_owner(agent_id).unwrap();
                (
                    agent_id.to_owned(),
                    crate::world_rules::OrganizationControllerBindingV1 {
                        root_agent_id: owner.primary_agent_id,
                        player_id: owner.player_id,
                        source_id: owner.source_id,
                        generation: owner.generation,
                    },
                )
            })
            .collect(),
        membership_controller_bindings: ["owner", "primary"]
            .into_iter()
            .map(|agent_id| {
                let owner = state.resolved_agent_owner(agent_id).unwrap();
                (
                    agent_id.to_owned(),
                    crate::world_rules::OrganizationControllerBindingV1 {
                        root_agent_id: owner.primary_agent_id,
                        player_id: owner.player_id,
                        source_id: owner.source_id,
                        generation: owner.generation,
                    },
                )
            })
            .collect(),
        activity_sources: BTreeMap::new(),
        activity_observation_started_at_world_tick: None,
        activity_close_cursor_world_tick: 10,
        activity_close_hash: None,
        activity_last_closed_event_id: None,
        activity_last_closed_event_hash: None,
    };
    let controller = state.resolved_agent_owner("primary").unwrap();
    let candidate = OrganizationSuccessorCandidateV1 {
        candidate_id: "primary".into(),
        controller_root_id: controller.primary_agent_id,
        controller_player_id: controller.player_id,
        controller_source_id: controller.source_id,
        controller_generation: controller.generation,
        scope_refs: scope_refs.clone(),
        budget_ref: MaterialLedgerId::agent("primary").to_string(),
        budget_snapshot: BTreeMap::new(),
        qualification_receipts: BTreeSet::from(["qual-1".into(), "qual-2".into()]),
        conflict_disclosures: BTreeSet::from(["none".into()]),
        acceptance_receipt: "accepted".into(),
        submitted_at_clock: 1,
        submission_order: 1,
        appointed: true,
    };
    let case = OrganizationContinuityCaseV1 {
        case_id: "case".into(),
        organization_id: "org".into(),
        registration_receipt: "registration".into(),
        policy_version: ORGANIZATION_CONTINUITY_POLICY_VERSION_V1,
        origin: OrganizationCaseOriginV1::Dormancy,
        revision: 7,
        phase: OrganizationContinuityPhaseV1::Protected,
        disposition_stage: OrganizationDispositionStageV1::Frozen,
        manifest_root: "manifest".into(),
        manifest_asset_refs: scope_refs.clone(),
        recipients: BTreeSet::from(["owner".into(), "recipient".into()]),
        trigger_activity_receipt: None,
        opened_at_world_tick: 1,
        opened_at_clock: 0,
        notice_receipt: Some("notice".into()),
        notified_at_clock: Some(1),
        notified_at_world_tick: Some(1),
        protection_started_at_clock: Some(1),
        protection_ends_at_clock: Some(1),
        candidate_window_starts_at_clock: Some(1),
        disposition_started_at_clock: None,
        candidates: BTreeMap::from([("primary".into(), candidate)]),
        remedies: BTreeMap::new(),
        quarantined_scope_refs: BTreeSet::new(),
        handoff_candidate_id: Some("primary".into()),
        handoff_scope_refs: scope_refs.clone(),
        handoff_expires_at_clock: Some(handoff_expiry),
        stage_receipts: vec!["case".into()],
    };
    let mut continuity = OrganizationContinuityStateV1 {
        registrations: BTreeMap::from([("org".into(), registration)]),
        availability: BTreeMap::from([(
            "org".into(),
            vec![OrganizationAvailabilityIntervalV1 {
                from_world_tick: 1,
                through_world_tick: 10,
                world_writes_available: true,
                notice_available: true,
                remedy_available: true,
                evidence_status: OrganizationAvailabilityEvidenceStatusV1::Certified,
                world_binding: WORLD_BINDING.into(),
                registration_receipt: "registration".into(),
                certificate_hash: "signed".into(),
            }],
        )]),
        cases: BTreeMap::from([("case".into(), case)]),
        ..Default::default()
    };
    for asset_ref in &scope_refs {
        continuity.assets.insert(
            asset_ref.clone(),
            OrganizationManifestAssetV1 {
                asset_ref: asset_ref.clone(),
                source_receipt: format!("deposit-{asset_ref}"),
                organization_id: "org".into(),
                contributor_id: "recipient".into(),
                material: "alloy_plate".into(),
                amount: 77,
                held_in_trust: true,
                version: 1,
                disposed: false,
                effect_receipt: None,
            },
        );
    }
    let mut rules = crate::world_rules::WorldRulesStateV1 {
        organization_continuity: continuity,
        ..Default::default()
    };
    for order_id in ["qual-1", "qual-2"] {
        let quote = AgentManufacturingQuoteV1 {
            policy_version: 1,
            recipe_id: "recipe.assembler.agent_mk1".into(),
            owner_id: "primary".into(),
            owner_player_id: "player-primary".into(),
            owner_source_id: "qualification".into(),
            world_binding: Some(WORLD_BINDING.into()),
            factory_id: "factory".into(),
            quoted_at: 1,
            expires_at: 10,
            quote_digest: "quote".into(),
            dependency_digest: "dependencies".into(),
            delivery_location_id: "location".into(),
            inputs: BTreeMap::new(),
            input_ledger: MaterialLedgerId::agent("primary"),
            electricity: 48,
            claim_cost: crate::AgentClaimCostQuote {
                reputation_tier: 1,
                claim_cap: 1,
                slot_index: 2,
                activation_fee_amount: 0,
                activation_fee_burn_amount: 0,
                activation_fee_treasury_amount: 0,
                claim_bond_amount: 0,
                upkeep_per_epoch: 0,
                release_cooldown_epochs: 0,
                grace_epochs: 0,
                idle_warning_epochs: 0,
                forced_idle_reclaim_epochs: 0,
                forced_reclaim_penalty_bps: 0,
            },
            upfront_liquid: 0,
            liquid_after_reservation: 0,
            electricity_after_reservation: 0,
            maintenance_per_tick: 0,
        };
        rules.agent_orders.insert(
            order_id.into(),
            AgentIndustrialOrderV1 {
                order_id: order_id.into(),
                owner_id: "primary".into(),
                factory_id: "factory".into(),
                target_agent_id: format!("target-{order_id}"),
                policy_version: 1,
                inputs: BTreeMap::new(),
                confirmed_at: 1,
                ready_at: 1,
                delivered_at: Some(1),
                phase: AgentIndustrialPhaseV1::Delivered,
                quote,
                started_at: Some(1),
                produced_at: Some(1),
                blocked_at: None,
                completed_work_ticks: 12,
                last_progress_tick: Some(1),
                terminal_reason: None,
                refund_pending: false,
                refunded_at: None,
            },
        );
    }
    state.world_rules = Some(rules);
    state
}

fn custody_return_command(
    actor: &str,
    request: &str,
    asset_ref: &str,
    proposal: &str,
) -> WorldRuleCommandV1 {
    custody_return_command_at_revision(actor, request, asset_ref, proposal, 7)
}

fn custody_return_command_at_revision(
    actor: &str,
    request: &str,
    asset_ref: &str,
    proposal: &str,
    expected_revision: u64,
) -> WorldRuleCommandV1 {
    let effect_ref = request_key(actor, request);
    outer(ContinuityCommand::SettleEstateAsset {
        case_id: "case".into(),
        expected_revision,
        asset_ref: asset_ref.into(),
        expected_asset_version: 1,
        operation: oasis7_agent_api::world_action::OrganizationEstateOperationV1::ReturnCustody,
        amount: 77,
        allocations: BTreeMap::from([("recipient".into(), 77)]),
        effect_ref,
        governance_proposal_key: proposal.into(),
    })
}

#[test]
fn active_handoff_executes_only_scoped_custody_return_and_rejects_old_actor_and_expiry() {
    let mut state = custody_return_fixture(100);
    let old_actor_command =
        custody_return_command("owner", "old-return", "trust:one", "old-return");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "old-return", old_actor_command, WORLD_BINDING),
            2,
        )
        .is_err()
    );
    let replacement = outer(ContinuityCommand::CommitHandoff {
        case_id: "case".into(),
        expected_revision: 7,
        candidate_id: "primary".into(),
        scope_refs: BTreeSet::from(["trust:one".into()]),
        governance_proposal_key: "replace-active-handoff".into(),
    });
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding(
                "owner",
                "replace-active-handoff",
                replacement,
                WORLD_BINDING
            ),
            2,
        )
        .is_err()
    );

    state
        .starter_oc_claims
        .get_mut("primary")
        .unwrap()
        .claimed_at = 2;
    let rebound =
        custody_return_command("primary", "rebound-return", "trust:one", "rebound-return");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("primary", "rebound-return", rebound, WORLD_BINDING),
            2,
        )
        .is_err()
    );
    state
        .starter_oc_claims
        .get_mut("primary")
        .unwrap()
        .claimed_at = 1;

    let delegated_command =
        custody_return_command("primary", "delegate-return", "trust:one", "delegate-return");
    approve(&mut state, "primary", "delegate-return", &delegated_command);
    apply(
        &mut state,
        &event_with_binding(
            "primary",
            "delegate-return",
            delegated_command,
            WORLD_BINDING,
        ),
        2,
    );
    let rules = state.world_rules.as_ref().unwrap();
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("recipient")]["alloy_plate"],
        77
    );
    assert!(rules.organization_continuity.assets["trust:one"].disposed);
    assert_eq!(
        rules.organization_continuity.assets["trust:one"]
            .effect_receipt
            .as_deref(),
        Some(request_key("primary", "delegate-return").as_str())
    );

    let mut expired = custody_return_fixture(2);
    let expired_command =
        custody_return_command("primary", "expired-return", "trust:one", "expired-return");
    approve(&mut expired, "primary", "expired-return", &expired_command);
    assert!(
        PreparedWorldRule::prepare(
            &expired,
            &event_with_binding("primary", "expired-return", expired_command, WORLD_BINDING),
            2,
        )
        .is_err()
    );

    let mut malformed = custody_return_fixture(100);
    malformed
        .world_rules
        .as_mut()
        .unwrap()
        .organization_continuity
        .cases
        .get_mut("case")
        .unwrap()
        .handoff_expires_at_clock = None;
    let no_expiry = custody_return_command(
        "primary",
        "missing-expiry-return",
        "trust:one",
        "missing-expiry-return",
    );
    assert!(
        PreparedWorldRule::prepare(
            &malformed,
            &event_with_binding("primary", "missing-expiry-return", no_expiry, WORLD_BINDING,),
            2,
        )
        .is_err()
    );
}

#[test]
fn appointment_authority_can_return_custody_only_at_the_custody_stage() {
    use crate::world_rules::{OrganizationContinuityPhaseV1, OrganizationDispositionStageV1};

    let mut state = custody_return_fixture(0);
    {
        let case = state
            .world_rules
            .as_mut()
            .unwrap()
            .organization_continuity
            .cases
            .get_mut("case")
            .unwrap();
        case.handoff_candidate_id = None;
        case.handoff_scope_refs.clear();
        case.handoff_expires_at_clock = None;
    }
    let early = custody_return_command("owner", "early-return", "trust:one", "early-return");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "early-return", early, WORLD_BINDING),
            2,
        )
        .is_err()
    );

    let case = state
        .world_rules
        .as_mut()
        .unwrap()
        .organization_continuity
        .cases
        .get_mut("case")
        .unwrap();
    case.phase = OrganizationContinuityPhaseV1::Disposition;
    case.disposition_stage = OrganizationDispositionStageV1::ExistingContracts;
    let wrong_stage = custody_return_command("owner", "wrong-stage", "trust:one", "wrong-stage");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "wrong-stage", wrong_stage, WORLD_BINDING),
            2,
        )
        .is_err()
    );

    state
        .world_rules
        .as_mut()
        .unwrap()
        .organization_continuity
        .cases
        .get_mut("case")
        .unwrap()
        .disposition_stage = OrganizationDispositionStageV1::CustodyReturn;
    let valid = custody_return_command("owner", "stage-return", "trust:one", "stage-return");
    approve(&mut state, "owner", "stage-return", &valid);
    apply(
        &mut state,
        &event_with_binding("owner", "stage-return", valid, WORLD_BINDING),
        2,
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .organization_continuity
            .assets["trust:one"]
            .disposed
    );
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("recipient")]["alloy_plate"],
        77
    );
}

#[test]
fn partial_reclaim_keeps_nonoverlapping_handoff_and_same_control_adjudicator_is_blocked() {
    let mut state = custody_return_fixture(100);
    let mut case = state
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity
        .cases["case"]
        .clone();
    let scope_a = "trust:one".to_owned();
    let scope_b = "trust:two".to_owned();
    case.stage_receipts.push("prior-handoff-key".into());
    case.remedies.insert(
        "reclaim".into(),
        crate::world_rules::OrganizationRemedyRequestV1 {
            request_id: "reclaim".into(),
            requester_id: "owner".into(),
            kind: oasis7_agent_api::world_action::OrganizationRemedyKindV1::Reclaim,
            scope_refs: BTreeSet::from([scope_a.clone()]),
            fact_ref: "prior-handoff-key".into(),
            evidence_refs: vec!["prior-handoff-key".into()],
            admitted_at_clock: 0,
            deadline_clock: 100,
            outcome: None,
            supported_scope_refs: BTreeSet::new(),
            adjudication_ref: None,
            resolved_by: None,
        },
    );
    let rules = state.world_rules.as_mut().unwrap();
    rules
        .organization_continuity
        .cases
        .insert("case".into(), case);
    rules.requests.insert(
        "prior-handoff-key".into(),
        crate::world_rules::WorldRuleReceiptV1 {
            request_id: "prior-handoff".into(),
            actor_id: "owner".into(),
            command: outer(ContinuityCommand::CommitHandoff {
                case_id: "case".into(),
                expected_revision: 6,
                candidate_id: "primary".into(),
                scope_refs: BTreeSet::from([scope_a.clone(), scope_b.clone()]),
                governance_proposal_key: "prior".into(),
            }),
            committed_at: 1,
            result_id: "primary".into(),
        },
    );
    let make_resolve = || {
        outer(ContinuityCommand::ResolveRemedy {
            case_id: "case".into(),
            expected_revision: 7,
            request_id: "reclaim".into(),
            outcome:
                oasis7_agent_api::world_action::OrganizationRemedyOutcomeV1::PartiallySupported,
            supported_scope_refs: BTreeSet::from([scope_a.clone()]),
            adjudication_ref: "prior-handoff-key".into(),
            governance_proposal_key: "resolve-reclaim".into(),
        })
    };
    let conflicted = make_resolve();
    state
        .starter_oc_claims
        .get_mut("recipient")
        .unwrap()
        .player_id = "player-primary".into();
    approve(&mut state, "recipient", "resolve-reclaim", &conflicted);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding(
                "recipient",
                "same-controller-review",
                conflicted,
                WORLD_BINDING
            ),
            2,
        )
        .is_err()
    );
    state
        .starter_oc_claims
        .get_mut("recipient")
        .unwrap()
        .player_id = "player-recipient".into();

    let resolve = make_resolve();
    apply(
        &mut state,
        &event_with_binding("recipient", "resolve-reclaim", resolve, WORLD_BINDING),
        2,
    );
    let case = &state
        .world_rules
        .as_ref()
        .unwrap()
        .organization_continuity
        .cases["case"];
    assert_eq!(case.handoff_scope_refs, BTreeSet::from([scope_b.clone()]));
    assert_eq!(case.handoff_candidate_id.as_deref(), Some("primary"));
    assert_eq!(case.handoff_expires_at_clock, Some(100));

    let nonoverlapping_return = custody_return_command_at_revision(
        "primary",
        "return-nonoverlap",
        "trust:two",
        "return-nonoverlap",
        8,
    );
    approve(
        &mut state,
        "primary",
        "return-nonoverlap",
        &nonoverlapping_return,
    );
    apply(
        &mut state,
        &event_with_binding(
            "primary",
            "return-nonoverlap",
            nonoverlapping_return,
            WORLD_BINDING,
        ),
        2,
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .organization_continuity
            .assets["trust:two"]
            .disposed
    );
}

#[test]
fn cancelled_case_remedies_guard_only_overlapping_assets_and_remain_resolvable() {
    use crate::world_rules::{
        OrganizationContinuityPhaseV1 as Phase, OrganizationDispositionStageV1 as Stage,
        OrganizationRemedyRequestV1,
    };
    use oasis7_agent_api::world_action::{
        OrganizationEstateOperationV1 as EstateOperation, OrganizationRemedyKindV1 as RemedyKind,
        OrganizationRemedyOutcomeV1 as RemedyOutcome,
    };

    let mut state = custody_return_fixture(100);
    let continuity = &mut state.world_rules.as_mut().unwrap().organization_continuity;
    let old_case = continuity.cases.get_mut("case").unwrap();
    old_case.phase = Phase::Cancelled;
    old_case.handoff_candidate_id = None;
    old_case.handoff_scope_refs.clear();
    old_case.handoff_expires_at_clock = None;
    old_case.stage_receipts.push("prior-handoff-key".into());
    old_case.remedies.insert(
        "old-reclaim".into(),
        OrganizationRemedyRequestV1 {
            request_id: "old-reclaim".into(),
            requester_id: "owner".into(),
            kind: RemedyKind::Reclaim,
            scope_refs: BTreeSet::from(["trust:one".into()]),
            fact_ref: "prior-handoff-key".into(),
            evidence_refs: vec!["prior-handoff-key".into()],
            admitted_at_clock: 0,
            deadline_clock: 100,
            outcome: None,
            supported_scope_refs: BTreeSet::new(),
            adjudication_ref: None,
            resolved_by: None,
        },
    );
    let mut new_case = continuity.cases["case"].clone();
    new_case.case_id = "new-case".into();
    new_case.revision = 1;
    new_case.phase = Phase::Disposition;
    new_case.disposition_stage = Stage::CustodyReturn;
    new_case.handoff_candidate_id = None;
    new_case.handoff_scope_refs.clear();
    new_case.handoff_expires_at_clock = None;
    continuity.cases.insert("new-case".into(), new_case);
    state.world_rules.as_mut().unwrap().requests.insert(
        "prior-handoff-key".into(),
        crate::world_rules::WorldRuleReceiptV1 {
            request_id: "prior-handoff".into(),
            actor_id: "owner".into(),
            command: outer(ContinuityCommand::CommitHandoff {
                case_id: "case".into(),
                expected_revision: 6,
                candidate_id: "primary".into(),
                scope_refs: BTreeSet::from(["trust:one".into()]),
                governance_proposal_key: "prior".into(),
            }),
            committed_at: 1,
            result_id: "primary".into(),
        },
    );

    let return_asset = |asset_ref: &str, request: &str, proposal: &str| {
        outer(ContinuityCommand::SettleEstateAsset {
            case_id: "new-case".into(),
            expected_revision: 1,
            asset_ref: asset_ref.into(),
            expected_asset_version: 1,
            operation: EstateOperation::ReturnCustody,
            amount: 77,
            allocations: BTreeMap::from([("recipient".into(), 77)]),
            effect_ref: request_key("owner", request),
            governance_proposal_key: proposal.into(),
        })
    };

    let overlapping = return_asset("trust:one", "blocked-old-remedy", "blocked-old-remedy");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding("owner", "blocked-old-remedy", overlapping, WORLD_BINDING),
            2,
        )
        .is_err()
    );

    let disjoint = return_asset("trust:two", "disjoint-return", "disjoint-return");
    approve(&mut state, "owner", "disjoint-return", &disjoint);
    apply(
        &mut state,
        &event_with_binding("owner", "disjoint-return", disjoint, WORLD_BINDING),
        2,
    );
    assert!(
        state
            .world_rules
            .as_ref()
            .unwrap()
            .organization_continuity
            .assets["trust:two"]
            .disposed
    );

    let resolve = outer(ContinuityCommand::ResolveRemedy {
        case_id: "case".into(),
        expected_revision: 7,
        request_id: "old-reclaim".into(),
        outcome: RemedyOutcome::Supported,
        supported_scope_refs: BTreeSet::from(["trust:one".into()]),
        adjudication_ref: "prior-handoff-key".into(),
        governance_proposal_key: "resolve-old-reclaim".into(),
    });
    approve(&mut state, "recipient", "resolve-old-reclaim", &resolve);
    apply(
        &mut state,
        &event_with_binding("recipient", "resolve-old-reclaim", resolve, WORLD_BINDING),
        2,
    );
    let continuity = &state.world_rules.as_ref().unwrap().organization_continuity;
    assert_eq!(continuity.cases["case"].phase, Phase::Cancelled);
    assert!(
        continuity.cases["case"].remedies["old-reclaim"]
            .outcome
            .is_some()
    );
    assert!(continuity.cases["case"].handoff_scope_refs.is_empty());

    let after_quarantine = outer(ContinuityCommand::SettleEstateAsset {
        case_id: "new-case".into(),
        expected_revision: 2,
        asset_ref: "trust:one".into(),
        expected_asset_version: 1,
        operation: EstateOperation::ReturnCustody,
        amount: 77,
        allocations: BTreeMap::from([("recipient".into(), 77)]),
        effect_ref: request_key("owner", "blocked-old-quarantine"),
        governance_proposal_key: "blocked-old-quarantine".into(),
    });
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event_with_binding(
                "owner",
                "blocked-old-quarantine",
                after_quarantine,
                WORLD_BINDING
            ),
            2,
        )
        .is_err()
    );
}
