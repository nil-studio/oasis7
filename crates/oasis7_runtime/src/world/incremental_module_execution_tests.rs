//! Real GEN/accepted baseline + one complete scheduled module invocation.
use super::raw_complete::Scratch as FixtureScratch;
use super::work as runtime_genesis_work;
use super::*;
use crate::world::{
    incremental_body::BodyMutationContext,
    incremental_module_execution_effects::ModuleOutputEventContextV1,
    incremental_module_execution_worker::*,
    incremental_module_fee_contribution::RootModuleFeeWriteContextV1,
    incremental_module_invocation::*, incremental_world_event_counter::WorldEventCounterV1,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::*;
use oasis7_wasm_executor::bounded_worker::*;
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_runtime_artifact_config::SelectedModuleManifestLimitsV1,
    incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1,
    incremental_runtime_effect_records::RuntimeEffectRecordsWorkV1,
    incremental_runtime_module_authorization::ModuleAuthorizationReadWorkV1,
    incremental_value_mutation::ValueMutationContextV1,
};
use std::{
    fs::File,
    sync::atomic::{AtomicBool, Ordering},
};
#[path = "incremental_module_execution_fixture_store.rs"]
mod fixture_store;
#[path = "incremental_module_execution_fixture_genesis.rs"]
mod genesis;
use fixture_store::{Originals, Reader};
#[path = "incremental_action_body_tests.rs"]
mod action_body;
#[path = "incremental_factory_depreciation_tests.rs"]
mod factory_depreciation;
#[path = "incremental_factory_depreciation_reduce_tests.rs"]
mod factory_depreciation_reduce;
#[path = "incremental_factory_directory_orders_tests.rs"]
mod factory_order_directory;
#[path = "incremental_factory_directory_records_tests.rs"]
mod factory_recipe_directory;
#[path = "incremental_industrial_eligibility_tests.rs"]
mod industrial_eligibility;

