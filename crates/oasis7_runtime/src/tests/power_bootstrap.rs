#![cfg(feature = "wasmtime")]

use super::super::m1_builtin_wasm_artifact::m1_builtin_manifest_hash_tokens;
use super::super::*;
use super::pos;
use oasis7_wasm_executor::WasmExecutor;
use std::collections::BTreeMap;

fn has_active(world: &World, module_id: &str) -> bool {
    world.module_registry().active.contains_key(module_id)
}

fn power_module_sandbox() -> WasmExecutor {
    WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor")
}

fn apply_module_changes(world: &mut World, actor: &str, changes: ModuleChangeSet) {
    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).expect("serialize module change set"),
    );
    let manifest = Manifest {
        version: world.manifest().version.saturating_add(1),
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world
        .propose_manifest_update(manifest, actor.to_string())
        .expect("propose changes");
    world.shadow_proposal(proposal_id).expect("shadow proposal");
    world
        .approve_proposal(proposal_id, actor.to_string(), ProposalDecision::Approve)
        .expect("approve proposal");
    world.apply_proposal(proposal_id).expect("apply proposal");
}

fn upsert_online_manifest_entry_with_identity_hash(
    world: &mut World,
    module_id: &str,
) -> Result<(), WorldError> {
    let hash_tokens = m1_builtin_manifest_hash_tokens(module_id).ok_or_else(|| {
        WorldError::ModuleChangeInvalid {
            reason: format!("missing m1 builtin hash tokens for module_id={module_id}"),
        }
    })?;
    let wasm_hash =
        hash_tokens
            .first()
            .cloned()
            .ok_or_else(|| WorldError::ModuleChangeInvalid {
                reason: format!("empty hash token list for module_id={module_id}"),
            })?;
    let identity = m1_builtin_module_artifact_identity(module_id, wasm_hash.as_str())?;
    let mut artifact_identities = BTreeMap::new();
    artifact_identities.insert(wasm_hash, identity);
    world.upsert_builtin_release_manifest_entry(
        "m1",
        module_id,
        BuiltinReleaseManifestEntry {
            hash_tokens,
            artifact_identities,
        },
    )
}

#[test]
fn m1_builtin_module_ids_manifest_matches_runtime_constants() {
    let expected = vec![
        M1_MOVE_RULE_MODULE_ID,
        M1_VISIBILITY_RULE_MODULE_ID,
        M1_TRANSFER_RULE_MODULE_ID,
        M1_BODY_MODULE_ID,
        M1_SENSOR_MODULE_ID,
        M1_MOBILITY_MODULE_ID,
        M1_MEMORY_MODULE_ID,
        M1_STORAGE_CARGO_MODULE_ID,
        M1_RADIATION_POWER_MODULE_ID,
        M1_STORAGE_POWER_MODULE_ID,
    ];
    assert_eq!(m1_builtin_module_ids_manifest(), expected);
}

#[test]
fn install_power_bootstrap_modules_registers_and_activates() {
    let mut world = World::new();
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("install modules");

    assert!(has_active(&world, M1_RADIATION_POWER_MODULE_ID));
    assert!(has_active(&world, M1_STORAGE_POWER_MODULE_ID));

    let radiation_key =
        ModuleRegistry::record_key(M1_RADIATION_POWER_MODULE_ID, M1_POWER_MODULE_VERSION);
    let storage_key =
        ModuleRegistry::record_key(M1_STORAGE_POWER_MODULE_ID, M1_POWER_MODULE_VERSION);
    assert!(world.module_registry().records.contains_key(&radiation_key));
    assert!(world.module_registry().records.contains_key(&storage_key));
}

