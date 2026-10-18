use super::super::{Action, ActionEnvelope, CausedBy, DomainEvent, WorldEventBody};
use super::{World, WorldError};
use oasis7_agent_api::world_action::ResourceKind;

const OWNER: &str = "retirement-owner";
const BIDDER: &str = "retirement-bidder";

fn dispatch(world: &mut World, action: Action) -> Result<bool, WorldError> {
    world.try_apply_runtime_module_action(&ActionEnvelope {
        committed_recipe_origin: None,
        id: 991,
        action,
    })
}

fn register_agent(world: &mut World, id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: id.into(),
        pos: crate::tests::pos(0, 0),
    });
    world.step().unwrap();
    world
        .set_agent_resource_balance(id, ResourceKind::Electricity, 100_000)
        .unwrap();
}

fn fixture() -> (World, String) {
    let mut world = World::new();
    register_agent(&mut world, OWNER);
    register_agent(&mut world, BIDDER);
    let bytes = b"atomic-artifact-retirement";
    let hash = crate::util::sha256_hex(bytes);
    world.register_module_artifact(hash.clone(), bytes).unwrap();
    world
        .state
        .module_artifact_owners
        .insert(hash.clone(), OWNER.into());
    world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactListed {
                seller_agent_id: OWNER.into(),
                wasm_hash: hash.clone(),
                price_kind: ResourceKind::Electricity,
                price_amount: 10,
                order_id: 41,
                fee_kind: ResourceKind::Electricity,
                fee_amount: 0,
            },
            3,
        )
        .unwrap();
    world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactBidPlaced {
                bidder_agent_id: BIDDER.into(),
                wasm_hash: hash.clone(),
                order_id: 42,
                price_kind: ResourceKind::Electricity,
                price_amount: 9,
            },
            3,
        )
        .unwrap();
    world.load_module(&hash).unwrap();
    world.state.material_ledgers.clear();
    world
        .state
        .materials
        .insert("legacy-retirement-material".into(), 17);
    (world, hash)
}

fn assert_world_unchanged(world: &World, expected: &World) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(
        world.current_state_root_hash(),
        expected.current_state_root_hash()
    );
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
    );
    assert_eq!(
        world.runtime_backpressure_stats(),
        expected.runtime_backpressure_stats()
    );
    assert_eq!(
        world.tick_consensus_records(),
        expected.tick_consensus_records()
    );
    assert_eq!(world.module_artifacts, expected.module_artifacts);
    assert_eq!(world.module_artifact_bytes, expected.module_artifact_bytes);
    assert_eq!(world.module_cache, expected.module_cache);
    assert_eq!(world.module_registry, expected.module_registry);
}

fn assert_raw_tail_failure_unchanged(mut world: World, event: DomainEvent) {
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(WorldEventBody::Domain(event), None)
        .expect_err("retirement event must honor postprepare publication failure");
    assert!(
        format!("{error:?}").contains("injected"),
        "unexpected error: {error:?}"
    );
    assert_world_unchanged(&world, &before);
}

#[test]
fn raw_delist_tail_failure_preserves_state_and_publication() {
    let (world, hash) = fixture();
    assert_raw_tail_failure_unchanged(
        world,
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: OWNER.into(),
            wasm_hash: hash,
            order_id: Some(41),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 1,
        },
    );
}

#[test]
fn raw_bid_cancel_tail_failure_preserves_state_and_publication() {
    let (world, hash) = fixture();
    assert_raw_tail_failure_unchanged(
        world,
        DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id: BIDDER.into(),
            wasm_hash: hash,
            order_id: 42,
            reason: "bidder-cancelled".into(),
        },
    );
}

#[test]
fn raw_destroy_tail_failure_preserves_state_and_publication() {
    let (world, hash) = fixture();
    assert_raw_tail_failure_unchanged(
        world,
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "raw-retirement".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 1,
        },
    );
}

#[test]
fn destroy_action_tail_failure_preserves_state_bytes_and_cache() {
    let (mut world, hash) = fixture();
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    let error = dispatch(
        &mut world,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "action-retirement".into(),
        },
    )
    .expect_err("destroy action must honor postprepare publication failure");
    assert!(
        format!("{error:?}").contains("injected"),
        "unexpected error: {error:?}"
    );
    assert_world_unchanged(&world, &before);
}

fn publish(world: &mut World, event: DomainEvent) {
    world
        .append_event(WorldEventBody::Domain(event), None)
        .unwrap();
}

fn assert_published_root_and_replay(world: &World, baseline: crate::Snapshot) {
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap()
    );
    let replay = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replay.state, world.state);
    assert_eq!(
        replay.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
}