fn leb(mut n: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (n & 127) as u8;
        n >>= 7;
        out.push(b | if n != 0 { 128 } else { 0 });
        if n == 0 {
            break;
        }
    }
    out
}
fn section(wasm: &mut Vec<u8>, id: u8, bytes: &[u8]) {
    wasm.push(id);
    wasm.extend(leb(bytes.len()));
    wasm.extend(bytes);
}
fn wasm(output: &ModuleOutput) -> Vec<u8> {
    let raw = oasis7_wasm_abi::encode_canonical_cbor(output).unwrap();
    assert!(raw.len() < 4096);
    let mut wasm = b"\0asm\x01\0\0\0".to_vec();
    section(
        &mut wasm,
        1,
        &[
            2, 0x60, 1, 0x7f, 1, 0x7f, 0x60, 2, 0x7f, 0x7f, 2, 0x7f, 0x7f,
        ],
    );
    section(&mut wasm, 3, &[2, 0, 1]);
    section(&mut wasm, 5, &[1, 0, 1]);
    section(
        &mut wasm,
        7,
        &[
            4, 6, b'm', b'e', b'm', b'o', b'r', b'y', 2, 0, 5, b'a', b'l', b'l', b'o', b'c', 0, 0,
            4, b'c', b'a', b'l', b'l', 0, 1, 6, b'r', b'e', b'd', b'u', b'c', b'e', 0, 1,
        ],
    );
    let alloc = [0, 0x41, 0x80, 0x20, 0x0b];
    let mut call = vec![0, 0x41, 0, 0x41];
    let mut length = leb(raw.len());
    if length.last().unwrap() & 0x40 != 0 {
        *length.last_mut().unwrap() |= 0x80;
        length.push(0);
    }
    call.extend(length);
    call.push(0x0b);
    let mut code = vec![2];
    code.extend(leb(alloc.len()));
    code.extend(alloc);
    code.extend(leb(call.len()));
    code.extend(call);
    section(&mut wasm, 10, &code);
    let mut data = vec![1, 0, 0x41, 0, 0x0b];
    data.extend(leb(raw.len()));
    data.extend(raw);
    section(&mut wasm, 11, &data);
    wasm
}
fn manifest(id: &str, kind: ModuleKind, wasm: &[u8]) -> ModuleManifest {
    use sha2::Digest;
    ModuleManifest {
        module_id: id.into(),
        name: id.into(),
        version: "1".into(),
        kind: kind.clone(),
        role: ModuleRole::Domain,
        wasm_hash: hex::encode(sha2::Sha256::digest(wasm)),
        interface_version: "wasm-1".into(),
        exports: vec![kind.entrypoint().into()],
        subscriptions: vec![],
        required_caps: vec![],
        abi_contract: ModuleAbiContract::default(),
        artifact_identity: None,
        limits: ModuleLimits {
            max_mem_bytes: 65536,
            max_gas: 1_000_000,
            max_call_rate: 100,
            max_output_bytes: 16 << 10,
            max_effects: 4,
            max_emits: 4,
        },
    }
}
fn initialized_world() -> World {
    use oasis7_agent_api::{
        world_action::{GeoPos, ModuleInstallTarget, ResourceKind},
        world_entity::{ModuleVisualAnchor, ModuleVisualEntity},
    };
    let mut world = World::new();
    world.state.time = 11;
    let mut alice = oasis7_world_state::agent_state::AgentState::new("alice", GeoPos::new(0, 0, 0));
    alice.resources.set(ResourceKind::Data, 1000).unwrap();
    alice
        .resources
        .set(ResourceKind::Electricity, 1000)
        .unwrap();
    world
        .state
        .agents
        .insert("alice".into(), crate::AgentCell::new(alice, 11));
    world.state.location_anchors.insert(
        "site".into(),
        crate::LocationAnchorV1 {
            location_id: "site".into(),
            active: true,
            authority_revision: 1,
            effective_at: 11,
        },
    );
    let entity = ModuleVisualEntity {
        entity_id: "visual-1".into(),
        module_id: "main".into(),
        kind: "marker".into(),
        label: Some("real wasm".into()),
        anchor: ModuleVisualAnchor::Location {
            location_id: "site".into(),
        },
    };
    let mixed = ModuleOutput {
        new_state: Some(b"new".to_vec()),
        effects: vec![ModuleEffectIntent {
            kind: "fixture.effect".into(),
            params: serde_json::json!({"amount":1}),
            cap_ref: String::new(),
            cap_slot: Some("main".into()),
        }],
        emits: vec![ModuleEmit {
            kind: "module_visual_entity_upserted".into(),
            payload: serde_json::json!({"entity":entity}),
        }],
        tick_lifecycle: Some(ModuleTickLifecycleDirective::WakeAfterTicks { ticks: 2 }),
        output_bytes: 0,
    };
    let pure = ModuleOutput {
        new_state: None,
        effects: vec![],
        emits: vec![],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    for (id, kind, output) in [
        ("main", ModuleKind::Reducer, mixed),
        ("policy", ModuleKind::Pure, pure),
    ] {
        let bytes = wasm(&output);
        let mut manifest = manifest(id, kind, &bytes);
        if id == "main" {
            manifest.subscriptions.push(ModuleSubscription {
                event_kinds: vec![],
                action_kinds: vec![],
                stage: Some(ModuleSubscriptionStage::Tick),
                filters: None,
            });
            manifest.required_caps.push("cap-main".into());
            manifest
                .abi_contract
                .cap_slots
                .insert("main".into(), "cap-main".into());
            manifest.abi_contract.policy_hooks.push("policy".into());
            world
                .state
                .module_artifact_owners
                .insert(manifest.wasm_hash.clone(), "alice".into());
        }
        world
            .register_module_artifact(manifest.wasm_hash.clone(), &bytes)
            .unwrap();
        world.module_registry.active.insert(id.into(), "1".into());
        world.module_registry.records.insert(
            crate::ModuleRegistry::record_key(id, "1"),
            crate::ModuleRecord {
                manifest: manifest.clone(),
                registered_at: 11,
                registered_by: "genesis".into(),
                audit_event_id: None,
            },
        );
        let key = if id == "main" {
            "main#stable"
        } else {
            "policy#inactive"
        };
        world.state.module_instances.insert(
            key.into(),
            crate::ModuleInstanceState {
                instance_id: key.into(),
                module_id: id.into(),
                module_version: "1".into(),
                wasm_hash: manifest.wasm_hash,
                owner_agent_id: "alice".into(),
                install_target: ModuleInstallTarget::SelfAgent,
                active: id == "main",
                installed_at: 11,
            },
        );
    }
    world
        .state
        .module_states
        .insert("main#stable".into(), b"old".to_vec());
    world.module_tick_schedule.insert("main#stable".into(), 11);
    world.capabilities.insert(
        "cap-main".into(),
        crate::CapabilityGrant::new("cap-main", vec!["fixture.effect".into()]),
    );
    world.policies = crate::PolicySet::allow_all();
    world
}
fn stopped(stop: &AtomicBool) -> impl FnMut() -> bool + '_ {
    move || stop.load(Ordering::Acquire)
}
fn execution_work() -> RootModuleExecutionWorkV1 {
    use crate::world::{
        incremental_module_execution_authorization::RootModuleAuthorizationLimitsV1,
        incremental_module_execution_effects::RootModuleEffectWorkV1,
        incremental_module_execution_output::RootModuleOutputWorkV1,
        incremental_module_execution_visual_decode::RootModuleVisualDecodeWorkV1,
    };
    RootModuleExecutionWorkV1 {
        input: genesis::canonical(),
        routing: oasis7_wasm_router::bounded::RouterWorkV1 {
            nodes_remaining: 100_000,
            edges_remaining: 100_000,
            string_bytes_remaining: 8 << 20,
            match_steps_remaining: 2_000_000,
            max_depth: 64,
        },
        max_input_bytes: 64 << 10,
        max_policy_selections: 8,
        max_prepared_effects: 8,
        lookup_steps_remaining: 1000,
        transient_heap_bytes: 32 << 20,
        max_state_bytes: 64 << 10,
        max_owner_bytes: 4096,
        anchor_scan: genesis::scan(),
        anchor_cells_remaining: 100000,
        visual: oasis7_world_state::incremental_module_marketplace::ModuleVisualOverlayWorkV1 {
            pending_entries_remaining: 10000,
            body_scan: genesis::scan(),
            cells_remaining: 100000,
            max_owner_bytes: 4096,
        },
        visual_decode: RootModuleVisualDecodeWorkV1 {
            nodes_remaining: 100000,
            text_bytes_remaining: 8 << 20,
            max_depth: 64,
            max_heap_bytes: 8 << 20,
        },
        effects: RootModuleEffectWorkV1 {
            clone_nodes_remaining: 100000,
            clone_bytes_remaining: 8 << 20,
            max_json_depth: 64,
            max_policy_rules: 100,
            limits: RootModuleAuthorizationLimitsV1 {
                max_text_bytes: 4096,
                max_effect_kinds: 100,
            },
        },
        effect_records: RuntimeEffectRecordsWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 100000,
            max_json_depth: 64,
            max_pending_node_bytes: 8 << 20,
        },
        authorization: ModuleAuthorizationReadWorkV1 {
            cells_remaining: 100000,
            comparison_bytes_remaining: 8 << 20,
            max_payload_bytes: 8 << 20,
        },
        output: RootModuleOutputWorkV1 {
            calls_remaining: 8,
            structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                bytes_remaining: 16 << 20,
                values_remaining: 100000,
                max_depth: 64,
            },
            max_identity_bytes: 4096,
            max_original_output_bytes: 16 << 10,
            max_output_heap_bytes: 16 << 20,
        },
        selection_scan: genesis::scan(),
        selection: genesis::config_work(),
        original: RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 1000,
            bytes_remaining: 32 << 20,
            max_object_bytes: 1 << 20,
        },
        selection_limits: ModuleInvocationPreparationLimitsV1 {
            selection_heap_bytes: 128 << 10,
            max_text_bytes: 4096,
            manifest: SelectedModuleManifestLimitsV1 {
                heap_bytes: 256 << 10,
                json_heap_bytes: 128 << 10,
                max_collection_items: 1000,
            },
            manifest_json_bytes_remaining: 16 << 20,
        },
    }
}

