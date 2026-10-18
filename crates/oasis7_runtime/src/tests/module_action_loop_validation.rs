#[test]
fn destroy_module_artifact_removes_owner_and_artifact_bytes() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_bytes = b"module-action-loop-destroy-success".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    let before_electricity = world
        .agent_resource_balance("owner-1", ResourceKind::Electricity)
        .expect("owner electricity before destroy");

    world.submit_action(Action::DestroyModuleArtifact {
        owner_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        reason: "retire obsolete module".to_string(),
    });
    world.step().expect("destroy artifact");

    let (destroyed_hash, fee_kind, fee_amount) = {
        let event = world.journal().events.last().expect("last event");
        let WorldEventBody::Domain(DomainEvent::ModuleArtifactDestroyed {
            owner_agent_id,
            wasm_hash: destroyed_hash,
            reason,
            fee_kind,
            fee_amount,
        }) = &event.body
        else {
            panic!("expected module artifact destroyed event: {:?}", event.body);
        };
        assert_eq!(owner_agent_id, "owner-1");
        assert_eq!(reason, "retire obsolete module");
        (destroyed_hash.clone(), *fee_kind, *fee_amount)
    };
    assert_eq!(destroyed_hash, wasm_hash);
    assert_eq!(fee_kind, ResourceKind::Electricity);
    assert!(fee_amount > 0);
    assert!(!world
        .state()
        .module_artifact_owners
        .contains_key(&wasm_hash));
    assert!(!world
        .state()
        .module_artifact_listings
        .contains_key(&wasm_hash));
    assert!(world.load_module(&wasm_hash).is_err());
    let after_electricity = world
        .agent_resource_balance("owner-1", ResourceKind::Electricity)
        .expect("owner electricity after destroy");
    assert_eq!(after_electricity, before_electricity - fee_amount);
}

#[test]
fn destroy_module_artifact_rejects_when_artifact_is_used_by_active_module() {
    let mut world = World::new();
    register_agent(&mut world, "owner-1");

    let wasm_bytes = b"module-action-loop-destroy-active-guard".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "owner-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "owner-1".to_string(),
        manifest: base_manifest("m.loop.destroy-guard", "0.1.0", &wasm_hash),
        activate: true,
    });
    world.step().expect("install module");

    let action_id = world.submit_action(Action::DestroyModuleArtifact {
        owner_agent_id: "owner-1".to_string(),
        wasm_hash,
        reason: "cleanup".to_string(),
    });
    world.step().expect("destroy guarded artifact");

    assert_last_rejection_note(&world, action_id, "used by active module");
}

#[test]
fn module_artifact_bid_auto_matches_on_listing() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "buyer-1");
    set_agent_resource(&mut world, "buyer-1", ResourceKind::Data, 30);

    let wasm_bytes = b"module-action-loop-bid-auto-match".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "buyer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 9,
    });
    world.step().expect("place bid");

    let bid_order_id = match &world.journal().events.last().expect("bid event").body {
        WorldEventBody::Domain(DomainEvent::ModuleArtifactBidPlaced { order_id, .. }) => *order_id,
        other => panic!("expected module artifact bid placed event: {other:?}"),
    };
    assert!(bid_order_id > 0);

    world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 7,
    });
    world.step().expect("list and match");

    let event = world.journal().events.last().expect("sale event");
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactSaleCompleted {
        buyer_agent_id,
        seller_agent_id,
        price_kind,
        price_amount,
        bid_order_id: matched_bid_order_id,
        ..
    }) = &event.body
    else {
        panic!(
            "expected module artifact sale completed event for auto match: {:?}",
            event.body
        );
    };
    assert_eq!(buyer_agent_id, "buyer-1");
    assert_eq!(seller_agent_id, "seller-1");
    assert_eq!(*price_kind, ResourceKind::Data);
    assert_eq!(*price_amount, 7);
    assert_eq!(*matched_bid_order_id, Some(bid_order_id));
    assert_eq!(
        world.state().module_artifact_owners.get(&wasm_hash),
        Some(&"buyer-1".to_string())
    );
    assert!(!world
        .state()
        .module_artifact_listings
        .contains_key(&wasm_hash));
    assert!(!world.state().module_artifact_bids.contains_key(&wasm_hash));
}

#[test]
fn cancel_module_artifact_bid_removes_order() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "buyer-1");
    set_agent_resource(&mut world, "buyer-1", ResourceKind::Data, 20);

    let wasm_bytes = b"module-action-loop-bid-cancel".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "buyer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 8,
    });
    world.step().expect("place bid");
    let bid_order_id = match &world.journal().events.last().expect("bid event").body {
        WorldEventBody::Domain(DomainEvent::ModuleArtifactBidPlaced { order_id, .. }) => *order_id,
        other => panic!("expected module artifact bid placed event: {other:?}"),
    };

    world.submit_action(Action::CancelModuleArtifactBid {
        bidder_agent_id: "buyer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        bid_order_id,
    });
    world.step().expect("cancel bid");

    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleArtifactBidCancelled {
            bidder_agent_id,
            order_id,
            ..
        })) if bidder_agent_id == "buyer-1" && *order_id == bid_order_id
    ));
    assert!(!world.state().module_artifact_bids.contains_key(&wasm_hash));
}

#[test]
fn module_artifact_bid_match_prefers_highest_price() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "buyer-low");
    register_agent(&mut world, "buyer-high");
    set_agent_resource(&mut world, "buyer-low", ResourceKind::Data, 20);
    set_agent_resource(&mut world, "buyer-high", ResourceKind::Data, 20);

    let wasm_bytes = b"module-action-loop-bid-priority".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "buyer-low".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 8,
    });
    world.step().expect("place low bid");
    world.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "buyer-high".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 9,
    });
    world.step().expect("place high bid");

    world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 7,
    });
    world.step().expect("list and match");

    let event = world.journal().events.last().expect("sale event");
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactSaleCompleted { buyer_agent_id, .. }) =
        &event.body
    else {
        panic!("expected sale completion event: {:?}", event.body);
    };
    assert_eq!(buyer_agent_id, "buyer-high");
}

#[test]
fn module_artifact_bid_match_prefers_earlier_order_for_equal_price() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "buyer-first");
    register_agent(&mut world, "buyer-second");
    set_agent_resource(&mut world, "buyer-first", ResourceKind::Data, 20);
    set_agent_resource(&mut world, "buyer-second", ResourceKind::Data, 20);

    let wasm_bytes = b"module-action-loop-bid-equal-price-priority".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "buyer-first".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 9,
    });
    world.step().expect("place first bid");
    let first_bid_order_id = match &world.journal().events.last().expect("first bid event").body {
        WorldEventBody::Domain(DomainEvent::ModuleArtifactBidPlaced { order_id, .. }) => *order_id,
        other => panic!("expected module artifact bid placed event: {other:?}"),
    };
    world.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "buyer-second".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 9,
    });
    world.step().expect("place second bid");

    world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash,
        price_kind: ResourceKind::Data,
        price_amount: 7,
    });
    world.step().expect("list and match");

    let event = world.journal().events.last().expect("sale event");
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactSaleCompleted {
        buyer_agent_id,
        bid_order_id,
        ..
    }) = &event.body
    else {
        panic!("expected sale completion event: {:?}", event.body);
    };
    assert_eq!(buyer_agent_id, "buyer-first");
    assert_eq!(*bid_order_id, Some(first_bid_order_id));
}
