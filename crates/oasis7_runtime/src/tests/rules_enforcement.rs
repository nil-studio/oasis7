use super::super::*;
use super::pos;
use crate::agent_state::BodyKernelView;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallRequest, ModuleEmit, ModuleOutput,
    ModuleSandbox,
};
use oasis7_wasm_executor::FixedSandbox;
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
use oasis7_wasm_executor::WasmExecutor;
use std::collections::BTreeMap;

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
fn register_m1_builtin_wasm_artifact(world: &mut World, module_id: &str) -> String {
    super::super::register_m1_builtin_wasm_module_artifact(world, module_id)
        .expect("register embedded m1 builtin wasm module artifact")
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
fn install_m1_move_rule(world: &mut World) {
    let wasm_hash = register_m1_builtin_wasm_artifact(world, M1_MOVE_RULE_MODULE_ID);

    let module_manifest = ModuleManifest {
        module_id: M1_MOVE_RULE_MODULE_ID.to_string(),
        name: "M1MoveRule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.move_agent".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024 * 1024,
            max_gas: 2_000_000,
            max_call_rate: 10,
            max_output_bytes: 2048,
            max_effects: 0,
            max_emits: 1,
        },
    };

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

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
fn install_m1_visibility_rule(world: &mut World) {
    let wasm_hash = register_m1_builtin_wasm_artifact(world, M1_VISIBILITY_RULE_MODULE_ID);

    let module_manifest = ModuleManifest {
        module_id: M1_VISIBILITY_RULE_MODULE_ID.to_string(),
        name: "M1VisibilityRule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.query_observation".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024 * 1024,
            max_gas: 2_000_000,
            max_call_rate: 10,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    };

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

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
fn install_m1_transfer_rule(world: &mut World) {
    let wasm_hash = register_m1_builtin_wasm_artifact(world, M1_TRANSFER_RULE_MODULE_ID);

    let module_manifest = ModuleManifest {
        module_id: M1_TRANSFER_RULE_MODULE_ID.to_string(),
        name: "M1TransferRule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["reduce".to_string()],
        subscriptions: vec![
            ModuleSubscription {
                event_kinds: vec![
                    "domain.agent_registered".to_string(),
                    "domain.agent_moved".to_string(),
                ],
                action_kinds: Vec::new(),
                stage: Some(ModuleSubscriptionStage::PostEvent),
                filters: None,
            },
            ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec!["action.transfer_resource".to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            },
        ],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 64 * 1024 * 1024,
            max_gas: 2_000_000,
            max_call_rate: 10,
            max_output_bytes: 4096,
            max_effects: 0,
            max_emits: 1,
        },
    };

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

#[derive(Debug, Clone)]
struct MapSandbox {
    outputs: BTreeMap<String, ModuleOutput>,
}

impl MapSandbox {
    fn new(outputs: BTreeMap<String, ModuleOutput>) -> Self {
        Self { outputs }
    }
}

impl ModuleSandbox for MapSandbox {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        self.outputs
            .get(&request.module_id)
            .cloned()
            .ok_or_else(|| ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::SandboxUnavailable,
                detail: "module output missing".to_string(),
            })
    }
}

