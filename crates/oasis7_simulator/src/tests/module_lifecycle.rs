use super::*;
use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn setup_kernel_with_agent(agent_id: &str) -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-home".to_string(),
        name: "home".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        location_id: "loc-home".to_string(),
    });
    kernel.step_until_empty();
    kernel
}

#[test]
fn module_lifecycle_deploy_and_install_succeeds_for_owner() {
    let mut kernel = setup_kernel_with_agent("agent-1");
    let wasm_bytes = b"simulator-module-lifecycle".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());

    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes: wasm_bytes.clone(),
        module_id_hint: Some("m.sim.lifecycle".to_string()),
    });
    let deploy_event = kernel.step().expect("deploy event");
    match deploy_event.kind {
        WorldEventKind::ModuleArtifactDeployed {
            publisher_agent_id,
            wasm_hash: event_hash,
            bytes_len,
            module_id_hint,
            ..
        } => {
            assert_eq!(publisher_agent_id, "agent-1");
            assert_eq!(event_hash, wasm_hash);
            assert_eq!(bytes_len, wasm_bytes.len() as u64);
            assert_eq!(module_id_hint.as_deref(), Some("m.sim.lifecycle"));
        }
        other => panic!("unexpected deploy event: {other:?}"),
    }

    let artifact = kernel
        .model()
        .module_artifacts
        .get(&wasm_hash)
        .expect("artifact exists");
    assert_eq!(artifact.publisher_agent_id, "agent-1");
    assert_eq!(artifact.wasm_bytes, wasm_bytes);

    kernel.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "agent-1".to_string(),
        module_id: "m.sim.lifecycle".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash: wasm_hash.clone(),
        activate: true,
    });
    let install_event = kernel.step().expect("install event");
    match install_event.kind {
        WorldEventKind::ModuleInstalled {
            installer_agent_id,
            module_id,
            module_version,
            wasm_hash: event_hash,
            active,
            install_target,
        } => {
            assert_eq!(installer_agent_id, "agent-1");
            assert_eq!(module_id, "m.sim.lifecycle");
            assert_eq!(module_version, "0.1.0");
            assert_eq!(event_hash, wasm_hash);
            assert!(active);
            assert_eq!(install_target, ModuleInstallTarget::SelfAgent);
        }
        other => panic!("unexpected install event: {other:?}"),
    }

    let installed = kernel
        .model()
        .installed_modules
        .get("m.sim.lifecycle")
        .expect("installed module exists");
    assert_eq!(installed.wasm_hash, wasm_hash);
    assert!(installed.active);
}

#[test]
fn module_lifecycle_install_rejects_non_owner() {
    let mut kernel = setup_kernel_with_agent("agent-owner");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-other".to_string(),
        location_id: "loc-home".to_string(),
    });
    kernel.step().expect("register second agent");

    let wasm_bytes = b"simulator-module-non-owner".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: None,
    });
    let _ = kernel.step().expect("deploy event");

    kernel.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "agent-other".to_string(),
        module_id: "m.sim.owner-check".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash,
        activate: true,
    });
    let event = kernel.step().expect("install reject event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes.iter().any(|note| note.contains("not artifact owner")),
                "unexpected notes: {notes:?}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn module_lifecycle_install_to_location_infrastructure_succeeds_when_colocated() {
    let mut kernel = setup_kernel_with_agent("agent-1");
    let wasm_bytes = b"simulator-module-location-target".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());

    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.location.target".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id: "agent-1".to_string(),
        module_id: "m.sim.location.target".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash: wasm_hash.clone(),
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-home".to_string(),
        },
    });
    let event = kernel.step().expect("install to location event");
    match event.kind {
        WorldEventKind::ModuleInstalled {
            module_id,
            install_target,
            ..
        } => {
            assert_eq!(module_id, "m.sim.location.target");
            assert_eq!(
                install_target,
                ModuleInstallTarget::LocationInfrastructure {
                    location_id: "loc-home".to_string(),
                }
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let installed = kernel
        .model()
        .installed_modules
        .get("m.sim.location.target")
        .expect("installed module exists");
    assert_eq!(
        installed.install_target,
        ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-home".to_string(),
        }
    );
}

#[test]
fn module_lifecycle_install_to_location_infrastructure_rejects_missing_location() {
    let mut kernel = setup_kernel_with_agent("agent-1");
    let wasm_bytes = b"simulator-module-location-missing".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());

    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.location.missing".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id: "agent-1".to_string(),
        module_id: "m.sim.location.missing".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash,
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-missing".to_string(),
        },
    });
    let event = kernel.step().expect("install reject event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::LocationNotFound { location_id },
        } => {
            assert_eq!(location_id, "loc-missing");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn module_lifecycle_install_to_location_infrastructure_rejects_not_colocated() {
    let mut kernel = setup_kernel_with_agent("agent-1");
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-remote".to_string(),
        name: "remote".to_string(),
        pos: pos(10, 0),
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register remote location");

    let wasm_bytes = b"simulator-module-location-not-colocated".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.location.not_colocated".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id: "agent-1".to_string(),
        module_id: "m.sim.location.not_colocated".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash,
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-remote".to_string(),
        },
    });
    let event = kernel.step().expect("install reject event");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("target infrastructure is loc-remote")),
                "unexpected notes: {notes:?}"
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn module_lifecycle_replay_restores_artifact_and_install_state() {
    let mut kernel = setup_kernel_with_agent("agent-1");
    let snapshot = kernel.kernel_snapshot();

    let wasm_bytes = b"sim-replay-module".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.replay".to_string()),
    });
    kernel.step().expect("deploy event");
    kernel.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "agent-1".to_string(),
        module_id: "m.sim.replay".to_string(),
        module_version: "0.2.0".to_string(),
        wasm_hash: wasm_hash.clone(),
        activate: true,
    });
    kernel.step().expect("install event");

    let journal = kernel.kernel_journal_snapshot();
    let replayed =
        WorldKernel::replay_from_kernel_snapshot(snapshot, journal).expect("replay from snapshot");

    assert!(replayed.model().module_artifacts.contains_key(&wasm_hash));
    let installed = replayed
        .model()
        .installed_modules
        .get("m.sim.replay")
        .expect("installed module present after replay");
    assert_eq!(installed.module_version, "0.2.0");
    assert!(installed.active);
}