#[path = "incremental_module_execution_fixture_worker.rs"]
mod fixture_worker;

#[test]
#[cfg(feature = "wasmtime")]
fn actual_scheduled_module_vm_fee_policy_state_effect_visual_lifecycle_and_counter() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = initialized_world();
    let expected_manifest_hash = crate::util::hash_json(&world.manifest).unwrap();
    let fixture = genesis::build(&world, &memory, workspace);
    assert!(fixture.memory.same_pool(&memory));
    drop(world); // Execution below has no materialized World to hydrate/fall back to.
    assert_eq!(fixture.manifest_hash.hash(), expected_manifest_hash);
    let image = fixture_worker::Image::new(&memory);
    let stop = AtomicBool::new(false);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut original_reader = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut transaction = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [91; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &transaction, &memory).unwrap();
    assert_eq!(counter.journal_height(&transaction).unwrap(), 0);
    let input = fixture
        .objects
        .put(b"actual scheduled module component input");
    transaction
        .append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: input,
        })
        .unwrap();
    assert_eq!(
        counter.journal_height(&transaction).unwrap(),
        0,
        "Input is not a WorldEvent"
    );
    let live = transaction.parent_fence();
    let mut live_owner = || live;
    let mut value_io = genesis::io();
    let mut semantic = genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut semantic,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: stopped(&stop),
    };
    let mut member_cas = fixture.cas.clone();
    let mut fee_cas = fixture.cas.clone();
    let mut event_cas = fixture.cas.clone();
    let mut member_io = genesis::io();
    let mut member_keys = genesis::keys();
    let mut members = genesis::member(
        &mut member_cas,
        &memory,
        &mut member_io,
        &mut member_keys,
        stopped(&stop),
    );
    let mut fee_io = genesis::io();
    let mut fee_keys = genesis::keys();
    let mut fees = RootModuleFeeWriteContextV1 {
        values: ValueMutationContextV1 {
            sink: &mut fee_cas,
            memory: &memory,
            io: &mut fee_io,
            cancelled: stopped(&stop),
        },
        key_work: &mut fee_keys,
        member_limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8 << 20,
            record_bytes: 9 << 20,
        },
    };
    let mut event_io = genesis::io();
    let saved = std::rc::Rc::new(std::cell::RefCell::new(Vec::with_capacity(16)));
    let saved_sink = saved.clone();
    let objects = fixture.objects.clone();
    let mut events = ModuleOutputEventContextV1 {
        values: &reader,
        world_events: &counter,
        mutation: BodyMutationContext {
            sink: &mut event_cas,
            memory: &memory,
            io: &mut event_io,
            cancelled: stopped(&stop),
        },
        persist_original:
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                let length=usize::try_from(stream.original_bytes()).unwrap();assert!(length<=64<<10);
                let fee=memory.reserve(length+128,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                let mut bytes=Vec::with_capacity(length);stream.write_to(&mut bytes)?;
                assert_eq!(bytes.len(),length);assert_eq!(*blake3::hash(&bytes).as_bytes(),stream.original_id());
                if cancelled(){return Err(ValueTreeError::Cancelled.into());}
                let id=objects.put(&bytes);assert_eq!(id,stream.original_id());
                assert!(saved_sink.borrow().len()<16);saved_sink.borrow_mut().push(id);drop(fee);Ok(())
            })
            },
        max_event_bytes: 64 << 10,
        original_work: genesis::canonical(),
    };
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let mut worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&image.file),
        allocator: &allocator,
        cancelled: &stop,
        limits: WorkerLimitsV1 {
            address_space_bytes: 256 << 20,
            max_request_bytes: 1 << 20,
            max_output_bytes: 16 << 10,
            wall_time_ms: 30_000,
        },
    };
    let mut work = execution_work();
    let initial_calls = work.output.calls_remaining;
    let mut hash_work = genesis::hash_work();
    let result = {
        let mut target = RootModuleExecutionContextV1 {
            families: RootModuleExecutionFamiliesV1 {
                roots: &fixture.roots,
                parent: &fixture.runtime,
                head: &fixture.head,
                instances: fixture.current.raw().instances(),
                config: fixture.current.runtime().config(),
                content: fixture.current.runtime().content(),
                agents: fixture.current.raw().agents(),
                authorization: fixture.current.runtime().module_authorization(),
                effects: fixture.current.runtime().effects(),
                state: fixture.current.raw().module_states(),
                agent_authority: fixture.current.native().agent_authority(),
                marketplace: fixture.current.native().market(),
            },
            now: 11,
            state: &reader,
            ctx: &mut overlay,
            members: &mut members,
            fees: &mut fees,
            events: &mut events,
            originals: &mut original_reader,
            worker: &mut worker,
            manifest_cache: &fixture.manifest_hash,
            manifest_work: &mut hash_work,
            work: &mut work,
            #[cfg(test)]
            input_observation: None,
        };
        target.execute_scheduled_tick_v1("main#stable", &mut live_owner)
    };
    if !cfg!(target_os = "linux") {
        assert!(
            matches!(
                result,
                Err(RootModuleExecutionErrorV1::Worker(
                    RootModuleWorkerErrorV1::Host(_)
                ))
            ),
            "actual unsupported host must never fall back: {result:?}"
        );
        assert!(saved.borrow().is_empty());
        assert_eq!(work.output.calls_remaining, initial_calls - 1);
        drop(overlay);
        let guard = transaction.guard();
        assert!(
            transaction
                .seal(&guard, &live, fixture.roots.height + 1)
                .is_err(),
            "Host failure poisons provisional due removal"
        );
    } else {
        let result = result.unwrap();
        assert!(result.due_consumed());
        assert!(result.invoked());
        assert_eq!(
            work.output.calls_remaining,
            initial_calls - 2,
            "main and actual active Pure registry module both used real workers"
        );
        assert_eq!(saved.borrow().len(), 5);
        assert_eq!(counter.journal_height(overlay.transaction).unwrap(), 5);
        let ids = saved.borrow();
        let mut bodies = Vec::new();
        for id in ids.iter() {
            let bytes = fixture.objects.original(id);
            assert!(bytes.len() <= 64 << 10);
            let event: crate::WorldEvent = serde_cbor::from_slice(&bytes).unwrap();
            assert_eq!(event.time, 11);
            assert_eq!(event.caused_by, None);
            bodies.push(event.body);
        }
        assert!(
            matches!(&bodies[0],crate::WorldEventBody::ModuleRuntimeCharged(charge)
            if charge.payer_agent_id=="alice"&&charge.compute_fee_amount==5&&charge.electricity_fee_amount==4)
        );
        assert!(matches!(
            &bodies[1],
            crate::WorldEventBody::PolicyDecisionRecorded(_)
        ));
        assert!(
            matches!(&bodies[2],crate::WorldEventBody::ModuleStateUpdated(update) if update.module_id=="main#stable"&&update.state==b"new")
        );
        assert!(
            matches!(&bodies[3],crate::WorldEventBody::EffectQueued(intent) if intent.kind=="fixture.effect"&&intent.cap_ref=="cap-main")
        );
        assert!(
            matches!(&bodies[4],crate::WorldEventBody::ModuleEmitted(event) if event.kind=="module_visual_entity_upserted")
        );
        let data = crate::world::incremental_module_fee_accounts::resource(
            "alice",
            oasis7_agent_api::world_action::ResourceKind::Data,
            fixture.current.raw().agents(),
            &mut overlay,
        )
        .unwrap();
        let power = crate::world::incremental_module_fee_accounts::resource(
            "alice",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            fixture.current.raw().agents(),
            &mut overlay,
        )
        .unwrap();
        assert_eq!(data, Some(995));
        assert_eq!(power, Some(996));
        let mut state = Vec::new();
        crate::world::incremental_module_state::read_iws_module_state(
            fixture.current.raw().module_states(),
            "main#stable",
            overlay.transaction,
            &mut overlay.values,
            &mut overlay.cancelled,
            64 << 10,
            &mut state,
        )
        .unwrap();
        assert_eq!(state, b"new");
        // Same now, actual wake=13: the original due owner refuses another VM.
        let calls = work.output.calls_remaining;
        let again = {
            let mut target = RootModuleExecutionContextV1 {
                families: RootModuleExecutionFamiliesV1 {
                    roots: &fixture.roots,
                    parent: &fixture.runtime,
                    head: &fixture.head,
                    instances: fixture.current.raw().instances(),
                    config: fixture.current.runtime().config(),
                    content: fixture.current.runtime().content(),
                    agents: fixture.current.raw().agents(),
                    authorization: fixture.current.runtime().module_authorization(),
                    effects: fixture.current.runtime().effects(),
                    state: fixture.current.raw().module_states(),
                    agent_authority: fixture.current.native().agent_authority(),
                    marketplace: fixture.current.native().market(),
                },
                now: 11,
                state: &reader,
                ctx: &mut overlay,
                members: &mut members,
                fees: &mut fees,
                events: &mut events,
                originals: &mut original_reader,
                worker: &mut worker,
                manifest_cache: &fixture.manifest_hash,
                manifest_work: &mut hash_work,
                work: &mut work,
                #[cfg(test)]
                input_observation: None,
            };
            target
                .execute_scheduled_tick_v1("main#stable", &mut live_owner)
                .unwrap()
        };
        assert!(!again.due_consumed());
        assert!(!again.invoked());
        assert_eq!(work.output.calls_remaining, calls);
        assert_eq!(counter.journal_height(overlay.transaction).unwrap(), 5);
        drop(overlay);
        let guard = transaction.guard();
        let prepared = transaction
            .seal(&guard, &live, fixture.roots.height + 1)
            .unwrap();
        assert_eq!(
            prepared.record.facts.len(),
            6,
            "one actual Input plus five WorldEvent facts; final dual Results belong to enclosing driver"
        );
    }
}

