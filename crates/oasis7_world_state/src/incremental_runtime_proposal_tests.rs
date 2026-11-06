use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1, incremental_semantic_key::SemanticKeyBudget,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_state::*,
    incremental_value::*,
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
            _metadata: memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 65_536);
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
        if !value.uses_budget(memory) || class != MemoryClass::Ordinary || value.len() > max {
            return Err(ValueTreeError::Invalid("grant test admission"));
        }
        Ok(value)
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
fn install(
    memory: &MemoryBudget,
    pages: &mut Pages,
    roots: &mut RootSetV1,
    rows: &[ReservedValue<ExecutionWriteV1>],
) {
    let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory
        .reserve((writes.len() + 1) * (512 << 10), MemoryClass::Ordinary)
        .unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
            writes,
            pages,
            8 << 20,
        )
        .unwrap(),
    );
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        pages
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap() = delta.after.clone();
}
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

impl ValueNodeReader for Sink<'_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        ValueNodeReader::read_node(&*self.0.borrow(), id, max, memory, class)
    }
}
impl StatePageWriter for Sink<'_> {}

use crate::{
    Manifest, ManifestPatch, ManifestPatchOp, Proposal, ProposalStatus,
    incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1},
};
use oasis7_formal_protocol::incremental_execution::*;
fn ww() -> RuntimeProposalWriteWorkV1 {
    RuntimeProposalWriteWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 1 << 20,
        json: JsonWriteWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 1000,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
    }
}
fn rw() -> RuntimeProposalReadWorkV1 {
    RuntimeProposalReadWorkV1 {
        cells_remaining: 100000,
        json: JsonReadWorkV1 {
            cells_remaining: 100000,
            json_nodes_remaining: 1000,
            max_depth: 16,
            max_payload_bytes: 2 << 20,
        },
        max_selected_heap: 4 << 20,
        max_text_bytes: 2 << 20,
    }
}
fn proposal(status: ProposalStatus, patch: bool) -> Proposal {
    Proposal {
        id: 77,
        author: "作者".into(),
        base_manifest_hash: "outer-base".into(),
        manifest: Manifest {
            version: u64::MAX,
            content: serde_json::json!({"bits":[null,-0.0,u64::MAX],"empty":[]}),
        },
        patch: patch.then(|| ManifestPatch {
            base_manifest_hash: "independent-patch-base".into(),
            ops: vec![
                ManifestPatchOp::Set {
                    path: vec!["".into(), "2".into()],
                    value: serde_json::json!({"zero":-0.0}),
                },
                ManifestPatchOp::Remove {
                    path: vec!["0".into()],
                },
                ManifestPatchOp::Set {
                    path: vec![],
                    value: serde_json::Value::Null,
                },
            ],
            new_version: Some(0),
        }),
        queued_at_tick: None,
        not_before_tick: Some(0),
        activate_epoch: Some(u64::MAX),
        timelock_ticks: 0,
        status,
    }
}
fn seed(memory: &MemoryBudget, entries: &BTreeMap<u64, Proposal>) -> (Pages, RootSetV1) {
    let mut pages = Pages::new(memory);
    let mut rows = Vec::new();
    prepare_runtime_proposals_genesis_v1(
        entries,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
        &mut ww(),
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("proposal-test".into(), [103; 32]).unwrap();
    install(memory, &mut pages, &mut roots, &rows);
    (pages, roots)
}
#[test]
fn proposal_cold_dto_preserves_map_identity_variants_option_order_and_float_bits() {
    for status in [
        ProposalStatus::Proposed,
        ProposalStatus::Shadowed {
            manifest_hash: "".into(),
        },
        ProposalStatus::Approved {
            manifest_hash: "h".into(),
            approver: "".into(),
        },
        ProposalStatus::Rejected {
            reason: "原始理由".into(),
        },
        ProposalStatus::Applied {
            manifest_hash: "h".into(),
        },
    ] {
        for patch in [false, true] {
            let memory = MemoryBudget::new(192 << 20, 8 << 20).unwrap();
            let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let expected = proposal(status.clone(), patch);
            let mut entries = BTreeMap::new();
            entries.insert(13, expected.clone());
            let (pages, roots) = seed(&memory, &entries);
            drop(entries);
            let mut value_io = io();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            };
            let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
            ctx.semantic_bytes = 4096;
            ctx.record_bytes = 3 << 20;
            let family =
                qualify_runtime_proposals_v1(&roots, &mut ctx, &mut scan(), &mut rw()).unwrap();
            assert_eq!(family.proposals(), 1);
            let scope = read_runtime_proposal_body_qualification_v1(&family, &roots, 13, &mut ctx)
                .unwrap()
                .unwrap();
            assert_eq!(scope.original_key(), 13);
            let result = recover_runtime_proposal_body_v1(
                scope.root(),
                &mut ctx,
                &mut scan(),
                &mut rw().json,
                4 << 20,
            )
            .unwrap();
            assert_eq!(result.value(), &expected);
            assert_eq!(result.value().id, 77);
            assert_eq!(
                result.value().manifest.content["bits"][1]
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                (-0.0f64).to_bits()
            );
            assert!(
                read_runtime_proposal_body_qualification_v1(&family, &roots, 77, &mut ctx)
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[path = "incremental_runtime_proposal_transaction_tests.rs"]
mod transaction;

#[path = "incremental_runtime_proposal_stream_tests.rs"]
mod stream;
