#![cfg(any(feature = "test_tier_required", feature = "test_tier_full"))]

mod common;

use oasis7::GeoPos;
use oasis7::runtime::{
    Action, Manifest, ModuleAbiContract, ModuleActivation, ModuleChangeSet, ModuleKind,
    ModuleLimits, ModuleManifest, ModuleRole, ModuleSubscription, ModuleSubscriptionStage,
    PolicySet, ProposalDecision, World, WorldEvent,
};
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallInput, ModuleCallRequest, ModuleOutput, ModuleSandbox,
};
use sha2::{Digest, Sha256};

#[cfg(feature = "test_tier_full")]
use oasis7::runtime::ActionEnvelope;

fn pos(x: i64, y: i64) -> GeoPos {
    GeoPos {
        x_cm: x,
        y_cm: y,
        z_cm: 0,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn module_manifest_hash(manifest: &ModuleManifest) -> String {
    let bytes = serde_json::to_vec(manifest).expect("serialize module manifest");
    sha256_hex(&bytes)
}

struct InspectSandbox {
    last_request: Option<ModuleCallRequest>,
}

impl InspectSandbox {
    fn new() -> Self {
        Self { last_request: None }
    }
}

impl ModuleSandbox for InspectSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.last_request = Some(request.clone());
        Ok(ModuleOutput {
            new_state: None,
            effects: Vec::new(),
            emits: Vec::new(),
            tick_lifecycle: None,
            output_bytes: 0,
        })
    }
}

fn apply_module_manifest(world: &mut World, module_manifest: ModuleManifest) {
    let changes = ModuleChangeSet {
        register: vec![module_manifest.clone()],
        activate: vec![ModuleActivation {
            module_id: module_manifest.module_id.clone(),
            version: module_manifest.version.clone(),
        }],
        ..ModuleChangeSet::default()
    };

    let mut content = serde_json::Map::new();
    content.insert(
        "module_changes".to_string(),
        serde_json::to_value(&changes).unwrap(),
    );
    let manifest = Manifest {
        version: 2,
        content: serde_json::Value::Object(content),
    };

    let proposal_id = world.propose_manifest_update(manifest, "alice").unwrap();
    world.shadow_proposal(proposal_id).unwrap();
    world
        .approve_proposal(proposal_id, "bob", ProposalDecision::Approve)
        .unwrap();
    world.apply_proposal(proposal_id).unwrap();
}

#[cfg(feature = "test_tier_required")]
#[test]
fn module_route_encodes_event_input_as_cbor() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-cbor-input";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.cbor".to_string(),
        name: "CBOR".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    let expected_manifest_hash = module_manifest_hash(&module_manifest);
    apply_module_manifest(&mut world, module_manifest);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let event = world.journal().events.last().unwrap().clone();
    let config_hash = world.current_manifest_hash().unwrap();
    let mut sandbox = InspectSandbox::new();
    world.route_event_to_modules(&event, &mut sandbox).unwrap();

    let request = sandbox.last_request.unwrap();
    let decoded: ModuleCallInput = serde_cbor::from_slice(&request.input).unwrap();
    assert_eq!(decoded.ctx.module_id, "m.cbor");
    assert_eq!(decoded.ctx.origin.kind, "event");
    assert_eq!(decoded.ctx.origin.id, event.id.to_string());
    assert_eq!(decoded.ctx.stage.as_deref(), Some("post_event"));
    assert_eq!(decoded.ctx.world_config_hash, Some(config_hash));
    assert_eq!(decoded.ctx.manifest_hash, Some(expected_manifest_hash));
    assert_eq!(decoded.ctx.journal_height, Some(event.id));
    assert_eq!(decoded.ctx.module_version.as_deref(), Some("0.1.0"));
    assert_eq!(decoded.ctx.module_kind.as_deref(), Some("reducer"));
    assert_eq!(decoded.ctx.module_role.as_deref(), Some("domain"));
    assert_eq!(decoded.state, Some(Vec::new()));
    let event_bytes = decoded.event.expect("event bytes");
    let decoded_event: WorldEvent = serde_cbor::from_slice(&event_bytes).unwrap();
    assert_eq!(decoded_event.id, event.id);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_route_encodes_action_input_as_cbor() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-cbor-action";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.cbor.action".to_string(),
        name: "CBOR Action".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    let expected_manifest_hash = module_manifest_hash(&module_manifest);
    apply_module_manifest(&mut world, module_manifest);

    let envelope = ActionEnvelope {
        committed_recipe_origin: None,
        id: 1,
        action: Action::RegisterAgent {
            agent_id: "agent-1".to_string(),
            pos: pos(0, 0),
        },
    };
    let config_hash = world.current_manifest_hash().unwrap();
    let expected_journal_height = world.journal().events.len() as u64;

    let mut sandbox = InspectSandbox::new();
    world
        .route_action_to_modules(&envelope, &mut sandbox)
        .unwrap();

    let request = sandbox.last_request.unwrap();
    let decoded: ModuleCallInput = serde_cbor::from_slice(&request.input).unwrap();
    assert_eq!(decoded.ctx.module_id, "m.cbor.action");
    assert_eq!(decoded.ctx.origin.kind, "action");
    assert_eq!(decoded.ctx.origin.id, envelope.id.to_string());
    assert_eq!(decoded.ctx.stage.as_deref(), Some("pre_action"));
    assert_eq!(decoded.ctx.world_config_hash, Some(config_hash));
    assert_eq!(decoded.ctx.manifest_hash, Some(expected_manifest_hash));
    assert_eq!(decoded.ctx.journal_height, Some(expected_journal_height));
    assert_eq!(decoded.ctx.module_version.as_deref(), Some("0.1.0"));
    assert_eq!(decoded.ctx.module_kind.as_deref(), Some("reducer"));
    assert_eq!(decoded.ctx.module_role.as_deref(), Some("domain"));
    assert_eq!(decoded.state, Some(Vec::new()));
    let action_bytes = decoded.action.expect("action bytes");
    let decoded_action: ActionEnvelope = serde_cbor::from_slice(&action_bytes).unwrap();
    assert_eq!(decoded_action.id, envelope.id);
}

#[cfg(feature = "test_tier_full")]
#[test]
fn module_route_pure_input_omits_state() {
    let mut world = oasis7::runtime_host::world::new();
    world.set_policy(PolicySet::allow_all());

    let wasm_bytes = b"module-cbor-pure";
    let wasm_hash = sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.cbor.pure".to_string(),
        name: "CBOR Pure".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: vec!["domain.agent_registered".to_string()],
            action_kinds: Vec::new(),
            stage: Some(ModuleSubscriptionStage::PostEvent),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(common::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 0,
        },
    };

    apply_module_manifest(&mut world, module_manifest);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    let event = world.journal().events.last().unwrap().clone();
    let mut sandbox = InspectSandbox::new();
    world.route_event_to_modules(&event, &mut sandbox).unwrap();

    let request = sandbox.last_request.unwrap();
    let decoded: ModuleCallInput = serde_cbor::from_slice(&request.input).unwrap();
    assert_eq!(decoded.ctx.stage.as_deref(), Some("post_event"));
    assert_eq!(decoded.ctx.module_kind.as_deref(), Some("pure"));
    assert_eq!(decoded.ctx.module_role.as_deref(), Some("domain"));
    assert!(decoded.state.is_none());
}
