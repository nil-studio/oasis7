use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type PageMap = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    memory: MemoryBudget,
    nodes: RefCell<PageMap>,
    _metadata: MemoryReservation,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 131_072);
        self.nodes.borrow_mut().insert(id, Arc::new(value));
        Ok(())
    }
}
impl ValueNodeReader for Pages {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let value = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("business test missing page"))?;
        if !value.uses_budget(memory) || value.len() > max {
            return Err(ValueTreeError::Invalid("grant test admission"));
        }
        if class == MemoryClass::Ordinary {
            return Ok(value);
        }
        let charge = memory.reserve(value.len(), class)?;
        Ok(Arc::new(charge.retain(value.to_vec())))
    }
}
struct Node(Arc<ReservedValue<Vec<u8>>>);
impl AsRef<[u8]> for Node {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
impl MerkleNodeReader for Pages {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        let id = *blake3::Hash::from_hex(id)
            .map_err(|e| e.to_string())?
            .as_bytes();
        self.nodes
            .borrow()
            .get(&id)
            .map(|n| {
                if n.len() > max {
                    Err("business state bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}

impl StatePageWriter for Pages {}
fn context<'a>(
    pages: &'a Pages,
    roots: &'a RootSetV1,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError> + use<>,
    impl FnMut() -> bool + use<>,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100_000,
    }
}

use crate::incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1};
fn write_work() -> RevocationWriteWorkV1 {
    RevocationWriteWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 4 << 20,
        json: JsonWriteWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 10000,
            max_json_depth: 32,
            max_pending_node_bytes: 4 << 20,
        },
    }
}
fn read_work() -> RevocationReadWorkV1 {
    RevocationReadWorkV1 {
        max_retained_trees: 1024,
        max_tree_heap: 64 << 10,
        json: JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 10000,
            max_depth: 32,
            max_payload_bytes: 2 << 20,
        },
    }
}
#[path = "incremental_runtime_revocation_fixture.rs"]
mod fixture;
#[test]
fn nonempty_all_revocation_families_roundtrip_from_only_persisted_pages() {
    let memory = MemoryBudget::new(256 << 20, 64 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let expected = fixture::value();
    let root = {
        let mut io = io();
        // This all-family fixture emits repeated signed nested DTOs. Its
        // measured 100,000-node failure left 57,620,913 write bytes unused.
        io.nodes_remaining = 500_000;
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let result = {
            let mut ctx = MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io,
                key_work: &mut key_work,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 2 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            };
            prepare_runtime_revocation_body_v1(&expected, &mut ctx, &mut write_work())
        };
        eprintln!(
            "revocation-gen quota nodes_remaining={} read_bytes_remaining={} write_bytes_remaining={} unique_pages={}",
            io.nodes_remaining,
            io.read_bytes_remaining,
            io.write_bytes_remaining,
            pages.nodes.borrow().len()
        );
        result.unwrap()
    };
    let roots = RootSetV1::empty("revocation-test".into(), [1; 32]).unwrap();
    let mut read_io = io();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    eprintln!(
        "revocation-body baseline node_root={} leaf_count={}",
        root.node_root, root.leaf_count
    );
    let mut full_scan = scan();
    full_scan.nodes_remaining = 500_000;
    let result = {
        let mut ctx = context(&pages, &roots, &memory, &mut read_io, &mut key_work);
        ctx.record_bytes = 2 << 20;
        ctx.semantic_bytes = 1 << 20;
        recover_runtime_revocation_body_v1(&root, &mut ctx, &mut full_scan, &mut read_work())
    };
    eprintln!(
        "revocation-cold quota scan_nodes_remaining={} scan_leaves_remaining={} scan_read_bytes_remaining={} value_nodes_remaining={} value_read_bytes_remaining={}",
        full_scan.nodes_remaining,
        full_scan.leaves_remaining,
        full_scan.read_bytes_remaining,
        read_io.nodes_remaining,
        read_io.read_bytes_remaining
    );
    let recovered = result.unwrap();
    assert_eq!(recovered.value(), &expected);
    assert_eq!(
        recovered.value().safe_alternatives["safe"]["negative_zero"]
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    // The charged DTO survives after all borrowed contexts have been released.
    assert_eq!(
        recovered
            .value()
            .task_terminals_v2
            .as_ref()
            .unwrap()
            .records["task"]
            .stop_predecessor_canonical_cbor,
        Some(vec![0, 255, 1, 0])
    );
}
#[test]
fn absent_terminal_and_present_empty_original_collections_remain_distinct() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let expected = crate::CapabilityRevocationState::default();
    let root = prepare_runtime_revocation_body_v1(
        &expected,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 4096,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap();
    let roots = RootSetV1::empty("revocation-test".into(), [1; 32]).unwrap();
    let mut read_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut read_io, &mut keys);
    let result =
        recover_runtime_revocation_body_v1(&root, &mut ctx, &mut scan(), &mut read_work()).unwrap();
    assert_eq!(result.value(), &expected);
}

