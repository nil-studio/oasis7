use super::*;

fn issue_event() -> DomainEvent {
    DomainEvent::RestrictedStarterClaimGrantIssued {
        issuer_id: "liveops".into(),
        beneficiary_account_id: "alice".into(),
        source_treasury_bucket_id: MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
            .into(),
        amount: 40,
        issuance_reason: "qa".into(),
        spend_scope: RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY.into(),
        issued_at_epoch: 1,
        expires_at_epoch: 5,
    }
}

fn grant_world() -> World {
    let mut world = World::new();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 0,
        total_issued: 100,
        total_burned: 0,
    });
    world
        .set_main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
            100,
        )
        .unwrap();
    world
}

fn install_liveops_admin(world: &mut World) {
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".into(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(),
                "controller".into(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from(["liveops".into()]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([format!("{:064x}", 1)]),
                    },
                ),
                (
                    "controller".into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([format!("{:064x}", 2)]),
                    },
                ),
                (
                    "liveops".into(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([format!("{:064x}", 3)]),
                    },
                ),
            ]),
        })
        .unwrap();
}

fn terminal_event(revoked: bool) -> (World, DomainEvent) {
    let mut world = grant_world();
    world
        .append_event_for_test(WorldEventBody::Domain(issue_event()), None)
        .unwrap();
    let common = (
        "alice".to_string(),
        "liveops".to_string(),
        "qa".to_string(),
        RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY.to_string(),
        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.to_string(),
    );
    let event = if revoked {
        DomainEvent::RestrictedStarterClaimGrantRevoked {
            beneficiary_account_id: common.0,
            issuer_id: common.1,
            issuance_reason: common.2,
            spend_scope: common.3,
            source_treasury_bucket_id: common.4,
            issued_amount: 40,
            revoked_amount: 40,
            issued_at_epoch: 1,
            revoked_at_epoch: 3,
            configured_expires_at_epoch: 5,
            revoke_reason: "policy".into(),
        }
    } else {
        DomainEvent::RestrictedStarterClaimGrantExpired {
            beneficiary_account_id: common.0,
            issuer_id: common.1,
            issuance_reason: common.2,
            spend_scope: common.3,
            source_treasury_bucket_id: common.4,
            issued_amount: 40,
            expired_amount: 40,
            issued_at_epoch: 1,
            expired_at_epoch: 5,
            configured_expires_at_epoch: 5,
        }
    };
    (world, event)
}

fn fixtures() -> Vec<(World, DomainEvent)> {
    let mut topup = World::new();
    topup
        .set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, 50)
        .unwrap();
    vec![
        (
            topup,
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                controller_account_id: "controller".into(),
                top_up_id: "topup".into(),
                source_treasury_bucket_id: MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.into(),
                target_treasury_bucket_id:
                    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.into(),
                amount: 20,
                topped_up_at_epoch: 1,
            },
        ),
        (grant_world(), issue_event()),
        terminal_event(false),
        terminal_event(true),
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
fn all_four_raw_events_are_atomic_retryable_and_replayable() {
    for (index, (mut world, event)) in fixtures().into_iter().enumerate() {
        let baseline = world.snapshot();
        let before = world.clone();
        let root = world.current_state_root_hash().unwrap();
        let len = world.journal().events.len();
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event_for_test(
                WorldEventBody::Domain(event.clone()),
                Some(CausedBy::Action(4_300 + index as u64)),
            )
            .expect_err("postprepare fault");
        assert!(format!("{error:?}").contains("publication preparation"));
        unchanged(&world, &before, &root);
        let cause = Some(CausedBy::Action(4_400 + index as u64));
        world
            .append_event_for_test(WorldEventBody::Domain(event), cause.clone())
            .unwrap();
        assert_eq!(world.journal().events.len(), len + 1);
        assert_eq!(world.journal().events.last().unwrap().caused_by, cause);
        let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
        assert_eq!(replay.snapshot(), world.snapshot());
        assert_eq!(
            replay.current_state_root_hash().unwrap(),
            world.current_state_root_hash().unwrap()
        );
    }
}