#[test]
fn raw_teardown_success_preserves_artifact_sidecars_and_replays_state() {
    let (mut world, hash) = fixture();
    let baseline = world.snapshot();
    let owner_mailbox = world.state.agents[OWNER].mailbox.len();
    let bidder_mailbox = world.state.agents[BIDDER].mailbox.len();
    let artifacts = world.module_artifacts.clone();
    let bytes = world.module_artifact_bytes.clone();
    let cache = world.module_cache.clone();
    publish(
        &mut world,
        DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id: BIDDER.into(),
            wasm_hash: hash.clone(),
            order_id: 42,
            reason: " raw cancel reason ".into(),
        },
    );
    publish(
        &mut world,
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: OWNER.into(),
            wasm_hash: hash.clone(),
            order_id: Some(41),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
    );
    publish(
        &mut world,
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash.clone(),
            reason: " raw destroy reason ".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
    );
    assert!(!world.state.module_artifact_owners.contains_key(&hash));
    assert!(!world.state.module_artifact_listings.contains_key(&hash));
    assert!(!world.state.module_artifact_bids.contains_key(&hash));
    assert_eq!(world.module_artifacts, artifacts);
    assert_eq!(world.module_artifact_bytes, bytes);
    assert_eq!(world.module_cache, cache);
    assert_eq!(world.state.agents[BIDDER].mailbox.len(), bidder_mailbox + 1);
    assert_eq!(world.state.agents[OWNER].mailbox.len(), owner_mailbox + 2);
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactDestroyed { reason, .. }) =
        &world.journal().events.last().unwrap().body
    else {
        panic!("expected raw destroyed event");
    };
    assert_eq!(reason, " raw destroy reason ");
    assert_published_root_and_replay(&world, baseline);
}

#[test]
fn destroy_action_success_removes_sidecars_charges_and_routes_exact_reason() {
    let (mut world, hash) = fixture();
    world.set_module_cache_max(7);
    let unrelated_bytes = b"unrelated-cached-artifact";
    let unrelated_hash = crate::util::sha256_hex(unrelated_bytes);
    world
        .register_module_artifact(unrelated_hash.clone(), unrelated_bytes)
        .unwrap();
    world.load_module(&unrelated_hash).unwrap();
    let max_cached = world.module_cache.max_cached_modules();
    let registry = world.module_registry.clone();
    let instances = world.state.module_instances.clone();
    let schedules = world.module_tick_schedule.clone();
    let before_balance = world
        .agent_resource_balance(OWNER, ResourceKind::Electricity)
        .unwrap();
    let before_treasury = world
        .state
        .resources
        .get(&ResourceKind::Electricity)
        .copied()
        .unwrap_or(0);
    let before_mailbox = world.state.agents[OWNER].mailbox.len();
    let baseline = world.snapshot();
    let reason = "  preserve retirement reason bytes  ";
    dispatch(
        &mut world,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash.clone(),
            reason: reason.into(),
        },
    )
    .unwrap();
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactDestroyed {
        reason: published,
        fee_amount,
        ..
    }) = &world.journal().events.last().unwrap().body
    else {
        panic!("expected destroyed event");
    };
    assert_eq!(published, reason);
    assert_eq!(
        world.journal().events.last().unwrap().caused_by,
        Some(CausedBy::Action(991))
    );
    assert_eq!(
        world.agent_resource_balance(OWNER, ResourceKind::Electricity),
        Ok(before_balance - fee_amount)
    );
    assert_eq!(
        world.state.resources[&ResourceKind::Electricity],
        before_treasury + fee_amount
    );
    assert_eq!(world.state.agents[OWNER].mailbox.len(), before_mailbox + 1);
    assert!(!world.module_artifacts.contains(&hash));
    assert!(!world.module_artifact_bytes.contains_key(&hash));
    assert!(world.module_artifacts.contains(&unrelated_hash));
    assert!(world.module_artifact_bytes.contains_key(&unrelated_hash));
    assert_eq!(world.module_cache_len(), 0);
    assert_eq!(world.module_cache.max_cached_modules(), max_cached);
    assert_eq!(world.module_registry, registry);
    assert_eq!(world.state.module_instances, instances);
    assert_eq!(world.module_tick_schedule, schedules);
    assert_published_root_and_replay(&world, baseline);
}

