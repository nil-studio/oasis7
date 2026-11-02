use super::*;
use crate::{incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1};
use oasis7_formal_protocol::incremental_material::OriginalObjectReader;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::MemoryReservation;
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
type Nodes = BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>;
struct Pages {
    memory: MemoryBudget,
    nodes: RefCell<Nodes>,
    _metadata: MemoryReservation,
}
impl Pages {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            memory: memory.clone(),
            nodes: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 16384);
        assert_eq!(blake3::hash(&bytes).as_bytes(), &id);
        self.nodes.borrow_mut().insert(id, Arc::new(bytes));
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
        let bytes = self
            .nodes
            .borrow()
            .get(id)
            .cloned()
            .ok_or(ValueTreeError::Invalid("missing fixture page"))?;
        if !bytes.uses_budget(memory) || class != MemoryClass::Ordinary || bytes.len() > max {
            return Err(ValueTreeError::Invalid("fixture page admission"));
        }
        Ok(bytes)
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
                    Err("fixture state bound".into())
                } else {
                    Ok(Node(n.clone()))
                }
            })
            .transpose()
    }
}
const ORIGINAL_BYTES: &[u8] = b"original artifact bytes";
const NEW_BYTES: &[u8] = b"new original content";
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn seed(memory: &MemoryBudget) -> (Pages, RootSetV1) {
    let mut pages = Pages::new(memory);
    let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let registered = std::collections::BTreeSet::from(["".into(), " registered-only 界 ".into()]);
    let originals: BTreeMap<String, Arc<[u8]>> = BTreeMap::from([
        ("".into(), Arc::from(ORIGINAL_BYTES)),
        ("bytes-only".into(), Arc::from(ORIGINAL_BYTES)),
    ]);
    let mut rows = Vec::with_capacity(6);
    let mut remaining = 6;
    prepare_runtime_artifact_content_genesis_v1(
        &registered,
        &originals,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
        &mut remaining,
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
        &mut |original, bytes| {
            assert_eq!(original.id, *blake3::hash(ORIGINAL_BYTES).as_bytes());
            assert_eq!(bytes, ORIGINAL_BYTES);
            Ok(())
        },
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(remaining, 0);
    let mut roots = RootSetV1::empty("artifact-content-fixture".into(), [29; 32]).unwrap();
    let _work = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        roots.state_root(StateNamespaceV1::Artifacts).clone(),
        writes,
        &pages,
        8 << 20,
    )
    .unwrap();
    for node in delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages.put_node(id, charge.retain(node.bytes)).unwrap();
    }
    roots.state[5] = delta.after;
    (pages, roots)
}
struct Originals<'a> {
    memory: &'a MemoryBudget,
    missing: bool,
    calls: usize,
}
impl OriginalObjectReader for Originals<'_> {
    type Error = &'static str;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        panic!("stream only")
    }
}
impl OriginalObjectStreamReader for Originals<'_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        _: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        self.calls += 1;
        if self.missing {
            return Err("missing required raw original");
        }
        let bytes = if id == blake3::hash(ORIGINAL_BYTES).as_bytes() {
            ORIGINAL_BYTES
        } else if id == blake3::hash(NEW_BYTES).as_bytes() {
            NEW_BYTES
        } else {
            return Err("missing required raw original");
        };
        assert_eq!(max, bytes.len() as u64);
        output
            .write_all(bytes)
            .map_err(|_| "fixture output refused")?;
        Ok(bytes.len() as u64)
    }
}
#[test]
fn borrowed_genesis_actual_merkle_roots_require_both_metadata_and_every_raw_original() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory);
    for mode in 0..3 {
        let mut selected = roots.clone();
        if mode == 2 {
            selected.state[5] = StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap();
        }
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: &selected.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut originals = Originals {
            memory: &memory,
            missing: mode == 1,
            calls: 0,
        };
        let mut work = RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        };
        let result = qualify_runtime_artifact_content_v1(
            &selected,
            &mut ctx,
            &mut scan(),
            &mut originals,
            &mut work,
        );
        match mode {
            0 => {
                let proof = result.unwrap();
                proof.validate_binding(&selected).unwrap();
                assert_eq!(originals.calls, 2);
                assert_eq!(work.objects_remaining, 0);
                let mut wrong = selected.clone();
                wrong.height = 1;
                assert!(proof.validate_binding(&wrong).is_err());
            }
            1 => assert!(matches!(
                result,
                Err(RuntimeArtifactOriginalErrorV1::Source(
                    "missing required raw original"
                ))
            )),
            _ => {
                assert!(matches!(
                    result,
                    Err(RuntimeArtifactOriginalErrorV1::Value(
                        ValueTreeError::Invalid(_)
                    ))
                ));
                assert_eq!(originals.calls, 0);
            }
        }
    }
}

