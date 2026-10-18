use super::super::*;
use super::pos;
use oasis7_agent_api::world_action::ModuleInstallTarget;
use oasis7_agent_api::world_action::ResourceKind;
use ed25519_dalek::{Signer, SigningKey};
use std::collections::BTreeMap;

const TEST_FINALITY_SIGNER_NODE_1: &str = "governance.local.finality.signer.1";
const TEST_FINALITY_SIGNER_SEED_1: &str = "oasis7-governance-local-finality-signer-1-v1";
const TEST_FINALITY_SIGNER_NODE_2: &str = "governance.local.finality.signer.2";
const TEST_FINALITY_SIGNER_SEED_2: &str = "oasis7-governance-local-finality-signer-2-v1";

#[path = "module_action_loop_market_tests.rs"]
mod market_tests;

fn register_agent(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register agent");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Electricity, 128)
        .expect("seed electricity");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Data, 64)
        .expect("seed data");
}

fn set_agent_resource(world: &mut World, agent_id: &str, kind: ResourceKind, amount: i64) {
    world
        .set_agent_resource_balance(agent_id, kind, amount)
        .expect("set agent resource balance");
}

fn base_manifest(module_id: &str, version: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: format!("module-{module_id}"),
        version: version.to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::default(),
    }
}

fn set_test_governance_finality_epoch_snapshot(
    world: &mut World,
    threshold: u16,
    signer_node_ids: &[&str],
) {
    let epoch_len = world
        .governance_execution_policy()
        .epoch_length_ticks
        .max(1);
    let epoch_id = world.state().time / epoch_len;
    world
        .set_governance_finality_epoch_snapshot(GovernanceFinalityEpochSnapshot {
            epoch_id,
            threshold,
            signer_node_ids: signer_node_ids
                .iter()
                .map(|signer| signer.to_string())
                .collect(),
            ..GovernanceFinalityEpochSnapshot::default()
        })
        .expect("set test finality epoch snapshot");
}

fn test_finality_signing_key(seed_label: &str) -> SigningKey {
    let seed = util::sha256_hex(seed_label.as_bytes());
    let seed_bytes = hex::decode(seed).expect("decode test finality seed");
    let private_key_bytes: [u8; 32] = seed_bytes
        .as_slice()
        .try_into()
        .expect("test finality seed is 32 bytes");
    SigningKey::from_bytes(&private_key_bytes)
}

fn test_finality_seed_label(node_id: &str) -> &'static str {
    match node_id {
        TEST_FINALITY_SIGNER_NODE_1 => TEST_FINALITY_SIGNER_SEED_1,
        TEST_FINALITY_SIGNER_NODE_2 => TEST_FINALITY_SIGNER_SEED_2,
        _ => panic!("missing test finality seed for signer {node_id}"),
    }
}

fn extract_latest_module_apply_proposal_id(world: &World) -> ProposalId {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ModuleInstalled { proposal_id, .. })
            | WorldEventBody::Domain(DomainEvent::ModuleUpgraded { proposal_id, .. })
            | WorldEventBody::Domain(DomainEvent::ModuleRollbackApplied { proposal_id, .. }) => {
                Some(*proposal_id)
            }
            _ => None,
        })
        .expect("module apply proposal id")
}

fn extract_queued_manifest_hash(world: &World, proposal_id: ProposalId) -> String {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Governance(GovernanceEvent::Queued {
                proposal_id: event_proposal_id,
                manifest_hash,
                ..
            }) if *event_proposal_id == proposal_id => Some(manifest_hash.clone()),
            _ => None,
        })
        .expect("governance queued manifest hash")
}

fn extract_governance_applied_consensus_height(world: &World, proposal_id: ProposalId) -> u64 {
    world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Governance(GovernanceEvent::Applied {
                proposal_id: event_proposal_id,
                consensus_height: Some(consensus_height),
                ..
            }) if *event_proposal_id == proposal_id => Some(*consensus_height),
            _ => None,
        })
        .expect("governance applied consensus height")
}

