//! Both independent owners share Business/Authorization without key aliasing.
use super::*;
use oasis7_runtime::incremental_material_records as material;
pub(super) fn both_owners_qualify_from_the_same_roots() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let directory = Directory::new();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut source = source();
    source.material_profiles.insert(" original profile ".into(),serde_json::from_value(serde_json::json!({"kind":" original embedded kind ","tier":255,"category":"","stack_limit":i64::MIN,"transport_loss_class":"high","decay_bps_per_tick":i64::MAX,"default_priority":"urgent"})).unwrap());
    source.direct_material_transfer_receipts.insert(
        u64::MAX,
        oasis7_runtime::MaterialTransferReceiptV1 {
            transfer_id: 17,
            requester_agent_id: " raw requester ".into(),
            from_ledger: MaterialLedgerId::World,
            to_ledger: MaterialLedgerId::Factory(" raw factory ".into()),
            kind: " raw kind ".into(),
            amount: i64::MIN,
            distance_km: i64::MAX,
            priority: MaterialTransitPriority::Urgent,
            route_id: Some(String::new()),
        },
    );
    let mut roots = seed(&store, &memory, &source);
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows = Vec::with_capacity(16);
    let mut values = store.value_session(physical_scope());
    let count = material::prepare_material_records_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory: &memory,
            io: &mut logical(),
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
        &mut 1000,
        &mut material::MaterialWorkBudgetV1 {
            cells_remaining: 1000,
            max_pending_node_bytes: 2 << 20,
            scan: work().scan,
        },
        &mut |row| {
            assert!(rows.len() < 16);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, 2);
    assert_eq!(rows.len(), 6);
    for namespace in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let index = roots
            .state
            .iter()
            .position(|root| root.namespace == namespace)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(roots.state[index].clone(), writes, &state, 8 << 20).unwrap(),
        );
        values.put_state_pages(&delta, &memory).unwrap();
        roots.state[index] = delta.after.clone();
    }
    assert_eq!(
        qualify(&store, &memory, &roots, MemoryClass::Ordinary).record_counts(),
        [1, 2]
    );
    let state = store
        .state_session(physical_scope(), MemoryClass::Ordinary)
        .unwrap();
    let source = store.value_session(physical_scope());
    let root = roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Business)
        .unwrap();
    let qualification = material::qualify_material_family_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root,
            state: &state,
            values: ValueReadContext {
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("combined owner state"))
            },
            cancelled: || false,
        },
        &mut work().scan,
        &mut work().scan,
        &mut 1000,
        4096,
    )
    .unwrap();
    assert_eq!(qualification.record_counts(), [1, 1]);
    qualification
        .validate_binding(&roots.digest().unwrap())
        .unwrap();
}
