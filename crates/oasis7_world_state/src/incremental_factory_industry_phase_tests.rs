//! Real GEN -> Recovery qualifier -> independently sealed ordinary no-op.
use crate::{
    WorldState,
    incremental_factory_records::*,
    incremental_industry_jobs::*,
    incremental_semantic_key::hash_semantic_key_v1,
    incremental_typed_record_test_support::{Pages, Writer, io},
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
    },
    incremental_state::{MerkleNodeReader, StateDeltaV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use serde_json::json;
use std::sync::Arc;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1000000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 10000,
    }
}
fn fw() -> FactoryWorkBudgetV1 {
    FactoryWorkBudgetV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
fn iw() -> IndustryWorkBudgetV1 {
    IndustryWorkBudgetV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
struct Recovery<'a> {
    pages: &'a Pages,
    memory: &'a MemoryBudget,
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl ValueNodeReader for Recovery<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if class != MemoryClass::Recovery || !memory.same_pool(self.memory) {
            return Err(ValueTreeError::Invalid("phase recovery source"));
        }
        let bytes = ValueNodeReader::read_node(self.pages, id, max, memory, MemoryClass::Ordinary)?;
        let charge = memory.reserve(bytes.len() + 128, class)?;
        Ok(Arc::new(charge.retain(bytes.to_vec())))
    }
}
impl MerkleNodeReader for Recovery<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Recovery)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        ValueNodeReader::read_node(self, &id, max, self.memory, MemoryClass::Recovery)
            .map(|n| Some(Node(n)))
            .map_err(|e| e.to_string())
    }
}
fn roots(memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    roots_from(memory, pages, &WorldState::default())
}
fn roots_from(memory: &MemoryBudget, pages: &Pages, source: &WorldState) -> RootSetV1 {
    let mut rows = Vec::new();
    let mut writer = Writer(pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut records = 10000;
    prepare_factory_records_genesis_v1(source, &mut ctx, &mut records, &mut fw(), &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    prepare_industry_jobs_genesis_v1(source, &mut ctx, &mut records, &mut iw(), &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    let mut roots = RootSetV1::empty("factory-industry-phase".into(), [9; 32]).unwrap();
    for root in &mut roots.state {
        let mut writes: Vec<StateWriteV1> = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        if writes.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(root.clone(), writes, pages, 8 << 20).unwrap();
        for node in &delta.new_nodes {
            Writer(pages)
                .put_node(
                    *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                    memory
                        .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        *root = delta.after;
    }
    roots
}

fn source_with_outer_records() -> WorldState {
    let mut source = WorldState::default();
    let factory = serde_json::from_value(json!({
        "factory_id": "embedded-factory-id",
        "site_id": "site-1",
        "builder_agent_id": "builder-1",
        "spec": {
            "factory_id": "embedded-factory-id",
            "display_name": "test factory",
            "tier": 1,
            "tags": [],
            "build_cost": [],
            "build_time_ticks": 1,
            "base_power_draw": 1,
            "recipe_slots": 1,
            "throughput_bps": 1,
            "maintenance_per_tick": 1
        },
        "built_at": 1
    }))
    .unwrap();
    source.factories.insert("outer-factory-key".into(), factory);
    let recipe = serde_json::from_value(json!({
        "job_id": 73,
        "requester_agent_id": "requester-1",
        "factory_id": "embedded-factory-id",
        "recipe_id": "recipe-1",
        "accepted_batches": 1,
        "consume": [],
        "produce": [],
        "byproducts": [],
        "power_required": 0,
        "duration_ticks": 1,
        "consume_ledger": "world",
        "output_ledger": "world",
        "bottleneck_tags": [],
        "logistics_route_ids": [],
        "logistics_path_ids": [],
        "ready_at": 1
    }))
    .unwrap();
    source.pending_recipe_jobs.insert(73, recipe);
    source
}

fn qualify_factory(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
) -> ReservedValue<FactoryFamilyQualificationV1> {
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut outer = scan();
    qualify_factory_family_v1(
        roots,
        &mut FactoryFamilyReadContextV1 {
            state: pages,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            max_payload_bytes: 1 << 20,
            outer_scan: &mut outer,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut fw(),
    )
    .unwrap()
}

fn qualify_industry(
    roots: &RootSetV1,
    pages: &Pages,
    memory: &MemoryBudget,
) -> ReservedValue<IndustryFamilyQualificationV1> {
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut outer = scan();
    qualify_industry_family_v1(
        roots,
        &mut IndustryFamilyReadContextV1 {
            state: pages,
            values: ValueReadContext {
                source: pages,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            max_payload_bytes: 1 << 20,
            outer_scan: &mut outer,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut iw(),
    )
    .unwrap()
}

fn factory_outer_key(memory: &MemoryBudget) -> WorldFieldKeyV1 {
    let mut budget = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let semantic = hash_semantic_key_v1(
        "outer-factory-key",
        memory,
        MemoryClass::Ordinary,
        &mut budget,
        || false,
    )
    .unwrap();
    WorldFieldKeyV1::new(
        FactoryRecordKeyV1::Factory("outer-factory-key")
            .descriptor()
            .unwrap(),
        WorldFieldRecordKindV1::Entry,
        Some(semantic.digest),
    )
    .unwrap()
}

fn recipe_outer_key(memory: &MemoryBudget) -> WorldFieldKeyV1 {
    let mut budget = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let semantic =
        hash_semantic_key_v1(&73u64, memory, MemoryClass::Ordinary, &mut budget, || false).unwrap();
    WorldFieldKeyV1::new(
        IndustryRecordKeyV1 {
            kind: IndustryRecordKindV1::Recipe,
            id: 73,
        }
        .descriptor()
        .unwrap(),
        WorldFieldRecordKindV1::Entry,
        Some(semantic.digest),
    )
    .unwrap()
}
macro_rules! phase_case{($name:ident,$qualify:ident,$context:ident,$verify:ident,$receiver:ident,$work:ident,$readmit:ident)=>{
 #[test]fn $name(){let memory=MemoryBudget::new(128<<20,16<<20).unwrap();let _source=memory.reserve(8<<20,MemoryClass::Ordinary).unwrap();let pages=Pages::with_limit(&memory,16384);let roots=roots(&memory,&pages);let recovery=Recovery{pages:&pages,memory:&memory};let mut quota=io();let mut keys=SemanticKeyBudget{bytes_remaining:8<<20};let mut outer=scan();
 let proof=$qualify(&roots,&mut $context{state:&recovery,values:ValueReadContext{source:&recovery,memory:&memory,class:MemoryClass::Recovery,io:&mut quota},key_work:&mut keys,semantic_bytes:4096,record_bytes:2<<20,max_payload_bytes:1<<20,outer_scan:&mut outer,take_state_error:||None,cancelled:||false},&mut $work()).unwrap();
 assert_eq!(proof.allocation_class(),MemoryClass::Recovery);
 let parent=ExecutionParent{roots:roots.clone(),commit:[1;32],authority:AuthorityBinding{token:[2;32],epoch:3}};let live=parent.fence().unwrap();let tx=ExecutionTransaction::begin(parent.clone(),[3;32],&pages,&memory,TransactionLimits{max_reads:8,max_writes:8,max_facts:8,max_pending_node_bytes:8<<20}).unwrap();let guard=tx.guard();let prepared=tx.seal(&guard,&live,1).unwrap();
 let result=$verify(&parent,[3;32],&prepared.record,&proof,&mut $receiver{values:ValueReadContext{source:&pages,memory:&memory,class:MemoryClass::Ordinary,io:&mut quota},key_work:&mut keys,semantic_bytes:4096,record_bytes:2<<20,max_payload_bytes:1<<20,cancelled:||false},&mut $work());
 assert!(result.is_err(),"Recovery-qualified family must not authorize an Ordinary no-op receiver without explicit readmission");
 let promoted=$readmit(&proof,&roots,&memory,||false).unwrap();
 assert_eq!(promoted.allocation_class(),MemoryClass::Ordinary);
 assert_eq!(proof.record_counts(),promoted.record_counts());
 let result=$verify(&parent,[3;32],&prepared.record,&promoted,&mut $receiver{values:ValueReadContext{source:&pages,memory:&memory,class:MemoryClass::Ordinary,io:&mut quota},key_work:&mut keys,semantic_bytes:4096,record_bytes:2<<20,max_payload_bytes:1<<20,cancelled:||false},&mut $work());
 result.unwrap();
 assert!($readmit(&promoted,&roots,&memory,||false).is_err());
 let mut wrong=roots.clone();wrong.simulation_tick=wrong.simulation_tick.wrapping_add(1);
 assert!($readmit(&proof,&wrong,&memory,||false).is_err());
 let foreign=MemoryBudget::new(128<<20,16<<20).unwrap();
 assert!($readmit(&proof,&roots,&foreign,||false).is_err());
 assert!(matches!($readmit(&proof,&roots,&memory,||true),Err(ValueTreeError::Cancelled)));
 let used=memory.used();
 let exhausted=memory.reserve((112<<20)-used,MemoryClass::Ordinary).unwrap();
 assert!(matches!($readmit(&proof,&roots,&memory,||false),Err(ValueTreeError::Memory(_))));
 drop(exhausted);
 let mut polls=0;
 assert!(matches!($readmit(&proof,&roots,&memory,||{polls+=1;polls>1}),Err(ValueTreeError::Cancelled)));

 }
}}
phase_case!(
    factory_recovery_token_cannot_enter_ordinary_noop,
    qualify_factory_family_v1,
    FactoryFamilyReadContextV1,
    verify_factory_transition_v1,
    FactoryTransitionReadContextV1,
    fw,
    readmit_factory_family_ordinary_v1
);
phase_case!(
    industry_recovery_token_cannot_enter_ordinary_noop,
    qualify_industry_family_v1,
    IndustryFamilyReadContextV1,
    verify_industry_transition_v1,
    IndustryTransitionReadContextV1,
    iw,
    readmit_industry_family_ordinary_v1
);

#[test]
fn factory_current_identity_reads_real_outer_key_and_pending_delete() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65_536);
    let roots = roots_from(&memory, &pages, &source_with_outer_records());
    let proof = qualify_factory(&roots, &pages, &memory);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut outer = scan();
    let mut context = FactoryFamilyReadContextV1 {
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut outer,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut identities = Vec::new();
    visit_qualified_factory_parent_identities_v1(
        &roots,
        &proof,
        &mut context,
        &mut fw(),
        |identity| {
            identities.push(identity);
            Ok(())
        },
    )
    .unwrap();
    assert!(identities.iter().any(|identity| matches!(
        &**identity,
        FactoryQualifiedOuterIdentityV1::Record { id, record_root, .. }
            if id == "outer-factory-key" && record_root.namespace == oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business
    )));

    let mut wrong_roots = roots.clone();
    wrong_roots.simulation_tick = wrong_roots.simulation_tick.wrapping_add(1);
    assert!(
        visit_qualified_factory_parent_identities_v1(
            &wrong_roots,
            &proof,
            &mut context,
            &mut fw(),
            |_| Ok(()),
        )
        .is_err()
    );
    let mut limited = fw();
    limited.cells_remaining = 0;
    let mut rejected_emissions = 0;
    assert!(
        visit_qualified_factory_parent_identities_v1(
            &roots,
            &proof,
            &mut context,
            &mut limited,
            |_| {
                rejected_emissions += 1;
                Ok(())
            },
        )
        .is_err()
    );
    assert_eq!(rejected_emissions, 0);
    let mut partial_emissions = 0;
    assert!(
        visit_qualified_factory_parent_identities_v1(
            &roots,
            &proof,
            &mut context,
            &mut fw(),
            |_| {
                partial_emissions += 1;
                Err(ValueTreeError::Invalid("partial factory directory"))
            },
        )
        .is_err()
    );
    assert_eq!(partial_emissions, 1);
    let mut current_quota = io();
    let mut current_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };

    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [41; 32],
        authority: AuthorityBinding {
            token: [42; 32],
            epoch: 1,
        },
    };
    let mut transaction = ExecutionTransaction::begin(
        parent,
        [43; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 8,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let key = factory_outer_key(&memory);
    transaction
        .write(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            key.as_bytes(),
            None,
        )
        .unwrap();
    let mut pending = Vec::new();
    transaction
        .contribute(|transaction| {
            visit_factory_current_outer_changes_v1(
                transaction,
                &proof,
                &mut FactoryTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &pages,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut current_quota,
                    },
                    key_work: &mut current_keys,
                    semantic_bytes: 4096,
                    record_bytes: 2 << 20,
                    max_payload_bytes: 1 << 20,
                    cancelled: || false,
                },
                &mut fw(),
                |change| {
                    pending.push(change);
                    Ok(())
                },
            )?;
            Ok::<(), crate::incremental_field_overlay::WorldFieldOverlayErrorV1>(())
        })
        .unwrap();
    assert!(pending.iter().any(|change| matches!(
        &**change,
        FactoryCurrentOuterChangeV1::Record { id, before: Some(_), after: None }
            if id == "outer-factory-key"
    )));
}

#[test]
fn recipe_current_identity_reads_real_outer_key_and_pending_delete() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _source = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 65_536);
    let roots = roots_from(&memory, &pages, &source_with_outer_records());
    let proof = qualify_industry(&roots, &pages, &memory);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut outer = scan();
    let mut context = IndustryFamilyReadContextV1 {
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut outer,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut identities = Vec::new();
    visit_qualified_recipe_parent_identities_v1(
        &roots,
        &proof,
        &mut context,
        &mut iw(),
        |identity| {
            identities.push(identity);
            Ok(())
        },
    )
    .unwrap();
    assert!(identities.iter().any(|identity| matches!(
        &**identity,
        IndustryQualifiedOuterIdentityV1::Record { id: 73, record_root, .. }
            if record_root.namespace == oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler
    )));
    let mut wrong_roots = roots.clone();
    wrong_roots.simulation_tick = wrong_roots.simulation_tick.wrapping_add(1);
    assert!(
        visit_qualified_recipe_parent_identities_v1(
            &wrong_roots,
            &proof,
            &mut context,
            &mut iw(),
            |_| Ok(()),
        )
        .is_err()
    );
    let mut current_quota = io();
    let mut current_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };

    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [51; 32],
        authority: AuthorityBinding {
            token: [52; 32],
            epoch: 1,
        },
    };
    let mut transaction = ExecutionTransaction::begin(
        parent,
        [53; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 8,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let key = recipe_outer_key(&memory);
    transaction
        .write(
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
            key.as_bytes(),
            None,
        )
        .unwrap();
    let mut pending = Vec::new();
    transaction
        .contribute(|transaction| {
            visit_recipe_current_outer_changes_v1(
                transaction,
                &proof,
                &mut IndustryTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &pages,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut current_quota,
                    },
                    key_work: &mut current_keys,
                    semantic_bytes: 4096,
                    record_bytes: 2 << 20,
                    max_payload_bytes: 1 << 20,
                    cancelled: || false,
                },
                &mut iw(),
                |change| {
                    pending.push(change);
                    Ok(())
                },
            )?;
            Ok::<(), crate::incremental_field_overlay::WorldFieldOverlayErrorV1>(())
        })
        .unwrap();
    assert!(pending.iter().any(|change| matches!(
        &**change,
        IndustryCurrentOuterChangeV1::Record {
            id: 73,
            before: Some(_),
            after: None
        }
    )));
}
