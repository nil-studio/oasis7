//! Nondefault Linux RuleVM artifact: minimum original fixture dependency closure.
//! Only the closed four-case runner is public; no World, qualification, cache,
//! Selected source, Rule outcome, arbitrary budget or callback escapes this module.
use super::*;
use crate::world::{
    incremental_body::BodyMutationContext,
    incremental_module_execution_effects::ModuleOutputEventContextV1,
    incremental_module_execution_worker::*,
    incremental_module_fee_contribution::RootModuleFeeWriteContextV1,
    incremental_module_invocation::*, incremental_world_event_counter::WorldEventCounterV1,
};
use crate::world::{
    incremental_genesis::RuntimeGenesisWorkV1, incremental_genesis_raw::RuntimeRawGenesisWorkV1,
};
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_wasm_abi::*;
use oasis7_wasm_executor::bounded_worker::*;
use oasis7_world_state::incremental_field_member::MemberRecordWriteContext;
use oasis7_world_state::{
    incremental_field_member::MemberRecordLimitsV1,
    incremental_module_state_recovery::ModuleStateContentBudgetV1,
};
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
// These are only the original typed budget constructors, not the old tests tree.
mod typed_genesis_budgets {
    use crate::world::incremental_genesis_typed::*;
    use oasis7_formal_protocol::incremental_state::StateScanBudget;
    use oasis7_world_state::incremental_world_state_genesis::*;
    fn scan() -> StateScanBudget {
        StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        }
    }
    fn json() -> JsonWriteWorkV1 {
        JsonWriteWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 64,
            max_pending_node_bytes: 8 << 20,
        }
    }
    fn world_work() -> WorldStateGenesisWorkV1 {
        WorldStateGenesisWorkV1 {
            records_remaining: 1000000,
            factory: FactoryWorkBudgetV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                scan: scan(),
            },
            factory_authority: FactoryAuthorityWorkV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
            },
            material: MaterialWorkBudgetV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                scan: scan(),
            },
            logistics: LogisticsWorkBudgetV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                scan: scan(),
            },
            industry: IndustryWorkBudgetV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                scan: scan(),
            },
            agent_authority: AgentAuthorityWorkBudgetV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                scan: scan(),
            },
            governance: GovernanceWorkBudgetV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                scan: scan(),
            },
            fixed_economy: FixedEconomyWorkV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
            },
            main_token_policy: MainTokenPolicyWorkV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
            },
            rewards: RewardRecordsWorkV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
            },
            rule_composites: WorldRuleCompositeWriteWorkV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                    cells_remaining: 1000000,
                    json_nodes_remaining: 100000,
                    max_json_depth: 64,
                    max_pending_node_bytes: 8 << 20,
                },
            },
            native: TypedRecordWriteWorkV1 {
                cells_remaining: 1000000,
                max_pending_node_bytes: 8 << 20,
                json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                    cells_remaining: 1000000,
                    json_nodes_remaining: 100000,
                    max_json_depth: 64,
                    max_pending_node_bytes: 8 << 20,
                },
            },
            rules_max_pending_node_bytes: 8 << 20,
            diplomacy_max_pending_node_bytes: 8 << 20,
        }
    }
    pub(super) fn work() -> RuntimeTypedGenesisWorkV1 {
        RuntimeTypedGenesisWorkV1 {
        output_rows_remaining: 1_000_000,
        records_remaining: 1_000_000,
        world: world_work(),
        pending_actions: oasis7_world_state::incremental_runtime_pending_action_record::PendingActionWriteWorkV1 { cells_remaining: 1_000_000, max_pending_node_bytes: 8 << 20, json: json() },
        artifact_config: ArtifactConfigWriteWorkV1 {
            cells_remaining: 1_000_000,
            json: json(),
            max_pending_node_bytes: 8 << 20,
        },
        artifact_resources: RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        artifact_originals: RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 1000,
            bytes_remaining: 64 << 20,
            max_object_bytes: 16 << 20,
        },
        effects: RuntimeEffectRecordsWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 100_000,
            max_json_depth: 64,
            max_pending_node_bytes: 8 << 20,
        },
        capabilities: RuntimeCapabilityRecordsWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        module_authorization: ModuleAuthorizationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        authorization: AuthorizationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        json: json(),
        proposals: RuntimeProposalWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            json: json(),
        },
        revocation: RevocationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            json: json(),
        },
        governance: RuntimeGovernanceWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        rollback: RollbackWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
        scheduler_controls: RuntimeSchedulerControlsWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
        },
    }
    }
}
fn runtime_genesis_work() -> RuntimeGenesisWorkV1 {
    RuntimeGenesisWorkV1 {
        typed: typed_genesis_budgets::work(),
        raw: RuntimeRawGenesisWorkV1 {
            writes_remaining: 1000,
            agents_remaining: 100,
            instances_remaining: 100,
            instance_count_rows_remaining: 100,
            schedule_entries_remaining: 100,
            module_states: ModuleStateContentBudgetV1 {
                states_remaining: 100,
                logical_bytes_remaining: 1 << 20,
                max_state_bytes: 1 << 20,
            },
            max_scalar_bytes: 1 << 20,
        },
    }
}

