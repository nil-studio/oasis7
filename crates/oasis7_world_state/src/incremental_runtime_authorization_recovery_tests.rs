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
use crate::incremental_runtime_authorization_genesis::*;
fn json_work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 10000,
        json_nodes_remaining: 100,
        max_depth: 16,
        max_payload_bytes: 1 << 20,
    }
}
fn receipt() -> crate::CapabilityAuthorizationAuditReceipt {
    serde_json::from_value(serde_json::json!({
"receipt_id":"inner-original", "root_receipt_id":"", "subject":{"kind":"future","raw":[null,-7,18446744073709551615u64]}, "presenter":{"unrecognized":"raw-json"}, "audience":null,"scope_hash":"scope","decision":"pending","budget_before":-1,"budget_after":0,"world_head_before":0,"world_head_after":0,"branch_id":"","finality_epoch":0,"finality_status":"","state_hash_before":"","canonical_request_hash":"req","canonical_result_hash":"result","committed_effect_receipt_ids":["second","first"]})).unwrap()
}
fn produce<T>(
    memory: &MemoryBudget,
    pages: &mut Pages,
    source: &T,
    f: impl FnOnce(
        &T,
        &mut MemberRecordWriteContext<'_, Pages, fn() -> bool>,
        &mut AuthorizationWriteWorkV1,
        &mut JsonWriteWorkV1,
    ) -> Result<StateRootV1, MemberRecordError>,
) -> StateRootV1 {
    f(
        source,
        &mut MemberRecordWriteContext {
            sink: pages,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: (|| false) as fn() -> bool,
        },
        &mut AuthorizationWriteWorkV1 {
            cells_remaining: 10000,
            max_pending_node_bytes: 1 << 20,
        },
        &mut JsonWriteWorkV1 {
            cells_remaining: 10000,
            json_nodes_remaining: 100,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap()
}
fn add_unknown_column(root: StateRootV1, pages: &mut Pages, memory: &MemoryBudget) -> StateRootV1 {
    use crate::incremental_record_tree::{RecordTreeLabelsV1, RecordTreeWritePolicyV1, append};
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: pages,
        memory,
        io: &mut value_io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let row = write_member_record_v1(
        AuthorizationBodyKindV1::Audit.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &"unknown.closed.column",
        MemberPayloadRefV1::Unit,
        &mut ctx,
    )
    .unwrap();
    append(
        root,
        &row.field_key,
        row.value_root,
        &mut ctx,
        RecordTreeWritePolicyV1 {
            max_pending_node_bytes: 1 << 20,
            labels: RecordTreeLabelsV1 {
                read: "test read",
                pending: "test pending",
                tree: "test tree",
                digest: "test digest",
                read_allowance: "test read allowance",
                admission: "test admission",
                bound: "test bound",
                producer_pool: "test pool",
                page: "test page",
                write_allowance: "test write",
            },
        },
    )
    .unwrap()
}
#[test]
fn audit_body_recovery_rebuilds_all_columns_and_keeps_output_charged() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let source = receipt();
    let expected = source.clone();
    let root = produce(
        &memory,
        &mut pages,
        &source,
        prepare_authorization_audit_body_v1,
    );
    drop(source);
    let roots = RootSetV1::empty("auth-cold".into(), [95; 32]).unwrap();
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    let recovered = recover_authorization_audit_body_v1(
        &root,
        &mut ctx,
        &mut scan(),
        &mut json_work(),
        1 << 20,
    )
    .unwrap();
    assert_eq!(recovered.value(), &expected);
    assert!(
        recover_authorization_invocation_body_v1(&root, &mut ctx, &mut scan(), 1 << 20).is_err()
    );
    assert!(matches!(
        recover_authorization_audit_body_v1(&root, &mut ctx, &mut scan(), &mut json_work(), 0),
        Err(ValueTreeError::ResourceLimited(_))
    ));
    drop(recovered);
    let mut short = json_work();
    short.json_nodes_remaining = 0;
    assert!(
        recover_authorization_audit_body_v1(&root, &mut ctx, &mut scan(), &mut short, 1 << 20)
            .is_err()
    );
    drop(ctx);
    let corrupt = add_unknown_column(root, &mut pages, &memory);
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    assert!(
        recover_authorization_audit_body_v1(
            &corrupt,
            &mut ctx,
            &mut scan(),
            &mut json_work(),
            1 << 20
        )
        .is_err()
    );
}
#[test]
fn invocation_body_recovery_preserves_actual_abi_variants_and_optional_presence() {
    for subject in [
        serde_json::json!({"kind":"agent","agent_id":"a","owner_binding":"o","generation":u64::MAX}),
        serde_json::json!({"kind":"module","module_id":"inner","module_version":"v","instance_id":"i"}),
        serde_json::json!({"kind":"system","system_id":"s","epoch":u64::MAX}),
    ] {
        let source:crate::CapabilityInvocationContext=serde_json::from_value(serde_json::json!({"grant_id":"g","subject":subject,"presenter":{"presenter_id":"p","presenter_kind":"provider","session_id":"","attestation_ref":null},"audience":{"world_id":"w","branch_id":"b","finality_epoch":0,"target_kind":"world","target_id":""},"catalog_snapshot_id":"c","module_id":"outer","module_version":"v","response_nonce":"n"})).unwrap();
        let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
        let mut pages = Pages::new(&memory);
        let root = produce(
            &memory,
            &mut pages,
            &source,
            prepare_authorization_invocation_body_v1,
        );
        let roots = RootSetV1::empty("auth-cold".into(), [95; 32]).unwrap();
        let mut value_io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
        let result =
            recover_authorization_invocation_body_v1(&root, &mut ctx, &mut scan(), 1 << 20)
                .unwrap();
        assert_eq!(result.value(), &source);
        assert!(
            recover_authorization_audit_body_v1(
                &root,
                &mut ctx,
                &mut scan(),
                &mut json_work(),
                1 << 20
            )
            .is_err()
        );
    }
}

#[path = "incremental_runtime_authorization_post_scan_cancel_tests.rs"]
mod post_scan_cancel;