fn natural(mut world: World, event: DomainEvent, expected: &str) {
    let before = world.clone();
    let root = world.current_state_root_hash().unwrap();
    let error = world
        .append_event_for_test(WorldEventBody::Domain(event), None)
        .expect_err("natural error");
    assert!(format!("{error:?}").contains(expected), "{error:?}");
    unchanged(&world, &before, &root);
}

#[test]
fn exact_priorities_and_late_compound_failures_never_leak() {
    let (mut world, topup) = fixtures().swap_remove(0);
    world
        .set_main_token_treasury_balance(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
            u64::MAX,
        )
        .unwrap();
    natural(world, topup, "treasury balance overflow");
    let mut world = grant_world();
    world.set_main_token_supply(MainTokenSupplyState {
        total_supply: u64::MAX,
        circulating_supply: u64::MAX,
        total_issued: 0,
        total_burned: 0,
    });
    natural(world, issue_event(), "circulating overflow");
    for revoked in [false, true] {
        let (mut world, event) = terminal_event(revoked);
        world.set_main_token_supply(MainTokenSupplyState {
            total_supply: 100,
            circulating_supply: 0,
            total_issued: 100,
            total_burned: 0,
        });
        natural(world, event, "circulating insufficient");
    }
}

#[test]
fn successes_preserve_materials_and_route_only_actor_events() {
    let world_key = MaterialLedgerId::world();
    let other = MaterialLedgerId::site("other-41b");
    for (mut world, event) in fixtures() {
        let mut state = world.state().clone();
        state.materials.insert("legacy".into(), 7);
        state.material_ledgers.remove(&world_key);
        state
            .material_ledgers
            .insert(other.clone(), [("ore".into(), 9)].into());
        world = World::new_with_state(state);
        world
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        assert_eq!(world.state().materials["legacy"], 7);
        assert_eq!(world.state().material_ledgers[&other]["ore"], 9);
        match event {
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. } => assert_eq!(
                world.main_token_treasury_balance(
                    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
                ),
                20
            ),
            DomainEvent::RestrictedStarterClaimGrantIssued { .. } => assert_eq!(
                world.main_token_restricted_starter_claim_balance("alice"),
                40
            ),
            DomainEvent::RestrictedStarterClaimGrantExpired { .. }
            | DomainEvent::RestrictedStarterClaimGrantRevoked { .. } => assert_eq!(
                world.main_token_restricted_starter_claim_balance("alice"),
                0
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn raw_validation_priority_table_is_exact_and_nonmutating() {
    let topup_case = |mutate: fn(&mut DomainEvent), expected: &str| {
        let (world, mut event) = fixtures().swap_remove(0);
        mutate(&mut event);
        natural(world, event, expected);
    };
    topup_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                controller_account_id,
                top_up_id,
                ..
            } = event
            {
                controller_account_id.clear();
                top_up_id.clear();
            }
        },
        "controller_account_id cannot be empty",
    );
    topup_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { top_up_id, .. } = event
            {
                top_up_id.clear();
            }
        },
        "top_up_id cannot be empty",
    );
    topup_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                source_treasury_bucket_id,
                ..
            } = event
            {
                *source_treasury_bucket_id = "wrong".into();
            }
        },
        "source bucket must be ecosystem_pool",
    );
    topup_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                target_treasury_bucket_id,
                ..
            } = event
            {
                *target_treasury_bucket_id = "wrong".into();
            }
        },
        "target bucket must be restricted starter claim liveops pool",
    );
    topup_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { amount, .. } = event {
                *amount = 0;
            }
        },
        "amount must be > 0",
    );
    let (mut world, event) = fixtures().swap_remove(0);
    world
        .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
        .unwrap();
    natural(world, event, "top_up_id already exists");

    let issue_case = |mutate: fn(&mut DomainEvent), expected: &str| {
        let mut event = issue_event();
        mutate(&mut event);
        natural(grant_world(), event, expected);
    };
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued {
                issuer_id,
                beneficiary_account_id,
                ..
            } = event
            {
                issuer_id.clear();
                beneficiary_account_id.clear();
            }
        },
        "issuer_id cannot be empty",
    );
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued {
                beneficiary_account_id,
                ..
            } = event
            {
                beneficiary_account_id.clear();
            }
        },
        "beneficiary_account_id cannot be empty",
    );
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued {
                source_treasury_bucket_id,
                ..
            } = event
            {
                source_treasury_bucket_id.clear();
            }
        },
        "source_treasury_bucket_id cannot be empty",
    );
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued {
                issuance_reason, ..
            } = event
            {
                issuance_reason.clear();
            }
        },
        "issuance_reason cannot be empty",
    );
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued { spend_scope, .. } = event {
                spend_scope.clear();
            }
        },
        "spend_scope cannot be empty",
    );
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued { amount, .. } = event {
                *amount = 0;
            }
        },
        "amount must be > 0",
    );
    issue_case(
        |event| {
            if let DomainEvent::RestrictedStarterClaimGrantIssued {
                expires_at_epoch, ..
            } = event
            {
                *expires_at_epoch = 1;
            }
        },
        "expires_at_epoch must be > issued_at_epoch",
    );
    let mut poor = grant_world();
    poor.set_main_token_treasury_balance(
        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
        39,
    )
    .unwrap();
    natural(poor, issue_event(), "treasury insufficient");

    for revoked in [false, true] {
        let (world, mut event) = terminal_event(revoked);
        if revoked {
            if let DomainEvent::RestrictedStarterClaimGrantRevoked {
                issuance_reason,
                revoke_reason,
                ..
            } = &mut event
            {
                issuance_reason.clear();
                revoke_reason.clear();
            }
            natural(world, event, "revoke metadata mismatch");
        } else {
            if let DomainEvent::RestrictedStarterClaimGrantExpired {
                issuance_reason,
                expired_at_epoch,
                ..
            } = &mut event
            {
                issuance_reason.clear();
                *expired_at_epoch = 0;
            }
            natural(world, event, "expiration metadata mismatch");
        }
    }
    let (world, mut event) = terminal_event(false);
    if let DomainEvent::RestrictedStarterClaimGrantExpired {
        expired_at_epoch, ..
    } = &mut event
    {
        *expired_at_epoch = 4;
    }
    natural(world, event, "expired before configured epoch");
    let (world, mut event) = terminal_event(true);
    if let DomainEvent::RestrictedStarterClaimGrantRevoked { revoke_reason, .. } = &mut event {
        *revoke_reason = "   ".into();
    }
    natural(world, event, "revoke_reason cannot be empty");
}

