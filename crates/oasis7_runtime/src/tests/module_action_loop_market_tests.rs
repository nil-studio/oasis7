use super::*;

#[test]
fn module_artifact_listing_and_purchase_transfers_owner_and_settles_price() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "buyer-1");
    set_agent_resource(&mut world, "seller-1", ResourceKind::Data, 3);
    set_agent_resource(&mut world, "buyer-1", ResourceKind::Data, 20);

    let wasm_bytes = b"module-action-loop-market-list-and-buy".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 7,
    });
    world.step().expect("list artifact");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(DomainEvent::ModuleArtifactListed {
            seller_agent_id,
            wasm_hash: listed_hash,
            price_kind: ResourceKind::Data,
            price_amount: 7,
            ..
        })) if seller_agent_id == "seller-1" && listed_hash == &wasm_hash
    ));

    world.submit_action(Action::BuyModuleArtifact {
        buyer_agent_id: "buyer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
    });
    world.step().expect("buy artifact");
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::Domain(
            DomainEvent::ModuleArtifactSaleCompleted {
                buyer_agent_id,
                seller_agent_id,
                wasm_hash: sold_hash,
                price_kind: ResourceKind::Data,
                price_amount: 7,
                ..
            }
        )) if buyer_agent_id == "buyer-1" && seller_agent_id == "seller-1" && sold_hash == &wasm_hash
    ));

    assert_eq!(
        world.state().module_artifact_owners.get(&wasm_hash),
        Some(&"buyer-1".to_string())
    );
    assert!(!world
        .state()
        .module_artifact_listings
        .contains_key(&wasm_hash));
    assert_eq!(
        world
            .agent_resource_balance("buyer-1", ResourceKind::Data)
            .expect("buyer resource"),
        13
    );
    assert_eq!(
        world
            .agent_resource_balance("seller-1", ResourceKind::Data)
            .expect("seller resource"),
        9
    );
}

#[test]
fn list_module_artifact_for_sale_rejects_non_owner() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "intruder-1");

    let wasm_bytes = b"module-action-loop-list-non-owner".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    let action_id = world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "intruder-1".to_string(),
        wasm_hash,
        price_kind: ResourceKind::Data,
        price_amount: 5,
    });
    world.step().expect("list non-owner");

    assert_last_rejection_note(&world, action_id, "does not own");
}

#[test]
fn buy_module_artifact_rejects_when_buyer_has_insufficient_price_resource() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "buyer-1");
    set_agent_resource(&mut world, "buyer-1", ResourceKind::Data, 2);

    let wasm_bytes = b"module-action-loop-buy-insufficient".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");
    world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 5,
    });
    world.step().expect("list artifact");

    let action_id = world.submit_action(Action::BuyModuleArtifact {
        buyer_agent_id: "buyer-1".to_string(),
        wasm_hash,
    });
    world.step().expect("buy insufficient");

    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ActionRejected {
        action_id: rejected_action_id,
        reason:
            RejectReason::InsufficientResource {
                agent_id,
                kind: ResourceKind::Data,
                requested,
                available,
            },
    }) = &event.body
    else {
        panic!(
            "expected insufficient resource rejection for buy action: {:?}",
            event.body
        );
    };
    assert_eq!(*rejected_action_id, action_id);
    assert_eq!(agent_id, "buyer-1");
    assert_eq!(*requested, 5);
    assert_eq!(*available, 2);
}

#[test]
fn install_module_from_artifact_rejects_non_owner_when_owner_is_registered() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");
    register_agent(&mut world, "installer-1");

    let wasm_bytes = b"module-action-loop-install-owner-check".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    let action_id = world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: base_manifest("m.loop.owner-guard", "0.1.0", &wasm_hash),
        activate: true,
    });
    world.step().expect("install owner-guard");

    assert_last_rejection_note(&world, action_id, "does not own");
    assert!(world.module_registry().records.is_empty());
}

#[test]
fn delist_module_artifact_removes_listing_and_charges_data_fee() {
    let mut world = World::new();
    register_agent(&mut world, "seller-1");

    let wasm_bytes = b"module-action-loop-delist-success".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");
    world.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 5,
    });
    world.step().expect("list artifact");

    let before_data = world
        .agent_resource_balance("seller-1", ResourceKind::Data)
        .expect("seller data before delist");

    world.submit_action(Action::DelistModuleArtifact {
        seller_agent_id: "seller-1".to_string(),
        wasm_hash: wasm_hash.clone(),
    });
    world.step().expect("delist artifact");

    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactDelisted {
        seller_agent_id,
        wasm_hash: delisted_hash,
        order_id: _,
        fee_kind,
        fee_amount,
    }) = &event.body
    else {
        panic!("expected module artifact delisted event: {:?}", event.body);
    };
    assert_eq!(seller_agent_id, "seller-1");
    assert_eq!(delisted_hash, &wasm_hash);
    assert_eq!(*fee_kind, ResourceKind::Data);
    assert!(*fee_amount > 0);
    assert!(!world
        .state()
        .module_artifact_listings
        .contains_key(&wasm_hash));
    let after_data = world
        .agent_resource_balance("seller-1", ResourceKind::Data)
        .expect("seller data after delist");
    assert_eq!(after_data, before_data - *fee_amount);
}