// Original bounded scratch store only. Do not bring its protocol/Binder/tests.
mod scratch {
    use super::*;
    use oasis7_formal_protocol::incremental_scratch_counter::{
        CounterScratchStore, ScratchCounterDelta,
    };
    use std::{cell::RefCell, collections::BTreeMap};
    pub(super) struct ScratchNode(ReservedValue<Vec<u8>>);
    impl AsRef<[u8]> for ScratchNode {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }
    pub(super) struct Scratch<'a> {
        memory: &'a MemoryBudget,
        class: MemoryClass,
        pages: RefCell<BTreeMap<String, ReservedValue<Vec<u8>>>>,
        _metadata: oasis7_proto::memory_budget::MemoryReservation,
    }
    impl<'a> Scratch<'a> {
        pub(super) fn new(memory: &'a MemoryBudget, class: MemoryClass) -> Self {
            Self {
                memory,
                class,
                pages: RefCell::new(BTreeMap::new()),
                _metadata: memory.reserve(4096 * 256 + 512, class).unwrap(),
            }
        }
    }
    impl MerkleNodeReader for Scratch<'_> {
        type Node = ScratchNode;
        fn allocation_budget(&self) -> Option<&MemoryBudget> {
            Some(self.memory)
        }
        fn allocation_class(&self) -> Option<MemoryClass> {
            Some(self.class)
        }
        fn read_node(&self, id: &str, max: usize) -> Result<Option<ScratchNode>, String> {
            let pages = self.pages.borrow();
            let Some(bytes) = pages.get(id) else {
                return Ok(None);
            };
            if bytes.len() > max {
                return Err("scratch page bound".into());
            }
            Ok(Some(ScratchNode(
                self.memory
                    .reserve(bytes.len() + 128, self.class)
                    .map_err(|e| e.to_string())?
                    .retain(bytes.to_vec()),
            )))
        }
    }
    impl CounterScratchStore for Scratch<'_> {
        fn put_scratch_pages(
            &self,
            delta: &ReservedValue<ScratchCounterDelta>,
        ) -> Result<(), String> {
            if !delta.uses_budget(self.memory) {
                return Err("scratch pool".into());
            }
            delta.verify_pages()?;
            for page in &delta.new_nodes {
                let mut pages = self.pages.borrow_mut();
                if pages.len() == 4096 && !pages.contains_key(&page.digest) {
                    return Err("scratch finite pages".into());
                }
                let copy = self
                    .memory
                    .reserve(page.bytes.len() + 128, self.class)
                    .map_err(|e| e.to_string())?
                    .retain(page.bytes.clone());
                pages.insert(page.digest.clone(), copy);
            }
            Ok(())
        }
    }
}
use scratch::Scratch as FixtureScratch;
// Preserve the original composed runner's helper path without loading the
// entire action_body tests tree or any of its nested consumer fixtures.
mod action_body {
    use super::*;
    pub(super) fn audit_work()
    -> oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
        oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
            structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                bytes_remaining: 16 << 20,
                values_remaining: 1000000,
                max_depth: 64,
            },
            canonical: genesis::canonical(),
            max_original_bytes: 64 << 10,
            max_heap_bytes: 64 << 20,
        }
    }
}
#[path = "incremental_module_execution_rule_phase_tests.rs"]
mod rule_cases;
#[path = "incremental_rule_core_test_runner.rs"]
mod runner;
pub use runner::{IwsRuleCoreCaseV1, run_iws_rule_core_case_v1};
