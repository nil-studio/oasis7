use super::*;
pub(super) fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1024,
        payload_bytes: 165,
        record_bytes: 8192,
    }
}
pub(super) type Rows = Vec<(Vec<u8>, [u8; 32])>;
pub(super) fn produce(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    source: &WorldState,
) -> Rows {
    let mut values = store.value_session(physical());
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(8);
    let mut remaining = 4096;
    let count = prepare_diplomacy_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut remaining,
        4 << 20,
        &mut |row| {
            assert_eq!(row.namespace, Ns::Business);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count.into_iter().sum::<u64>() as usize + 4, rows.len());
    rows.iter()
        .map(|row| (row.mutation.key.clone(), row.mutation.after.unwrap()))
        .collect()
}
pub(super) fn build(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    rows: &Rows,
) -> StateRootV1 {
    build_from(
        store,
        memory,
        &StateRootV1::empty(Ns::Business).unwrap(),
        &Vec::new(),
        rows,
    )
}
pub(super) fn build_from(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &StateRootV1,
    before: &Rows,
    rows: &Rows,
) -> StateRootV1 {
    let source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let work = memory
        .reserve(
            (rows.len() + before.len()) * (512 << 10) + (128 << 10),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let before: std::collections::BTreeMap<_, _> =
        before.iter().map(|(k, v)| (k.as_slice(), *v)).collect();
    let after: std::collections::BTreeMap<_, _> =
        rows.iter().map(|(k, v)| (k.as_slice(), *v)).collect();
    let keys: std::collections::BTreeSet<_> = before.keys().chain(after.keys()).copied().collect();
    let writes = keys
        .into_iter()
        .filter_map(|key| {
            let old = before.get(key).copied();
            let new = after.get(key).copied();
            (old != new).then(|| StateWriteV1 {
                key: key.to_vec(),
                before: old,
                after: new,
            })
        })
        .collect();
    let delta = work.retain(StateDeltaV1::build(parent.clone(), writes, &source, 4 << 20).unwrap());
    assert_eq!(delta.after.leaf_count, rows.len() as u64);
    store
        .value_session(physical())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.clone()
}
pub(super) fn raw(store: &IncrementalPageStore, memory: &MemoryBudget, id: &[u8; 32]) -> Vec<u8> {
    let values = store.value_session(physical());
    let mut quota = logical();
    let mut reader = ValueReadContext {
        source: &values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    let root = reader.load_root(id).unwrap();
    let mut output = Vec::with_capacity(8192);
    reader
        .read_range(&root, 0, root.0.logical_len, &mut output, || false)
        .unwrap();
    output
}
pub(super) fn replace(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    rows: &Rows,
    target: &WorldFieldKeyV1,
    mutate: impl FnOnce(&mut Vec<u8>),
) -> Rows {
    let mut rows = rows.clone();
    let row = rows
        .iter_mut()
        .find(|(key, _)| key == target.as_bytes())
        .unwrap();
    let mut bytes = raw(store, memory, &row.1);
    mutate(&mut bytes);
    row.1 = write_value(
        &mut bytes.as_slice(),
        bytes.len() as u64,
        &mut store.value_session(physical()),
        memory,
        &mut logical(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap();
    rows
}

pub(super) fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(error) => ValueTreeError::Io(error),
        _ => ValueTreeError::Invalid("state page failure"),
    }
}