#[test]
fn materials_routing_trim_and_topup_action_raw_parity_are_exact() {
    let world_key = MaterialLedgerId::world();
    let unrelated = MaterialLedgerId::site("other-41b2");
    for mode in 0..3 {
        let (world, mut event) = fixtures().swap_remove(3);
        if let DomainEvent::RestrictedStarterClaimGrantRevoked { revoke_reason, .. } = &mut event {
            *revoke_reason = "  policy  ".into();
        }
        let mut state = world.state().clone();
        state.materials.insert("legacy".into(), 7);
        state
            .material_ledgers
            .insert(unrelated.clone(), [("ore".into(), 9)].into());
        match mode {
            0 => {
                state.material_ledgers.remove(&world_key);
            }
            1 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), Default::default());
            }
            2 => {
                state
                    .material_ledgers
                    .insert(world_key.clone(), [("canonical".into(), 11)].into());
            }
            _ => unreachable!(),
        }
        let mut world = World::new_with_state(state);
        world
            .append_event_for_test(
                WorldEventBody::Domain(DomainEvent::AgentRegistered {
                    agent_id: "liveops".into(),
                    pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
                }),
                None,
            )
            .unwrap();
        let mailbox = world.state().agents["liveops"].mailbox.len();
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        assert_eq!(world.state().agents["liveops"].mailbox.len(), mailbox + 1);
        assert_eq!(world.state().material_ledgers[&unrelated]["ore"], 9);
        assert_eq!(
            world.state().materials,
            world.state().material_ledgers[&world_key]
        );
        let grant = world.restricted_starter_claim_grant("alice").unwrap();
        assert_eq!(grant.status_reason.as_deref(), Some("  policy  "));
    }

    let (world, mut zero_expiration) = terminal_event(false);
    let mut state = world.state().clone();
    state.main_token_balances.remove("alice");
    if let DomainEvent::RestrictedStarterClaimGrantExpired { expired_amount, .. } =
        &mut zero_expiration
    {
        *expired_amount = 0;
    }
    let mut missing_account = World::new_with_state(state);
    missing_account
        .append_event_for_test(WorldEventBody::Domain(zero_expiration), None)
        .unwrap();
    assert_eq!(
        missing_account
            .restricted_starter_claim_grant("alice")
            .unwrap()
            .status,
        RestrictedStarterClaimGrantStatus::Expired
    );

    for (index, (world, event)) in fixtures().into_iter().enumerate() {
        let actor = match &event {
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp {
                controller_account_id,
                ..
            } => Some(controller_account_id.clone()),
            DomainEvent::RestrictedStarterClaimGrantIssued { issuer_id, .. }
            | DomainEvent::RestrictedStarterClaimGrantRevoked { issuer_id, .. } => {
                Some(issuer_id.clone())
            }
            DomainEvent::RestrictedStarterClaimGrantExpired { .. } => None,
            _ => unreachable!(),
        };
        let mut world = world;
        if let Some(actor) = &actor {
            world
                .append_event_for_test(
                    WorldEventBody::Domain(DomainEvent::AgentRegistered {
                        agent_id: actor.clone(),
                        pos: oasis7_agent_api::world_action::GeoPos::new(index as i64, 0, 0),
                    }),
                    None,
                )
                .unwrap();
        }
        let before = actor
            .as_ref()
            .map(|actor| world.state().agents[actor].mailbox.len());
        world
            .append_event_for_test(WorldEventBody::Domain(event), None)
            .unwrap();
        if let (Some(actor), Some(before)) = (actor, before) {
            assert_eq!(world.state().agents[&actor].mailbox.len(), before + 1);
        } else {
            assert!(
                world
                    .state()
                    .agents
                    .values()
                    .all(|agent| agent.mailbox.is_empty())
            );
        }
    }

    let mut base = World::new();
    set_main_token_controller_registry_for_tests(&mut base, "controller");
    base.set_main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, 50)
        .unwrap();
    let mut action = base.clone();
    action.submit_action(Action::TopUpRestrictedStarterClaimLiveopsPool {
        controller_account_id: " controller ".into(),
        top_up_id: " topup-action ".into(),
        amount: 20,
    });
    action.step().unwrap();
    let event = action
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(
                event @ DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. },
            ) => Some(event.clone()),
            _ => None,
        })
        .expect("action top-up event");
    let mut raw = base;
    raw.step().unwrap();
    raw.append_event_for_test(WorldEventBody::Domain(event), None)
        .unwrap();
    assert_eq!(raw.state(), action.state());
    assert_eq!(
        raw.current_state_root_hash().unwrap(),
        action.current_state_root_hash().unwrap()
    );
}