#[test]
fn destroy_action_retry_after_tail_failure_commits_once() {
    let (mut world, hash) = fixture();
    let before_balance = world
        .agent_resource_balance(OWNER, ResourceKind::Electricity)
        .unwrap();
    let before_events = world.journal().events.len();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(
        dispatch(
            &mut world,
            Action::DestroyModuleArtifact {
                owner_agent_id: OWNER.into(),
                wasm_hash: hash.clone(),
                reason: "retry retirement".into(),
            },
        )
        .is_err()
    );
    dispatch(
        &mut world,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "retry retirement".into(),
        },
    )
    .unwrap();
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactDestroyed { fee_amount, .. }) =
        &world.journal().events.last().unwrap().body
    else {
        panic!("expected destroyed event");
    };
    assert_eq!(world.journal().events.len(), before_events + 1);
    assert_eq!(
        world.agent_resource_balance(OWNER, ResourceKind::Electricity),
        Ok(before_balance - fee_amount)
    );
}

#[test]
fn prepared_retirement_rejects_wrong_variant_or_hash_without_mutation() {
    let (mut world, hash) = fixture();
    for event in [
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: OWNER.into(),
            wasm_hash: "wrong-retirement-hash".into(),
            reason: "wrong hash".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: OWNER.into(),
            wasm_hash: hash.clone(),
            order_id: Some(41),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
    ] {
        let retirement = world.prepare_module_artifact_retirement(hash.clone());
        let before = world.clone();
        world
            .append_module_artifact_retirement(event, None, retirement)
            .expect_err("retirement sidecar must remain bound to destroyed hash");
        assert_world_unchanged(&world, &before);
    }
}

#[test]
fn raw_teardown_retention_advances_event_era() {
    let (mut world, hash) = fixture();
    world.runtime_memory_limits.max_journal_events = 2;
    world.next_event_id = u64::MAX;
    world.next_event_id_era = 20;
    publish(
        &mut world,
        DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id: BIDDER.into(),
            wasm_hash: hash,
            order_id: 42,
            reason: "retention".into(),
        },
    );
    assert_eq!(world.next_event_id_era, 21);
    assert_eq!(world.journal().events.len(), 2);
    assert!(world.runtime_backpressure_stats.journal_events_evicted > 0);
}

fn last_rejection(world: &World) -> String {
    let WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) =
        &world.journal().events.last().unwrap().body
    else {
        panic!("expected action rejection");
    };
    format!("{reason:?}")
}

#[test]
fn destroy_action_preserves_validation_priority() {
    let mut missing_actor = World::new();
    dispatch(
        &mut missing_actor,
        Action::DestroyModuleArtifact {
            owner_agent_id: "missing".into(),
            wasm_hash: "missing".into(),
            reason: "".into(),
        },
    )
    .unwrap();
    assert!(last_rejection(&missing_actor).contains("AgentNotFound"));

    let (mut blank_reason, hash) = fixture();
    dispatch(
        &mut blank_reason,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "   ".into(),
        },
    )
    .unwrap();
    assert!(last_rejection(&blank_reason).contains("reason is empty"));

    let mut missing_artifact = World::new();
    register_agent(&mut missing_artifact, OWNER);
    dispatch(
        &mut missing_artifact,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: "missing".into(),
            reason: "valid".into(),
        },
    )
    .unwrap();
    assert!(last_rejection(&missing_artifact).contains("missing artifact"));

    let (mut missing_owner, hash) = fixture();
    missing_owner.state.module_artifact_owners.remove(&hash);
    dispatch(
        &mut missing_owner,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "valid".into(),
        },
    )
    .unwrap();
    assert!(last_rejection(&missing_owner).contains("owner missing"));

    let (mut wrong_owner, hash) = fixture();
    wrong_owner
        .state
        .module_artifact_owners
        .insert(hash.clone(), BIDDER.into());
    dispatch(
        &mut wrong_owner,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "valid".into(),
        },
    )
    .unwrap();
    assert!(last_rejection(&wrong_owner).contains("does not own"));

    let (mut unfunded, hash) = fixture();
    unfunded
        .set_agent_resource_balance(OWNER, ResourceKind::Electricity, 0)
        .unwrap();
    dispatch(
        &mut unfunded,
        Action::DestroyModuleArtifact {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "valid".into(),
        },
    )
    .unwrap();
    assert!(last_rejection(&unfunded).contains("InsufficientResource"));
}

fn assert_raw_error_unchanged(mut world: World, event: DomainEvent, needle: &str) {
    let before = world.clone();
    let error = world.state.apply_domain_event(&event, 9).unwrap_err();
    assert!(
        format!("{error:?}").contains(needle),
        "unexpected error: {error:?}"
    );
    assert_world_unchanged(&world, &before);
}

