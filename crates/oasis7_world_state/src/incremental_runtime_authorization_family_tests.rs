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
            _metadata: memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap(),
        }
    }
}
impl ValueNodeWriter for Pages {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        assert!(self.nodes.borrow().len() < 16_384);
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

use crate::incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1};
use crate::incremental_runtime_authorization_family_genesis::*;
use crate::incremental_runtime_authorization_genesis::*;
use oasis7_formal_protocol::incremental_execution::*;
fn jw() -> JsonWriteWorkV1 {
    JsonWriteWorkV1 {
        cells_remaining: 100000,
        json_nodes_remaining: 1000,
        max_json_depth: 16,
        max_pending_node_bytes: 1 << 20,
    }
}
fn jr() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 100000,
        json_nodes_remaining: 1000,
        max_depth: 16,
        max_payload_bytes: 1 << 20,
    }
}
fn receipt() -> crate::CapabilityAuthorizationAuditReceipt {
    serde_json::from_value(serde_json::json!({"receipt_id":"independent-inner","subject":{"raw":[null,-7,u64::MAX]},"audience":null,"scope_hash":"s","decision":"pending","budget_before":-1,"budget_after":0,"world_head_before":0,"branch_id":"","finality_epoch":0,"finality_status":"","state_hash_before":"","canonical_request_hash":"req","canonical_result_hash":"res","committed_effect_receipt_ids":["a","b"]})).unwrap()
}
#[test]
fn authorization_family_same_transaction_updates_replay_and_wrong_expected_poison() {
    let memory = MemoryBudget::new(192 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let original = receipt();
    let mut audits = BTreeMap::new();
    audits.insert("original-outer-key".into(), original.clone());
    let mut rows = Vec::new();
    prepare_authorization_record_families_genesis_v1(
        &audits,
        &BTreeMap::new(),
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut AuthorizationWriteWorkV1 {
            cells_remaining: 100000,
            max_pending_node_bytes: 1 << 20,
        },
        &mut jw(),
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("auth-transaction".into(), [96; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut read_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let proof = qualify_authorization_record_families_v1(
        &roots,
        &mut context(&pages, &roots, &memory, &mut read_io, &mut keys),
        &mut scan(),
        &mut scan(),
        &mut jr(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(proof.audit_receipts(), 1);
    assert_eq!(proof.invocation_contexts(), 0);
    let extra = RefCell::new(Pages::new(&memory));
    let values = Combined {
        base: &pages,
        extra: &extra,
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
    let limits = TransactionLimits {
        max_reads: 64,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    };
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [6; 32], &values, &memory, limits).unwrap();
    let mut after = original.clone();
    after.budget_after = Some(17);
    let mut second = after.clone();
    second.budget_after = Some(18);
    let mut reads = io();
    let mut writes = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut sink = Sink(&extra);
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: &values,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut writes,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut work = AuthorizationWriteWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 1 << 20,
    };
    let mut jw = jw();
    let mut json_reads = jr();
    let mut body_scan = scan();
    for (expected, next) in [(&original, &after), (&after, &second)] {
        let guard = tx.guard();
        assert!(
            change_authorization_record_overlay_v1(
                &proof,
                &roots,
                AuthorizationRecordChangeV1::Audit {
                    key: "original-outer-key",
                    expected: Some(expected),
                    after: Some(next)
                },
                &mut tx,
                &mut read,
                &mut write,
                &guard,
                &live,
                &mut body_scan,
                &mut json_reads,
                1 << 20,
                &mut work,
                &mut jw
            )
            .unwrap()
        );
    }
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    for node in &prepared.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        extra
            .borrow_mut()
            .put_node(
                id,
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let mut after_reads = io();
    let mut after_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: prepared
            .record
            .after
            .state_root(StateNamespaceV1::Authorization),
        state: &values,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut after_reads,
        },
        key_work: &mut after_keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [6; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let accepted = accept_authorization_record_families_transition_v1(
        &proof,
        &verifier,
        &prepared.record,
        &mut ctx,
        &mut scan(),
        &mut jr(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(accepted.audit_receipts(), 1);
    let full = qualify_authorization_record_families_v1(
        &prepared.record.after,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut jr(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(accepted.audit_receipts(), full.audit_receipts());
    let (entry, _) = ctx
        .key(
            descriptor(0),
            WorldFieldRecordKindV1::Entry,
            &"original-outer-key",
        )
        .unwrap();
    let value = ctx.lookup(&entry).unwrap().unwrap();
    let body_root = child(AuthorizationBodyKindV1::Audit, &entry, &value, &mut ctx).unwrap();
    let recovered =
        recover_authorization_audit_body_v1(&body_root, &mut ctx, &mut scan(), &mut jr(), 1 << 20)
            .unwrap();
    assert_eq!(recovered.value().budget_after, Some(18));
    assert_eq!(recovered.value().receipt_id, "independent-inner");
    let mut rejected = ExecutionTransaction::begin(
        parent,
        [7; 32],
        &values,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let guard = rejected.guard();
    assert!(
        change_authorization_record_overlay_v1(
            &proof,
            &roots,
            AuthorizationRecordChangeV1::Audit {
                key: "original-outer-key",
                expected: Some(&after),
                after: Some(&second)
            },
            &mut rejected,
            &mut read,
            &mut write,
            &guard,
            &live,
            &mut scan(),
            &mut jr(),
            1 << 20,
            &mut work,
            &mut jw
        )
        .is_err()
    );
    let guard = rejected.guard();
    assert!(rejected.seal(&guard, &live, 1).is_err());
}

#[path = "incremental_runtime_authorization_exact_json_tests.rs"]
mod exact_json;
