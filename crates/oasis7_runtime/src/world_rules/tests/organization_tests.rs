use super::*;

#[test]
fn organization_disposition_scope_handoff_and_single_effect() {
    use oasis7_agent_api::world_action::{
        OrganizationEstateRightV1 as Right, OrganizationEstateScopeV1 as Scope,
    };
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "owner",
            "deposit",
            WorldRuleCommandV1::DepositOrganizationEstate {
                organization_id: "org".into(),
                material: "alloy_plate".into(),
                amount: 9,
                held_in_trust: true,
            },
        ),
        10,
    );
    let deposit = request_key("owner", "deposit");
    apply(
        &mut state,
        &event(
            "owner",
            "dissolve",
            WorldRuleCommandV1::BeginOrganizationDissolution {
                organization_id: "org".into(),
            },
        ),
        11,
    );
    let case_id = request_key("owner", "dissolve");
    apply(
        &mut state,
        &event(
            "owner",
            "obligations",
            WorldRuleCommandV1::AdvanceOrganization {
                case_id: case_id.clone(),
                expected_revision: 1,
            },
        ),
        12,
    );
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "skip",
                WorldRuleCommandV1::AdvanceOrganization {
                    case_id: case_id.clone(),
                    expected_revision: 2
                }
            ),
            13
        )
        .is_err()
    );
    let scopes = BTreeMap::from([(deposit.clone(), Right::ReturnTrust)])
        .into_iter()
        .map(|(deposit_id, right)| Scope {
            object_id: deposit_id,
            right,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let grant = WorldRuleCommandV1::GrantOrganizationScope {
        case_id: case_id.clone(),
        expected_revision: 2,
        proposal_key: "grant".into(),
        delegate_id: "owner".into(),
        scopes: scopes.clone(),
        expires_at: 200,
        replace_grant_id: None,
    };
    approve(&mut state, "owner", "grant", &grant);
    apply(&mut state, &event("owner", "grant", grant), 14);
    let grant_id = request_key("owner", "grant");
    let handoff = WorldRuleCommandV1::GrantOrganizationScope {
        case_id: case_id.clone(),
        expected_revision: 3,
        proposal_key: "handoff".into(),
        delegate_id: "recipient".into(),
        scopes,
        expires_at: 200,
        replace_grant_id: Some(grant_id.clone()),
    };
    approve(&mut state, "owner", "handoff", &handoff);
    apply(&mut state, &event("owner", "handoff", handoff), 15);
    let stale = event(
        "owner",
        "stale",
        WorldRuleCommandV1::DisposeOrganizationEstate {
            case_id: case_id.clone(),
            expected_revision: 4,
            grant_id,
            grant_revision: 1,
            deposit_id: deposit.clone(),
        },
    );
    assert!(PreparedWorldRule::prepare(&state, &stale, 16).is_err());
    let dispose = event(
        "recipient",
        "return",
        WorldRuleCommandV1::DisposeOrganizationEstate {
            case_id: case_id.clone(),
            expected_revision: 4,
            grant_id: request_key("owner", "handoff"),
            grant_revision: 1,
            deposit_id: deposit.clone(),
        },
    );
    apply(&mut state, &dispose, 16);
    apply(&mut state, &dispose, 17);
    assert_eq!(
        state.material_ledgers[&MaterialLedgerId::agent("owner")]["alloy_plate"],
        120
    );
    for (revision, request) in [(5, "business"), (6, "residual"), (7, "finish")] {
        apply(
            &mut state,
            &event(
                "owner",
                request,
                WorldRuleCommandV1::AdvanceOrganization {
                    case_id: case_id.clone(),
                    expected_revision: revision,
                },
            ),
            18,
        );
    }
    assert!(!state.alliances.contains_key("org"));
    assert!(state.agents.contains_key("primary"));
    assert!(state.agent_claims.contains_key("primary"));
    assert!(
        state.world_rules.as_ref().unwrap().organization_estate[&deposit]
            .effect_receipt
            .is_some()
    );
}

#[test]
fn organization_pending_recovery_does_not_end_protection() {
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "recipient",
            "notice",
            WorldRuleCommandV1::OpenOrganizationProtection {
                organization_id: "org".into(),
            },
        ),
        700,
    );
    let case_id = request_key("recipient", "notice");
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "early",
                WorldRuleCommandV1::AdvanceOrganization {
                    case_id: case_id.clone(),
                    expected_revision: 1
                }
            ),
            819
        )
        .is_err()
    );
    apply(
        &mut state,
        &event(
            "recipient",
            "protect",
            WorldRuleCommandV1::AdvanceOrganization {
                case_id: case_id.clone(),
                expected_revision: 1,
            },
        ),
        820,
    );
    apply(
        &mut state,
        &event(
            "primary",
            "appeal",
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id: case_id.clone(),
                expected_revision: 2,
                appeal: true,
            },
        ),
        821,
    );
    assert_eq!(
        state.world_rules.as_ref().unwrap().organization_cases[&case_id].phase,
        OrganizationProtectionPhaseV1::Protected
    );
    let reject = WorldRuleCommandV1::ResolveOrganizationRecovery {
        case_id: case_id.clone(),
        expected_revision: 3,
        recovery_request_id: request_key("primary", "appeal"),
        approved: false,
        proposal_key: "reject".into(),
    };
    approve(&mut state, "owner", "reject", &reject);
    apply(&mut state, &event("owner", "reject", reject), 822);
    assert_eq!(
        state.world_rules.as_ref().unwrap().organization_cases[&case_id].phase,
        OrganizationProtectionPhaseV1::Protected
    );
}

