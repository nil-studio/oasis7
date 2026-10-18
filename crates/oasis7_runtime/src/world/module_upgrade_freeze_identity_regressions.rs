use super::World;
use crate::state::ModuleAdmissionFreeze;
use crate::{ModuleChangeSet, ModuleUpgrade};
use oasis7_wasm_abi::{
    ModuleAbiContract, ModuleActivation, ModuleEvent, ModuleEventKind, ModuleKind, ModuleLimits,
    ModuleManifest, ModuleRole,
};

const MODULE_ID: &str = "m.rollback-freeze-identity";
const SOURCE_VERSION: &str = "1.0.0";
const FROZEN_VERSION: &str = "2.0.0";

fn manifest(version: &str, wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: MODULE_ID.into(),
        name: format!("Rollback Freeze Identity {version}"),
        version: version.into(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.into(),
        interface_version: "wasm-1".into(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".into()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(crate::tests::signed_test_artifact_identity(
            wasm_hash,
        )),
        limits: ModuleLimits::default(),
    }
}

fn fixture() -> (World, String) {
    let mut world = World::new();
    let source_bytes = b"rollback-freeze-source";
    let source_hash = crate::util::sha256_hex(source_bytes);
    world
        .register_module_artifact(source_hash.clone(), source_bytes)
        .expect("register active source artifact");
    let source = manifest(SOURCE_VERSION, &source_hash);
    world
        .apply_module_event(
            &ModuleEvent {
                proposal_id: 1,
                kind: ModuleEventKind::RegisterModule {
                    module: source,
                    registered_by: "fixture".into(),
                },
            },
            world.state.time,
        )
        .expect("register source module");
    world
        .apply_module_event(
            &ModuleEvent {
                proposal_id: 1,
                kind: ModuleEventKind::ActivateModule {
                    module_id: MODULE_ID.into(),
                    version: SOURCE_VERSION.into(),
                    activated_by: "fixture".into(),
                },
            },
            world.state.time,
        )
        .expect("activate source module");

    let frozen_bytes = b"rollback-freeze-frozen";
    let frozen_hash = crate::util::sha256_hex(frozen_bytes);
    world
        .register_module_artifact(frozen_hash.clone(), frozen_bytes)
        .expect("register frozen artifact bytes");
    let key = ModuleAdmissionFreeze::key(MODULE_ID, FROZEN_VERSION, &frozen_hash);
    world.state.module_admission_freezes.insert(
        key,
        ModuleAdmissionFreeze {
            module_id: MODULE_ID.into(),
            module_version: FROZEN_VERSION.into(),
            wasm_hash: frozen_hash.clone(),
            rollback_proposal_id: 2,
            source_release_request_id: None,
            reason: "rollback_stop_new_admission".into(),
        },
    );
    (world, frozen_hash)
}

fn upgrade_changes(to_version: &str, wasm_hash: &str, manifest: ModuleManifest) -> ModuleChangeSet {
    ModuleChangeSet {
        upgrade: vec![ModuleUpgrade {
            module_id: MODULE_ID.into(),
            from_version: SOURCE_VERSION.into(),
            to_version: to_version.into(),
            wasm_hash: wasm_hash.into(),
            manifest,
        }],
        activate: vec![ModuleActivation {
            module_id: MODULE_ID.into(),
            version: to_version.into(),
        }],
        ..ModuleChangeSet::default()
    }
}

#[test]
fn generic_upgrade_rejects_target_version_mismatch_before_admission() {
    let (world, frozen_hash) = fixture();
    let changes = upgrade_changes(
        FROZEN_VERSION,
        &frozen_hash,
        manifest("3.0.0", &frozen_hash),
    );

    let error = world
        .validate_module_changes(&changes)
        .expect_err("a frozen registry target must not be hidden by a different manifest version");
    assert!(format!("{error:?}").contains("upgrade target version"));
}

#[test]
fn generic_upgrade_rejects_redundant_wasm_hash_mismatch_before_admission() {
    let (world, frozen_hash) = fixture();
    let other_bytes = b"rollback-freeze-other";
    let other_hash = crate::util::sha256_hex(other_bytes);
    let mut world = world;
    world
        .register_module_artifact(other_hash.clone(), other_bytes)
        .expect("register alternate artifact bytes");
    let changes = upgrade_changes(
        FROZEN_VERSION,
        &frozen_hash,
        manifest(FROZEN_VERSION, &other_hash),
    );

    let error = world
        .validate_module_changes(&changes)
        .expect_err("redundant upgrade hash must match the manifest before admission");
    assert!(format!("{error:?}").contains("upgrade wasm_hash"));
}
