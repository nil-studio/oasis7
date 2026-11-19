use super::*;
#[test]
fn authorization_full_family_streams_large_original_text_json_and_set_ids() {
    let memory = MemoryBudget::new(192 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let mut audit = receipt();
    audit.receipt_id = "receipt".repeat(16384);
    audit.subject = serde_json::Value::String("json".repeat(65536));
    audit
        .committed_effect_receipt_ids
        .insert("世".repeat(32768));
    let invocation:crate::CapabilityInvocationContext=serde_json::from_value(serde_json::json!({
        "grant_id":"grant","subject":{"kind":"agent","agent_id":"a","owner_binding":"binding".repeat(16384),"generation":u64::MAX},
        "presenter":{"presenter_id":"p","presenter_kind":"provider","session_id":"","attestation_ref":null},
        "audience":{"world_id":"w","branch_id":"b","finality_epoch":0,"target_kind":"world","target_id":""},
        "catalog_snapshot_id":"c","module_id":"outer","module_version":"v","response_nonce":"n"
    })).unwrap();
    let audits = BTreeMap::from([("outer".into(), audit)]);
    let invocations = BTreeMap::from([("invocation".into(), invocation)]);
    let mut rows = Vec::new();
    prepare_authorization_record_families_genesis_v1(
        &audits,
        &invocations,
        &mut MemberRecordWriteContext {
            sink: &mut pages,
            memory: &memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 32 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 512 << 10,
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
    drop(audits);
    drop(invocations);
    let mut roots = RootSetV1::empty("auth-stream".into(), [66; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut read_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut read_io, &mut keys);
    ctx.semantic_bytes = 512 << 10;
    ctx.record_bytes = 2 << 20;
    let proof = qualify_authorization_record_families_v1(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut jr(),
        1,
    )
    .unwrap();
    assert_eq!(proof.audit_receipts(), 1);
    assert_eq!(proof.invocation_contexts(), 1);
    let (key, _) = ctx
        .key(descriptor(0), WorldFieldRecordKindV1::Entry, &"outer")
        .unwrap();
    let id = ctx.lookup(&key).unwrap().unwrap();
    let root = child(AuthorizationBodyKindV1::Audit, &key, &id, &mut ctx).unwrap();
    assert!(matches!(
        recover_authorization_audit_body_v1(&root, &mut ctx, &mut scan(), &mut jr(), 1),
        Err(ValueTreeError::ResourceLimited(_))
    ));
}

/// Component inheritance only. Removing a physical page deliberately prevents
/// WorldReady; the owner must separately retain its complete physical lease.
#[test]
fn audit_changed_scalar_skips_unchanged_effect_set_value_objects() {
    use super::super::{delta as proof_delta, stream as proof_stream};
    let memory = MemoryBudget::new(192 << 20, 4 << 20).unwrap();
    let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::new(&memory);
    let mut old = receipt();
    old.committed_effect_receipt_ids = (0..64).map(|i| format!("effect-{i:04}")).collect();
    let mut new = old.clone();
    new.decision = "committed".into();
    let mut prepare = |value: &crate::CapabilityAuthorizationAuditReceipt| {
        prepare_authorization_audit_body_v1(
            value,
            &mut MemberRecordWriteContext {
                sink: &mut pages,
                memory: &memory,
                io: &mut io(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
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
        )
        .unwrap()
    };
    let before = prepare(&old);
    let after = prepare(&new);
    drop(prepare);
    drop(old);
    drop(new);
    let roots = RootSetV1::empty("auth-delta-component".into(), [67; 32]).unwrap();
    let mut value_io = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut ctx = context(&pages, &roots, &memory, &mut value_io, &mut keys);
    ctx.semantic_bytes = 4096;
    ctx.record_bytes = 2 << 20;
    proof_stream::qualify(
        AuthorizationBodyKindV1::Audit,
        &before,
        &mut ctx,
        &mut scan(),
        &mut jr(),
    )
    .unwrap();
    let mut records = ctx.records();
    let (key, _) = records
        .key(
            AuthorizationBodyKindV1::Audit.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &("committed_effect_receipt_ids", ("member", "effect-0000")),
        )
        .unwrap();
    let id =
        crate::incremental_record_tree::lookup(&before, &key, &mut records, proof_stream::LABELS)
            .unwrap()
            .unwrap();
    drop(records);
    assert!(pages.nodes.borrow_mut().remove(&id).is_some());
    let mut limited = jr();
    limited.cells_remaining = 512;
    proof_delta::advance(
        AuthorizationBodyKindV1::Audit,
        &before,
        &after,
        &mut ctx,
        &mut limited,
    )
    .unwrap();
    assert!(
        limited.cells_remaining > 0,
        "unchanged set cardinality is not delta work"
    );
}