#[test]
fn accepted_user_work_counts_as_activity_but_system_cleanup_does_not() {
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "recipient",
            "notice",
            WorldRuleCommandV1::OpenOrganizationProtection {
                organization_id: "org".into(),
            },
        ),
        700,
    );
    let case = request_key("recipient", "notice");
    apply(
        &mut state,
        &event(
            "primary",
            "reclaim",
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id: case.clone(),
                expected_revision: 1,
                appeal: false,
            },
        ),
        710,
    );
    assert_eq!(state.agents["primary"].last_active, 710);
    let mut auto = event(
        "owner",
        "advance",
        WorldRuleCommandV1::AdvanceOrganization {
            case_id: case.clone(),
            expected_revision: 2,
        },
    );
    if let DomainEvent::WorldRuleCommitted { action_id, .. } = &mut auto {
        *action_id = 0;
    }
    let previous = state.agents["owner"].last_active;
    apply(&mut state, &auto, 820);
    assert_eq!(state.agents["owner"].last_active, previous);
    assert_eq!(
        state.world_rules.as_ref().unwrap().organization_cases[&case].phase,
        OrganizationProtectionPhaseV1::Resolved
    );
}

#[test]
fn recovery_member_cannot_exhaust_other_members_request_slots() {
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "recipient",
            "notice",
            WorldRuleCommandV1::OpenOrganizationProtection {
                organization_id: "org".into(),
            },
        ),
        700,
    );
    let case = request_key("recipient", "notice");
    apply(
        &mut state,
        &event(
            "owner",
            "first",
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id: case.clone(),
                expected_revision: 1,
                appeal: true,
            },
        ),
        701,
    );
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "spam",
                WorldRuleCommandV1::RequestOrganizationRecovery {
                    case_id: case.clone(),
                    expected_revision: 2,
                    appeal: true
                }
            ),
            702
        )
        .is_err()
    );
    apply(
        &mut state,
        &event(
            "primary",
            "other-member",
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id: case,
                expected_revision: 2,
                appeal: true,
            },
        ),
        703,
    );
}

#[test]
fn blocked_founder_cannot_advance_dissolution() {
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "owner",
            "case",
            WorldRuleCommandV1::BeginOrganizationDissolution {
                organization_id: "org".into(),
            },
        ),
        10,
    );
    state.gameplay_policy.blocked_agents.push("owner".into());
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "advance",
                WorldRuleCommandV1::AdvanceOrganization {
                    case_id: request_key("owner", "case"),
                    expected_revision: 1
                }
            ),
            11
        )
        .is_err()
    );
}

#[test]
fn case_freeze_rejects_new_war_on_action_and_raw_event_paths() {
    let mut state = organization_fixture();
    state.alliances.insert(
        "other".into(),
        crate::AllianceState {
            alliance_id: "other".into(),
            members: vec!["recipient".into()],
            charter: "other".into(),
            formed_by_agent_id: "recipient".into(),
            formed_at: 0,
        },
    );
    apply(
        &mut state,
        &event(
            "owner",
            "case",
            WorldRuleCommandV1::BeginOrganizationDissolution {
                organization_id: "org".into(),
            },
        ),
        10,
    );
    let mut world = World::new_with_state(state);
    world.submit_action(Action::DeclareWar {
        initiator_agent_id: "owner".into(),
        war_id: "war".into(),
        aggressor_alliance_id: "org".into(),
        defender_alliance_id: "other".into(),
        objective: "test".into(),
        intensity: 1,
    });
    world.step().unwrap();
    assert!(world.state().wars.is_empty());
    assert!(world.state().organization_risk_frozen("org"));
}

