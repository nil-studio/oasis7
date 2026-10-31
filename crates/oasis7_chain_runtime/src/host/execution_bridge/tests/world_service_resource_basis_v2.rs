//! Actual signed World component, not custody MFA/prepayment or production acceptance.
use super::*;
use oasis7::world_service::{
    private_goal_prepare::sign_goal_prepare, private_goal_read::publication,
    private_goal_store::PrivateGoalStore,
};
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::resource_basis as wire;
use std::sync::{Arc, Mutex};
use crate::world_service_api::private_goal::resource_component::adapter as quote_adapter;
fn preload(world: &mut oasis7_runtime::World) {
    use oasis7_runtime as r;
    world
        .set_location_anchor(r::LocationAnchorV1 {
            location_id: "resource-location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_agent_location_authority(r::AgentLocationAuthorityV1 {
            agent_id: "agent-a".into(),
            location_id: "resource-location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_factory_site_authority(r::FactorySiteAuthorityV1 {
            site_id: "resource-site".into(),
            location_id: "resource-location".into(),
            owner_agent_id: "agent-a".into(),
            authorized_agent_ids: vec![],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .unwrap();
    world
        .set_ledger_material_balance(r::MaterialLedgerId::agent("agent-a"), "iron_ore", 20)
        .unwrap();
    world
        .set_ledger_material_balance(r::MaterialLedgerId::site("resource-site"), "iron_ore", 0)
        .unwrap();
    world
        .set_agent_resource_balance(
            "agent-a",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            100,
        )
        .unwrap();
    let owner = world.current_agent_owner("agent-a").unwrap();
    world
        .install_test_native_delegation_grant(r::AgentDelegationGrantV1 {
            grant_id: "preinstalled-resource-grant".into(),
            source_id: owner.source_id,
            issuer_id: owner.player_id.clone(),
            owner_id: owner.player_id,
            organization_id: None,
            agent_id: "agent-a".into(),
            object_id: "agent:agent-a->site:resource-site:iron_ore".into(),
            action_kinds: vec!["TransferMaterial".into()],
            revision: 1,
            period_id: "resource-period".into(),
            valid_from_tick: 0,
            valid_until_tick: u64::MAX,
            limit_units: 1000,
            resource_kind: "electricity".into(),
            revoked: false,
        })
        .unwrap();
    // Publish the legitimate fixture mutations before the baseline is persisted.
    world.step().unwrap();
}
#[test]
fn resource_basis_v2_signed_full_goal_and_actual_commit_delivery_fence() {
    exercise(false);
}
#[test]
fn resource_basis_v2_actual_multihop_quote_complete_signed_envelope() {
    exercise(true);
}
fn routed_preload(world: &mut oasis7_runtime::World) {
    use oasis7_runtime::{Action, MaterialLedgerId as L, MaterialTransitPriority as P};
    for (from, to, distance) in [
        (L::agent("agent-a"), L::site("resource-relay"), 3000),
        (L::site("resource-relay"), L::site("resource-site"), 4000),
    ] {
        let before = world.journal().events.len();
        world.submit_action(Action::RegisterLogisticsRoute {
            requester_agent_id: "agent-a".into(),
            from_ledger: from,
            to_ledger: to,
            kind: "iron_ore".into(),
            distance_km: distance,
            priority: P::Standard,
            capacity_units: 9_007_199_254_740_993,
            tariff_electricity_per_unit: 2,
        });
        world.step().unwrap();
        let registered = world.journal().events[before..]
            .iter()
            .filter(|event| {
                matches!(
                    &event.body,
                    oasis7_runtime::WorldEventBody::Domain(
                        oasis7_runtime::DomainEvent::LogisticsRouteRegistered { .. }
                    )
                )
            })
            .count();
        assert_eq!(
            registered, 1,
            "fixed route registration stage must produce its real receipt"
        );
    }
    assert_eq!(world.state().logistics_routes.len(), 2);
    preload(world);
}
fn exercise(routed: bool) {
    let (dir, mut driver, world, owner, public) =
        canonical_agent_chat::driver_fixture_with_resource_preload(if routed {
            routed_preload
        } else {
            preload
        });
    let mut height = driver.execution_world.state().time;
    let create = game_task_tests::create(&world, &owner, "resource-component-task");
    height += 1;
    commit(&mut driver, height, create.clone());
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let store = PrivateGoalStore::open(
        &dir.canonicalize().unwrap().join("resource-private-goals"),
        [7; 32],
    )
    .unwrap();
    let content = GoalContentV1 {
        goal_text: "actual confidential resource component goal".into(),
        soft_constraints: vec!["no resource dispatch".into()],
    };
    let proof = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            task_id: c.task_id.clone(),
            agent_id: c.agent_id.clone(),
            owner_public_key: public.clone(),
            owner_binding: c.owner_binding.clone(),
            owner_generation: c.owner_generation,
            branch_id: c.branch_id.clone(),
            reorg_epoch: c.reorg_epoch,
            expected_task_revision: c.task_revision,
            expected_latest_goal_revision: c.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "resource-prepare".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&c.task_id, DecimalU64(1), &content).unwrap(),
            content,
        },
        &owner,
    )
    .unwrap();
    let prepared = store.prepare(&proof).unwrap();
    let publish = publication(&proof, &prepared, "resource-publish".into()).unwrap();
    height += 1;
    commit(
        &mut driver,
        height,
        request(
            &world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, &owner).unwrap(),
            ),
        ),
    );
    let delegate = sign_read_request("key", (), &hex::encode([73; 32]))
        .unwrap()
        .subject_public_key;
    let record = AgentSignerDelegationChangeV1 {
        world_id: world.world_id.clone(),
        branch_id: "main".into(),
        agent_id: "agent-a".into(),
        owner_binding: "owner-a".into(),
        agent_identity_generation: 1,
        generation: 1,
        delegate_public_key: delegate.clone(),
        revoked: false,
        nonce: 1,
    };
    height += 1;
    commit(
        &mut driver,
        height,
        request(
            &world,
            WorldServicePayloadV1::Delegation(
                sign_read_request("delegation", record.clone(), &owner).unwrap(),
            ),
        ),
    );
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    height += 1;
    commit(
        &mut driver,
        height,
        request(
            &world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "resource-bind".into(),
                        fence: GameTaskFenceV1::from_control(&c),
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "resource-executor".into(),
                            executor_public_key: delegate,
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let expected = wire::ResourceSignerRecordV1 {
        world_id: record.world_id,
        branch_id: record.branch_id,
        agent_id: record.agent_id,
        owner_binding: record.owner_binding,
        agent_identity_generation: DecimalU64(record.agent_identity_generation),
        generation: DecimalU64(record.generation),
        delegate_public_key: record.delegate_public_key,
        revoked: record.revoked,
        nonce: DecimalU64(record.nonce),
    };
    let selection = wire::ResourceBasisSelectionV1::Transfer {
        from: wire::ResourceLedgerV1::Agent {
            agent_id: "agent-a".into(),
        },
        to: wire::ResourceLedgerV1::Site {
            site_id: "resource-site".into(),
        },
        material: "iron_ore".into(),
        amount: wire::ResourceDecimalI64V1(5),
        distance_km: wire::ResourceDecimalI64V1(0),
        priority: oasis7_runtime::MaterialTransitPriority::Standard,
        routing: if routed {
            let routes = &driver.execution_world.state().logistics_routes;
            wire::ResourceRoutingV1::Routes {
                route_ids: [
                    oasis7_runtime::MaterialLedgerId::agent("agent-a"),
                    oasis7_runtime::MaterialLedgerId::site("resource-relay"),
                ]
                .iter()
                .map(|from| {
                    routes
                        .values()
                        .find(|r| &r.from_ledger == from)
                        .unwrap()
                        .route_id
                        .clone()
                })
                .collect(),
            }
        } else {
            wire::ResourceRoutingV1::Direct
        },
    };
    let q: wire::SignedResourceReadV1<_> = sign_read_request(
        wire::RESOURCE_GOAL_PATH_V2,
        wire::OwnerSetupFinalResourceReadV2 {
            api_version: 2,
            setup_id: "component-only-not-approved".into(),
            provision_id: "component-provision".into(),
            plan_digest: "cd".repeat(32),
            world: (&world).into(),
            expected_fence: GameTaskFenceV1::from_control(&control),
            expected_signer_delegation: expected,
            goal_reference: prepared.goal_ref.clone(),
            selection,
            observation_nonce: "de".repeat(32),
        },
        &owner,
    )
    .unwrap()
    .into();
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: public.clone(),
    };
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let before = serde_json::to_vec(&driver.execution_world.snapshot()).unwrap();
    let journal = driver.execution_world.journal().clone();
    let (status, payload) = private_goal_tests::http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        wire::RESOURCE_GOAL_PATH_V2,
        &q,
        "",
    );
    assert_eq!(status, 200);
    let response: wire::SignedResourceResponseV1<wire::OwnerSetupFinalResourceObservationV2> =
        serde_json::from_slice(&serde_json::to_vec(&payload).unwrap()).unwrap();
    wire::verify_resource_goal_response(&q, &response, &public).unwrap();
    let wire::ResourceBasisResolutionV1::Ready { fact } = &response.payload.resource else {
        panic!("genuine preinstalled grant and current material facts must resolve");
    };
    let wire::ResourceBasisQuoteV1::Transfer(transfer) = &fact.quote else {
        panic!("selected actual Transfer quote");
    };
    let raw = driver
        .execution_world
        .native_resource_basis(
            "agent-a",
            &oasis7_runtime::ResourceBasisSelectionV1::Transfer {
                from: oasis7_runtime::MaterialLedgerId::agent("agent-a"),
                to: oasis7_runtime::MaterialLedgerId::site("resource-site"),
                material: "iron_ore".into(),
                amount: 5,
                distance_km: 0,
                priority: oasis7_runtime::MaterialTransitPriority::Standard,
                routing: if routed {
                    let wire::ResourceBasisSelectionV1::Transfer {
                        routing: wire::ResourceRoutingV1::Routes { route_ids },
                        ..
                    } = &q.request.selection
                    else {
                        unreachable!()
                    };
                    oasis7_runtime::ResourceBasisRoutingV1::Routes {
                        route_ids: route_ids.clone(),
                    }
                } else {
                    oasis7_runtime::ResourceBasisRoutingV1::Direct
                },
            },
        )
        .unwrap();
    let oasis7_runtime::ResourceBasisQuoteV1::Transfer(native) = raw.quote else {
        unreachable!()
    };
    assert_eq!(
        transfer.quote.ready_at.0,
        fact.quoted_at_tick
            .0
            .saturating_add(1)
            .saturating_add(transfer.quote.ticks_until_arrival.0)
    );
    assert_eq!(
        quote_adapter::transfer_native(transfer.quote.clone()).unwrap(),
        native.quote,
        "all 28 actual logistics fields must roundtrip"
    );
    assert_eq!(transfer.quote.ready_at.0, native.quote.ready_at);
    if routed {
        assert_eq!(transfer.quote.route_ids.len(), 2);
        assert!(transfer.quote.path_id.is_some());
        assert_eq!(transfer.quote.route_ids, native.quote.route_ids);
        assert_eq!(
            transfer.quote.tariff_electricity_total.0,
            native.quote.tariff_electricity_total
        );
        assert!(transfer.quote.tariff_electricity_total.0 > 0);
        assert_eq!(transfer.quote.distance_km.0, 7000);
        for id in &transfer.quote.route_ids {
            assert!(fact.derived_resource_refs.contains(id));
        }
    }
    let encoded = wire::encode_native_resource_envelope(response.clone()).unwrap();
    assert!(encoded.len() <= wire::MAX_RESOURCE_ENVELOPE_BYTES);
    let delivered: wire::NativeResourceEnvelopeV1<wire::OwnerSetupFinalResourceObservationV2> =
        serde_json::from_slice(&encoded).unwrap();
    assert!(
        delivered.world_response == response,
        "no quote/fact/signature field may be clipped by final envelope"
    );
    wire::verify_resource_goal_response(&q, &delivered.world_response, &public).unwrap();
    // Correct World signatures cannot make the old off-by-one quote valid.
    let mut wrong = response.payload.clone();
    let wire::ResourceBasisResolutionV1::Ready { fact: bad_fact } = &mut wrong.resource else {
        unreachable!()
    };
    let wire::ResourceBasisQuoteV1::Transfer(bad_quote) = &mut bad_fact.quote else {
        unreachable!()
    };
    bad_quote.quote.ready_at = wire::DecimalU64(
        bad_fact
            .quoted_at_tick
            .0
            .saturating_add(bad_quote.quote.ticks_until_arrival.0),
    );
    let resign: wire::SignedResourceResponseV1<_> = sign_service_response(
        wire::RESOURCE_GOAL_PATH_V2,
        response.request_digest.clone(),
        wrong,
        &owner,
    )
    .unwrap()
    .into();
    assert!(
        wire::verify_resource_goal_response(&q, &resign, &public).is_err(),
        "correct signature must not excuse old ready_at semantics"
    );
    // Pure Runtime arithmetic/typed-fact checks, not claims of a fresh max-tick
    // canonical read: new_with_state constructs the authoritative fixture root.
    for tick in [u64::MAX - 1, u64::MAX] {
        let mut state = driver.execution_world.state().clone();
        state.time = tick;
        let mut extreme = oasis7_runtime::World::new_with_state(state);
        // new_with_state carries only WorldState, not World.cognition's grant
        // ledger. Install the exact actual quote grant via the legal test-only
        // bootstrap; retain production source/object/window/budget validation.
        extreme
            .install_test_native_delegation_grant(native.authorization.grant.clone())
            .unwrap();
        let native_selection = quote_adapter::selection(&q.request.selection).unwrap();
        let raw = extreme
            .native_resource_basis("agent-a", &native_selection)
            .unwrap();
        let typed = quote_adapter::fact(&raw, &q.request.selection).unwrap();
        assert!(
            matches!(quote_adapter::resolution(&extreme, "agent-a", &q.request.selection).unwrap(), wire::ResourceBasisResolutionV1::Ready { fact } if *fact == typed)
        );
        let mut owner_at_tick = response.payload.owner.clone();
        owner_at_tick.logical_tick = wire::DecimalU64(tick);
        typed
            .validate(&q.request.selection, &owner_at_tick)
            .unwrap();
        let wire::ResourceBasisQuoteV1::Transfer(t) = typed.quote else {
            unreachable!()
        };
        assert_eq!(t.quote.ready_at.0, u64::MAX);
    }

    assert_eq!(
        response
            .payload
            .current_goal
            .active_goal
            .as_ref()
            .unwrap()
            .goal_ref,
        prepared.goal_ref
    );
    assert_eq!(
        serde_json::to_vec(&driver.execution_world.snapshot()).unwrap(),
        before
    );
    assert_eq!(driver.execution_world.journal(), &journal);
    let other = sign_read_request("key", (), &hex::encode([74; 32]))
        .unwrap()
        .subject_public_key;
    assert!(wire::verify_resource_goal_response(&q, &response, &other).is_err());
    let original_fence = GameTaskFenceV1::from_control(&control);
    let (status, _) = private_goal_tests::http_with_hook(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        wire::RESOURCE_GOAL_PATH_V2,
        &q,
        "",
        || {
            height += 1;
            commit(&mut driver, height, create);
        },
    );
    assert_eq!(
        GameTaskFenceV1::from_control(
            &game_task::current_control(&driver.execution_world, "agent-a")
                .unwrap()
                .unwrap()
        ),
        original_fence
    );
    assert_eq!(
        status, 409,
        "a real new canonical commit must be rejected even with unchanged owner/task fence"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