fn build_external_finality_certificate(
    world: &World,
    proposal_id: ProposalId,
    manifest_hash: &str,
    consensus_height: u64,
) -> GovernanceFinalityCertificate {
    let epoch_len = world
        .governance_execution_policy()
        .epoch_length_ticks
        .max(1);
    let epoch_id = world.state().time / epoch_len;
    let snapshot = world
        .governance_finality_epoch_snapshots()
        .get(&epoch_id)
        .cloned()
        .expect("governance finality snapshot for current epoch");
    let min_unique_signers = snapshot.effective_min_unique_signers();
    let mut signatures = BTreeMap::new();
    for node_id in &snapshot.signer_node_ids {
        let payload = GovernanceFinalityCertificate::signing_payload_v1(
            proposal_id,
            manifest_hash,
            consensus_height,
            epoch_id,
            snapshot.validator_set_hash.as_str(),
            snapshot.stake_root.as_str(),
            snapshot.threshold_bps,
            min_unique_signers,
            node_id.as_str(),
        );
        let signing_key = test_finality_signing_key(test_finality_seed_label(node_id.as_str()));
        let signature = signing_key.sign(payload.as_slice());
        signatures.insert(
            node_id.clone(),
            format!(
                "{}{}",
                GovernanceFinalityCertificate::SIGNATURE_PREFIX_ED25519_V1,
                hex::encode(signature.to_bytes())
            ),
        );
    }

    GovernanceFinalityCertificate {
        proposal_id,
        manifest_hash: manifest_hash.to_string(),
        consensus_height,
        epoch_id,
        validator_set_hash: snapshot.validator_set_hash,
        stake_root: snapshot.stake_root,
        threshold_bps: snapshot.threshold_bps,
        min_unique_signers,
        threshold: min_unique_signers,
        signatures,
    }
}

fn derive_module_action_finality_certificate(
    world: &World,
    submit_action: impl FnOnce(&mut World),
) -> GovernanceFinalityCertificate {
    let mut simulated = world.clone();
    if !simulated
        .release_security_policy()
        .allow_local_finality_signing
    {
        let mut policy = simulated.release_security_policy().clone();
        policy.allow_local_finality_signing = true;
        simulated.set_release_security_policy(policy);
    }
    submit_action(&mut simulated);
    simulated.step().expect("simulate module action finality");
    let proposal_id = extract_latest_module_apply_proposal_id(&simulated);
    let manifest_hash = extract_queued_manifest_hash(&simulated, proposal_id);
    let consensus_height = extract_governance_applied_consensus_height(&simulated, proposal_id);
    build_external_finality_certificate(world, proposal_id, &manifest_hash, consensus_height)
}

fn assert_last_rejection_note(world: &World, action_id: ActionId, expected: &str) {
    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ActionRejected {
        action_id: rejected_action_id,
        reason: RejectReason::RuleDenied { notes },
    }) = &event.body
    else {
        panic!(
            "expected action rejected rule denied event: {:?}",
            event.body
        );
    };
    assert_eq!(*rejected_action_id, action_id);
    assert!(
        notes.iter().any(|note| note.contains(expected)),
        "missing expected note `{expected}` in {notes:?}"
    );
}

fn sample_module_source_package() -> ModuleSourcePackage {
    ModuleSourcePackage {
        manifest_path: "Cargo.toml".to_string(),
        files: BTreeMap::from([
            (
                "Cargo.toml".to_string(),
                br#"[package]
name = "sample_module"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]
"#
                .to_vec(),
            ),
            (
                "src/lib.rs".to_string(),
                b"#[unsafe(no_mangle)] pub extern \"C\" fn reduce() {}".to_vec(),
            ),
        ]),
    }
}

#[test]
fn deploy_module_artifact_action_registers_artifact_bytes() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");

    let wasm_bytes = b"module-action-loop-deploy".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes: wasm_bytes.clone(),
    });
    world.step().expect("deploy artifact");

    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ModuleArtifactDeployed {
        publisher_agent_id,
        wasm_hash: event_hash,
        bytes_len,
        fee_kind,
        fee_amount,
    }) = &event.body
    else {
        panic!("expected module artifact deployed event: {:?}", event.body);
    };
    assert_eq!(publisher_agent_id, "publisher-1");
    assert_eq!(event_hash, &wasm_hash);
    assert_eq!(*bytes_len, wasm_bytes.len() as u64);
    assert_eq!(*fee_kind, ResourceKind::Electricity);
    assert!(*fee_amount > 0);

    let loaded = world.load_module(&wasm_hash).expect("load deployed module");
    assert_eq!(loaded.wasm_hash, wasm_hash);
    assert_eq!(loaded.bytes, wasm_bytes.into());
}

