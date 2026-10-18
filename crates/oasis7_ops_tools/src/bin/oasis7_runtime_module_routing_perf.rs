use std::collections::VecDeque;
use std::hint::black_box;
use std::process;
use std::time::Instant;

use oasis7_runtime::{
    Action, ActionEnvelope, DomainEvent, Journal, ModuleAbiContract, ModuleArtifactIdentity,
    ModuleKind, ModuleLimits, ModuleManifest, ModuleRecord, ModuleRegistry, ModuleRole,
    ModuleSubscription, ModuleSubscriptionStage, PolicySet, World, WorldEvent, WorldEventBody,
};
use oasis7_wasm_abi::{ModuleCallFailure, ModuleCallRequest, ModuleOutput, ModuleSandbox};
use sha2::{Digest, Sha256};

const MODULE_COUNT: usize = 192;
const ITERATIONS: usize = 80;
const TEST_MODULE_ARTIFACT_SIGNER_NODE_ID: &str = "runtime.module.routing.perf";
const IDENTITY_HASH_SIGNATURE_SCHEME: &str = "identity_hash_v1";
const IDENTITY_HASH_SIGNATURE_PREFIX: &str = "idhash:";

fn main() {
    if let Err(err) = run() {
        eprintln!("runtime module routing perf failed: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let wasm_bytes = b"module-perf-router";
    let wasm_hash = sha256_hex(wasm_bytes);

    let mut manifests = Vec::with_capacity(MODULE_COUNT);
    for idx in 0..MODULE_COUNT {
        let module_id = format!("m.perf-router.{idx:03}");
        manifests.push(ModuleManifest {
            module_id: module_id.clone(),
            name: format!("PerfRouter{idx:03}"),
            version: "0.1.0".to_string(),
            kind: if idx % 5 == 0 {
                ModuleKind::Pure
            } else {
                ModuleKind::Reducer
            },
            role: ModuleRole::Domain,
            wasm_hash: wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec![if idx % 5 == 0 { "call" } else { "reduce" }.to_string()],
            subscriptions: vec![
                ModuleSubscription {
                    event_kinds: vec!["domain.agent_registered".to_string()],
                    action_kinds: Vec::new(),
                    stage: Some(ModuleSubscriptionStage::PostEvent),
                    filters: None,
                },
                ModuleSubscription {
                    event_kinds: Vec::new(),
                    action_kinds: vec!["action.register_agent".to_string()],
                    stage: Some(ModuleSubscriptionStage::PreAction),
                    filters: None,
                },
            ],
            required_caps: Vec::new(),
            artifact_identity: Some(identity_hash_artifact_identity(
                wasm_hash.as_str(),
                module_id.as_str(),
            )),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 1,
                max_output_bytes: 1024,
                max_effects: 0,
                max_emits: 0,
            },
        });
    }
    let mut world = build_world_with_active_manifests(&manifests, wasm_bytes, &wasm_hash)?;
    world.set_policy(PolicySet::allow_all());

    let event = WorldEvent {
        id: 1,
        time: 1,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
            agent_id: "agent-perf".to_string(),
            pos: pos(0, 0),
        }),
    };
    let action = ActionEnvelope {
        committed_recipe_origin: None,
        id: 1,
        action: Action::RegisterAgent {
            agent_id: "agent-perf".to_string(),
            pos: pos(0, 0),
        },
    };

    let mut warmup_sandbox = CaptureContextSandbox::default();
    let warm_event = world
        .route_event_to_modules(&event, &mut warmup_sandbox)
        .map_err(|err| format!("warm route_event_to_modules: {err:?}"))?;
    let warm_action = world
        .route_action_to_modules(&action, &mut warmup_sandbox)
        .map_err(|err| format!("warm route_action_to_modules: {err:?}"))?;
    if warm_event != MODULE_COUNT || warm_action != MODULE_COUNT {
        return Err(format!(
            "unexpected warmup invocation counts event={warm_event} action={warm_action}"
        ));
    }

    let event_started_at = Instant::now();
    let mut event_invoked = 0usize;
    for _ in 0..ITERATIONS {
        let mut sandbox = CaptureContextSandbox::default();
        let invoked = world
            .route_event_to_modules(&event, &mut sandbox)
            .map_err(|err| format!("route_event_to_modules: {err:?}"))?;
        event_invoked = event_invoked.saturating_add(black_box(invoked));
    }
    let event_elapsed = event_started_at.elapsed();

    let action_started_at = Instant::now();
    let mut action_invoked = 0usize;
    for _ in 0..ITERATIONS {
        let mut sandbox = CaptureContextSandbox::default();
        let invoked = world
            .route_action_to_modules(&action, &mut sandbox)
            .map_err(|err| format!("route_action_to_modules: {err:?}"))?;
        action_invoked = action_invoked.saturating_add(black_box(invoked));
    }
    let action_elapsed = action_started_at.elapsed();

    if event_invoked != MODULE_COUNT * ITERATIONS {
        return Err(format!(
            "unexpected event invocation total expected={} actual={event_invoked}",
            MODULE_COUNT * ITERATIONS
        ));
    }
    if action_invoked != MODULE_COUNT * ITERATIONS {
        return Err(format!(
            "unexpected action invocation total expected={} actual={action_invoked}",
            MODULE_COUNT * ITERATIONS
        ));
    }

    println!(
        "perf_probe_runtime_module_routing_with_many_active_manifests: modules={} iterations={} event_total_ms={:.3} event_avg_ms={:.3} action_total_ms={:.3} action_avg_ms={:.3} event_invoked={} action_invoked={}",
        MODULE_COUNT,
        ITERATIONS,
        event_elapsed.as_secs_f64() * 1000.0,
        event_elapsed.as_secs_f64() * 1000.0 / ITERATIONS as f64,
        action_elapsed.as_secs_f64() * 1000.0,
        action_elapsed.as_secs_f64() * 1000.0 / ITERATIONS as f64,
        event_invoked,
        action_invoked,
    );

    Ok(())
}

