use super::super::{Action, ActionEnvelope, DomainEvent, ModuleSourcePackage, WorldEventBody};
use super::{ReleaseSecurityPolicy, World, WorldError};
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeMap;

fn dispatch(world: &mut World, action: Action) -> Result<bool, WorldError> {
    world.try_apply_runtime_module_action(&ActionEnvelope {
        committed_recipe_origin: None,
        id: 990,
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

fn assert_world_unchanged(world: &World, expected: &World) {
    assert_eq!(world.snapshot(), expected.snapshot());
    assert_eq!(world.journal(), expected.journal());
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        (expected.next_event_id, expected.next_event_id_era)
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

#[test]
fn binary_deploy_tail_failure_preserves_registration_and_publication() {
    let mut world = World::new();
    register_agent(&mut world, "publisher");
    let bytes = b"atomic-binary-deploy".to_vec();
    let hash = crate::util::sha256_hex(&bytes);
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    dispatch(
        &mut world,
        Action::DeployModuleArtifact {
            publisher_agent_id: "publisher".into(),
            wasm_hash: hash,
            wasm_bytes: bytes,
        },
    )
    .expect_err("deployment must honor postprepare publication failure");
    assert_world_unchanged(&world, &before);
}

#[test]
fn redeploy_tail_failure_preserves_existing_owner_orders_and_bytes() {
    let mut world = World::new();
    for id in ["old-owner", "new-publisher", "bidder-a", "bidder-b"] {
        register_agent(&mut world, id);
    }
    let bytes = b"atomic-redeployment".to_vec();
    let hash = crate::util::sha256_hex(&bytes);
    world
        .register_module_artifact(hash.clone(), &bytes)
        .unwrap();
    world
        .state
        .module_artifact_owners
        .insert(hash.clone(), "old-owner".into());
    world
        .state
        .apply_domain_event(
            &DomainEvent::ModuleArtifactListed {
                seller_agent_id: "old-owner".into(),
                wasm_hash: hash.clone(),
                price_kind: ResourceKind::Electricity,
                price_amount: 10,
                order_id: 11,
                fee_kind: ResourceKind::Electricity,
                fee_amount: 0,
            },
            5,
        )
        .unwrap();
    for (bidder, order) in [("bidder-a", 12), ("bidder-b", 13)] {
        world
            .state
            .apply_domain_event(
                &DomainEvent::ModuleArtifactBidPlaced {
                    bidder_agent_id: bidder.into(),
                    wasm_hash: hash.clone(),
                    order_id: order,
                    price_kind: ResourceKind::Electricity,
                    price_amount: 8,
                },
                5,
            )
            .unwrap();
    }
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    dispatch(
        &mut world,
        Action::DeployModuleArtifact {
            publisher_agent_id: "new-publisher".into(),
            wasm_hash: hash,
            wasm_bytes: bytes,
        },
    )
    .expect_err("redeployment must not replace existing business state on tail failure");
    assert_world_unchanged(&world, &before);
}

#[test]
fn raw_deployed_tail_failure_preserves_market_and_legacy_material_state() {
    let mut world = World::new();
    register_agent(&mut world, "publisher");
    world.state.material_ledgers.clear();
    world
        .state
        .materials
        .insert("legacy-deploy-material".into(), 9);
    world
        .state
        .module_artifact_owners
        .insert("raw-hash".into(), "publisher".into());
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    world
        .append_event(
            WorldEventBody::Domain(DomainEvent::ModuleArtifactDeployed {
                publisher_agent_id: "publisher".into(),
                wasm_hash: "raw-hash".into(),
                bytes_len: 777,
                fee_kind: ResourceKind::Electricity,
                fee_amount: 1,
            }),
            None,
        )
        .expect_err("raw deployment must honor postprepare publication failure");
    assert_world_unchanged(&world, &before);
}

#[test]
fn prepared_registration_rejects_mismatched_or_non_deployment_event_without_mutation() {
    let mut world = World::new();
    register_agent(&mut world, "publisher");
    let bytes = b"bound-registration";
    let hash = crate::util::sha256_hex(bytes);

    for event in [
        DomainEvent::ModuleArtifactDeployed {
            publisher_agent_id: "publisher".into(),
            wasm_hash: "different-hash".into(),
            bytes_len: bytes.len() as u64,
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
        DomainEvent::ModuleArtifactListed {
            seller_agent_id: "publisher".into(),
            wasm_hash: hash.clone(),
            price_kind: ResourceKind::Electricity,
            price_amount: 1,
            order_id: 1,
            fee_kind: ResourceKind::Electricity,
            fee_amount: 0,
        },
    ] {
        let registration = world
            .prepare_module_artifact_registration(hash.clone(), bytes)
            .unwrap();
        let before = world.clone();
        world
            .append_module_artifact_deployment(event, None, registration)
            .expect_err("registration and deployment event identity must remain bound");
        assert_world_unchanged(&world, &before);
    }
}

#[test]
fn source_compile_tail_failure_preserves_compiled_registration_and_publication() {
    let mut policy = ReleaseSecurityPolicy::production_hardened();
    policy.allow_runtime_source_compile = true;
    let mut world = World::new_with_release_security_policy(policy);
    struct Compiler;
    impl oasis7_agent_api::module_source::ModuleSourceCompiler for Compiler {
        fn compile(&self, _: &str, _: &ModuleSourcePackage) -> Result<Vec<u8>, oasis7_agent_api::module_source::ModuleSourceCompileError> {
            Ok(b"compiled-atomic-source".to_vec())
        }
    }
    world.set_module_source_compiler(std::sync::Arc::new(Compiler));
    register_agent(&mut world, "publisher");
    let before = world.clone();
    world.fail_next_append_after_publication_prepare_for_test();
    dispatch(
        &mut world,
        Action::CompileModuleArtifactFromSource {
            publisher_agent_id: "publisher".into(),
            module_id: "m.atomic.source".into(),
            source_package: ModuleSourcePackage {
                manifest_path: "Cargo.toml".into(),
                files: BTreeMap::from([
                    (
                        "Cargo.toml".into(),
                        b"[package]\nname='x'\nversion='0.1.0'\n".to_vec(),
                    ),
                    ("src/lib.rs".into(), b"pub fn x() {}".to_vec()),
                ]),
            },
        },
    )
    .expect_err("compiled deployment must honor postprepare publication failure");
    assert_world_unchanged(&world, &before);
}

fn deployment_action(publisher: &str, bytes: &[u8]) -> Action {
    Action::DeployModuleArtifact {
        publisher_agent_id: publisher.into(),
        wasm_hash: crate::util::sha256_hex(bytes),
        wasm_bytes: bytes.to_vec(),
    }
}

fn assert_published_root_and_state_replay(world: &World, baseline: crate::Snapshot) {
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
fn binary_deploy_success_and_retry_commit_bytes_fee_mailbox_and_root_once() {
    let mut world = World::new();
    register_agent(&mut world, "publisher");
    let bytes = b"atomic-binary-success".to_vec();
    let hash = crate::util::sha256_hex(&bytes);
    let baseline = world.snapshot();
    let before_balance = world
        .agent_resource_balance("publisher", ResourceKind::Electricity)
        .unwrap();
    let before_treasury = world
        .state
        .resources
        .get(&ResourceKind::Electricity)
        .copied()
        .unwrap_or(0);
    let before_mailbox = world.state.agents["publisher"].mailbox.len();
    let before_events = world.journal().events.len();
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(dispatch(&mut world, deployment_action("publisher", &bytes)).is_err());
    dispatch(&mut world, deployment_action("publisher", &bytes)).unwrap();
    let after_balance = world
        .agent_resource_balance("publisher", ResourceKind::Electricity)
        .unwrap();
    assert!(after_balance < before_balance);
    assert_eq!(
        world.state.resources[&ResourceKind::Electricity] - before_treasury,
        before_balance - after_balance
    );
    assert_eq!(world.state.module_artifact_owners[&hash], "publisher");
    assert!(world.module_artifacts.contains(&hash));
    assert_eq!(
        world.module_artifact_bytes[&hash].as_ref(),
        bytes.as_slice()
    );
    assert_eq!(
        world.state.agents["publisher"].last_active,
        world.state.time
    );
    assert_eq!(
        world.state.agents["publisher"].mailbox.len(),
        before_mailbox + 1
    );
    assert_eq!(world.journal().events.len(), before_events + 1);
    assert_published_root_and_state_replay(&world, baseline);
}

#[test]
fn raw_deployed_keeps_informational_payload_and_error_priority() {
    let mut world = World::new();
    register_agent(&mut world, "publisher");
    let raw = |publisher: &str, fee: i64| DomainEvent::ModuleArtifactDeployed {
        publisher_agent_id: publisher.into(),
        wasm_hash: "informational-only-hash".into(),
        bytes_len: u64::MAX,
        fee_kind: ResourceKind::Electricity,
        fee_amount: fee,
    };
    world
        .append_event(WorldEventBody::Domain(raw("publisher", 0)), None)
        .unwrap();
    assert!(!world.module_artifacts.contains("informational-only-hash"));
    assert!(
        !world
            .module_artifact_bytes
            .contains_key("informational-only-hash")
    );
    assert_eq!(
        world.state.module_artifact_owners["informational-only-hash"],
        "publisher"
    );
    assert_raw_deployed_rejected(&mut world, raw("absent", -1), "fee must be >= 0");
    assert_raw_deployed_rejected(&mut world, raw("absent", 0), "AgentNotFound");
    world
        .set_agent_resource_balance("publisher", ResourceKind::Electricity, 0)
        .unwrap();
    assert_raw_deployed_rejected(&mut world, raw("publisher", 1), "fee debit failed");
    let before = world.clone();
    assert!(
        world
            .register_module_artifact("wrong-hash", b"bytes")
            .is_err()
    );
    assert_world_unchanged(&world, &before);
}

fn assert_raw_deployed_rejected(world: &mut World, event: DomainEvent, expected: &str) {
    let before = world.state.clone();
    let error = world.state.apply_domain_event(&event, 99).unwrap_err();
    assert!(
        format!("{error:?}").contains(expected),
        "unexpected error: {error:?}"
    );
    assert_eq!(world.state, before);
}

#[test]
fn identical_redeploy_charges_again_clears_only_target_orders_and_crosses_era() {
    let mut world = World::new();
    for id in ["first", "second", "bidder"] {
        register_agent(&mut world, id);
    }
    let bytes = b"identical-redeploy-success".to_vec();
    let hash = crate::util::sha256_hex(&bytes);
    dispatch(&mut world, deployment_action("first", &bytes)).unwrap();
    for target in [hash.as_str(), "unrelated-hash"] {
        world
            .state
            .module_artifact_owners
            .insert(target.into(), "first".into());
        world
            .state
            .apply_domain_event(
                &DomainEvent::ModuleArtifactListed {
                    seller_agent_id: "first".into(),
                    wasm_hash: target.into(),
                    price_kind: ResourceKind::Electricity,
                    price_amount: 10,
                    order_id: 21,
                    fee_kind: ResourceKind::Electricity,
                    fee_amount: 0,
                },
                8,
            )
            .unwrap();
        world
            .state
            .apply_domain_event(
                &DomainEvent::ModuleArtifactBidPlaced {
                    bidder_agent_id: "bidder".into(),
                    wasm_hash: target.into(),
                    order_id: 22,
                    price_kind: ResourceKind::Electricity,
                    price_amount: 8,
                },
                8,
            )
            .unwrap();
    }
    let before_balance = world
        .agent_resource_balance("second", ResourceKind::Electricity)
        .unwrap();
    world.runtime_memory_limits.max_journal_events = 2;
    world.next_event_id = u64::MAX;
    world.next_event_id_era = 12;
    dispatch(&mut world, deployment_action("second", &bytes)).unwrap();
    assert_eq!(world.next_event_id_era, 13);
    assert_eq!(world.journal().events.len(), 2);
    assert_eq!(world.state.module_artifact_owners[&hash], "second");
    assert!(!world.state.module_artifact_listings.contains_key(&hash));
    assert!(!world.state.module_artifact_bids.contains_key(&hash));
    assert!(
        world
            .state
            .module_artifact_listings
            .contains_key("unrelated-hash")
    );
    assert!(
        world
            .state
            .module_artifact_bids
            .contains_key("unrelated-hash")
    );
    assert!(
        world
            .agent_resource_balance("second", ResourceKind::Electricity)
            .unwrap()
            < before_balance
    );
    assert_eq!(
        world.module_artifact_bytes[&hash].as_ref(),
        bytes.as_slice()
    );
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
}
