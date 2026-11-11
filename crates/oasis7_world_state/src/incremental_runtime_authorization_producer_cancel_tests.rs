//! Read-side cancellation must stop provisional production at the next boundary.
use super::*;
use std::{cell::Cell, rc::Rc};
struct CancelSink {
    pages: Pages,
    cancel: Rc<Cell<bool>>,
    puts: Rc<Cell<usize>>,
}
impl ValueNodeWriter for CancelSink {
    fn put_node(
        &mut self,
        id: [u8; 32],
        value: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.puts.set(self.puts.get() + 1);
        self.pages.put_node(id, value)?;
        self.cancel.set(true);
        Ok(())
    }
}
impl ValueNodeReader for CancelSink {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        ValueNodeReader::read_node(&self.pages, id, max, memory, class)
    }
}
impl StatePageWriter for CancelSink {}
#[test]
fn audit_producer_observes_read_only_cancellation_after_first_put() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let original = receipt();
    let mut after = original.clone();
    after.subject = serde_json::Value::String("x".repeat(128 << 10));
    after.committed_effect_receipt_ids = (0..256).map(|i| format!("effect-{i}")).collect();
    let mut audits = BTreeMap::new();
    audits.insert("key".into(), original.clone());
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
    let mut roots = RootSetV1::empty("audit-cancel".into(), [97; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut read_io = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut read = context(&pages, &roots, &memory, &mut read_io, &mut rk);
    let proof = qualify_authorization_record_families_v1(
        &roots,
        &mut read,
        &mut scan(),
        &mut scan(),
        &mut jr(),
        1 << 20,
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
        parent,
        [6; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let guard = tx.guard();
    let cancelled = Rc::new(Cell::new(false));
    let puts = Rc::new(Cell::new(0));
    let mut sink = CancelSink {
        pages: Pages::new(&memory),
        cancel: cancelled.clone(),
        puts: puts.clone(),
    };
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || cancelled.get(),
    };
    let mut writes = io();
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
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
    let result = change_authorization_record_overlay_v1(
        &proof,
        &roots,
        AuthorizationRecordChangeV1::Audit {
            key: "key",
            expected: Some(&original),
            after: Some(&after),
        },
        &mut tx,
        &mut read,
        &mut write,
        &guard,
        &live,
        &mut scan(),
        &mut jr(),
        1 << 20,
        &mut AuthorizationWriteWorkV1 {
            cells_remaining: 100000,
            max_pending_node_bytes: 1 << 20,
        },
        &mut jw(),
    );
    assert!(result.is_err());
    assert!(cancelled.get(), "fixture reached real producer put");
    assert_eq!(
        puts.get(),
        1,
        "read cancellation must stop before more staged pages"
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
#[test]
fn grant_producer_observes_read_only_cancellation_after_first_put() {
    use crate::{
        incremental_runtime_authorization_grants::*,
        incremental_runtime_authorization_grants_family::*,
    };
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let original = serde_json::json!({"raw":0});
    let after = serde_json::Value::String("x".repeat(128 << 10));
    let mut grants = BTreeMap::new();
    grants.insert("key".into(), original.clone());
    let mut rows = Vec::new();
    prepare_authorization_grants_genesis_v1(
        &grants,
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
        &mut jw(),
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let mut roots = RootSetV1::empty("grant-cancel".into(), [97; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut read_io = io();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut read = context(&pages, &roots, &memory, &mut read_io, &mut rk);
    let proof =
        qualify_authorization_grants_family_v1(&roots, &mut read, &mut scan(), &mut jr()).unwrap();
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
        parent,
        [6; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let guard = tx.guard();
    let cancelled = Rc::new(Cell::new(false));
    let puts = Rc::new(Cell::new(0));
    let mut sink = CancelSink {
        pages: Pages::new(&memory),
        cancel: cancelled.clone(),
        puts: puts.clone(),
    };
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Authorization),
        state: &pages,
        values: ValueReadContext {
            source: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || cancelled.get(),
    };
    let mut writes = io();
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
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
    let result = change_authorization_grant_overlay_v1(
        &proof,
        &roots,
        AuthorizationGrantChangeV1 {
            key: "key",
            expected: Some(&original),
            after: Some(&after),
        },
        &mut tx,
        &mut read,
        &mut write,
        &guard,
        &live,
        &mut jr(),
        1 << 20,
        &mut jw(),
    );
    assert!(result.is_err());
    assert!(cancelled.get());
    assert_eq!(
        puts.get(),
        1,
        "grant read cancellation must stop producer early"
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