#[test]
fn blocked_founder_still_receives_automatic_dormancy_protection() {
    let mut state = organization_fixture();
    state.time = 700;
    state.gameplay_policy.blocked_agents.push("owner".into());
    let mut world = World::new_with_state(state);
    world.step().unwrap();
    let rules = world.state().world_rules.as_ref().unwrap();
    assert_eq!(
        rules.organization_cases.values().next().unwrap().phase,
        OrganizationProtectionPhaseV1::Notice
    );
}

#[test]
fn approved_appeal_blocks_founder_disposition_during_protection() {
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "owner",
            "case",
            WorldRuleCommandV1::BeginOrganizationDissolution {
                organization_id: "org".into(),
            },
        ),
        10,
    );
    let id = request_key("owner", "case");
    apply(
        &mut state,
        &event(
            "primary",
            "appeal",
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id: id.clone(),
                expected_revision: 1,
                appeal: true,
            },
        ),
        11,
    );
    let command = WorldRuleCommandV1::ResolveOrganizationRecovery {
        case_id: id.clone(),
        expected_revision: 2,
        recovery_request_id: request_key("primary", "appeal"),
        approved: true,
        proposal_key: "approve".into(),
    };
    approve(&mut state, "owner", "approve", &command);
    apply(&mut state, &event("owner", "approve", command), 12);
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event(
                "owner",
                "advance",
                WorldRuleCommandV1::AdvanceOrganization {
                    case_id: id,
                    expected_revision: 3
                }
            ),
            13
        )
        .is_err()
    );
}

#[test]
fn automatic_claim_upkeep_does_not_keep_idle_organization_active() {
    let state = organization_fixture();
    let last_active = state.agents["owner"].last_active;
    let mut world = World::new_with_state(state);
    for _ in 0..605 {
        world.step().unwrap();
    }
    assert_eq!(world.state().agents["owner"].last_active, last_active);
    assert!(
        world
            .state()
            .world_rules
            .as_ref()
            .unwrap()
            .organization_cases
            .values()
            .any(|c| c.organization_id == "org"
                && matches!(
                    c.phase,
                    OrganizationProtectionPhaseV1::Notice
                        | OrganizationProtectionPhaseV1::Protected
                ))
    );
}

#[test]
fn successful_appeal_after_distribution_can_resume_only_with_new_finalization_authority() {
    use oasis7_agent_api::world_action::{
        OrganizationEstateRightV1 as Right, OrganizationEstateScopeV1 as Scope,
    };
    let mut state = organization_fixture();
    apply(
        &mut state,
        &event(
            "owner",
            "case",
            WorldRuleCommandV1::BeginOrganizationDissolution {
                organization_id: "org".into(),
            },
        ),
        10,
    );
    let id = request_key("owner", "case");
    for (revision, request) in [(1, "obligations"), (2, "business"), (3, "residual")] {
        apply(
            &mut state,
            &event(
                "owner",
                request,
                WorldRuleCommandV1::AdvanceOrganization {
                    case_id: id.clone(),
                    expected_revision: revision,
                },
            ),
            11,
        );
    }
    apply(
        &mut state,
        &event(
            "primary",
            "appeal",
            WorldRuleCommandV1::RequestOrganizationRecovery {
                case_id: id.clone(),
                expected_revision: 4,
                appeal: true,
            },
        ),
        12,
    );
    let resolve = WorldRuleCommandV1::ResolveOrganizationRecovery {
        case_id: id.clone(),
        expected_revision: 5,
        recovery_request_id: request_key("primary", "appeal"),
        approved: true,
        proposal_key: "resolution".into(),
    };
    approve(&mut state, "owner", "resolution", &resolve);
    apply(&mut state, &event("owner", "resolve", resolve), 13);
    let grant = WorldRuleCommandV1::GrantOrganizationScope {
        case_id: id.clone(),
        expected_revision: 6,
        proposal_key: "finalize".into(),
        delegate_id: "recipient".into(),
        scopes: [Scope {
            object_id: id.clone(),
            right: Right::FinalizeCase,
        }]
        .into(),
        expires_at: 1000,
        replace_grant_id: None,
    };
    approve(&mut state, "owner", "finalize", &grant);
    assert!(
        PreparedWorldRule::prepare(&state, &event("owner", "grant", grant.clone()), 612).is_err()
    );
    apply(&mut state, &event("owner", "grant", grant), 613);
    let command = WorldRuleCommandV1::AdvanceOrganization {
        case_id: id,
        expected_revision: 7,
    };
    assert!(
        PreparedWorldRule::prepare(
            &state,
            &event("owner", "stale-founder", command.clone()),
            614
        )
        .is_err()
    );
    apply(&mut state, &event("recipient", "finalize", command), 614);
    assert!(!state.alliances.contains_key("org"));
}