#[test]
fn omitted_issue_and_terminal_guards_are_exact_and_nonmutating() {
    let (active, _) = terminal_event(false);
    natural(
        active,
        issue_event(),
        "already active or pending settlement",
    );

    let mut seeded = grant_world();
    seeded
        .set_main_token_account_balance_with_restricted("alice", 0, 0, 1)
        .unwrap();
    natural(seeded, issue_event(), "already has restricted balance");

    let mut exceeds = grant_world();
    exceeds.set_main_token_supply(MainTokenSupplyState {
        total_supply: 100,
        circulating_supply: 90,
        total_issued: 100,
        total_burned: 0,
    });
    natural(exceeds, issue_event(), "circulating exceeds total");

    for revoked in [false, true] {
        let (_, event) = terminal_event(revoked);
        natural(
            grant_world(),
            event,
            if revoked {
                "not found for revoke"
            } else {
                "not found for expiration"
            },
        );

        let (mut terminal, event) = terminal_event(revoked);
        terminal
            .append_event_for_test(WorldEventBody::Domain(event.clone()), None)
            .unwrap();
        natural(terminal, event, "already terminal");

        let (world, event) = terminal_event(revoked);
        let mut state = world.state().clone();
        state
            .main_token_balances
            .get_mut("alice")
            .unwrap()
            .restricted_starter_claim_balance = 39;
        natural(
            World::new_with_state(state),
            event.clone(),
            "balance insufficient",
        );

        let (world, event) = terminal_event(revoked);
        let mut state = world.state().clone();
        state.main_token_treasury_balances.insert(
            MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.into(),
            u64::MAX,
        );
        natural(
            World::new_with_state(state),
            event,
            "treasury balance overflow",
        );
    }
}