#[path = "incremental_action_admission_fixture.rs"]
mod action_admission_fixture;
#[path = "incremental_action_execution_tests.rs"]
mod action_execution_tests;

#[path = "incremental_module_execution_action_token_tests.rs"]
mod action_token;

#[path = "incremental_module_execution_pre_action_tests.rs"]
mod pre_action;

#[path = "incremental_module_invocation_enumerate_tests.rs"]
mod enumerate;

#[path = "incremental_rule_resources_tests.rs"]
mod rule_resources_tests;

#[path = "incremental_module_execution_rule_phase_tests.rs"]
mod rule_phase;

#[path = "incremental_module_execution_override_tests.rs"]
mod override_audit;

#[path = "incremental_factory_lifecycle_current_tests.rs"]
mod factory_lifecycle_current;
#[path = "incremental_factory_phase_tests.rs"]
mod factory_phase;
#[path = "incremental_material_normalization_tests.rs"]
mod material_normalization;

#[path = "incremental_directory_bundle_tests.rs"]
mod directory_bundle;

use crate::world::incremental_body_event::materials::test_support as core_materials;
#[path = "incremental_commit_directories_tests.rs"]
mod commit_directories;

#[path = "incremental_safe_alternative_signed_tests.rs"]
mod signed_safe_alternative;
