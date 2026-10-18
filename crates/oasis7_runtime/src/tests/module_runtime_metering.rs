use super::super::*;
use super::pos;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{
    ModuleCallFailure, ModuleCallRequest, ModuleEmit, ModuleOutput, ModuleSandbox,
};
use serde_json::json;

#[derive(Clone)]
struct FixedOutputSandbox {
    output: ModuleOutput,
}

impl ModuleSandbox for FixedOutputSandbox {
    fn call(&mut self, _request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
        Ok(self.output.clone())
    }
}

fn setup_active_module_world(owner_agent_id: &str, module_id: &str, wasm_bytes: &[u8]) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: owner_agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register owner agent");
    world
        .set_agent_resource_balance(owner_agent_id, ResourceKind::Electricity, 10_000)
        .expect("seed electricity");
    world
        .set_agent_resource_balance(owner_agent_id, ResourceKind::Data, 10_000)
        .expect("seed data");

    let wasm_hash = util::sha256_hex(wasm_bytes);
    world.submit_action(Action::DeployModuleArtifact {
        publisher_agent_id: owner_agent_id.to_string(),
        wasm_hash: wasm_hash.clone(),
        wasm_bytes: wasm_bytes.to_vec(),
    });
    world.step().expect("deploy module artifact");

    let manifest = ModuleManifest {
        module_id: module_id.to_string(),
        name: "MeteringModule".to_string(),
        version: "0.1.0".to_string(),
        kind: ModuleKind::Reducer,
        role: ModuleRole::Domain,
        wasm_hash: wasm_hash.clone(),
        interface_version: "wasm-1".to_string(),
        exports: vec!["reduce".to_string()],
        subscriptions: Vec::new(),
        required_caps: Vec::new(),
        artifact_identity: Some(super::signed_test_artifact_identity(wasm_hash.as_str())),
        abi_contract: ModuleAbiContract::default(),
        limits: ModuleLimits::unbounded(),
    };
    world.submit_action(Action::InstallModuleFromArtifact {
        installer_agent_id: owner_agent_id.to_string(),
        manifest,
        activate: true,
    });
    world.step().expect("install module");
    world
}

#[test]
fn module_runtime_metering_charges_data_and_electricity() {
    let owner_agent_id = "owner-metering";
    let module_id = "m.metering";
    let mut world = setup_active_module_world(owner_agent_id, module_id, b"wasm-metering-ok");
    world
        .set_agent_resource_balance(owner_agent_id, ResourceKind::Data, 10)
        .expect("set data balance");
    world
        .set_agent_resource_balance(owner_agent_id, ResourceKind::Electricity, 10)
        .expect("set electricity balance");
    let treasury_data_before = world.resource_balance(ResourceKind::Data);
    let treasury_electricity_before = world.resource_balance(ResourceKind::Electricity);

    let mut sandbox = FixedOutputSandbox {
        output: ModuleOutput {
            new_state: Some(vec![1, 2, 3]),
            effects: Vec::new(),
            emits: vec![
                ModuleEmit {
                    kind: "metering.emit.a".to_string(),
                    payload: json!({"n": 1}),
                },
                ModuleEmit {
                    kind: "metering.emit.b".to_string(),
                    payload: json!({"n": 2}),
                },
            ],
            tick_lifecycle: None,
            output_bytes: 2_300,
        },
    };
    world
        .execute_module_call(
            module_id,
            "trace-metering-ok",
            vec![7_u8; 1_500],
            &mut sandbox,
        )
        .expect("module call succeeds");

    let charge = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::ModuleRuntimeCharged(charge)
                if charge.trace_id == "trace-metering-ok" =>
            {
                Some(charge.clone())
            }
            _ => None,
        })
        .expect("module runtime charge event");
    assert_eq!(charge.module_id, module_id);
    assert_eq!(charge.payer_agent_id, owner_agent_id);
    assert_eq!(charge.compute_fee_kind, ResourceKind::Data);
    assert_eq!(charge.compute_fee_amount, 7);
    assert_eq!(charge.electricity_fee_kind, ResourceKind::Electricity);
    assert_eq!(charge.electricity_fee_amount, 4);
    assert_eq!(charge.input_bytes, 1_500);
    assert_eq!(charge.output_bytes, 2_300);
    assert_eq!(charge.effect_count, 0);
    assert_eq!(charge.emit_count, 2);

    assert_eq!(
        world
            .agent_resource_balance(owner_agent_id, ResourceKind::Data)
            .expect("data balance"),
        3
    );
    assert_eq!(
        world
            .agent_resource_balance(owner_agent_id, ResourceKind::Electricity)
            .expect("electricity balance"),
        6
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Data),
        treasury_data_before + 7
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        treasury_electricity_before + 4
    );
}

#[test]
fn module_runtime_metering_rejects_when_payer_resource_is_insufficient() {
    let owner_agent_id = "owner-metering-fail";
    let module_id = "m.metering.fail";
    let mut world = setup_active_module_world(owner_agent_id, module_id, b"wasm-metering-fail");
    world
        .set_agent_resource_balance(owner_agent_id, ResourceKind::Data, 1)
        .expect("set data balance");
    world
        .set_agent_resource_balance(owner_agent_id, ResourceKind::Electricity, 100)
        .expect("set electricity balance");
    let treasury_data_before = world.resource_balance(ResourceKind::Data);
    let treasury_electricity_before = world.resource_balance(ResourceKind::Electricity);
    let journal_len_before = world.journal().events.len();

    let mut sandbox = FixedOutputSandbox {
        output: ModuleOutput {
            new_state: Some(vec![9, 9, 9]),
            effects: Vec::new(),
            emits: vec![ModuleEmit {
                kind: "metering.emit.fail".to_string(),
                payload: json!({"fail": true}),
            }],
            tick_lifecycle: None,
            output_bytes: 1_024,
        },
    };
    let err = world
        .execute_module_call(
            module_id,
            "trace-metering-fail",
            vec![1_u8; 2_000],
            &mut sandbox,
        )
        .expect_err("module call should fail on runtime metering");
    let WorldError::ModuleCallFailed {
        module_id: failed_module_id,
        trace_id,
        ..
    } = err
    else {
        panic!("expected WorldError::ModuleCallFailed");
    };
    assert_eq!(failed_module_id, module_id);
    assert_eq!(trace_id, "trace-metering-fail");

    assert_eq!(world.journal().events.len(), journal_len_before + 1);
    assert!(matches!(
        world.journal().events.last().map(|event| &event.body),
        Some(WorldEventBody::ModuleCallFailed(_))
    ));
    assert!(
        !world.journal().events.iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::ModuleRuntimeCharged(charge)
                    if charge.trace_id == "trace-metering-fail"
            )
        }),
        "insufficient metering should not append charge event"
    );
    assert!(
        !world.journal().events.iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::ModuleStateUpdated(update)
                    if update.trace_id == "trace-metering-fail"
            )
        }),
        "insufficient metering should not apply module state"
    );
    assert!(
        !world.journal().events.iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::ModuleEmitted(emit)
                    if emit.trace_id == "trace-metering-fail"
            )
        }),
        "insufficient metering should not emit module events"
    );

    assert_eq!(
        world
            .agent_resource_balance(owner_agent_id, ResourceKind::Data)
            .expect("data balance"),
        1
    );
    assert_eq!(
        world
            .agent_resource_balance(owner_agent_id, ResourceKind::Electricity)
            .expect("electricity balance"),
        100
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Data),
        treasury_data_before
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        treasury_electricity_before
    );
    assert!(
        !world.state().module_states.contains_key(module_id),
        "insufficient metering should not persist module state"
    );
}