fn captured_restricted_event(world: &World, issued: bool) -> DomainEvent {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|entry| match &entry.body {
            WorldEventBody::Domain(
                event @ DomainEvent::RestrictedStarterClaimGrantIssued { .. },
            ) if issued => Some(event.clone()),
            WorldEventBody::Domain(
                event @ DomainEvent::RestrictedStarterClaimGrantRevoked { .. },
            ) if !issued => Some(event.clone()),
            _ => None,
        })
        .expect("restricted grant action event")
}

#[test]
fn issue_and_revoke_action_raw_state_and_published_roots_match() {
    let mut issue_base = grant_world();
    install_liveops_admin(&mut issue_base);
    let mut issue_action = issue_base.clone();
    issue_action.submit_action(Action::IssueRestrictedStarterClaimGrant {
        issuer_account_id: " liveops ".into(),
        beneficiary_account_id: " alice ".into(),
        amount: 40,
        issuance_reason: " qa ".into(),
        expires_at_epoch: 5,
    });
    issue_action.step().unwrap();
    let issued = captured_restricted_event(&issue_action, true);
    let mut issue_raw = issue_base;
    issue_raw.step().unwrap();
    issue_raw
        .append_event_for_test(WorldEventBody::Domain(issued), None)
        .unwrap();
    assert_eq!(issue_raw.state(), issue_action.state());
    assert_eq!(
        issue_raw.current_state_root_hash().unwrap(),
        issue_action.current_state_root_hash().unwrap()
    );

    let mut revoke_base = grant_world();
    install_liveops_admin(&mut revoke_base);
    revoke_base
        .append_event_for_test(WorldEventBody::Domain(issue_event()), None)
        .unwrap();
    let mut revoke_action = revoke_base.clone();
    revoke_action.submit_action(Action::RevokeRestrictedStarterClaimGrant {
        issuer_account_id: " liveops ".into(),
        beneficiary_account_id: " alice ".into(),
        revoke_reason: " policy ".into(),
    });
    revoke_action.step().unwrap();
    let revoked = captured_restricted_event(&revoke_action, false);
    let mut revoke_raw = revoke_base;
    revoke_raw.step().unwrap();
    revoke_raw
        .append_event_for_test(WorldEventBody::Domain(revoked), None)
        .unwrap();
    assert_eq!(revoke_raw.state(), revoke_action.state());
    assert_eq!(
        revoke_raw.current_state_root_hash().unwrap(),
        revoke_action.current_state_root_hash().unwrap()
    );
}