#[test]
fn changed_only_receiver_inherits_old_raws_and_rejects_metadata_and_bad_new_references() {
    use crate::incremental_semantic_key::hash_semantic_key_v1;
    use oasis7_formal_protocol::incremental_execution::*;
    for mode in 0..4 {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let (mut pages, roots) = seed(&memory);
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        };
        let parent_proof = qualify_runtime_artifact_content_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut Originals {
                memory: &memory,
                missing: false,
                calls: 0,
            },
            &mut RuntimeArtifactOriginalWorkV1 {
                objects_remaining: 2,
                bytes_remaining: 4096,
                max_object_bytes: 4096,
            },
        )
        .unwrap();
        let (field, kind, key_text) = if mode == 0 {
            (
                RuntimeArtifactContentFieldV1::Registered,
                WorldFieldRecordKindV1::Entry,
                " registered-only 界 ",
            )
        } else {
            (
                RuntimeArtifactContentFieldV1::Original,
                if mode == 1 {
                    WorldFieldRecordKindV1::Metadata
                } else {
                    WorldFieldRecordKindV1::Bytes
                },
                "bytes-only",
            )
        };
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let key = if mode == 1 {
            WorldFieldKeyV1::new(field.descriptor(), kind, None).unwrap()
        } else {
            let semantic = hash_semantic_key_v1(
                key_text,
                &memory,
                MemoryClass::Ordinary,
                &mut keys,
                &mut || false,
            )
            .unwrap();
            WorldFieldKeyV1::new(field.descriptor(), kind, Some(semantic.digest)).unwrap()
        };
        let after = if mode < 2 {
            None
        } else {
            let mut bytes = runtime_artifact_original_bytes_v1(OriginalArtifactRefV1 {
                id: *blake3::hash(ORIGINAL_BYTES).as_bytes(),
                byte_len: ORIGINAL_BYTES.len() as u64 + 1,
            });
            if mode == 2 {
                bytes[0] ^= 1;
            }
            let row = write_member_record_v1(
                field.descriptor(),
                kind,
                key_text,
                MemberPayloadRefV1::Bytes(&bytes),
                &mut MemberRecordWriteContext {
                    sink: &mut pages,
                    memory: &memory,
                    io: &mut io(),
                    key_work: &mut keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            Some(row.value_root)
        };
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 8,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        tx.write(StateNamespaceV1::Artifacts, key.as_bytes(), after)
            .unwrap();
        let guard = tx.guard();
        let prepared = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(prepared.record.writes.len(), 1);
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        verifier.verify(&prepared.record).unwrap();
        let mut reads = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: &prepared.record.after.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut originals = Originals {
            memory: &memory,
            missing: true,
            calls: 0,
        };
        let mut allowance = RuntimeArtifactOriginalWorkV1 {
            objects_remaining: if mode == 3 { 1 } else { 0 },
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        };
        let result = accept_runtime_artifact_content_transition_v1(
            &parent_proof,
            &verifier,
            &prepared.record,
            &mut ctx,
            &mut originals,
            &mut allowance,
        );
        match mode {
            0 => {
                result
                    .unwrap()
                    .validate_binding(&prepared.record.after)
                    .unwrap();
                assert_eq!(originals.calls, 0);
            }
            3 => {
                assert!(matches!(
                    result,
                    Err(RuntimeArtifactOriginalErrorV1::Source(
                        "missing required raw original"
                    ))
                ));
                assert_eq!(originals.calls, 1);
            }
            _ => {
                assert!(matches!(
                    result,
                    Err(RuntimeArtifactOriginalErrorV1::Value(
                        ValueTreeError::Invalid(_)
                    ))
                ));
                assert_eq!(originals.calls, 0);
            }
        }
    }
}