#[test]
fn delist_none_and_mismatch_preserve_order_book_contract() {
    let (mut none, hash) = fixture();
    none.state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactDelisted {
                seller_agent_id: OWNER.into(),
                wasm_hash: hash.clone(),
                order_id: None,
                fee_kind: ResourceKind::Electricity,
                fee_amount: 0,
            },
            9,
        )
        .unwrap();
    assert!(!none.state.module_artifact_listings.contains_key(&hash));
    assert!(none.state.module_artifact_bids.contains_key(&hash));

    let (mismatch, hash) = fixture();
    assert_raw_error_unchanged(
        mismatch,
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: OWNER.into(),
            wasm_hash: hash,
            order_id: Some(999),
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
        "order mismatch",
    );
}

#[test]
fn cancel_removes_all_duplicates_and_preserves_unrelated_bids_without_actor() {
    let (mut world, hash) = fixture();
    for order_id in [42, 43] {
        world
            .state
            .apply_domain_event(
                &DomainEvent::ModuleArtifactBidPlaced {
                    bidder_agent_id: BIDDER.into(),
                    wasm_hash: hash.clone(),
                    order_id,
                    price_kind: ResourceKind::Electricity,
                    price_amount: 8,
                },
                4,
            )
            .unwrap();
    }
    world.state.agents.remove(BIDDER);
    world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactBidCancelled {
                bidder_agent_id: BIDDER.into(),
                wasm_hash: hash.clone(),
                order_id: 42,
                reason: "missing actor accepted".into(),
            },
            9,
        )
        .unwrap();
    let remaining = &world.state.module_artifact_bids[&hash];
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].order_id, 43);

    world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactBidCancelled {
                bidder_agent_id: BIDDER.into(),
                wasm_hash: hash.clone(),
                order_id: 43,
                reason: "remove final".into(),
            },
            10,
        )
        .unwrap();
    assert!(!world.state.module_artifact_bids.contains_key(&hash));
}

#[test]
fn raw_teardown_validation_priority_is_atomic() {
    let (missing_listing, hash) = fixture();
    let missing_hash = format!("missing-{hash}");
    assert_raw_error_unchanged(
        missing_listing,
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: "wrong".into(),
            wasm_hash: missing_hash,
            order_id: Some(999),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "listing missing",
    );
    let (seller_first, hash) = fixture();
    assert_raw_error_unchanged(
        seller_first,
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: BIDDER.into(),
            wasm_hash: hash,
            order_id: Some(999),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "seller mismatch",
    );
    let (mut order_first, hash) = fixture();
    order_first.state.module_artifact_owners.remove(&hash);
    assert_raw_error_unchanged(
        order_first,
        DomainEvent::ModuleArtifactDelisted {
            seller_agent_id: OWNER.into(),
            wasm_hash: hash,
            order_id: Some(999),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "order mismatch",
    );

    let (missing_bids, hash) = fixture();
    let missing_hash = format!("missing-{hash}");
    assert_raw_error_unchanged(
        missing_bids,
        DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id: BIDDER.into(),
            wasm_hash: missing_hash,
            order_id: 42,
            reason: "missing".into(),
        },
        "bids missing",
    );
    let (missing_target, hash) = fixture();
    assert_raw_error_unchanged(
        missing_target,
        DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id: BIDDER.into(),
            wasm_hash: hash,
            order_id: 999,
            reason: "missing".into(),
        },
        "target not found",
    );

    let (mut blank_reason, hash) = fixture();
    blank_reason.state.module_artifact_owners.remove(&hash);
    assert_raw_error_unchanged(
        blank_reason,
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: "wrong".into(),
            wasm_hash: hash,
            reason: "   ".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "reason cannot be empty",
    );
    let (mut missing_owner, hash) = fixture();
    missing_owner.state.module_artifact_owners.remove(&hash);
    assert_raw_error_unchanged(
        missing_owner,
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: "wrong".into(),
            wasm_hash: hash,
            reason: "valid".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "owner missing",
    );
    let (wrong_owner, hash) = fixture();
    assert_raw_error_unchanged(
        wrong_owner,
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: BIDDER.into(),
            wasm_hash: hash,
            reason: "valid".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "owner mismatch",
    );
    let (negative_fee, hash) = fixture();
    assert_raw_error_unchanged(
        negative_fee,
        DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id: OWNER.into(),
            wasm_hash: hash,
            reason: "valid".into(),
            fee_kind: ResourceKind::Electricity,
            fee_amount: -1,
        },
        "fee must be >= 0",
    );
}