#[test]
fn deploy_module_artifact_action_rejects_hash_mismatch() {
    let mut world = World::new();
    register_agent(&mut world, "publisher-1");

    let action_id = world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "publisher-1".to_string(),
        wasm_hash: "sha256-mismatch".to_string(),
        wasm_bytes: b"module-action-loop-deploy-mismatch".to_vec(),
    });
    world.step().expect("deploy mismatch action");

    assert_last_rejection_note(&world, action_id, "artifact hash mismatch");
}

#[test]
fn install_module_from_artifact_action_runs_governance_closure() {
    let mut world = World::new();
    register_agent(&mut world, "installer-1");

    let wasm_bytes = b"module-action-loop-install".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    let manifest = base_manifest("m.loop.active", "0.1.0", &wasm_hash);
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: manifest.clone(),
        activate: true,
    });
    world.step().expect("install module action");

    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ModuleInstalled {
        installer_agent_id,
        instance_id,
        module_id,
        module_version,
        wasm_hash: event_wasm_hash,
        active,
        install_target,
        proposal_id,
        manifest_hash,
        fee_kind,
        fee_amount,
    }) = &event.body
    else {
        panic!("expected module installed event: {:?}", event.body);
    };
    assert_eq!(installer_agent_id, "installer-1");
    assert!(!instance_id.is_empty());
    assert_eq!(module_id, "m.loop.active");
    assert_eq!(module_version, "0.1.0");
    assert_eq!(event_wasm_hash, &wasm_hash);
    assert!(*active);
    assert_eq!(*install_target, ModuleInstallTarget::SelfAgent);
    assert!(!manifest_hash.is_empty());
    assert_eq!(*fee_kind, ResourceKind::Electricity);
    assert!(*fee_amount > 0);

    let key = ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
    assert!(world.module_registry().records.contains_key(&key));
    assert_eq!(
        world.module_registry().active.get(&manifest.module_id),
        Some(&manifest.version)
    );
    assert!(matches!(
        world
            .proposals()
            .get(proposal_id)
            .map(|proposal| &proposal.status),
        Some(ProposalStatus::Applied { .. })
    ));
}

#[test]
fn install_module_to_target_from_artifact_action_emits_target() {
    let mut world = World::new();
    register_agent(&mut world, "installer-1");

    let wasm_bytes = b"module-action-loop-install-target".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    let manifest = base_manifest("m.loop.targeted", "0.1.0", &wasm_hash);
    world.submit_action(Action::InstallModuleToTargetFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: manifest.clone(),
        activate: true,
        install_target: ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-edge".to_string(),
        },
    });
    world.step().expect("install module to target action");

    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ModuleInstalled {
        module_id,
        install_target,
        ..
    }) = &event.body
    else {
        panic!("expected module installed event: {:?}", event.body);
    };
    assert_eq!(module_id, "m.loop.targeted");
    assert_eq!(
        *install_target,
        ModuleInstallTarget::LocationInfrastructure {
            location_id: "loc-edge".to_string(),
        }
    );
}

#[test]
fn install_module_from_artifact_action_without_activate_keeps_module_inactive() {
    let mut world = World::new();
    register_agent(&mut world, "installer-1");

    let wasm_bytes = b"module-action-loop-install-inactive".to_vec();
    let wasm_hash = util::sha256_hex(&wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes,
    });
    world.step().expect("deploy artifact");

    let manifest = base_manifest("m.loop.inactive", "0.1.0", &wasm_hash);
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: manifest.clone(),
        activate: false,
    });
    world.step().expect("install module inactive action");

    let event = world.journal().events.last().expect("last event");
    let WorldEventBody::Domain(DomainEvent::ModuleInstalled {
        active, module_id, ..
    }) = &event.body
    else {
        panic!("expected module installed event: {:?}", event.body);
    };
    assert!(!*active);
    assert_eq!(module_id, "m.loop.inactive");

    let key = ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
    assert!(world.module_registry().records.contains_key(&key));
    assert!(
        !world
            .module_registry()
            .active
            .contains_key(&manifest.module_id)
    );
}

#[test]
fn install_module_from_artifact_action_rejects_missing_artifact() {
    let mut world = World::new();
    register_agent(&mut world, "installer-1");

    let action_id = world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: base_manifest("m.loop.missing", "0.1.0", "sha256-missing"),
        activate: true,
    });
    world.step().expect("install missing artifact action");

    assert_last_rejection_note(&world, action_id, "module artifact missing");
    assert!(world.module_registry().records.is_empty());
    assert!(world.module_registry().active.is_empty());
}

