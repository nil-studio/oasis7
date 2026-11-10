//! Shared genuine Rule/VM fixture and independent old World oracle.
use super::*;
pub(super) fn pre_action_world(with_regex: bool, case: u8, core_mode: u8) -> World {
    let mut world = initialized_world();
    if core_mode != 0 {
        use oasis7_agent_api::world_action::ResourceKind;
        world.state.resources.insert(
            ResourceKind::Electricity,
            if core_mode == 3 { 5 } else { 100 },
        );
        world.state.resources.insert(ResourceKind::Data, 100);
        if core_mode == 2 {
            world
                .state
                .gameplay_policy
                .forbidden_location_ids
                .push("20:30:40".into());
        }
    }
    let record = world
        .module_registry
        .records
        .get_mut(&crate::ModuleRegistry::record_key("main", "1"))
        .unwrap();
    record.manifest.subscriptions[0].stage = Some(ModuleSubscriptionStage::PreAction);
    record.manifest.subscriptions[0].action_kinds = vec!["action.move_agent".into()];
    if with_regex {
        record.manifest.subscriptions[0].filters = Some(serde_json::json!({
            "action": [{"path": "/action/data/agent_id", "re": "^alice$"}]
        }));
    }
    world.submit_action(crate::Action::MoveAgent {
        agent_id: "alice".into(),
        to: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
    });
    let output = rule_output_for_core_mode(case, core_mode);
    let bytes = wasm(&output);
    use sha2::Digest;
    let hash = hex::encode(sha2::Sha256::digest(&bytes));
    world
        .register_module_artifact(hash.clone(), &bytes)
        .unwrap();
    world
        .state
        .module_artifact_owners
        .insert(hash.clone(), "alice".into());
    world
        .module_registry
        .records
        .get_mut(&crate::ModuleRegistry::record_key("main", "1"))
        .unwrap()
        .manifest
        .wasm_hash = hash.clone();
    world
        .state
        .module_instances
        .get_mut("main#stable")
        .unwrap()
        .wasm_hash = hash;
    if case == 1 {
        let bytes = wasm(&rule_output_for_core_mode(0, core_mode));
        let mut pricing = manifest("m1.rule.move", ModuleKind::Reducer, &bytes);
        pricing.subscriptions = vec![ModuleSubscription {
            event_kinds: vec![],
            action_kinds: vec!["action.move_agent".into()],
            stage: Some(ModuleSubscriptionStage::PreAction),
            filters: None,
        }];
        pricing.required_caps = vec!["cap-main".into()];
        pricing
            .abi_contract
            .cap_slots
            .insert("main".into(), "cap-main".into());
        pricing.abi_contract.policy_hooks = vec!["policy".into()];
        world
            .register_module_artifact(pricing.wasm_hash.clone(), &bytes)
            .unwrap();
        world
            .state
            .module_artifact_owners
            .insert(pricing.wasm_hash.clone(), "alice".into());
        world
            .module_registry
            .active
            .insert("m1.rule.move".into(), "1".into());
        world.module_registry.records.insert(
            crate::ModuleRegistry::record_key("m1.rule.move", "1"),
            crate::ModuleRecord {
                manifest: pricing.clone(),
                registered_at: 11,
                registered_by: "genesis".into(),
                audit_event_id: None,
            },
        );
        let mut instance = world.state.module_instances["main#stable"].clone();
        instance.instance_id = "pricing#real".into();
        instance.module_id = "m1.rule.move".into();
        instance.wasm_hash = pricing.wasm_hash;
        world
            .state
            .module_instances
            .insert(instance.instance_id.clone(), instance);
    }
    world
}
pub(super) struct RuleOracle {
    pub(super) case: u8,
    pub(super) core_mode: u8,
    pub(super) last: Option<(usize, [u8; 32])>,
}
impl ModuleSandbox for RuleOracle {
    fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        if matches!(request.module_id.as_str(), "main" | "m1.rule.move") {
            self.last = Some((
                request.input.len(),
                *blake3::hash(&request.input).as_bytes(),
            ));
        }
        let mut output = if request.module_id == "main" {
            rule_output_for_core_mode(self.case, self.core_mode)
        } else if request.module_id == "m1.rule.move" {
            rule_output_for_core_mode(0, self.core_mode)
        } else {
            ModuleOutput {
                new_state: None,
                effects: vec![],
                emits: vec![],
                tick_lifecycle: None,
                output_bytes: 0,
            }
        };
        // Match the real legacy executor's measured raw ModuleOutput length,
        // rather than trusting the fixture DTO's module-supplied zero.
        output.output_bytes = u64::try_from(
            oasis7_wasm_abi::encode_canonical_cbor(&output)
                .unwrap()
                .len(),
        )
        .unwrap();
        Ok(output)
    }
}

// The insufficient-cost case remains insufficient after both genuine
// module invocations credit their fees to the global treasury.
fn rule_output_for_core_mode(case: u8, core_mode: u8) -> ModuleOutput {
    let mut output = rule_output(case);
    if core_mode == 3 {
        let mut decision: crate::RuleDecision =
            serde_json::from_value(output.emits[0].payload.clone()).unwrap();
        decision.cost.entries.insert(
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            -30,
        );
        output.emits[0].payload = serde_json::to_value(decision).unwrap();
    }
    output
}

fn rule_output(case: u8) -> ModuleOutput {
    let mut output = ModuleOutput {
        new_state: Some(b"new".to_vec()),
        effects: vec![ModuleEffectIntent {
            kind: "fixture.effect".into(),
            params: serde_json::json!({"amount":1}),
            cap_ref: String::new(),
            cap_slot: Some("main".into()),
        }],
        emits: vec![ModuleEmit {
            kind: "rule.decision".into(),
            payload: serde_json::to_value(crate::RuleDecision {
                action_id: 1,
                verdict: crate::RuleVerdict::Allow,
                override_action: None,
                cost: crate::ResourceDelta {
                    entries: [(
                        oasis7_agent_api::world_action::ResourceKind::Electricity,
                        -3,
                    )]
                    .into(),
                },
                notes: vec!["real-rule".into()],
            })
            .unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    match case {
        1 => {
            output.emits[0].payload["verdict"] = serde_json::json!("modify");
            output.emits[0].payload["override_action"] =
                serde_json::to_value(crate::Action::MoveAgent {
                    agent_id: "alice".into(),
                    to: oasis7_agent_api::world_action::GeoPos::new(20, 30, 40),
                })
                .unwrap();
        }
        7 => {
            output.emits[0].payload["verdict"] = serde_json::json!("deny");
            output.emits[0].payload["notes"] = serde_json::json!(["real-rule-denial"]);
        }
        2 => {
            output.emits[0].payload = serde_json::json!({"action_id":"not-an-id","verdict":"allow"})
        }
        3 => output.emits[0].payload["action_id"] = serde_json::json!(999),
        4 => output.emits.push(output.emits[0].clone()),
        5 => {
            output.emits[0].payload =
                serde_json::json!({"action_id":"not-an-id","verdict":"allow"});
            output.emits.push(output.emits[0].clone());
        }
        6 => {
            output.emits[0].payload["action_id"] = serde_json::json!(999);
            output.emits.push(output.emits[0].clone());
        }
        _ => {}
    }
    output
}
