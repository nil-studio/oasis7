use super::*;
struct InterruptedOnce {
    bytes: Vec<u8>,
    calls: usize,
}
impl io::Write for InterruptedOnce {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls == 1 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn public_ledger_reader_retries_interrupted_id_without_replaying_consumed_header() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let dir = Directory::new();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (source, _, _) = source();
    let roots = seed(&store, &memory, &source, None);
    let oasis7_runtime::types::MaterialLedgerId::Agent(expected) =
        &source.factories["outer factory"].input_ledger
    else {
        panic!("agent ID")
    };
    for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
        let _output = memory.reserve(4096, class).unwrap();
        let state = store.state_session(quota(), class).unwrap();
        let values = store.value_session(quota());
        let mut output = InterruptedOnce {
            bytes: Vec::with_capacity(256),
            calls: 0,
        };
        let result = read_factory_cell_v1(
            FactoryRecordKeyV1::Factory("outer factory"),
            FactoryCellV1::Column(FactoryColumnV1::FactoryInputLedger),
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 4096,
                },
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            1024,
            &mut output,
        );
        eprintln!(
            "ledger interrupted class={class:?} result={result:?} calls={} exact_id={}",
            output.calls,
            output.bytes == expected.as_bytes()
        );
        let FactoryCellReadV1::Value(FactoryValueV1::Ledger(info)) = result.unwrap() else {
            panic!("successful typed ledger read required")
        };
        assert_eq!(info.kind as u8, 1);
        assert_eq!(info.id_bytes, expected.len() as u64);
        assert!(output.calls >= 2, "real Interrupted retry");
        assert_eq!(
            output.bytes,
            expected.as_bytes(),
            "consumed header must not be replayed as original ID output"
        );
    }
}
#[test]
fn missing_record_marker_with_real_children_cannot_authorize_absent_creation() {
    use oasis7_formal_protocol::incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1};
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let directory = Directory::new();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (source, _, _) = source();
    let original = seed(&store, &memory, &source, None);
    let state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
    let mut values = store.value_session(quota());
    let descriptor = field("pending_factory_builds", StateNamespaceV1::Scheduler);
    let mut semantic_bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    semantic_bytes.extend_from_slice(&unsigned_token(u64::MAX));
    let semantic_digest = *blake3::hash(&semantic_bytes).as_bytes();
    let outer_key = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(semantic_digest),
    )
    .unwrap();
    let before = original.state[2]
        .get(outer_key.as_bytes(), &state)
        .unwrap()
        .unwrap();
    let mut bytes = [0u8; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &before,
        &outer_key,
        &mut ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut logical(),
        },
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: semantic_bytes.len() as u64,
            limits: limits(),
        },
        &mut io::sink(),
        &mut &mut bytes[..],
        || false,
    )
    .unwrap();
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).unwrap();
    let child = reference
        .validate_binding(
            &FACTORY_BUILD_SCHEMA_V1,
            &semantic_digest,
            StateNamespaceV1::Scheduler,
        )
        .unwrap();
    let marker = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Unit,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: limits(),
            cancelled: || false,
        },
    )
    .unwrap();
    let marker_before = child
        .get(marker.field_key.as_bytes(), &state)
        .unwrap()
        .unwrap();
    let admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let broken_child = admission.retain(
        StateDeltaV1::build(
            child.clone(),
            vec![StateWriteV1 {
                key: marker.field_key.as_bytes().to_vec(),
                before: Some(marker_before),
                after: None,
            }],
            &state,
            1 << 20,
        )
        .unwrap(),
    );
    values.put_state_pages(&broken_child, &memory).unwrap();
    let broken_reference = RecordRootRefV1::new(
        FACTORY_BUILD_SCHEMA_V1,
        semantic_digest,
        broken_child.after.clone(),
    )
    .unwrap();
    let row = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &u64::MAX,
        MemberPayloadRefV1::RecordRoot(&broken_reference),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: limits(),
            cancelled: || false,
        },
    )
    .unwrap();
    let admission = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let delta = admission.retain(
        StateDeltaV1::build(
            original.state[2].clone(),
            vec![StateWriteV1 {
                key: outer_key.as_bytes().to_vec(),
                before: Some(before),
                after: Some(row.value_root),
            }],
            &state,
            1 << 20,
        )
        .unwrap(),
    );
    values.put_state_pages(&delta, &memory).unwrap();
    let mut malformed = original.clone();
    malformed.state[2] = delta.after.clone();
    // The original member remains actually reachable below a valid typed ref;
    // only the required inner record marker was removed.
    let token = pair_token(
        &pair_token(
            &pair_token(&[0], &text_token("construction_power_obligation")),
            &text_token("material_balances_before"),
        ),
        &text_token("ore"),
    );
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend_from_slice(&token);
    let member = WorldFieldKeyV1::new(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        Some(*blake3::hash(&bytes).as_bytes()),
    )
    .unwrap();
    assert!(
        broken_child
            .after
            .get(member.as_bytes(), &state)
            .unwrap()
            .is_some()
    );
    let mut outer_scan = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 10_000,
    };
    let mut work = factory_work();
    let mut qualify = |roots: &RootSetV1| {
        let qualified_state = store.state_session(quota(), MemoryClass::Ordinary).unwrap();
        let qualified_values = store.value_session(quota());
        qualify_factory_family_v1(
            roots,
            &mut FactoryFamilyReadContextV1 {
                state: &qualified_state,
                values: ValueReadContext {
                    source: &qualified_values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                },
                semantic_bytes: 1 << 20,
                record_bytes: 2 << 20,
                max_payload_bytes: 1 << 20,
                outer_scan: &mut outer_scan,
                take_state_error: || qualified_state.take_read_error().map(typed),
                cancelled: || false,
            },
            &mut work,
        )
    };
    let qualified = qualify(&original).unwrap();
    let malformed_result = qualify(&malformed);
    eprintln!("malformed inner-marker qualification={malformed_result:?}");
    assert!(matches!(malformed_result, Err(ValueTreeError::Invalid(_))));
    let parent = parent(malformed);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [93; 32], &state, &memory, tx_limits())
            .unwrap();
    let mut writer = store.value_session(quota());
    let result;
    context!(
        &mut tx,
        state,
        &values,
        &mut writer,
        &memory,
        || false,
        || false,
        |read, write| {
            let guard = read.transaction.guard();
            assert!(
                change_factory_cell_overlay_v1(
                    cell_change(
                        FactoryRecordKeyV1::Factory("outer factory"),
                        FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
                        Some(FactoryValueRefV1::Signed(i64::MIN)),
                        Some(FactoryValueRefV1::Signed(7))
                    ),
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .unwrap()
            );
            let guard = read.transaction.guard();
            result = create_factory_record_overlay_v1(
                FactoryRecordCreateV1 {
                    key: FactoryRecordKeyV1::Build(u64::MAX),
                    after: FactoryRecordRefV1::Build(&source.pending_factory_builds[&u64::MAX]),
                    qualification: &qualified,
                },
                &mut read,
                &mut write,
                &mut work,
                &guard,
                &live,
            );
        }
    );
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
            "factory family root binding"
        )))
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