#[test]
fn production_policy_requires_online_manifest_for_builtin_bootstrap() {
    let mut world = World::new();
    world.enable_production_release_policy();
    let err = world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect_err("production policy should require online manifest");
    match err {
        WorldError::ModuleChangeInvalid { reason } => {
            assert!(reason.contains("builtin release manifest entry missing"));
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn production_policy_rejects_identity_hash_even_with_online_manifest() {
    let mut world = World::new();
    world.enable_production_release_policy();
    upsert_online_manifest_entry_with_identity_hash(&mut world, M1_RADIATION_POWER_MODULE_ID)
        .expect("upsert online manifest for radiation module");
    upsert_online_manifest_entry_with_identity_hash(&mut world, M1_STORAGE_POWER_MODULE_ID)
        .expect("upsert online manifest for storage module");
    let err = world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect_err("identity_hash_v1 should be rejected in production policy");
    match err {
        WorldError::ModuleChangeInvalid { reason } => {
            assert!(
                reason.contains("signature_scheme identity_hash_v1 is disabled")
                    || reason.contains("builtin release artifact identity missing")
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn install_agent_default_modules_registers_and_activates() {
    let mut world = World::new();
    world
        .install_m1_agent_default_modules("bootstrap")
        .expect("install default modules");

    assert!(has_active(&world, M1_SENSOR_MODULE_ID));
    assert!(has_active(&world, M1_MOBILITY_MODULE_ID));
    assert!(has_active(&world, M1_MEMORY_MODULE_ID));
    assert!(has_active(&world, M1_STORAGE_CARGO_MODULE_ID));

    let sensor_key =
        ModuleRegistry::record_key(M1_SENSOR_MODULE_ID, M1_AGENT_DEFAULT_MODULE_VERSION);
    let mobility_key =
        ModuleRegistry::record_key(M1_MOBILITY_MODULE_ID, M1_AGENT_DEFAULT_MODULE_VERSION);
    let memory_key =
        ModuleRegistry::record_key(M1_MEMORY_MODULE_ID, M1_AGENT_DEFAULT_MODULE_VERSION);
    let cargo_key =
        ModuleRegistry::record_key(M1_STORAGE_CARGO_MODULE_ID, M1_AGENT_DEFAULT_MODULE_VERSION);

    assert!(world.module_registry().records.contains_key(&sensor_key));
    assert!(world.module_registry().records.contains_key(&mobility_key));
    assert!(world.module_registry().records.contains_key(&memory_key));
    assert!(world.module_registry().records.contains_key(&cargo_key));
}

#[test]
fn install_agent_default_modules_is_idempotent() {
    let mut world = World::new();
    world
        .install_m1_agent_default_modules("bootstrap")
        .expect("first install");
    let event_len = world.journal().len();

    world
        .install_m1_agent_default_modules("bootstrap")
        .expect("second install");

    assert_eq!(world.journal().len(), event_len);
}

#[test]
fn install_scenario_bootstrap_modules_supports_default_package_toggle() {
    let mut world = World::new();
    world
        .install_m1_scenario_bootstrap_modules(
            "bootstrap",
            M1ScenarioBootstrapConfig {
                install_default_module_package: false,
            },
        )
        .expect("install scenario bootstrap modules");

    assert!(has_active(&world, M1_RADIATION_POWER_MODULE_ID));
    assert!(has_active(&world, M1_STORAGE_POWER_MODULE_ID));
    assert!(!has_active(&world, M1_SENSOR_MODULE_ID));
    assert!(!has_active(&world, M1_MOBILITY_MODULE_ID));
    assert!(!has_active(&world, M1_MEMORY_MODULE_ID));
    assert!(!has_active(&world, M1_STORAGE_CARGO_MODULE_ID));
}

#[test]
fn install_scenario_bootstrap_modules_is_idempotent() {
    let mut world = World::new();
    let config = M1ScenarioBootstrapConfig::default();

    world
        .install_m1_scenario_bootstrap_modules("bootstrap", config)
        .expect("first scenario install");
    assert!(has_active(&world, M1_RADIATION_POWER_MODULE_ID));
    assert!(has_active(&world, M1_STORAGE_POWER_MODULE_ID));
    assert!(has_active(&world, M1_SENSOR_MODULE_ID));
    assert!(has_active(&world, M1_MOBILITY_MODULE_ID));
    assert!(has_active(&world, M1_MEMORY_MODULE_ID));
    assert!(has_active(&world, M1_STORAGE_CARGO_MODULE_ID));
    let event_len = world.journal().len();

    world
        .install_m1_scenario_bootstrap_modules("bootstrap", config)
        .expect("second scenario install");

    assert_eq!(world.journal().len(), event_len);
}

#[test]
fn install_power_bootstrap_modules_is_idempotent() {
    let mut world = World::new();
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("first install");
    let event_len = world.journal().len();

    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("second install");

    assert_eq!(world.journal().len(), event_len);
}

#[test]
fn install_power_bootstrap_modules_reactivates_registered_version() {
    let mut world = World::new();
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("initial install");

    let registered_count = world.module_registry().records.len();
    apply_module_changes(
        &mut world,
        "bootstrap",
        ModuleChangeSet {
            deactivate: vec![ModuleDeactivation {
                module_id: M1_STORAGE_POWER_MODULE_ID.to_string(),
                reason: "bootstrap test deactivate".to_string(),
            }],
            ..ModuleChangeSet::default()
        },
    );
    assert!(!has_active(&world, M1_STORAGE_POWER_MODULE_ID));

    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("reactivate install");

    assert!(has_active(&world, M1_STORAGE_POWER_MODULE_ID));
    assert_eq!(world.module_registry().records.len(), registered_count);
}

#[test]
fn radiation_module_emits_harvest_event() {
    let mut world = World::new();
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("install modules");

    let mut sandbox = power_module_sandbox();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register step");

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world.step_with_modules(&mut sandbox).expect("tick step");

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(100_000, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("harvest from move step");

    let found = world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::ModuleEmitted(module_event)
                if module_event.module_id == M1_RADIATION_POWER_MODULE_ID
                    && module_event.kind == "power.radiation_harvest"
        )
    });
    assert!(found);
}

#[test]
fn storage_module_blocks_continuous_move_when_power_runs_out() {
    let mut world = World::new();
    world
        .install_m1_power_bootstrap_modules("bootstrap")
        .expect("install modules");

    let mut sandbox = power_module_sandbox();

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("register step");

    for idx in 0..5 {
        world.submit_action(Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: pos((idx as i64 + 1) * 100_000, 0),
        });
        world
            .step_with_modules(&mut sandbox)
            .expect("move evaluation step");
    }

    let denied = world.journal().events.iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("storage insufficient"))
        )
    });
    let rejections: Vec<String> = world
        .journal()
        .events
        .iter()
        .filter_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                Some(format!("{reason:?}"))
            }
            _ => None,
        })
        .collect();
    assert!(
        denied,
        "expected storage deny, got rejections: {}",
        rejections.join(" | ")
    );
}
