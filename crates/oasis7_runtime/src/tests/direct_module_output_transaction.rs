use super::super::*;
use super::modules::activate_module_manifest;
use super::signed_test_artifact_identity;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleCommandDeclaration,
    ModuleCommandEnvelope, ModuleEmit, ModuleOutput, ModuleSandbox, ModuleSchemaDeclarations,
};

const MODULE_ID: &str = "m.direct-output.transaction";
const COMMAND_NAMESPACE: &str = "direct_output";
const COMMAND_NAME: &str = "mutate";
const COMMAND_SCHEMA_HASH: &str =
    "abababababababababababababababababababababababababababababababab";
const AUTHORITY_NODE_ID: &str = "authority.direct-output.transaction";

#[derive(Clone)]
struct FixedOutputSandbox {
    output: ModuleOutput,
    calls: usize,
}

impl FixedOutputSandbox {
    fn state_writer() -> Self {
        Self {
            output: ModuleOutput {
                // This is the mutation that the current implementation applies
                // before the enclosing event publication fails.
                new_state: Some(vec![0xd1, 0x5c, 0xa5]),
                effects: Vec::new(),
                emits: Vec::new(),
                tick_lifecycle: None,
                output_bytes: 3,
            },
            calls: 0,
        }
    }
}

impl ModuleSandbox for FixedOutputSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.calls = self.calls.saturating_add(1);
        Ok(self.output.clone())
    }
}

struct FailingSandbox;

impl ModuleSandbox for FailingSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Err(ModuleCallFailure {
            module_id: request.module_id.clone(),
            trace_id: request.trace_id.clone(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail: "deterministic direct failure".to_string(),
        })
    }
}

fn active_manifest(wasm_hash: &str) -> ModuleManifest {
    ModuleManifest {
        module_id: MODULE_ID.to_string(),
        name: "DirectOutputTransaction".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        abi_contract: ModuleAbiContract {
            declarations: ModuleSchemaDeclarations {
                commands: vec![ModuleCommandDeclaration {
                    namespace: COMMAND_NAMESPACE.to_string(),
                    name: COMMAND_NAME.to_string(),
                    schema_version: 1,
                    schema_hash: COMMAND_SCHEMA_HASH.to_string(),
                    max_payload_bytes: 64,
                }],
            },
            ..ModuleAbiContract::default()
        },
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits::unbounded(),
    }
}

fn command_envelope() -> ModuleCommandEnvelope {
    ModuleCommandEnvelope {
        namespace: COMMAND_NAMESPACE.to_string(),
        name: COMMAND_NAME.to_string(),
        schema_version: 1,
        schema_hash: COMMAND_SCHEMA_HASH.to_string(),
        payload: vec![0x42],
    }
}

fn world_with_active_module() -> World {
    let wasm_bytes = b"direct-module-output-transaction";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let mut world = World::new();
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .expect("register deterministic module artifact");
    activate_module_manifest(&mut world, active_manifest(&wasm_hash));

    world
}

fn world_with_active_module_and_authority_drift() -> World {
    let mut world = world_with_active_module();

    // The active module was installed under the builtin authority.  Changing
    // the authority for this same tick makes the next publication fail after
    // apply_event_body_at has already applied the module state update.
    world
        .bind_node_identity(AUTHORITY_NODE_ID, "authority-direct-output-key")
        .expect("bind deterministic test authority");
    world
        .set_tick_consensus_authority_source(AUTHORITY_NODE_ID)
        .expect("select deterministic test authority");
    world
}

fn visual_output(kind: &str, payload: serde_json::Value) -> ModuleOutput {
    ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: kind.to_string(),
            payload,
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    }
}

fn assert_no_partial_publication(
    world: &World,
    snapshot_before: &Snapshot,
    journal_before: &Journal,
    pending_effects_before: usize,
    backpressure_before: &WorldRuntimeBackpressureStats,
) {
    assert_eq!(world.snapshot(), *snapshot_before);
    assert_eq!(world.journal(), journal_before);
    assert_eq!(world.pending_effects_len(), pending_effects_before);
    assert_eq!(world.runtime_backpressure_stats(), backpressure_before);
    assert!(
        !world.state().module_states.contains_key(MODULE_ID),
        "failed direct output must not retain the module state update"
    );
}

#[test]
fn direct_module_call_output_failure_does_not_publish_partial_world() {
    let mut world = world_with_active_module_and_authority_drift();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let pending_effects_before = world.pending_effects_len();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut sandbox = FixedOutputSandbox::state_writer();

    let error = world
        .execute_module_call(
            MODULE_ID,
            "trace-direct-output-call",
            vec![0x01],
            &mut sandbox,
        )
        .expect_err("authority drift must fail output publication");

    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert_eq!(
        sandbox.calls, 1,
        "the direct call must reach the sandbox once"
    );
    assert_no_partial_publication(
        &world,
        &snapshot_before,
        &journal_before,
        pending_effects_before,
        &backpressure_before,
    );
    assert!(
        !world
            .journal()
            .events
            .iter()
            .any(|event| { matches!(event.body, WorldEventBody::ModuleCallFailed(_)) }),
        "infrastructure publication failure must not append a second failure audit event"
    );
}