fn install_rule_modules(world: &mut World, action_kind: &str, module_ids: &[&str]) {
    let mut manifests = Vec::new();
    for module_id in module_ids {
        let wasm_bytes = format!("rule-{module_id}").into_bytes();
        let wasm_hash = util::sha256_hex(&wasm_bytes);
        world
            .register_module_artifact(wasm_hash.clone(), &wasm_bytes)
            .unwrap();

        manifests.push(ModuleManifest {
            module_id: (*module_id).to_string(),
            name: format!("Rule-{module_id}"),
            version: "0.1.0".to_string(),
            kind: ModuleKind::Pure,
            role: ModuleRole::Rule,
            wasm_hash: wasm_hash.clone(),
            interface_version: "wasm-1".to_string(),
            abi_contract: ModuleAbiContract::default(),
            exports: vec!["call".to_string()],
            subscriptions: vec![ModuleSubscription {
                event_kinds: Vec::new(),
                action_kinds: vec![action_kind.to_string()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            }],
            required_caps: Vec::new(),
            artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
            limits: ModuleLimits {
                max_mem_bytes: 1024,
                max_gas: 10_000,
                max_call_rate: 10,
                max_output_bytes: 2048,
                max_effects: 0,
                max_emits: 1,
            },
        });
    }

    let changes = ModuleChangeSet {
        register: manifests.clone(),
        activate: manifests
            .iter()
            .map(|manifest| ModuleActivation {
                module_id: manifest.module_id.clone(),
                version: manifest.version.clone(),
            })
            .collect(),
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

fn rule_decision_with_notes(
    action_id: u64,
    verdict: RuleVerdict,
    override_action: Option<Action>,
    notes: &[&str],
) -> RuleDecision {
    RuleDecision {
        action_id,
        verdict,
        override_action,
        cost: ResourceDelta::default(),
        notes: notes.iter().map(|note| note.to_string()).collect(),
    }
}

fn rule_decision_output(decision: RuleDecision) -> ModuleOutput {
    ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "rule.decision".to_string(),
            payload: serde_json::to_value(decision).unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 256,
    }
}

#[test]
fn rule_decision_override_and_cost_apply() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());
    world.set_resource_balance(ResourceKind::Electricity, 5);

    let wasm_bytes = b"rule-decision-override";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.rule".to_string(),
        name: "Rule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };

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

    let override_action = Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(2, 3),
    };
    let mut cost = ResourceDelta::default();
    cost.entries.insert(ResourceKind::Electricity, -3);
    let decision = RuleDecision {
        action_id: 1,
        verdict: RuleVerdict::Modify,
        override_action: Some(override_action.clone()),
        cost,
        notes: vec!["override".to_string()],
    };
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "rule.decision".to_string(),
            payload: serde_json::to_value(&decision).unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 128,
    };
    let mut sandbox = FixedSandbox::succeed(output);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let agent = world.state().agents.get("agent-1").unwrap();
    assert_eq!(agent.state.pos, pos(2, 3));
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 2);
    assert!(world
        .journal()
        .events
        .iter()
        .any(|event| matches!(event.body, WorldEventBody::ActionOverridden(_))));
}