struct Combined<'a> {
    base: &'a Pages,
    extra: &'a RefCell<Pages>,
}
impl ValueNodeReader for Combined<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if self.extra.borrow().nodes.borrow().contains_key(id) {
            ValueNodeReader::read_node(&*self.extra.borrow(), id, max, memory, class)
        } else {
            ValueNodeReader::read_node(self.base, id, max, memory, class)
        }
    }
}
impl MerkleNodeReader for Combined<'_> {
    type Node = Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(&self.base.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Node>, String> {
        if let Some(value) = MerkleNodeReader::read_node(&*self.extra.borrow(), id, max)? {
            Ok(Some(value))
        } else {
            MerkleNodeReader::read_node(self.base, id, max)
        }
    }
}
struct Sink<'a>(&'a RefCell<Pages>);
impl ValueNodeWriter for Sink<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.0.borrow_mut().put_node(id, bytes)
    }
}
#[test]
fn shared_transaction_keyed_reads_changes_and_noops_pass_changed_receiver_then_raw_reopen() {
    use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
    use oasis7_formal_protocol::incremental_execution::*;
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory);
    let extra = RefCell::new(Pages::new(&memory));
    let combined = Combined {
        base: &pages,
        extra: &extra,
    };
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let parent_proof = qualify_runtime_artifact_content_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut Originals {
            memory: &memory,
            missing: false,
            calls: 0,
        },
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [4; 32],
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [6; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 32,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let original = OriginalArtifactRefV1 {
        id: *blake3::hash(NEW_BYTES).as_bytes(),
        byte_len: NEW_BYTES.len() as u64,
    };
    let mut persisted = 0;
    {
        let mut reads = io();
        let mut writes = io();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut sink = Sink(&extra);
        let mut read = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &combined,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut writes,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        let mut work = RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 2,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        };
        macro_rules! change {
            ($request:expr) => {{
                let guard = read.transaction.guard();
                change_runtime_artifact_content_overlay_v1(
                    $request,
                    &mut read,
                    &mut write,
                    &mut work,
                    &mut |actual, bytes| {
                        assert_eq!(actual, original);
                        assert_eq!(bytes, NEW_BYTES);
                        persisted += 1;
                        Ok(())
                    },
                    &guard,
                    &live,
                )
                .unwrap()
            }};
        }
        assert!(change!(RuntimeArtifactContentChangeV1::Registered {
            key: "new registered-only",
            expected: false,
            after: true
        }));
        assert!(!change!(RuntimeArtifactContentChangeV1::Registered {
            key: "new registered-only",
            expected: true,
            after: true
        }));
        assert_eq!(
            read_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentFieldV1::Registered,
                "new registered-only",
                &mut read
            )
            .unwrap(),
            RuntimeArtifactContentValueV1::Registered(true)
        );
        assert!(change!(RuntimeArtifactContentChangeV1::Registered {
            key: " registered-only 界 ",
            expected: true,
            after: false
        }));
        assert!(change!(RuntimeArtifactContentChangeV1::Original {
            key: "new bytes-only",
            expected: None,
            after: Some(NEW_BYTES)
        }));
        assert_eq!(
            read_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentFieldV1::Original,
                "new bytes-only",
                &mut read
            )
            .unwrap(),
            RuntimeArtifactContentValueV1::Original(Some(original))
        );
        assert!(!change!(RuntimeArtifactContentChangeV1::Original {
            key: "new bytes-only",
            expected: Some(original),
            after: Some(NEW_BYTES)
        }));
        assert_eq!(work.objects_remaining, 0);
    }
    assert_eq!(persisted, 1);
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(prepared.record.writes.len(), 3);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let verified = verifier.verify(&prepared.record).unwrap();
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut originals = Originals {
        memory: &memory,
        missing: false,
        calls: 0,
    };
    accept_runtime_artifact_content_transition_v1(
        &parent_proof,
        &verifier,
        &prepared.record,
        &mut WorldFieldReadContextV1 {
            root: &prepared.record.after.state[0],
            state: &pages,
            values: ValueReadContext {
                source: &combined,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut originals,
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 1,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
    )
    .unwrap()
    .validate_binding(&prepared.record.after)
    .unwrap();
    assert_eq!(originals.calls, 1);
    for node in &verified.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        extra
            .borrow_mut()
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut originals = Originals {
        memory: &memory,
        missing: false,
        calls: 0,
    };
    qualify_runtime_artifact_content_v1(
        &prepared.record.after,
        &mut WorldFieldReadContextV1 {
            root: &prepared.record.after.state[0],
            state: &combined,
            values: ValueReadContext {
                source: &combined,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        &mut originals,
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 3,
            bytes_remaining: 4096,
            max_object_bytes: 4096,
        },
    )
    .unwrap()
    .validate_binding(&prepared.record.after)
    .unwrap();
    assert_eq!(originals.calls, 3);
}

#[path = "incremental_runtime_artifact_content_pages_tests.rs"]
mod pages_tests;