#[test]
fn module_lifecycle_replay_legacy_module_installed_defaults_install_target_to_self_agent() {
    let mut kernel = setup_kernel_with_agent("agent-1");
    let snapshot = kernel.kernel_snapshot();

    let wasm_bytes = b"sim-replay-legacy-install-target".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.replay.legacy".to_string()),
    });
    kernel.step().expect("deploy event");
    kernel.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "agent-1".to_string(),
        module_id: "m.sim.replay.legacy".to_string(),
        module_version: "0.3.0".to_string(),
        wasm_hash: wasm_hash.clone(),
        activate: true,
    });
    kernel.step().expect("install event");

    let mut journal_value =
        serde_json::to_value(kernel.kernel_journal_snapshot()).expect("serialize journal");
    let events = journal_value
        .get_mut("events")
        .and_then(|value| value.as_array_mut())
        .expect("events array");
    let install_event = events
        .iter_mut()
        .find(|event| {
            event
                .get("kind")
                .and_then(|kind| kind.get("data"))
                .and_then(|data| data.get("module_id"))
                .and_then(|value| value.as_str())
                == Some("m.sim.replay.legacy")
        })
        .expect("module_installed event");
    install_event
        .get_mut("kind")
        .and_then(|kind| kind.get_mut("data"))
        .and_then(|data| data.as_object_mut())
        .expect("module_installed data")
        .remove("install_target");
    let legacy_journal: KernelJournal =
        serde_json::from_value(journal_value).expect("deserialize legacy journal");

    let replayed = WorldKernel::replay_from_kernel_snapshot(snapshot, legacy_journal)
        .expect("replay from snapshot");
    let installed = replayed
        .model()
        .installed_modules
        .get("m.sim.replay.legacy")
        .expect("installed module present after replay");
    assert_eq!(installed.install_target, ModuleInstallTarget::SelfAgent);
}

#[test]
fn module_market_listing_and_buy_transfers_owner_and_price() {
    let mut kernel = setup_kernel_with_agent("agent-owner");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-buyer".to_string(),
        location_id: "loc-home".to_string(),
    });
    kernel.step().expect("register buyer");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-buyer".to_string(),
        },
        ResourceKind::Data,
        7,
    );

    let wasm_bytes = b"sim-market-buy".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.market".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 5,
    });
    let list_event = kernel.step().expect("list event");
    match list_event.kind {
        WorldEventKind::ModuleArtifactListed {
            seller_agent_id,
            wasm_hash: event_hash,
            price_kind,
            price_amount,
            order_id,
        } => {
            assert_eq!(seller_agent_id, "agent-owner");
            assert_eq!(event_hash, wasm_hash);
            assert_eq!(price_kind, ResourceKind::Data);
            assert_eq!(price_amount, 5);
            assert!(order_id > 0);
        }
        other => panic!("unexpected list event: {other:?}"),
    }

    kernel.submit_action(Action::BuyModuleArtifact {
        buyer_agent_id: "agent-buyer".to_string(),
        wasm_hash: wasm_hash.clone(),
    });
    let buy_event = kernel.step().expect("buy event");
    match buy_event.kind {
        WorldEventKind::ModuleArtifactSaleCompleted {
            buyer_agent_id,
            seller_agent_id,
            wasm_hash: event_hash,
            price_kind,
            price_amount,
            ..
        } => {
            assert_eq!(buyer_agent_id, "agent-buyer");
            assert_eq!(seller_agent_id, "agent-owner");
            assert_eq!(event_hash, wasm_hash);
            assert_eq!(price_kind, ResourceKind::Data);
            assert_eq!(price_amount, 5);
        }
        other => panic!("unexpected buy event: {other:?}"),
    }

    let artifact = kernel
        .model()
        .module_artifacts
        .get(&wasm_hash)
        .expect("artifact exists");
    assert_eq!(artifact.publisher_agent_id, "agent-buyer");
    assert!(
        !kernel
            .model()
            .module_artifact_listings
            .contains_key(&wasm_hash)
    );
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-owner")
            .expect("owner exists")
            .resources
            .get(ResourceKind::Data),
        5
    );
    assert_eq!(
        kernel
            .model()
            .agents
            .get("agent-buyer")
            .expect("buyer exists")
            .resources
            .get(ResourceKind::Data),
        2
    );
}

