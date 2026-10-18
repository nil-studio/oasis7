use super::super::{Action, ActionEnvelope, DomainEvent};
use super::World;
use oasis7_agent_api::world_action::ResourceKind;

fn fixture() -> World {
    let mut world = World::new();
    for id in ["seller", "buyer"] {
        world.submit_action(Action::RegisterAgent {
            agent_id: id.into(),
            pos: crate::tests::pos(0, 0),
        });
        world.step().unwrap();
        world
            .set_agent_resource_balance(id, ResourceKind::Electricity, 1000)
            .unwrap();
    }
    world
        .state
        .module_artifact_owners
        .insert("market-hash".into(), "seller".into());
    world
        .set_agent_resource_balance("seller", ResourceKind::Data, 100)
        .unwrap();
    world.module_artifacts.insert("market-hash".into());
    world
}

fn listing() -> DomainEvent {
    DomainEvent::ModuleArtifactListed {
        seller_agent_id: "seller".into(),
        wasm_hash: "market-hash".into(),
        price_kind: ResourceKind::Electricity,
        price_amount: 10,
        order_id: 1,
        fee_kind: ResourceKind::Electricity,
        fee_amount: 0,
    }
}

fn bid() -> DomainEvent {
    DomainEvent::ModuleArtifactBidPlaced {
        bidder_agent_id: "buyer".into(),
        wasm_hash: "market-hash".into(),
        order_id: 2,
        price_kind: ResourceKind::Electricity,
        price_amount: 10,
    }
}

fn sale(bid_order_id: Option<u64>) -> DomainEvent {
    DomainEvent::ModuleArtifactSaleCompleted {
        buyer_agent_id: "buyer".into(),
        seller_agent_id: "seller".into(),
        wasm_hash: "market-hash".into(),
        price_kind: ResourceKind::Electricity,
        price_amount: 10,
        sale_id: 1,
        listing_order_id: Some(1),
        bid_order_id,
    }
}

fn assert_rejected_sale_unchanged(mut world: World, bid_id: Option<u64>, reason: &str) {
    let before = world.state.clone();
    let error = world
        .state
        .apply_domain_event(&sale(bid_id), 99)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains(reason),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        world.state, before,
        "rejected sale must preserve every state field"
    );
}

#[test]
fn sale_missing_buyer_preserves_seller() {
    let mut world = fixture();
    world.state.apply_domain_event(&listing(), 10).unwrap();
    world.state.agents.remove("buyer");
    assert_rejected_sale_unchanged(world, None, "AgentNotFound");
}

#[test]
fn sale_insufficient_balance_preserves_both_agents() {
    let mut world = fixture();
    world.state.apply_domain_event(&listing(), 10).unwrap();
    world
        .set_agent_resource_balance("buyer", ResourceKind::Electricity, 0)
        .unwrap();
    assert_rejected_sale_unchanged(world, None, "buyer debit failed");
}

#[test]
fn sale_missing_bid_preserves_balances_and_ownership() {
    let mut world = fixture();
    world.state.apply_domain_event(&listing(), 10).unwrap();
    assert_rejected_sale_unchanged(world, Some(2), "bid missing");
}

#[test]
fn sale_wrong_bid_preserves_balances_and_ownership() {
    let mut world = fixture();
    world.state.apply_domain_event(&listing(), 10).unwrap();
    world.state.apply_domain_event(&bid(), 10).unwrap();
    assert_rejected_sale_unchanged(world, Some(3), "bid not found");
}