fn build_world_with_active_manifests(
    manifests: &[ModuleManifest],
    wasm_bytes: &[u8],
    wasm_hash: &str,
) -> Result<World, String> {
    let mut base = oasis7::runtime_host::world::new();
    base.set_policy(PolicySet::allow_all());
    base.register_module_artifact(wasm_hash.to_string(), wasm_bytes)
        .map_err(|err| format!("register_module_artifact: {err:?}"))?;

    let mut snapshot = base.snapshot();
    snapshot.module_registry = build_module_registry(manifests);

    let mut world = oasis7::runtime_host::world::from_snapshot(snapshot, Journal::default())
        .map_err(|err| format!("from_snapshot: {err:?}"))?;
    world
        .register_module_artifact(wasm_hash.to_string(), wasm_bytes)
        .map_err(|err| format!("register_module_artifact(restored): {err:?}"))?;
    Ok(world)
}

fn build_module_registry(manifests: &[ModuleManifest]) -> ModuleRegistry {
    let mut registry = ModuleRegistry::default();
    for manifest in manifests {
        let key = ModuleRegistry::record_key(&manifest.module_id, &manifest.version);
        registry.records.insert(
            key,
            ModuleRecord {
                manifest: manifest.clone(),
                registered_at: 0,
                registered_by: TEST_MODULE_ARTIFACT_SIGNER_NODE_ID.to_string(),
                audit_event_id: None,
            },
        );
        registry
            .active
            .insert(manifest.module_id.clone(), manifest.version.clone());
    }
    registry
}

fn identity_hash_artifact_identity(wasm_hash: &str, module_id: &str) -> ModuleArtifactIdentity {
    let source_hash = sha256_hex(format!("test-src:{wasm_hash}").as_bytes());
    let build_manifest_hash = sha256_hex(b"test-build-manifest-v1");
    let identity_hash =
        sha256_hex(format!("{module_id}:{source_hash}:{build_manifest_hash}").as_bytes());
    ModuleArtifactIdentity {
        source_hash,
        build_manifest_hash,
        signer_node_id: TEST_MODULE_ARTIFACT_SIGNER_NODE_ID.to_string(),
        signature_scheme: IDENTITY_HASH_SIGNATURE_SCHEME.to_string(),
        artifact_signature: format!("{IDENTITY_HASH_SIGNATURE_PREFIX}{identity_hash}"),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn pos(x: i64, y: i64) -> oasis7_agent_api::world_action::GeoPos {
    oasis7_agent_api::world_action::GeoPos {
        x_cm: x,
        y_cm: y,
        z_cm: 0,
    }
}

#[derive(Default)]
struct CaptureContextSandbox {
    requests: Vec<ModuleCallRequest>,
    outputs: VecDeque<ModuleOutput>,
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