#[test]
fn module_market_bid_auto_matches_listing() {
    let mut kernel = setup_kernel_with_agent("agent-owner");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-bidder".to_string(),
        location_id: "loc-home".to_string(),
    });
    kernel.step().expect("register bidder");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-bidder".to_string(),
        },
        ResourceKind::Data,
        10,
    );

    let wasm_bytes = b"sim-market-bid".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.market.bid".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 4,
    });
    kernel.step().expect("list event");

    kernel.submit_action(Action::PlaceModuleArtifactBid {
        bidder_agent_id: "agent-bidder".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 6,
    });
    let bid_event = kernel.step().expect("bid/sale event");
    match bid_event.kind {
        WorldEventKind::ModuleArtifactSaleCompleted {
            buyer_agent_id,
            seller_agent_id,
            wasm_hash: event_hash,
            price_amount,
            listing_order_id,
            bid_order_id,
            ..
        } => {
            assert_eq!(buyer_agent_id, "agent-bidder");
            assert_eq!(seller_agent_id, "agent-owner");
            assert_eq!(event_hash, wasm_hash);
            assert_eq!(price_amount, 4);
            assert!(listing_order_id.is_some());
            assert!(bid_order_id.is_some());
        }
        other => panic!("unexpected bid event: {other:?}"),
    }

    let artifact = kernel
        .model()
        .module_artifacts
        .get(&wasm_hash)
        .expect("artifact exists");
    assert_eq!(artifact.publisher_agent_id, "agent-bidder");
    assert!(
        !kernel
            .model()
            .module_artifact_listings
            .contains_key(&wasm_hash)
    );
}

#[test]
fn module_market_destroy_rejects_when_artifact_is_used_by_active_module() {
    let mut kernel = setup_kernel_with_agent("agent-owner");
    let wasm_bytes = b"sim-market-destroy-reject".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());

    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.market.destroy".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "agent-owner".to_string(),
        module_id: "m.sim.market.destroy".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash: wasm_hash.clone(),
        activate: true,
    });
    kernel.step().expect("install event");

    kernel.submit_action(Action::DestroyModuleArtifact {
        owner_agent_id: "agent-owner".to_string(),
        wasm_hash,
        reason: "cleanup".to_string(),
    });
    let event = kernel.step().expect("destroy reject");
    match event.kind {
        WorldEventKind::ActionRejected {
            reason: RejectReason::RuleDenied { notes },
        } => {
            assert!(
                notes
                    .iter()
                    .any(|note| note.contains("used by active module")),
                "unexpected notes: {notes:?}"
            );
        }
        other => panic!("unexpected destroy event: {other:?}"),
    }
}

#[test]
fn module_market_replay_restores_sale_and_market_counters() {
    let mut kernel = setup_kernel_with_agent("agent-owner");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-buyer".to_string(),
        location_id: "loc-home".to_string(),
    });
    kernel.step().expect("register buyer");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-buyer".to_string(),
        },
        ResourceKind::Data,
        9,
    );

    let snapshot = kernel.kernel_snapshot();

    let wasm_bytes = b"sim-market-replay".to_vec();
    let wasm_hash = sha256_hex(wasm_bytes.as_slice());
    kernel.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
        module_id_hint: Some("m.sim.market.replay".to_string()),
    });
    kernel.step().expect("deploy event");

    kernel.submit_action(Action::ListModuleArtifactForSale {
        seller_agent_id: "agent-owner".to_string(),
        wasm_hash: wasm_hash.clone(),
        price_kind: ResourceKind::Data,
        price_amount: 6,
    });
    kernel.step().expect("list event");

    kernel.submit_action(Action::BuyModuleArtifact {
        buyer_agent_id: "agent-buyer".to_string(),
        wasm_hash: wasm_hash.clone(),
    });
    kernel.step().expect("buy event");

    let journal = kernel.kernel_journal_snapshot();
    let replayed =
        WorldKernel::replay_from_kernel_snapshot(snapshot, journal).expect("replay from snapshot");

    let artifact = replayed
        .model()
        .module_artifacts
        .get(&wasm_hash)
        .expect("artifact after replay");
    assert_eq!(artifact.publisher_agent_id, "agent-buyer");
    assert!(
        !replayed
            .model()
            .module_artifact_listings
            .contains_key(&wasm_hash)
    );
    assert!(
        replayed.model().next_module_market_order_id >= 2,
        "unexpected next order id: {}",
        replayed.model().next_module_market_order_id
    );
    assert!(
        replayed.model().next_module_market_sale_id >= 2,
        "unexpected next sale id: {}",
        replayed.model().next_module_market_sale_id
    );
}
