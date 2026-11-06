use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
};

use crate::{
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_metadata_v1,
    },
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer},
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::sync::Arc;
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 200_000,
        read_bytes_remaining: 64 << 20,
        write_bytes_remaining: 64 << 20,
    }
}
fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 200_000,
        leaves_remaining: 10_000,
        read_bytes_remaining: 64 << 20,
    }
}
fn seed(memory: &MemoryBudget, text: &str) -> (Pages, RootSetV1, [u8; 32]) {
    let pages = Pages::new(memory);
    let _phase = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let mut out = Writer(&pages);
    let mut vi = io();
    let mut kw = keys();
    let meta = write_field_metadata_v1(
        authorization_root_descriptor_v1(),
        &mut PresenceRecordWriteContext {
            sink: &mut out,
            memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 1024,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let row = prepare_authorization_root_value_v1(
        text,
        &mut MemberRecordWriteContext {
            sink: &mut out,
            memory,
            io: &mut vi,
            key_work: &mut kw,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 2 << 20,
                record_bytes: 3 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes = vec![
        StateWriteV1 {
            key: meta.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(meta.value_root),
        },
        StateWriteV1 {
            key: row.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(row.value_root),
        },
    ];
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta = StateDeltaV1::build(
        StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
        writes,
        &pages,
        2 << 20,
    )
    .unwrap();
    for node in &delta.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        out.put_node(
            id,
            memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(node.bytes.clone()),
        )
        .unwrap();
    }
    let mut roots = RootSetV1::empty("original-authorization-string".into(), [9; 32]).unwrap();
    roots.state[3] = delta.after;
    (pages, roots, row.value_root)
}
use crate::incremental_typed_record_test_support::ClassedPages as Classed;
fn qualify(
    pages: &Pages,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> ReservedValue<RuntimeAuthorizationRootQualificationV1> {
    let source = Classed::new(pages, memory, class);
    let mut vi = io();
    let mut kw = keys();
    qualify_runtime_authorization_root_v1(
        roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[3],
            state: &source,
            values: ValueReadContext {
                source: &source,
                memory,
                class,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut scan(),
        2 << 20,
    )
    .unwrap()
}
struct Unreadable;
impl ValueNodeReader for Unreadable {
    fn read_node(
        &self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
        _: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        panic!("unchanged authorization Text was opened")
    }
}
fn parent(roots: &RootSetV1) -> ExecutionParent {
    ExecutionParent {
        roots: roots.clone(),
        commit: [2; 32],
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 4,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 64,
        max_writes: 8,
        max_facts: 4,
        max_pending_node_bytes: 2 << 20,
    }
}
#[test]
fn authorization_root_streams_non_hash_utf8_and_requires_real_ordinary_readmission() {
    let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
    let source_charge = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let text = "世界🙂".repeat(65_536);
    let expected = text.len() as u64;
    let (pages, roots, _) = seed(&memory, &text);
    drop(text);
    drop(source_charge);
    let recovered = qualify(&pages, &roots, &memory, MemoryClass::Recovery);
    assert_eq!(recovered.utf8_bytes(), expected);
    let p = parent(&roots);
    let live = p.fence().unwrap();
    let tx = ExecutionTransaction::begin(p.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &p,
        operation: [4; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let mut vi = io();
    let mut kw = keys();
    let mut ctx = WorldFieldReadContextV1 {
        root: &sealed.record.after.state[3],
        state: &pages,
        values: ValueReadContext {
            source: &Unreadable,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut vi,
        },
        key_work: &mut kw,
        semantic_bytes: 1024,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    assert!(matches!(
        accept_runtime_authorization_root_transition_v1(
            &recovered,
            &verifier,
            &sealed.record,
            &mut ctx,
            0
        ),
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
    ));
    let before = memory.used();
    let admitted =
        readmit_runtime_authorization_root_ordinary_v1(&recovered, &roots, &memory, || false)
            .unwrap();
    assert_eq!(memory.used(), before + admitted.reserved_bytes());
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    let next = accept_runtime_authorization_root_transition_v1(
        &admitted,
        &verifier,
        &sealed.record,
        &mut ctx,
        0,
    )
    .unwrap();
    assert_eq!(next.utf8_bytes(), expected);
    next.validate_binding(&sealed.record.after).unwrap();
    assert!(
        readmit_runtime_authorization_root_ordinary_v1(&admitted, &roots, &memory, || false)
            .is_err()
    );
    assert!(matches!(
        readmit_runtime_authorization_root_ordinary_v1(&recovered, &roots, &memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    let foreign = MemoryBudget::new(1 << 20, 1 << 18).unwrap();
    assert!(
        readmit_runtime_authorization_root_ordinary_v1(&recovered, &roots, &foreign, || false)
            .is_err()
    );
    let mut other = roots.clone();
    other.height += 1;
    assert!(
        readmit_runtime_authorization_root_ordinary_v1(&recovered, &other, &memory, || false)
            .is_err()
    );
    drop(next);
    drop(admitted);
    assert_eq!(memory.used(), before);
}
fn changed(mode: u8) {
    let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
    let (pages, roots, _) = seed(&memory, "");
    let family = qualify(&pages, &roots, &memory, MemoryClass::Ordinary);
    assert_eq!(family.utf8_bytes(), 0);
    let p = parent(&roots);
    let live = p.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(p.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let _phase = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let mut vi = io();
    let mut kw = keys();
    let mut out = Writer(&pages);
    let mut writer = MemberRecordWriteContext {
        sink: &mut out,
        memory: &memory,
        io: &mut vi,
        key_work: &mut kw,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1024,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let field = authorization_root_descriptor_v1();
    let unit = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(),
        &memory,
        MemoryClass::Ordinary,
        writer.key_work,
        &mut writer.cancelled,
    )
    .unwrap();
    let key =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(unit.digest)).unwrap();
    match mode {
        0 => {
            let row = prepare_authorization_root_value_v1("not a digest 🙂", &mut writer).unwrap();
            tx.write(
                StateNamespaceV1::Authorization,
                row.field_key.as_bytes(),
                Some(row.value_root),
            )
            .unwrap();
        }
        1 => {
            let row = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(),
                MemberPayloadRefV1::Bool(true),
                &mut writer,
            )
            .unwrap();
            tx.write(
                StateNamespaceV1::Authorization,
                row.field_key.as_bytes(),
                Some(row.value_root),
            )
            .unwrap();
        }
        2 => {
            tx.write(StateNamespaceV1::Authorization, key.as_bytes(), None)
                .unwrap();
        }
        3 => {
            let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None).unwrap();
            tx.write(StateNamespaceV1::Authorization, meta.as_bytes(), None)
                .unwrap();
        }
        4 => {
            let row = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                "wrong original key",
                MemberPayloadRefV1::Text("text"),
                &mut writer,
            )
            .unwrap();
            tx.write(
                StateNamespaceV1::Authorization,
                row.field_key.as_bytes(),
                Some(row.value_root),
            )
            .unwrap();
        }
        _ => unreachable!(),
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &p,
        operation: [4; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let verified = verifier
        .verify(&sealed.record)
        .expect("the malformed owner data is generic-valid");
    let mut vi = io();
    let mut kw = keys();
    let result = accept_runtime_authorization_root_transition_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[3],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        2 << 20,
    );
    if mode == 0 {
        let accepted = result.unwrap();
        assert_eq!(accepted.utf8_bytes(), "not a digest 🙂".len() as u64);
        accepted.validate_binding(&sealed.record.after).unwrap();
    } else {
        assert!(matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
        ));
    }
    for node in &verified.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        out.put_node(
            id,
            memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap()
                .retain(node.bytes.clone()),
        )
        .unwrap();
    }
    if mode != 0 {
        let mut vi = io();
        let mut kw = keys();
        let result = qualify_runtime_authorization_root_v1(
            &sealed.record.after,
            &mut WorldFieldReadContextV1 {
                root: &sealed.record.after.state[3],
                state: &pages,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut vi,
                },
                key_work: &mut kw,
                semantic_bytes: 1024,
                record_bytes: 3 << 20,
                take_state_error: || None,
                cancelled: || false,
            },
            &mut scan(),
            2 << 20,
        );
        assert!(matches!(result, Err(ValueTreeError::Invalid(_))));
    }
}
#[test]
fn authorization_root_changed_text_preserves_original_string_semantics() {
    changed(0)
}
#[test]
fn authorization_root_receiver_and_cold_reject_wrong_codec_deletion_and_extra_key() {
    for mode in 1..5 {
        changed(mode)
    }
}

#[test]
fn authorization_root_preserves_finite_scan_readmission_cancel_and_original_error() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (pages, roots, _) = seed(&memory, "original UTF8 🙂");
    let recovered = qualify(&pages, &roots, &memory, MemoryClass::Recovery);
    let before = memory.used();
    let fill = memory
        .reserve((32 << 20) - (4 << 20) - before - 512, MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_runtime_authorization_root_ordinary_v1(&recovered, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(fill);
    assert_eq!(memory.used(), before);
    let family =
        readmit_runtime_authorization_root_ordinary_v1(&recovered, &roots, &memory, || false)
            .unwrap();
    let mut vi = io();
    let mut kw = keys();
    let result = qualify_runtime_authorization_root_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: &roots.state[3],
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 0,
            leaves_remaining: 0,
            read_bytes_remaining: 0,
        },
        2 << 20,
    );
    assert!(matches!(result, Err(ValueTreeError::ResourceLimited(_))));
    let p = parent(&roots);
    let live = p.fence().unwrap();
    let tx = ExecutionTransaction::begin(p.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &p,
        operation: [4; 32],
        source: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let mut vi = io();
    let mut kw = keys();
    let result = accept_runtime_authorization_root_transition_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[3],
            state: &pages,
            values: ValueReadContext {
                source: &Unreadable,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 3 << 20,
            take_state_error: || Some(ValueTreeError::Invalid("original state failure")),
            cancelled: || false,
        },
        0,
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
            "original state failure"
        )))
    ));
    let mut vi = io();
    let mut kw = keys();
    let result = accept_runtime_authorization_root_transition_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[3],
            state: &pages,
            values: ValueReadContext {
                source: &Unreadable,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut vi,
            },
            key_work: &mut kw,
            semantic_bytes: 1024,
            record_bytes: 3 << 20,
            take_state_error: || None,
            cancelled: || true,
        },
        0,
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
    ));
}