#[test]
fn rule_decision_rejects_on_insufficient_resources() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());
    world.set_resource_balance(ResourceKind::Electricity, 1);

    let wasm_bytes = b"rule-decision-cost";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.rule.cost".to_string(),
        name: "RuleCost".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };

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

    let mut cost = ResourceDelta::default();
    cost.entries.insert(ResourceKind::Electricity, -3);
    let decision = RuleDecision {
        action_id: 1,
        verdict: RuleVerdict::Allow,
        override_action: None,
        cost,
        notes: Vec::new(),
    };
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "rule.decision".to_string(),
            payload: serde_json::to_value(&decision).unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 128,
    };
    let mut sandbox = FixedSandbox::succeed(output);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    assert!(!world.state().agents.contains_key("agent-1"));
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 1);
    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::InsufficientResources { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn rule_decision_cost_overflow_rejected_without_partial_apply() {
    let mut world = World::new();
    world.set_policy(PolicySet::allow_all());
    world.set_resource_balance(ResourceKind::Electricity, 5);
    world.set_resource_balance(ResourceKind::Data, i64::MAX);

    let wasm_bytes = b"rule-decision-cost-overflow";
    let wasm_hash = util::sha256_hex(wasm_bytes);
    world
        .register_module_artifact(wasm_hash.clone(), wasm_bytes)
        .unwrap();

    let module_manifest = ModuleManifest {
        module_id: "m.rule.cost.overflow".to_string(),
        name: "RuleCostOverflow".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Pure,
        role: ModuleRole::Rule,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        abi_contract: ModuleAbiContract::default(),
        exports: vec!["call".to_string()],
        subscriptions: vec![ModuleSubscription {
            event_kinds: Vec::new(),
            action_kinds: vec!["action.register_agent".to_string()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }],
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        limits: ModuleLimits {
            max_mem_bytes: 1024,
            max_gas: 10_000,
            max_call_rate: 1,
            max_output_bytes: 1024,
            max_effects: 0,
            max_emits: 1,
        },
    };

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

    let mut cost = ResourceDelta::default();
    cost.entries.insert(ResourceKind::Electricity, -1);
    cost.entries.insert(ResourceKind::Data, 1);
    let decision = RuleDecision {
        action_id: 1,
        verdict: RuleVerdict::Allow,
        override_action: None,
        cost,
        notes: Vec::new(),
    };
    let output = ModuleOutput {
        new_state: None,
        effects: Vec::new(),
        emits: vec![ModuleEmit {
            kind: "rule.decision".to_string(),
            payload: serde_json::to_value(&decision).unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 128,
    };
    let mut sandbox = FixedSandbox::succeed(output);

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world
        .step_with_modules(&mut sandbox)
        .expect("overflow cost should be rejected, not fatal");

    assert!(!world.state().agents.contains_key("agent-1"));
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 5);
    assert_eq!(world.resource_balance(ResourceKind::Data), i64::MAX);
    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => match reason {
            RejectReason::RuleDenied { notes } => {
                assert!(
                    notes
                        .iter()
                        .any(|note| note.contains("rule decision cost apply rejected")),
                    "expected overflow rejection reason, got {notes:?}"
                );
            }
            other => panic!("unexpected reject reason: {other:?}"),
        },
        other => panic!("unexpected event: {other:?}"),
    }
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_move_rule_rejects_when_insufficient_resources() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Electricity, 0);
    install_m1_move_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(100_000, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::InsufficientResources { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 0);
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_move_rule_denies_same_position() {
    let mut world = World::new();
    world.set_resource_balance(ResourceKind::Electricity, 10);
    install_m1_move_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    world.submit_action(Action::MoveAgent {
        agent_id: "agent-1".to_string(),
        to: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::RuleDenied { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 10);
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_visibility_rule_emits_observation() {
    let mut world = World::new();
    install_m1_visibility_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(10, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::Observation { observation }) => {
            assert_eq!(observation.agent_id, "agent-1");
            assert_eq!(observation.visible_agents.len(), 1);
            assert_eq!(observation.visible_agents[0].agent_id, "agent-2");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_visibility_rule_denies_when_missing_agent() {
    let mut world = World::new();
    install_m1_visibility_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::RuleDenied { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_transfer_rule_moves_resources() {
    let mut world = World::new();
    install_m1_transfer_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    world
        .set_agent_resource_balance("agent-1", ResourceKind::Electricity, 5)
        .unwrap();

    world.submit_action(Action::TransferResource {
        from_agent_id: "agent-1".to_string(),
        to_agent_id: "agent-2".to_string(),
        kind: ResourceKind::Electricity,
        amount: 3,
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ResourceTransferred {
            from_agent_id,
            to_agent_id,
            kind,
            amount,
        }) => {
            assert_eq!(from_agent_id, "agent-1");
            assert_eq!(to_agent_id, "agent-2");
            assert_eq!(*kind, ResourceKind::Electricity);
            assert_eq!(*amount, 3);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    assert_eq!(
        world
            .agent_resource_balance("agent-1", ResourceKind::Electricity)
            .unwrap(),
        2
    );
    assert_eq!(
        world
            .agent_resource_balance("agent-2", ResourceKind::Electricity)
            .unwrap(),
        3
    );
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_transfer_rule_rejects_when_insufficient() {
    let mut world = World::new();
    install_m1_transfer_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(0, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    world
        .set_agent_resource_balance("agent-1", ResourceKind::Electricity, 1)
        .unwrap();

    world.submit_action(Action::TransferResource {
        from_agent_id: "agent-1".to_string(),
        to_agent_id: "agent-2".to_string(),
        kind: ResourceKind::Electricity,
        amount: 3,
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::InsufficientResource { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
#[test]
fn m1_transfer_rule_denies_when_not_colocated() {
    let mut world = World::new();
    install_m1_transfer_rule(&mut world);

    let mut sandbox =
        WasmExecutor::new(super::test_wasm_executor_config()).expect("initialize wasm executor");

    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-2".to_string(),
        pos: pos(10, 0),
    });
    world.step_with_modules(&mut sandbox).unwrap();

    world
        .set_agent_resource_balance("agent-1", ResourceKind::Electricity, 5)
        .unwrap();

    world.submit_action(Action::TransferResource {
        from_agent_id: "agent-1".to_string(),
        to_agent_id: "agent-2".to_string(),
        kind: ResourceKind::Electricity,
        amount: 3,
    });
    world.step_with_modules(&mut sandbox).unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::RuleDenied { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn query_observation_requires_rule_module() {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        pos: pos(0, 0),
    });
    world.step().unwrap();

    world.submit_action(Action::QueryObservation {
        agent_id: "agent-1".to_string(),
    });
    world.step().unwrap();

    let last = world.journal().events.last().unwrap();
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
            assert!(matches!(reason, RejectReason::RuleDenied { .. }));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