#[test]
fn upgrade_module_from_artifact_action_updates_instance_and_emits_audit_event() {
    let mut world = World::new();
    register_agent(&mut world, "installer-1");

    let wasm_v1_bytes = b"module-action-loop-upgrade-v1".to_vec();
    let wasm_v1_hash = util::sha256_hex(&wasm_v1_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_v1_hash.clone(),
        wasm_bytes: wasm_v1_bytes,
    });
    world.step().expect("deploy v1 artifact");

    let manifest_v1 = base_manifest("m.loop.upgrade", "0.1.0", &wasm_v1_hash);
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: manifest_v1.clone(),
        activate: true,
    });
    world.step().expect("install v1 module");

    let (instance_id, install_target) = {
        let event = world.journal().events.last().expect("install event");
        let WorldEventBody::Domain(DomainEvent::ModuleInstalled {
            instance_id,
            install_target,
            ..
        }) = &event.body
        else {
            panic!("expected module installed event: {:?}", event.body);
        };
        (instance_id.clone(), install_target.clone())
    };

    let wasm_v2_bytes = b"module-action-loop-upgrade-v2".to_vec();
    let wasm_v2_hash = util::sha256_hex(&wasm_v2_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_v2_hash.clone(),
        wasm_bytes: wasm_v2_bytes,
    });
    world.step().expect("deploy v2 artifact");

    let manifest_v2 = base_manifest("m.loop.upgrade", "0.2.0", &wasm_v2_hash);
    world.submit_action(Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "installer-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.1.0".to_string(),
        manifest: manifest_v2.clone(),
        activate: true,
    });
    world.step().expect("upgrade module action");

    let proposal_id = {
        let event = world.journal().events.last().expect("upgrade event");
        let WorldEventBody::Domain(DomainEvent::ModuleUpgraded {
            upgrader_agent_id,
            instance_id: event_instance_id,
            module_id,
            from_module_version,
            to_module_version,
            wasm_hash,
            install_target: event_target,
            active,
            proposal_id,
            manifest_hash,
            fee_kind,
            fee_amount,
        }) = &event.body
        else {
            panic!("expected module upgraded event: {:?}", event.body);
        };
        assert_eq!(upgrader_agent_id, "installer-1");
        assert_eq!(event_instance_id, &instance_id);
        assert_eq!(module_id, "m.loop.upgrade");
        assert_eq!(from_module_version, "0.1.0");
        assert_eq!(to_module_version, "0.2.0");
        assert_eq!(wasm_hash, &wasm_v2_hash);
        assert_eq!(event_target, &install_target);
        assert!(*active);
        assert!(*proposal_id > 0);
        assert!(!manifest_hash.is_empty());
        assert_eq!(*fee_kind, ResourceKind::Electricity);
        assert!(*fee_amount > 0);
        *proposal_id
    };

    let upgraded = world
        .state()
        .module_instances
        .get(&instance_id)
        .expect("upgraded instance state");
    assert_eq!(upgraded.instance_id, instance_id);
    assert_eq!(upgraded.owner_agent_id, "installer-1");
    assert_eq!(upgraded.module_id, "m.loop.upgrade");
    assert_eq!(upgraded.module_version, "0.2.0");
    assert_eq!(upgraded.wasm_hash, wasm_v2_hash);
    assert_eq!(upgraded.install_target, install_target);
    assert!(upgraded.active);

    let v2_key = ModuleRegistry::record_key("m.loop.upgrade", "0.2.0");
    assert!(world.module_registry().records.contains_key(&v2_key));
    assert_eq!(
        world.module_registry().active.get("m.loop.upgrade"),
        Some(&"0.2.0".to_string())
    );
    assert!(matches!(
        world
            .proposals()
            .get(&proposal_id)
            .map(|proposal| &proposal.status),
        Some(ProposalStatus::Applied { .. })
    ));
}

