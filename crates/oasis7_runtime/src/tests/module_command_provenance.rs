use super::super::*;
use super::modules::activate_module_manifest;
use super::signed_test_artifact_identity;
use oasis7_wasm_abi::{
    ModuleCallCaller, ModuleCallFailure, ModuleCallInput, ModuleCallOrigin, ModuleCallRequest,
    ModuleCommandDeclaration, ModuleCommandEnvelope, ModuleInvocationProvenance, ModuleOutput,
    ModuleSandbox, ModuleSchemaDeclarations,
};
use serde_json::json;

fn command_declaration(
    namespace: &str,
    name: &str,
    schema_version: u32,
    schema_hash: &str,
    max_payload_bytes: u64,
) -> ModuleCommandDeclaration {
    ModuleCommandDeclaration {
        namespace: namespace.to_string(),
        name: name.to_string(),
        schema_version,
        schema_hash: schema_hash.to_string(),
        max_payload_bytes,
    }
}

fn execution_command_manifest(
    module_id: &str,
    wasm_hash: &str,
    schema_hash: &str,
) -> ModuleManifest {
    ModuleManifest {
        module_id: module_id.to_string(),
        name: module_id.to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::AgentInternal,
        wasm_hash: wasm_hash.to_string(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["call".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        abi_contract: ModuleAbiContract {
            declarations: ModuleSchemaDeclarations {
                commands: vec![command_declaration(
                    "weather",
                    "observe",
                    1,
                    schema_hash,
                    128,
                )],
            },
            ..ModuleAbiContract::default()
        },
        artifact_identity: Some(signed_test_artifact_identity(wasm_hash)),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 128,
            max_effects: 0,
            max_emits: 0,
        },
    }
}

fn command_envelope(schema_hash: &str) -> ModuleCommandEnvelope {
    ModuleCommandEnvelope {
        namespace: "weather".to_string(),
        name: "observe".to_string(),
        schema_version: 1,
        schema_hash: schema_hash.to_string(),
        payload: b"{}".to_vec(),
    }
}

struct CaptureContextSandbox {
    requests: Vec<ModuleCallRequest>,
    outputs: std::collections::VecDeque<ModuleOutput>,
}

impl CaptureContextSandbox {
    fn with_outputs(outputs: Vec<ModuleOutput>) -> Self {
        Self {
            requests: Vec::new(),
            outputs: outputs.into(),
        }
    }
}

impl ModuleSandbox for CaptureContextSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.requests.push(request.clone());
        Ok(self.outputs.pop_front().unwrap_or(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        }))
    }
}

#[test]
fn execute_module_command_rejects_inactive_before_sandbox_or_world_mutation() {
    let mut world = World::new();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    let baseline_events = world.journal().events.len();
    let baseline_cache = world.module_cache_len();

    let err = world
        .execute_module_command(
            "m.inactive",
            "trace-inactive",
            command_envelope(&"00".repeat(32)),
            &mut sandbox,
        )
        .unwrap_err();

    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert!(sandbox.requests.is_empty());
    assert_eq!(world.journal().events.len(), baseline_events);
    assert_eq!(world.module_cache_len(), baseline_cache);
}

#[test]
fn execute_module_command_rejects_undeclared_and_schema_mismatch_before_invocation() {
    let wasm_bytes = b"module-command-validation";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let schema_hash = "00".repeat(32);
    let mut world = World::new();
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();
    activate_module_manifest(
        &mut world,
        execution_command_manifest("m.commands", &wasm_hash, &schema_hash),
    );

    let baseline_events = world.journal().events.len();
    let baseline_cache = world.module_cache_len();

    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    let mut undeclared = command_envelope(&schema_hash);
    undeclared.name = "unknown".to_string();
    let err = world
        .execute_module_command("m.commands", "trace-undeclared", undeclared, &mut sandbox)
        .unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert!(sandbox.requests.is_empty());

    let mut mismatched = command_envelope(&schema_hash);
    mismatched.schema_hash = "ff".repeat(32);
    let err = world
        .execute_module_command("m.commands", "trace-mismatch", mismatched, &mut sandbox)
        .unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert!(sandbox.requests.is_empty());

    let mut malformed = command_envelope(&schema_hash);
    malformed.namespace = "Weather".to_string();
    let err = world
        .execute_module_command("m.commands", "trace-malformed", malformed, &mut sandbox)
        .unwrap_err();
    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert!(sandbox.requests.is_empty());
    assert_eq!(world.journal().events.len(), baseline_events);
    assert_eq!(world.module_cache_len(), baseline_cache);
}

