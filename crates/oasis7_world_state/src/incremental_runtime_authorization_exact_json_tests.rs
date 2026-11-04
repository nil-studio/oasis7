//! Regression for original f64 bit identity at typed exact-before boundaries.
use super::*;
#[test]
fn audit_positive_zero_cannot_match_persisted_negative_zero() {
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let mut original = receipt();
    original.subject = serde_json::json!({"zero":-0.0});
    let mut expected = original.clone();
    expected.subject = serde_json::json!({"zero":0.0});
    assert_eq!(original, expected);
    let mut audits = BTreeMap::new();
    audits.insert("key".into(), original);
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
    let mut roots = RootSetV1::empty("float-identity".into(), [98; 32]).unwrap();
    install(&memory, &mut pages, &mut roots, &rows);
    let mut reads = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut read = context(&pages, &roots, &memory, &mut reads, &mut keys);
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
    let mut output = Pages::new(&memory);
    let mut write_io = io();
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut output,
        memory: &memory,
        io: &mut write_io,
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
            expected: Some(&expected),
            after: Some(&expected),
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
    assert!(
        result.is_err(),
        "different original float bits must reject exact-before, including NoOp"
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
#[test]
fn grant_positive_zero_cannot_match_persisted_negative_zero() {
    use crate::{
        incremental_runtime_authorization_grants::*,
        incremental_runtime_authorization_grants_family::*,
    };
    let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
    let mut pages = Pages::new(&memory);
    let original = serde_json::json!({"zero":-0.0});
    let expected = serde_json::json!({"zero":0.0});
    assert_eq!(original, expected);
    let mut grants = BTreeMap::new();
    grants.insert("key".into(), original);
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
    let mut roots = RootSetV1::empty("float-grant".into(), [98; 32]).unwrap();
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
    let mut output = Pages::new(&memory);
    let mut write_io = io();
    let mut wk = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut output,
        memory: &memory,
        io: &mut write_io,
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
            expected: Some(&expected),
            after: Some(&expected),
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
    assert!(
        result.is_err(),
        "different original grant float bits must reject exact-before"
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