fn assert_match_tail_unchanged(list: bool) {
    let mut world = fixture();
    world
        .state
        .apply_domain_event(&if list { bid() } else { listing() }, 10)
        .unwrap();
    let before = world.snapshot();
    let journal = world.journal().clone();
    let event_cursor = (world.next_event_id, world.next_event_id_era);
    let consensus = world.tick_consensus_records().to_vec();
    world.fail_append_after_publication_prepare_on_nth_for_test(2);
    let action = if list {
        Action::ListModuleArtifactForSale {
            seller_agent_id: "seller".into(),
            wasm_hash: "market-hash".into(),
            price_kind: ResourceKind::Electricity,
            price_amount: 10,
        }
    } else {
        Action::PlaceModuleArtifactBid {
            bidder_agent_id: "buyer".into(),
            wasm_hash: "market-hash".into(),
            price_kind: ResourceKind::Electricity,
            price_amount: 10,
        }
    };
    let error = world
        .try_apply_runtime_module_action(&ActionEnvelope {
            committed_recipe_origin: None,
            id: 950,
            action,
        })
        .expect_err("matched sale must honor the second publication preparation failpoint");
    assert!(
        format!("{error:?}").contains("injected"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        world.snapshot(),
        before,
        "order and immediate sale must commit together"
    );
    assert_eq!(world.journal(), &journal);
    assert_eq!((world.next_event_id, world.next_event_id_era), event_cursor);
    assert_eq!(world.tick_consensus_records(), consensus.as_slice());
}

#[test]
fn list_immediate_sale_tail_failure_preserves_whole_batch() {
    assert_match_tail_unchanged(true);
}

#[test]
fn bid_immediate_sale_tail_failure_preserves_whole_batch() {
    assert_match_tail_unchanged(false);
}

fn publish(world: &mut World, event: DomainEvent) {
    world
        .append_event(crate::WorldEventBody::Domain(event), None)
        .unwrap();
}

fn check_roots(world: &World, baseline: crate::Snapshot) {
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

fn market_action(list: bool) -> ActionEnvelope {
    ActionEnvelope {
        committed_recipe_origin: None,
        id: 951,
        action: if list {
            Action::ListModuleArtifactForSale {
                seller_agent_id: "seller".into(),
                wasm_hash: "market-hash".into(),
                price_kind: ResourceKind::Data,
                price_amount: 10,
            }
        } else {
            Action::PlaceModuleArtifactBid {
                bidder_agent_id: "buyer".into(),
                wasm_hash: "market-hash".into(),
                price_kind: ResourceKind::Data,
                price_amount: 10,
            }
        },
    }
}

fn data_event(mut event: DomainEvent) -> DomainEvent {
    match &mut event {
        DomainEvent::ModuleArtifactListed { price_kind, .. }
        | DomainEvent::ModuleArtifactBidPlaced { price_kind, .. }
        | DomainEvent::ModuleArtifactSaleCompleted { price_kind, .. } => {
            *price_kind = ResourceKind::Data
        }
        _ => unreachable!(),
    }
    event
}

#[test]
fn market_composite_retry_merges_fee_credit_mailboxes_and_replays() {
    for list in [false, true] {
        let mut world = fixture();
        world
            .set_agent_resource_balance("seller", ResourceKind::Data, 100)
            .unwrap();
        world
            .set_agent_resource_balance("buyer", ResourceKind::Data, 100)
            .unwrap();
        world
            .state
            .apply_domain_event(&data_event(if list { bid() } else { listing() }), 10)
            .unwrap();
        let baseline = world.snapshot();
        let seller_mail = world.state.agents["seller"].mailbox.len();
        let buyer_mail = world.state.agents["buyer"].mailbox.len();
        let journal_len = world.journal().events.len();
        world.fail_append_after_publication_prepare_on_nth_for_test(2);
        assert!(
            world
                .try_apply_runtime_module_action(&market_action(list))
                .is_err()
        );
        assert_eq!(world.snapshot(), baseline);
        world
            .try_apply_runtime_module_action(&market_action(list))
            .unwrap();
        assert_eq!(world.journal().events.len(), journal_len + 2);
        assert_eq!(
            world
                .agent_resource_balance("seller", ResourceKind::Data)
                .unwrap(),
            if list { 109 } else { 110 }
        );
        assert_eq!(
            world
                .agent_resource_balance("buyer", ResourceKind::Data)
                .unwrap(),
            90
        );
        assert_eq!(
            world.state.agents["seller"].mailbox.len(),
            seller_mail + usize::from(list)
        );
        assert_eq!(
            world.state.agents["buyer"].mailbox.len(),
            buyer_mail + if list { 1 } else { 2 }
        );
        assert_eq!(world.state.module_artifact_owners["market-hash"], "buyer");
        assert!(
            !world
                .state
                .module_artifact_listings
                .contains_key("market-hash")
        );
        assert!(!world.state.module_artifact_bids.contains_key("market-hash"));
        check_roots(&world, baseline);
    }
}

#[test]
fn market_raw_bid_deletion_preserves_unrelated_and_optional_references() {
    for mode in 0..3 {
        let mut world = fixture();
        world.state.apply_domain_event(&listing(), 10).unwrap();
        world.state.apply_domain_event(&bid(), 10).unwrap();
        world.state.apply_domain_event(&bid(), 10).unwrap();
        if mode == 1 {
            let mut other = bid();
            if let DomainEvent::ModuleArtifactBidPlaced { order_id, .. } = &mut other {
                *order_id = 3;
            }
            world.state.apply_domain_event(&other, 10).unwrap();
        }
        let baseline = world.snapshot();
        publish(&mut world, sale(if mode == 2 { None } else { Some(2) }));
        match mode {
            0 => assert!(!world.state.module_artifact_bids.contains_key("market-hash")),
            1 => {
                let remaining = &world.state.module_artifact_bids["market-hash"];
                assert_eq!(remaining.len(), 1);
                assert_eq!(remaining[0].order_id, 3);
            }
            _ => assert_eq!(world.state.module_artifact_bids["market-hash"].len(), 2),
        }
        check_roots(&world, baseline);
    }
}

#[test]
fn market_raw_publication_faults_preserve_legacy_materials_and_all_world_fields() {
    for event in [listing(), bid(), sale(None)] {
        let mut world = fixture();
        if matches!(event, DomainEvent::ModuleArtifactSaleCompleted { .. }) {
            world.state.apply_domain_event(&listing(), 10).unwrap();
        }
        world.state.materials.insert("legacy".into(), 7);
        world.state.material_ledgers.clear();
        let baseline = world.snapshot();
        let journal = world.journal().clone();
        let cursor = (world.next_event_id, world.next_event_id_era);
        let consensus = world.tick_consensus_records().to_vec();
        world.fail_next_append_after_publication_prepare_for_test();
        let error = world
            .append_event(crate::WorldEventBody::Domain(event), None)
            .unwrap_err();
        assert!(format!("{error:?}").contains("injected"));
        assert_eq!(world.snapshot(), baseline);
        assert_eq!(world.journal(), &journal);
        assert_eq!((world.next_event_id, world.next_event_id_era), cursor);
        assert_eq!(world.tick_consensus_records(), consensus.as_slice());
    }
}

#[test]
fn market_sale_rejection_priority_and_credit_overflow_are_atomic() {
    for mode in 0..3 {
        let mut world = fixture();
        world.state.apply_domain_event(&listing(), 10).unwrap();
        let mut event = sale(Some(99));
        let reason = match mode {
            0 => {
                if let DomainEvent::ModuleArtifactSaleCompleted {
                    buyer_agent_id,
                    price_amount,
                    ..
                } = &mut event
                {
                    *buyer_agent_id = "seller".into();
                    *price_amount = 0;
                }
                "cannot be the same"
            }
            1 => {
                world
                    .set_agent_resource_balance("buyer", ResourceKind::Electricity, 0)
                    .unwrap();
                "buyer debit failed"
            }
            _ => {
                world
                    .set_agent_resource_balance("seller", ResourceKind::Electricity, i64::MAX)
                    .unwrap();
                "seller credit failed"
            }
        };
        let before = world.state.clone();
        let error = world.state.apply_domain_event(&event, 99).unwrap_err();
        assert!(format!("{error:?}").contains(reason), "{error:?}");
        assert_eq!(world.state, before);
    }
}

#[test]
fn market_raw_zero_ids_saturation_and_unfunded_bid_remain_accepted() {
    let mut world = fixture();
    world
        .set_agent_resource_balance("buyer", ResourceKind::Electricity, 0)
        .unwrap();
    let baseline = world.snapshot();
    let mut event = bid();
    if let DomainEvent::ModuleArtifactBidPlaced {
        wasm_hash,
        order_id,
        ..
    } = &mut event
    {
        *wasm_hash = "unknown".into();
        *order_id = u64::MAX;
    }
    publish(&mut world, event);
    assert_eq!(world.state.next_module_market_order_id, u64::MAX);
    assert_eq!(
        world
            .agent_resource_balance("buyer", ResourceKind::Electricity)
            .unwrap(),
        0
    );
    let mut event = listing();
    if let DomainEvent::ModuleArtifactListed { order_id, .. } = &mut event {
        *order_id = 0;
    }
    publish(&mut world, event);
    assert_eq!(world.state.next_module_market_order_id, u64::MAX);
    check_roots(&world, baseline);
    for id in [0, u64::MAX] {
        let mut world = fixture();
        world.state.apply_domain_event(&listing(), 10).unwrap();
        let before_counter = world.state.next_module_market_sale_id;
        let baseline = world.snapshot();
        let mut event = sale(None);
        if let DomainEvent::ModuleArtifactSaleCompleted { sale_id, .. } = &mut event {
            *sale_id = id;
        }
        publish(&mut world, event);
        assert_eq!(
            world.state.next_module_market_sale_id,
            if id == 0 { before_counter } else { u64::MAX }
        );
        check_roots(&world, baseline);
    }
}

#[test]
fn market_selection_preserves_price_order_vector_ties_and_eligibility() {
    for mode in 0..4 {
        let mut world = fixture();
        world.submit_action(Action::RegisterAgent {
            agent_id: "other".into(),
            pos: crate::tests::pos(0, 0),
        });
        world.step().unwrap();
        world
            .set_agent_resource_balance("seller", ResourceKind::Data, 100)
            .unwrap();
        world
            .set_agent_resource_balance("buyer", ResourceKind::Data, 10)
            .unwrap();
        world
            .set_agent_resource_balance("other", ResourceKind::Data, 10)
            .unwrap();
        // Quotes above current funds remain eligible when the listing price is affordable.
        let entries = match mode {
            0 => vec![
                ("buyer", 9, 20, ResourceKind::Data),
                ("other", 8, 30, ResourceKind::Data),
            ],
            1 => vec![
                ("buyer", 9, 30, ResourceKind::Data),
                ("other", 8, 30, ResourceKind::Data),
            ],
            2 => vec![
                ("buyer", 8, 30, ResourceKind::Data),
                ("other", 8, 30, ResourceKind::Data),
            ],
            _ => vec![
                ("seller", 1, 100, ResourceKind::Data),
                ("other", 2, 100, ResourceKind::Electricity),
                ("other", 3, 9, ResourceKind::Data),
                ("buyer", 4, 20, ResourceKind::Data),
            ],
        };
        for (actor, id, price, kind) in entries {
            world
                .state
                .apply_domain_event(
                    &DomainEvent::ModuleArtifactBidPlaced {
                        bidder_agent_id: actor.into(),
                        wasm_hash: "market-hash".into(),
                        order_id: id,
                        price_kind: kind,
                        price_amount: price,
                    },
                    10,
                )
                .unwrap();
        }
        let baseline = world.snapshot();
        world
            .try_apply_runtime_module_action(&market_action(true))
            .unwrap();
        let winner = if mode < 2 { "other" } else { "buyer" };
        assert_eq!(world.state.module_artifact_owners["market-hash"], winner);
        assert_eq!(
            world
                .agent_resource_balance(winner, ResourceKind::Data)
                .unwrap(),
            0
        );
        assert_eq!(
            world
                .agent_resource_balance("seller", ResourceKind::Data)
                .unwrap(),
            109
        );
        check_roots(&world, baseline);
    }
}

#[test]
fn market_nonmatching_orders_publish_once_and_existing_better_bid_wins() {
    for list in [false, true] {
        let mut world = fixture();
        world
            .set_agent_resource_balance("seller", ResourceKind::Data, 100)
            .unwrap();
        world
            .set_agent_resource_balance("buyer", ResourceKind::Data, 100)
            .unwrap();
        let baseline = world.snapshot();
        let count = world.journal().events.len();
        world
            .try_apply_runtime_module_action(&market_action(list))
            .unwrap();
        assert_eq!(world.journal().events.len(), count + 1);
        assert_eq!(world.state.module_artifact_owners["market-hash"], "seller");
        assert_eq!(
            world
                .agent_resource_balance("buyer", ResourceKind::Data)
                .unwrap(),
            100
        );
        check_roots(&world, baseline);
    }
    let mut world = fixture();
    world.submit_action(Action::RegisterAgent {
        agent_id: "other".into(),
        pos: crate::tests::pos(0, 0),
    });
    world.step().unwrap();
    for actor in ["buyer", "other"] {
        world
            .set_agent_resource_balance(actor, ResourceKind::Data, 100)
            .unwrap();
    }
    world
        .state
        .apply_domain_event(&data_event(listing()), 10)
        .unwrap();
    world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactBidPlaced {
                bidder_agent_id: "other".into(),
                wasm_hash: "market-hash".into(),
                order_id: 2,
                price_kind: ResourceKind::Data,
                price_amount: 20,
            },
            10,
        )
        .unwrap();
    let baseline = world.snapshot();
    world
        .try_apply_runtime_module_action(&market_action(false))
        .unwrap();
    assert_eq!(world.state.module_artifact_owners["market-hash"], "other");
    assert_eq!(world.state.module_artifact_bids["market-hash"].len(), 1);
    assert_eq!(
        world.state.module_artifact_bids["market-hash"][0].bidder_agent_id,
        "buyer"
    );
    check_roots(&world, baseline);
}

#[test]
fn market_composite_retention_crosses_event_era_with_published_root() {
    let mut world = fixture();
    world
        .set_agent_resource_balance("seller", ResourceKind::Data, 100)
        .unwrap();
    world
        .set_agent_resource_balance("buyer", ResourceKind::Data, 100)
        .unwrap();
    world
        .state
        .apply_domain_event(&data_event(bid()), 10)
        .unwrap();
    world.runtime_memory_limits.max_journal_events = 2;
    world.next_event_id = u64::MAX;
    world.next_event_id_era = 7;
    world
        .try_apply_runtime_module_action(&market_action(true))
        .unwrap();
    assert_eq!(world.next_event_id_era, 8);
    assert_eq!(world.journal().events.len(), 2);
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
    assert_eq!(world.state.module_artifact_owners["market-hash"], "buyer");
}

#[test]
fn market_raw_bid_validation_priority_preserves_state() {
    for (order_id, price_amount, reason) in [
        (0, 0, "order_id must be > 0"),
        (1, 0, "price must be > 0"),
        (1, 1, "AgentNotFound"),
    ] {
        let mut world = fixture();
        let before = world.state.clone();
        let event = DomainEvent::ModuleArtifactBidPlaced {
            bidder_agent_id: "missing".into(),
            wasm_hash: "unknown".into(),
            order_id,
            price_kind: ResourceKind::Data,
            price_amount,
        };
        let error = world.state.apply_domain_event(&event, 99).unwrap_err();
        assert!(format!("{error:?}").contains(reason), "{error:?}");
        assert_eq!(world.state, before);
    }
}

#[test]
fn market_missing_bidder_is_skipped_and_legacy_materials_normalize_on_success() {
    let mut world = fixture();
    world
        .set_agent_resource_balance("seller", ResourceKind::Data, 100)
        .unwrap();
    world
        .set_agent_resource_balance("buyer", ResourceKind::Data, 100)
        .unwrap();
    world
        .state
        .apply_domain_event(&data_event(bid()), 10)
        .unwrap();
    let mut absent = world.state.module_artifact_bids["market-hash"][0].clone();
    absent.bidder_agent_id = "missing".into();
    absent.price_amount = 1000;
    absent.order_id = 1;
    world
        .state
        .module_artifact_bids
        .get_mut("market-hash")
        .unwrap()
        .insert(0, absent);
    world.state.materials.insert("legacy".into(), 7);
    world.state.material_ledgers.clear();
    let baseline = world.snapshot();
    world
        .try_apply_runtime_module_action(&market_action(true))
        .unwrap();
    assert_eq!(world.state.module_artifact_owners["market-hash"], "buyer");
    assert_eq!(world.state.module_artifact_bids["market-hash"].len(), 1);
    assert_eq!(
        world.state.module_artifact_bids["market-hash"][0].bidder_agent_id,
        "missing"
    );
    assert_eq!(world.state.materials["legacy"], 7);
    assert_eq!(
        world.state.material_ledgers[&crate::MaterialLedgerId::world()]["legacy"],
        7
    );
    check_roots(&world, baseline);
}
