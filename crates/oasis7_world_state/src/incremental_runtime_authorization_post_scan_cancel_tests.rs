//! Cancellation becomes visible only after all selected JSON reads finish.
use super::*;
fn replace_last_set_codec(
    root: StateRootV1,
    pages: &mut Pages,
    memory: &MemoryBudget,
) -> StateRootV1 {
    let row = {
        let mut value_io = io();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        write_member_record_v1(
            AuthorizationBodyKindV1::Audit.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &("committed_effect_receipt_ids", ("member", "effect-031")),
            MemberPayloadRefV1::U64(7),
            &mut MemberRecordWriteContext {
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
            },
        )
        .unwrap()
    };
    let before = root.get(row.field_key.as_bytes(), pages).unwrap().unwrap();
    let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            root,
            vec![StateWriteV1 {
                key: row.field_key.as_bytes().to_vec(),
                before: Some(before),
                after: Some(row.value_root),
            }],
            pages,
            8 << 20,
        )
        .unwrap(),
    );
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        pages
            .put_node(
                id,
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    delta.after.clone()
}
#[test]
fn audit_post_scan_cancellation_precedes_late_bad_set_member() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut source = receipt();
    source.committed_effect_receipt_ids = (0..32).map(|i| format!("effect-{i:03}")).collect();
    let root = produce(
        &memory,
        &mut pages,
        &source,
        prepare_authorization_audit_body_v1,
    );
    drop(source);
    let root = replace_last_set_codec(root, &mut pages, &memory);
    let roots = RootSetV1::empty("post-scan-cancel".into(), [100; 32]).unwrap();
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    assert!(matches!(
        recover_authorization_audit_body_v1(
            &root,
            &mut ctx,
            &mut scan(),
            &mut json_work(),
            1 << 20
        ),
        Err(ValueTreeError::Invalid(_))
    ));
    let max_heap = 1 << 20;
    let json_bytes = probe::retained_json_bytes(&root, &pages, &roots, &memory, max_heap);
    let target = memory.used() + max_heap + json_bytes;
    // The column workspace retains its maximum until finish; the JSONs
    // retain measured used credit. Additional scan/materialization stacks
    // distinguish those phases from the post-JSON set reconstruction.
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut seen = false;
    let result = {
        let mut ctx = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Authorization),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || None,
            cancelled: || {
                let yes = memory.used() == target;
                seen |= yes;
                yes
            },
        };
        recover_authorization_audit_body_v1(
            &root,
            &mut ctx,
            &mut scan(),
            &mut json_work(),
            max_heap,
        )
    };
    assert!(
        seen,
        "fixture reached the post-scan output-retention boundary"
    );
    assert!(
        matches!(result, Err(ValueTreeError::Cancelled)),
        "cancellation must stop before visiting the later invalid set member"
    );
}

#[path = "incremental_runtime_authorization_post_scan_probe.rs"]
mod probe;