#[test]
fn direct_module_call_visual_emit_updates_snapshot_and_recovery_state() {
    const ENTITY_ID: &str = "direct-output-relay";

    let mut world = world_with_active_module();
    let baseline = world.snapshot();
    let mut upsert_sandbox = FixedOutputSandbox {
        output: visual_output(
            "module_visual_entity_upserted",
            serde_json::json!({
                "entity": {
                    "entity_id": ENTITY_ID,
                    "module_id": MODULE_ID,
                    "kind": "relay",
                    "label": "Direct output relay",
                    "anchor": {
                        "type": "absolute",
                        "data": { "pos": { "x_cm": 100, "y_cm": 200, "z_cm": 0 } }
                    }
                }
            }),
        ),
        calls: 0,
    };
    world
        .execute_module_call(
            MODULE_ID,
            "trace-direct-visual-upsert",
            Vec::new(),
            &mut upsert_sandbox,
        )
        .expect("real module output upsert");
    assert_eq!(
        world
            .state()
            .module_visual_entities
            .get(ENTITY_ID)
            .map(|entity| entity.module_id.as_str()),
        Some(MODULE_ID)
    );
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .expect("visual upsert consensus record")
            .block
            .header
            .state_root,
        world
            .current_state_root_hash()
            .expect("visual upsert state root")
    );

    let mut remove_sandbox = FixedOutputSandbox {
        output: visual_output(
            "module_visual_entity_removed",
            serde_json::json!({ "entity_id": ENTITY_ID }),
        ),
        calls: 0,
    };
    world
        .execute_module_call(
            MODULE_ID,
            "trace-direct-visual-remove",
            Vec::new(),
            &mut remove_sandbox,
        )
        .expect("real module output removal");
    assert!(!world.state().module_visual_entities.contains_key(ENTITY_ID));
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .expect("visual removal consensus record")
            .block
            .header
            .state_root,
        world
            .current_state_root_hash()
            .expect("visual removal state root")
    );

    let recovered = World::from_snapshot(baseline, world.journal().clone())
        .expect("recover real module visual output journal");
    assert!(
        !recovered
            .state()
            .module_visual_entities
            .contains_key(ENTITY_ID)
    );
}

#[test]
fn direct_module_call_failure_after_prepare_publishes_no_observable_delta() {
    let mut world = world_with_active_module();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let pending_effects_before = world.pending_effects_len();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut sandbox = FixedOutputSandbox::state_writer();

    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .execute_module_call(
            MODULE_ID,
            "trace-direct-output-post-prepare",
            vec![0x03],
            &mut sandbox,
        )
        .expect_err("post-prepare failure must abort direct module publication");

    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_eq!(
        sandbox.calls, 1,
        "the direct call must reach the sandbox once"
    );
    assert_no_partial_publication(
        &world,
        &snapshot_before,
        &journal_before,
        pending_effects_before,
        &backpressure_before,
    );
    assert_eq!(world.tick_consensus_records(), consensus_before.as_slice());
}

#[test]
fn direct_module_command_output_failure_does_not_publish_partial_world() {
    let mut world = world_with_active_module_and_authority_drift();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let pending_effects_before = world.pending_effects_len();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut sandbox = FixedOutputSandbox::state_writer();

    let error = world
        .execute_module_command(
            MODULE_ID,
            "trace-direct-output-command",
            command_envelope(),
            &mut sandbox,
        )
        .expect_err("authority drift must fail command output publication");

    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert_eq!(sandbox.calls, 1, "the command must reach the sandbox once");
    assert_no_partial_publication(
        &world,
        &snapshot_before,
        &journal_before,
        pending_effects_before,
        &backpressure_before,
    );
    assert!(
        !world
            .journal()
            .events
            .iter()
            .any(|event| { matches!(event.body, WorldEventBody::ModuleCallFailed(_)) }),
        "infrastructure publication failure must not append a second failure audit event"
    );
}

#[test]
fn direct_module_failure_audit_publication_is_atomic() {
    let mut world = world_with_active_module_and_authority_drift();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let pending_effects_before = world.pending_effects_len();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let mut sandbox = FailingSandbox;

    let error = world
        .execute_module_call(
            MODULE_ID,
            "trace-direct-failure-audit",
            vec![0x02],
            &mut sandbox,
        )
        .expect_err("authority drift must fail failure-audit publication");

    assert!(matches!(
        error,
        WorldError::DistributedValidationFailed { .. }
    ));
    assert_no_partial_publication(
        &world,
        &snapshot_before,
        &journal_before,
        pending_effects_before,
        &backpressure_before,
    );
}
