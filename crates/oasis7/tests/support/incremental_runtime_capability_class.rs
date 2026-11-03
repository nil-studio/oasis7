use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};

#[test]
fn recovery_qualification_cannot_cross_into_ordinary_reader() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory);
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Authorization)
        .unwrap();
    let recovery = store
        .state_session(physical(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical());
    let key = RuntimeCapabilityRecordKeyV1 {
        kind: RuntimeCapabilityRecordKindV1::Nonce,
        id: "nonce-key",
    };
    let (family, record) = {
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            root,
            &recovery,
            &values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_capability_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();
        let record = open_qualified_runtime_capability_record_v1(key, &roots, &family, &mut ctx)
            .unwrap()
            .unwrap();
        (family, record)
    };
    let ordinary = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = read_ctx!(
        root,
        &ordinary,
        &values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let ordinary_family = qualify_runtime_capability_family_v1(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 1000,
        165,
    )
    .unwrap();
    let key = RuntimeCapabilityRecordKeyV1 {
        kind: RuntimeCapabilityRecordKindV1::Nonce,
        id: "nonce-key",
    };
    assert!(
        open_qualified_runtime_capability_record_v1(key, &roots, &ordinary_family, &mut ctx)
            .unwrap()
            .is_some()
    );
    let result = open_qualified_runtime_capability_record_v1(key, &roots, &family, &mut ctx);
    assert!(
        matches!(result, Err(ValueTreeError::Invalid(_))),
        "Recovery qualifier crossed class: {result:?}"
    );
    assert_eq!(family.allocation_class(), MemoryClass::Recovery);
    assert_eq!(record.allocation_class(), MemoryClass::Recovery);
    let result = read_qualified_runtime_capability_cell_v1(
        RuntimeCapabilityCellRequestV1 {
            key,
            column: RuntimeCapabilityColumnV1::RequestHash,
            qualification: &record,
        },
        &roots,
        &mut ctx,
        165,
        &mut std::io::sink(),
    );
    assert!(matches!(result, Err(ValueTreeError::Invalid(_))));
    let mut wrong = roots.clone();
    wrong.height += 1;
    let foreign = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    assert!(matches!(
        readmit_runtime_capability_family_ordinary_v1(&family, &wrong, &memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        readmit_runtime_capability_record_ordinary_v1(&record, &wrong, &memory, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        readmit_runtime_capability_family_ordinary_v1(&family, &roots, &foreign, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    assert!(matches!(
        readmit_runtime_capability_record_ordinary_v1(&record, &roots, &foreign, || false),
        Err(ValueTreeError::Invalid(_))
    ));
    let polls = std::cell::Cell::new(0);
    let before = memory.used();
    assert!(matches!(
        readmit_runtime_capability_record_ordinary_v1(&record, &roots, &memory, || {
            polls.set(polls.get() + 1);
            polls.get() == 2
        }),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(polls.get(), 2);
    assert_eq!(memory.used(), before);
    let hold = memory
        .reserve((56 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(matches!(
        readmit_runtime_capability_family_ordinary_v1(&family, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    assert!(matches!(
        readmit_runtime_capability_record_ordinary_v1(&record, &roots, &memory, || false),
        Err(ValueTreeError::Memory(_))
    ));
    drop(hold);
    let admitted_family =
        readmit_runtime_capability_family_ordinary_v1(&family, &roots, &memory, || false).unwrap();
    let admitted =
        readmit_runtime_capability_record_ordinary_v1(&record, &roots, &memory, || false).unwrap();
    assert_eq!(admitted_family.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(admitted_family.record_counts(), family.record_counts());
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    assert_eq!(admitted.root(), record.root());
    assert!(
        open_qualified_runtime_capability_record_v1(key, &roots, &admitted_family, &mut ctx)
            .unwrap()
            .is_some()
    );
    let mut text = Vec::new();
    assert!(matches!(
        read_qualified_runtime_capability_cell_v1(
            RuntimeCapabilityCellRequestV1 {
                key,
                column: RuntimeCapabilityColumnV1::RequestHash,
                qualification: &admitted
            },
            &roots,
            &mut ctx,
            165,
            &mut text
        )
        .unwrap(),
        RuntimeCapabilityValueV1::Text { utf8_bytes: 16 }
    ));
    assert_eq!(text, b"original request");
    // Mutate a different record first: the rejected nonce root is still current,
    // so this exercises class rejection rather than a stale-record mismatch.
    let budget_key = RuntimeCapabilityRecordKeyV1 {
        kind: RuntimeCapabilityRecordKindV1::Budget,
        id: "agent-key",
    };
    let budget =
        open_qualified_runtime_capability_record_v1(budget_key, &roots, &ordinary_family, &mut ctx)
            .unwrap()
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
        &ordinary,
        &memory,
        TransactionLimits {
            max_reads: 160,
            max_writes: 16,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let mut overlay_io = logical();
    let mut overlay_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut overlay_io,
        },
        key_work: &mut overlay_keys,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || ordinary.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut writer = store.value_session(physical());
    let mut write_io = logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: limits(),
        cancelled: || false,
    };
    let guard = overlay.transaction.guard();
    let changed = change_qualified_runtime_capability_cell_v1(
        RuntimeCapabilityPointChangeV1 {
            request: RuntimeCapabilityPointRequestV1 {
                key: budget_key,
                column: RuntimeCapabilityColumnV1::RemainingUnits,
                qualification: &budget,
            },
            expected: RuntimeCapabilityValueRefV1::Signed(i64::MIN),
            after: RuntimeCapabilityValueRefV1::Signed(17),
            max_current_payload_bytes: 165,
            max_pending_node_bytes: 4 << 20,
        },
        &mut overlay,
        &mut write,
        &guard,
        &live,
    )
    .unwrap();
    assert!(changed.is_some());
    let guard = overlay.transaction.guard();
    let result = read_qualified_runtime_capability_overlay_v1(
        RuntimeCapabilityPointRequestV1 {
            key,
            column: RuntimeCapabilityColumnV1::RequestHash,
            qualification: &record,
        },
        &mut overlay,
        165,
        &mut std::io::sink(),
        &guard,
        &live,
    );
    assert!(
        matches!(
            result,
            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
        ),
        "{result:?}"
    );
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
    let before = memory.used();
    let old_fee = record.reserved_bytes();
    let new_fee = admitted.reserved_bytes();
    drop(record);
    assert_eq!(memory.used() + old_fee, before);
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    drop(admitted);
    assert_eq!(memory.used() + old_fee + new_fee, before);
}