#[test]
fn upgrade_module_from_artifact_rejects_incompatible_interface() {
    let mut world = World::new();
    register_agent(&mut world, "installer-1");

    let wasm_v1_bytes = b"module-action-loop-upgrade-guard-v1".to_vec();
    let wasm_v1_hash = util::sha256_hex(&wasm_v1_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_v1_hash.clone(),
        wasm_bytes: wasm_v1_bytes,
    });
    world.step().expect("deploy v1 artifact");

    let manifest_v1 = base_manifest("m.loop.upgrade.guard", "0.1.0", &wasm_v1_hash);
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: "installer-1".to_string(),
        manifest: manifest_v1.clone(),
        activate: true,
    });
    world.step().expect("install v1 module");
    let instance_id = {
        let event = world.journal().events.last().expect("install event");
        let WorldEventBody::Domain(DomainEvent::ModuleInstalled { instance_id, .. }) = &event.body
        else {
            panic!("expected module installed event: {:?}", event.body);
        };
        instance_id.clone()
    };

    let wasm_v2_bytes = b"module-action-loop-upgrade-guard-v2".to_vec();
    let wasm_v2_hash = util::sha256_hex(&wasm_v2_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: "installer-1".to_string(),
        wasm_hash: wasm_v2_hash.clone(),
        wasm_bytes: wasm_v2_bytes,
    });
    world.step().expect("deploy v2 artifact");

    let mut incompatible_manifest = base_manifest("m.loop.upgrade.guard", "0.2.0", &wasm_v2_hash);
    incompatible_manifest.interface_version = "wasm-2".to_string();
    let action_id = world.submit_action(Action::UpgradeModuleFromArtifact {
        upgrader_agent_id: "installer-1".to_string(),
        instance_id: instance_id.clone(),
        from_module_version: "0.1.0".to_string(),
        manifest: incompatible_manifest,
        activate: true,
    });
    world.step().expect("upgrade incompatible module");

    assert_last_rejection_note(&world, action_id, "interface_version mismatch");
    let instance = world
        .state()
        .module_instances
        .get(&instance_id)
        .expect("instance state after rejected upgrade");
    assert_eq!(instance.module_version, "0.1.0");
    let v2_key = ModuleRegistry::record_key("m.loop.upgrade.guard", "0.2.0");
    assert!(!world.module_registry().records.contains_key(&v2_key));
}

#[test]
fn module_installed_domain_event_legacy_payload_defaults_install_target() {
    let current = DomainEvent::ModuleInstalled {
        installer_agent_id: "agent-legacy".to_string(),
        instance_id: "m.legacy#1".to_string(),
        module_id: "m.legacy".to_string(),
        module_version: "0.1.0".to_string(),
        wasm_hash: "legacy-hash".to_string(),
        install_target: ModuleInstallTarget::SelfAgent,
        active: true,
        proposal_id: 7,
        manifest_hash: "manifest-hash".to_string(),
        fee_kind: ResourceKind::Electricity,
        fee_amount: 2,
    };
    let mut value = serde_json::to_value(current).expect("serialize domain event");
    value
        .get_mut("data")
        .and_then(|data| data.as_object_mut())
        .expect("domain event data")
        .remove("install_target");
    let decoded: DomainEvent = serde_json::from_value(value).expect("deserialize legacy payload");
    let DomainEvent::ModuleInstalled { install_target, .. } = decoded else {
        panic!("expected module installed event");
    };
    assert_eq!(install_target, ModuleInstallTarget::SelfAgent);
}


#[test]
fn runtime_source_compiler_is_process_local_and_policy_gated() {
    use oasis7_agent_api::module_source::{ModuleSourceCompileError, ModuleSourceCompiler};
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    struct Compiler(Arc<AtomicUsize>);
    impl ModuleSourceCompiler for Compiler {
        fn compile(&self, _: &str, _: &ModuleSourcePackage) -> Result<Vec<u8>, ModuleSourceCompileError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(b"injected-runtime-source-artifact".to_vec())
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    for production in [false, true] {
        let mut world = World::new();
        register_agent(&mut world, "publisher-1");
        if production { world.set_release_security_policy(ReleaseSecurityPolicy::production_hardened()); }
        world.set_module_source_compiler(Arc::new(Compiler(calls.clone())));
        let encoded = serde_json::to_value(&world).expect("serialize world");
        assert!(encoded.get("source_compiler").is_none());
        let before = calls.load(Ordering::SeqCst);
        let action_id = world.submit_action(Action::CompileModuleArtifactFromSource {
            publisher_agent_id: "publisher-1".into(),
            module_id: "m.injected.compiler".into(),
            source_package: sample_module_source_package(),
        });
        assert_eq!(calls.load(Ordering::SeqCst), before, "submission cannot compile");
        world.step().expect("execute compilation action");
        assert_eq!(calls.load(Ordering::SeqCst), before + usize::from(!production));
        if production {
            assert_last_rejection_note(&world, action_id, "disabled by production release policy");
        } else {
            let hash = util::sha256_hex(b"injected-runtime-source-artifact");
            assert!(world.load_module(&hash).is_ok());
        }
    }
}