fn save_delta(delta: StateDeltaV1, pages: &mut Pages, memory: &MemoryBudget) -> StateRootV1 {
    for page in delta.new_nodes {
        let id = *blake3::Hash::from_hex(&page.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(page.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .nodes
            .borrow_mut()
            .insert(id, Arc::new(charge.retain(page.bytes)));
    }
    delta.after
}
#[test]
fn missing_required_column_and_new_well_formed_unknown_column_fail_closed() {
    for extra in [false, true] {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let mut pages = Pages::new(&memory);
        let root = prepare_runtime_revocation_body_v1(
            &crate::CapabilityRevocationState::default(),
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
            &mut write_work(),
        )
        .unwrap();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 4 << 20,
        };
        let name = if extra { "unrecognized field" } else { "epoch" };
        let semantic =
            hash_semantic_key_v1(name, &memory, MemoryClass::Ordinary, &mut keys, &mut || {
                false
            })
            .unwrap();
        let key = WorldFieldKeyV1::new(
            field(),
            WorldFieldRecordKindV1::Entry,
            Some(semantic.digest),
        )
        .unwrap();
        let after = if extra {
            Some(
                write_member_record_v1(
                    field(),
                    WorldFieldRecordKindV1::Entry,
                    name,
                    MemberPayloadRefV1::U64(0),
                    &mut MemberRecordWriteContext {
                        sink: &mut pages,
                        memory: &memory,
                        io: &mut io(),
                        key_work: &mut keys,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 4096,
                            record_bytes: 4096,
                        },
                        cancelled: || false,
                    },
                )
                .unwrap()
                .value_root,
            )
        } else {
            None
        };
        let before = root.get(key.as_bytes(), &pages).unwrap();
        let delta = StateDeltaV1::build(
            root,
            vec![StateWriteV1 {
                key: key.as_bytes().to_vec(),
                before,
                after,
            }],
            &pages,
            4 << 20,
        )
        .unwrap();
        let bad = save_delta(delta, &mut pages, &memory);
        let roots = RootSetV1::empty("revocation-test".into(), [1; 32]).unwrap();
        let mut read_io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 4 << 20,
        };
        let mut ctx = context(&pages, &roots, &memory, &mut read_io, &mut keys);
        assert!(
            recover_runtime_revocation_body_v1(&bad, &mut ctx, &mut scan(), &mut read_work())
                .is_err()
        );
    }
}

#[path = "incremental_runtime_revocation_transaction_tests.rs"]
mod transactions;

#[test]
fn original_json_negative_zero_is_a_distinct_typed_body() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut value = crate::CapabilityRevocationState::default();
    let mut roots = Vec::with_capacity(2);
    for number in [-0.0_f64, 0.0_f64] {
        value
            .safe_alternatives
            .insert("float".into(), serde_json::json!(number));
        let root = prepare_runtime_revocation_body_v1(
            &value,
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4 << 20,
                },
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 4096,
                    record_bytes: 4096,
                },
                cancelled: || false,
            },
            &mut write_work(),
        )
        .unwrap();
        roots.push(root);
    }
    assert_ne!(roots[0], roots[1]);
}

#[test]
fn nested_small_children_do_not_retain_each_configured_decode_ceiling() {
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let expected = crate::CapabilityRevocationState::default();
    let root = prepare_runtime_revocation_body_v1(
        &expected,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 4096,
            },
            cancelled: || false,
        },
        &mut write_work(),
    )
    .unwrap();
    let roots = RootSetV1::empty("revocation-test".into(), [1; 32]).unwrap();
    let baseline = memory.used();
    let mut read_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    let mut work = read_work();
    work.max_tree_heap = 4 << 20;
    let recovered = recover_runtime_revocation_body_v1(
        &root,
        &mut context(&pages, &roots, &memory, &mut read_io, &mut keys),
        &mut scan(),
        &mut work,
    )
    .unwrap();
    assert_eq!(recovered.value(), &expected);
    assert!(memory.used() - baseline < 1 << 20);
    drop(recovered);
    assert_eq!(memory.used(), baseline);
}
