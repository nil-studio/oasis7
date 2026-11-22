use super::*;
fn key(name: &'static str, column: Option<&str>) -> (WorldFieldKeyV1, Vec<u8>) {
    let mut semantic = b"oasis7.iws.semantic-key.v1\0".to_vec();
    if let Some(column) = column {
        semantic.extend_from_slice(&[5, 0, 1]);
        semantic.extend_from_slice(&(column.len() as u64).to_be_bytes());
        semantic.extend_from_slice(column.as_bytes());
    } else {
        semantic.push(0);
    }
    let descriptor = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name,
        shape: FieldShape::Value,
        destination: FieldDestination::State(StateNamespaceV1::Scheduler),
    };
    (
        WorldFieldKeyV1::new(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            Some(*blake3::hash(&semantic).as_bytes()),
        )
        .unwrap(),
        semantic,
    )
}
#[test]
fn original_literal_field_names_semantic_preimages_and_payload_bytes() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("literal"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let roots = seed(&store, &memory);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let top = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Scheduler)
        .unwrap();
    let mut ctx = read_ctx!(
        top,
        &state,
        &values,
        &memory,
        MemoryClass::Ordinary,
        &mut io,
        &mut keys
    );
    let family = qualify_runtime_scheduler_controls_v1(
        &roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut 1000,
        32,
    )
    .unwrap();
    let _scratch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
    let (outer, semantic) = key("module_tick_routing_metrics", None);
    let value = top.get(outer.as_bytes(), &state).unwrap().unwrap();
    let record = ctx.values.load_root(&value).unwrap();
    let mut frame = Vec::with_capacity(4096);
    ctx.values
        .read_range(&record, 0, record.0.logical_len, &mut frame, || false)
        .unwrap();
    let offset = FIELD_MEMBER_HEADER_BYTES + outer.as_bytes().len() + semantic.len();
    assert_eq!(frame[FIELD_MEMBER_DOMAIN.len() + 2], 10);
    let edge =
        oasis7_formal_protocol::incremental_record_root::RecordRootRefV1::from_canonical_bytes(
            &frame[offset..],
        )
        .unwrap();
    let mut schema = blake3::Hasher::new();
    schema.update(b"oasis7.iws.runtime-routing-controls.v1");
    schema.update(&[0]);
    edge.validate_binding(
        schema.finalize().as_bytes(),
        blake3::hash(&semantic).as_bytes(),
        StateNamespaceV1::Scheduler,
    )
    .unwrap();
    for (name, column, codec, data) in [
        ("scheduler_cursor", None, 5, vec![]),
        (
            "runtime_memory_limits.max_pending_actions",
            None,
            2,
            (usize::MAX as u64).to_be_bytes().to_vec(),
        ),
        (
            "runtime_memory_limits.max_pending_effects",
            None,
            2,
            0u64.to_be_bytes().to_vec(),
        ),
        (
            "runtime_memory_limits.max_inflight_effects",
            None,
            2,
            17u64.to_be_bytes().to_vec(),
        ),
        (
            "runtime_backpressure_stats.pending_actions_evicted",
            None,
            2,
            u64::MAX.to_be_bytes().to_vec(),
        ),
        (
            "runtime_backpressure_stats.pending_effects_evicted",
            None,
            2,
            0u64.to_be_bytes().to_vec(),
        ),
        (
            "runtime_backpressure_stats.inflight_effects_evicted",
            None,
            2,
            19u64.to_be_bytes().to_vec(),
        ),
        (
            "module_tick_routing_metrics",
            Some("last_due_count"),
            2,
            (usize::MAX as u64).to_be_bytes().to_vec(),
        ),
        (
            "module_tick_routing_metrics",
            Some("last_invoked_count"),
            2,
            0u64.to_be_bytes().to_vec(),
        ),
        (
            "module_tick_routing_metrics",
            Some("last_missing_invocation_count"),
            2,
            23u64.to_be_bytes().to_vec(),
        ),
        (
            "module_tick_routing_metrics",
            Some("missing_invocation_count"),
            2,
            u64::MAX.to_be_bytes().to_vec(),
        ),
        (
            "module_tick_routing_metrics",
            Some("oldest_overdue_ticks"),
            2,
            0u64.to_be_bytes().to_vec(),
        ),
        (
            "module_tick_routing_metrics",
            Some("routing_count"),
            2,
            29u64.to_be_bytes().to_vec(),
        ),
    ] {
        let (key, semantic) = key(name, column);
        let root = if column.is_some() {
            family.routing_root()
        } else {
            top
        };
        let id = root.get(key.as_bytes(), &state).unwrap().unwrap();
        let value = ctx.values.load_root(&id).unwrap();
        frame.clear();
        ctx.values
            .read_range(&value, 0, value.0.logical_len, &mut frame, || false)
            .unwrap();
        let offset = FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len() + semantic.len();
        assert_eq!(
            &frame[FIELD_MEMBER_HEADER_BYTES..FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()],
            key.as_bytes()
        );
        assert_eq!(
            &frame[FIELD_MEMBER_HEADER_BYTES + key.as_bytes().len()..offset],
            semantic
        );
        assert_eq!(frame[FIELD_MEMBER_DOMAIN.len() + 2], codec);
        assert_eq!(&frame[offset..], data);
    }
}