#[test]
fn execute_module_command_passes_canonical_envelope_through_existing_call_pipeline() {
    let wasm_bytes = b"module-command-success";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let schema_hash = "11".repeat(32);
    let mut world = World::new();
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();
    activate_module_manifest(
        &mut world,
        execution_command_manifest("m.commands", &wasm_hash, &schema_hash),
    );

    let envelope = command_envelope(&schema_hash);
    let canonical = envelope.encode_canonical().unwrap();
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    }]);

    world
        .execute_module_command("m.commands", "trace-command", envelope, &mut sandbox)
        .unwrap();

    assert_eq!(sandbox.requests.len(), 1);
    let input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[0].input).expect("decode module call input");
    assert_eq!(input.action.as_deref(), Some(canonical.as_slice()));
    assert!(input.event.is_none());
    assert!(input.state.is_none());
    assert_eq!(sandbox.requests[0].trace_id, "trace-command");
    assert_eq!(world.module_cache_len(), 1);
}

#[test]
fn execute_module_command_with_provenance_injects_agent_identity_outside_payload() {
    let wasm_bytes = b"module-command-provenance";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let schema_hash = "22".repeat(32);
    let mut world = World::new();
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();
    activate_module_manifest(
        &mut world,
        execution_command_manifest("m.provenance", &wasm_hash, &schema_hash),
    );

    // The command payload is untrusted data. It deliberately contains a
    // spoofed context-shaped value; the host must inject the real provenance.
    let spoofed_payload = serde_cbor::to_vec(&json!({
        "ctx": {
            "caller": {"agent_id": "spoofed-agent"},
            "origin": {"kind": "spoofed", "id": "spoofed-decision"}
        }
    }))
    .unwrap();
    let envelope = ModuleCommandEnvelope {
        payload: spoofed_payload,
        ..command_envelope(&schema_hash)
    };
    let provenance = ModuleInvocationProvenance {
        caller: ModuleCallCaller::Agent {
            agent_id: "agent-7".to_string(),
        },
        origin: ModuleCallOrigin {
            kind: "agent_decision".to_string(),
            id: "decision-7".to_string(),
        },
    };
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    }]);

    world
        .execute_module_command_with_provenance(
            "m.provenance",
            "trace-provenance",
            envelope,
            provenance.clone(),
            &mut sandbox,
        )
        .unwrap();

    let input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[0].input).expect("decode module call input");
    assert_eq!(input.ctx.caller, provenance.caller);
    assert_eq!(input.ctx.origin, provenance.origin);
    assert_ne!(input.ctx.origin.id, "spoofed-decision");
}

#[test]
fn legacy_execute_module_command_has_explicit_unspecified_provenance() {
    let wasm_bytes = b"module-command-legacy-provenance";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let schema_hash = "33".repeat(32);
    let mut world = World::new();
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();
    activate_module_manifest(
        &mut world,
        execution_command_manifest("m.legacy", &wasm_hash, &schema_hash),
    );
    let mut sandbox = CaptureContextSandbox::with_outputs(vec![ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: Vec::new(),
        tick_lifecycle: None,
        output_bytes: 0,
    }]);

    world
        .execute_module_command(
            "m.legacy",
            "trace-legacy",
            command_envelope(&schema_hash),
            &mut sandbox,
        )
        .unwrap();

    let input: ModuleCallInput =
        serde_cbor::from_slice(&sandbox.requests[0].input).expect("decode module call input");
    assert_eq!(input.ctx.caller, ModuleCallCaller::LegacyUnspecified);
    assert_eq!(input.ctx.origin.kind, "legacy_unspecified");
}

#[test]
fn expired_required_capability_is_rejected_before_module_side_effects() {
    let wasm_bytes = b"module-command-expired-cap";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    let schema_hash = "44".repeat(32);
    let mut world = World::new();
    world.add_capability(CapabilityGrant {
        name: "cap.expiring".to_string(),
        effect_kinds: vec!["*".to_string()],
        expiry: Some(0),
    });
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();
    let mut module = execution_command_manifest("m.expiring", &wasm_hash, &schema_hash);
    module.required_caps = vec!["cap.expiring".to_string()];
    activate_module_manifest(&mut world, module);
    world.step().unwrap();

    let baseline_events = world.journal().events.len();
    let baseline_cache = world.module_cache_len();
    let mut sandbox = CaptureContextSandbox::with_outputs(Vec::new());
    let err = world
        .execute_module_command_with_provenance(
            "m.expiring",
            "trace-expired-cap",
            command_envelope(&schema_hash),
            ModuleInvocationProvenance {
                caller: ModuleCallCaller::System {
                    system_id: "runtime-test".to_string(),
                },
                origin: ModuleCallOrigin {
                    kind: "system".to_string(),
                    id: "runtime-test".to_string(),
                },
            },
            &mut sandbox,
        )
        .unwrap_err();

    assert!(matches!(err, WorldError::ModuleChangeInvalid { .. }));
    assert!(sandbox.requests.is_empty());
    assert_eq!(world.journal().events.len(), baseline_events);
    assert_eq!(world.module_cache_len(), baseline_cache);
}
