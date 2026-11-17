//! Actual retained rejection Audit Result originals from public Runtime refusals.
//! Ordered DATA occurrences carry no evicted history or accepted position.
use super::*;
use oasis7_world_state::{
    consensus::{TickConsensusRejectionAuditEvent, TickConsensusSubmissionRole},
    incremental_runtime_history_original::persist_tick_consensus_rejection_audit_original_with_work_v1,
};

fn audit_source(cfg: &GenesisConfigV1) -> World {
    let mut world = tick_source(cfg);
    assert!(
        world.tick_consensus_rejection_audit_events().is_empty(),
        "unchanged Journal/Snapshot/Tick source has unexpected audits: {:?}",
        world.tick_consensus_rejection_audit_events()
    );
    let tick = world.state().time;
    let authority = world.tick_consensus_authority_source().to_owned();
    // These fixture-only copies retain the real before values under the same
    // 32MiB input admission. Production only borrows the actual retained slice.
    let before_ticks = world.tick_consensus_records().to_vec();
    let before_journal =
        oasis7_world_state::codec::journal_events_cbor(&world.journal().events).unwrap();
    let original = before_ticks.last().unwrap();
    assert_eq!(original.block.header.tick, tick);
    for ordinal in 0..2 {
        // Direct public rejection keeps the real audit; a failed staged step
        // would discard its World and is deliberately not used as this source.
        let error = world
            .record_tick_consensus_propagation_for_tick(tick, &authority)
            .expect_err("actual Authority tick must reject Propagation overwrite");
        let reason = match error {
            oasis7_runtime::WorldError::DistributedValidationFailed { reason } => reason,
            other => panic!("unexpected actual rejection: {other:?}"),
        };
        assert!(reason.contains("non-authoritative submission rejected"));
        let audits = world.tick_consensus_rejection_audit_events();
        assert_eq!(audits.len(), ordinal + 1);
        let audit = &audits[ordinal];
        assert_eq!(audit.recorded_at_tick, tick);
        assert_eq!(audit.tick, tick);
        assert_eq!(
            audit.consensus_height,
            original.certificate.consensus_height
        );
        assert_eq!(audit.attempted_source, authority);
        assert_eq!(
            audit.attempted_role,
            TickConsensusSubmissionRole::Propagation
        );
        assert_eq!(audit.existing_source.as_deref(), Some(authority.as_str()));
        assert_eq!(
            audit.existing_role,
            Some(TickConsensusSubmissionRole::Authority)
        );
        assert_eq!(audit.reason, reason);
    }
    assert_eq!(world.tick_consensus_records(), before_ticks.as_slice());
    assert_eq!(
        oasis7_world_state::codec::journal_events_cbor(&world.journal().events).unwrap(),
        before_journal
    );
    let audits = world.tick_consensus_rejection_audit_events();
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0], audits[1]);
    assert_eq!(world.pending_actions_len(), 0);
    assert!(world.state().agents.is_empty());
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
    world
}

fn audit_wire(record: &TickConsensusRejectionAuditEvent) -> Vec<u8> {
    let expected = oasis7_wasm_abi::encode_canonical_cbor(&serde_json::json!({
        "domain": "oasis7.runtime.history.original.v1",
        "schema": "tick-consensus-rejection-audit.v1",
        "value": record,
    }))
    .unwrap();
    assert!(expected.len() <= 8192);
    expected
}

#[derive(serde::Deserialize)]
struct AuditEnvelope {
    domain: String,
    schema: String,
    value: TickConsensusRejectionAuditEvent,
}

fn inspect_audits(
    inspection: &mut Inspection,
    stores: &[IncrementalPageStore; 3],
    world: &World,
    expected: &[Vec<u8>],
    visible: usize,
) {
    for (i, store) in stores.iter().enumerate() {
        let objects = IncrementalObjectStore::open(
            store,
            MemoryClass::Recovery,
            &mut inspection.physical[i],
            &inspection.stop,
        )
        .unwrap();
        let before_output = inspection.logical[i].write_bytes_remaining;
        let mut io = ObjectIo {
            physical: &mut inspection.physical[i],
            logical: &mut inspection.logical[i],
            class: MemoryClass::Recovery,
            cancelled: &inspection.stop,
        };
        let mut output = 0;
        for (ordinal, bytes) in expected.iter().enumerate() {
            let id = *blake3::hash(bytes).as_bytes();
            let actual = objects.read_object_owned(&id, bytes.len(), &mut io);
            if ordinal < visible {
                let actual = actual.unwrap();
                assert_eq!(actual.as_slice(), bytes.as_slice());
                assert_eq!(*blake3::hash(&actual).as_bytes(), id);
                // Exact trusted expected bytes are checked BEFORE this test
                // oracle decodes, under the original 32MiB fixture admission.
                let decoded: AuditEnvelope = serde_cbor::from_slice(&actual).unwrap();
                assert_eq!(decoded.domain, "oasis7.runtime.history.original.v1");
                assert_eq!(decoded.schema, "tick-consensus-rejection-audit.v1");
                assert_eq!(
                    decoded.value,
                    world.tick_consensus_rejection_audit_events()[ordinal]
                );
                output += bytes.len() as u64;
            } else {
                assert!(matches!(actual, Err(PageStoreError::Missing(missing)) if missing == id));
            }
        }
        assert_eq!(
            before_output - inspection.logical[i].write_bytes_remaining,
            output
        );
        assert_eq!(inspection.physical[i].writes_remaining, 0);
        assert_eq!(inspection.physical[i].write_bytes_remaining, 0);
    }
}

fn retained_audit_fanout(first_count_failure: bool) {
    let fixture = Fixture::new();
    // Deliberately independent pools: only source ownership is tied to Common.
    let memory: [MemoryBudget; 3] =
        std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
    let input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = audit_source(&cfg);
    let journal_expected: Vec<_> = world
        .journal()
        .events
        .iter()
        .map(|event| oasis7_wasm_abi::encode_canonical_cbor(event).unwrap())
        .collect();
    let snapshot_expected: Vec<_> = world
        .snapshot_catalog()
        .records
        .iter()
        .map(snapshot_wire)
        .collect();
    let tick_expected: Vec<_> = world
        .tick_consensus_records()
        .iter()
        .map(tick_wire)
        .collect();
    let expected: Vec<_> = world
        .tick_consensus_rejection_audit_events()
        .iter()
        .map(audit_wire)
        .collect();
    assert_eq!(blake3::hash(&expected[0]), blake3::hash(&expected[1]));
    let sequence = oasis7_world_state::codec::journal_events_cbor(&world.journal().events).unwrap();
    let event_root = *blake3::hash(&sequence).as_bytes();
    let cancelled = AtomicBool::new(false);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::create(
            &fixture.0.join(PATHS[i]),
            memory[i].clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let core = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
    let mut stop = || cancelled.load(Ordering::Acquire);
    let mut common = FormalBoundedContextV1::new(&memory[0], &core, &mut stop);
    let original_core = core.identity();
    let mut transfer = transfer_work();
    let mut producer = work::work();
    let mut logical = work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut json = json_read_work();
    let mut json_producer = JsonWriteWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_json_depth: 64,
        max_pending_node_bytes: 8 << 20,
    };
    let mut changed = JsonChangeWorkV1 {
        json: json_read_work(),
        paths_remaining: 1_000_000,
        max_pending_pairs: 4096,
    };
    let mut processing = BusinessDiffProcessingWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
    };
    // The production entry owns the single capability creation. Before it,
    // readonly Open must report the fresh namespace's real missing capability.
    let mut inspection = Inspection::new();
    inspection.check_fresh(&stores);
    if first_count_failure {
        let prefix = u64::try_from(journal_expected.len())
            .unwrap()
            .checked_add(u64::try_from(snapshot_expected.len()).unwrap())
            .unwrap()
            .checked_add(u64::try_from(tick_expected.len()).unwrap())
            .unwrap();
        assert!(transfer.originals_remaining >= prefix);
        transfer.originals_remaining = prefix;
    }
    let before_lanes = object_lanes(&transfer);
    let before_canonical = (
        transfer.canonical.visits_remaining,
        transfer.canonical.bytes_remaining,
    );
    let before_count = transfer.originals_remaining;
    let baseline = memory.each_ref().map(MemoryBudget::used);
    let result = prepare_projected_iws_genesis_for_endpoints_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory[0],
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut producer,
        work::physical(),
        &mut transfer,
        &mut CognitionRetentionWorkV1 {
            json: &mut json,
            producer: &mut json_producer,
            changed: &mut changed,
            processing: &mut processing,
        },
        &mut common,
    );
    if let Err(ref error) = result {
        eprintln!("GEN_PREPARE_ERROR {error:?}");
    }
    assert_eq!(core.identity(), original_core);
    assert!(!core.is_child_active());
    assert!(core.remaining_steps() < STEPS);
    assert!(core.remaining_read_bytes() < BYTES);
    assert!(transfer.canonical.visits_remaining < before_canonical.0);
    assert!(transfer.canonical.bytes_remaining < before_canonical.1);
    let after_lanes = object_lanes(&transfer);
    for i in 0..3 {
        assert!(after_lanes[i][2] < before_lanes[i][2]);
        for lane in 0..7 {
            assert!(after_lanes[i][lane] <= before_lanes[i][lane]);
        }
    }
    let material = if first_count_failure {
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("first retained Audit count refusal returned an owner"),
        };
        assert!(matches!(error, IwsGenesisTransferErrorV1::Genesis(
            RuntimeGenesisPhysicalErrorV1::Projected(
                oasis7_runtime::incremental_genesis_projection::RuntimeProjectedGenesisErrorV1::Source(
                    BodyMutationError::Invalid("IWS original transfer occurrences")
                )
            )
        )));
        assert_eq!(transfer.originals_remaining, 0);
        assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
        None
    } else {
        let material = result.unwrap();
        material.validate_context(&mut common).unwrap();
        let projected = material.projected();
        assert_eq!(
            projected
                .projected()
                .formal_event_commitment_bounded_v1(&mut common)
                .unwrap(),
            (event_root, journal_expected.len())
        );
        let journal = projected.journal_originals_bounded_v1(&mut common).unwrap();
        assert_eq!(journal.len(), journal_expected.len());
        for (reference, bytes) in journal.iter().zip(&journal_expected) {
            assert_eq!(reference.id, *blake3::hash(bytes).as_bytes());
            assert_eq!(reference.byte_len, bytes.len() as u64);
        }
        let snapshots = projected
            .snapshot_originals_bounded_v1(&mut common)
            .unwrap();
        assert_eq!(snapshots.len(), snapshot_expected.len());
        for (reference, bytes) in snapshots.iter().zip(&snapshot_expected) {
            assert_eq!(reference.id, *blake3::hash(bytes).as_bytes());
            assert_eq!(reference.byte_len, bytes.len() as u64);
        }
        let (ticks, archived_count) = projected
            .hot_tick_originals_bounded_v1(&mut common)
            .unwrap();
        assert_eq!(ticks.len(), tick_expected.len());
        for (reference, bytes) in ticks.iter().zip(&tick_expected) {
            assert_eq!(reference.id, *blake3::hash(bytes).as_bytes());
            assert_eq!(reference.byte_len, bytes.len() as u64);
        }
        assert_eq!(
            archived_count,
            world.borrow_iws_runtime_history_v1().archived_tick_count
        );
        assert_eq!(archived_count, 0);
        let refs = projected
            .retained_audit_originals_bounded_v1(&mut common)
            .unwrap();
        assert!(refs.uses_budget(&memory[0]));
        assert_eq!(refs.len(), expected.len());
        assert_eq!(refs.capacity(), expected.len());
        assert_eq!(
            refs.reserved_bytes(),
            expected.len() * size_of::<OriginalArtifactRefV1>()
        );
        for (reference, bytes) in refs.iter().zip(&expected) {
            assert_eq!(reference.id, *blake3::hash(bytes).as_bytes());
            assert_eq!(reference.byte_len, bytes.len() as u64);
        }
        let business_count = 4 + material.delta().delta().node_writes.len() as u64;
        assert_eq!(
            before_count - transfer.originals_remaining,
            journal_expected.len() as u64
                + snapshot_expected.len() as u64
                + tick_expected.len() as u64
                + expected.len() as u64
                + business_count
        );
        let source = projected.business_source_bounded_v1(&mut common).unwrap();
        assert_eq!(source.world_result_count(), 0);
        drop(source);
        // Readonly getters keep the original Core/pool contract.
        let foreign = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
        let mut never = || false;
        assert!(
            projected
                .retained_audit_originals_bounded_v1(&mut FormalBoundedContextV1::new(
                    &memory[0], &foreign, &mut never
                ),)
                .is_err()
        );
        assert!(
            projected
                .retained_audit_originals_bounded_v1(&mut FormalBoundedContextV1::new(
                    &memory[1], &core, &mut never
                ),)
                .is_err()
        );
        let mut stopped = || true;
        assert!(
            projected
                .retained_audit_originals_bounded_v1(&mut FormalBoundedContextV1::new(
                    &memory[0],
                    &core,
                    &mut stopped
                ),)
                .is_err()
        );
        assert!(!core.is_child_active());
        Some(material)
    };
    let settled = (core.remaining_steps(), core.remaining_read_bytes());
    // Close EVERY production PageStore/object handle before independent reads.
    drop(stores);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::open(
            &fixture.0.join(PATHS[i]),
            memory[i].clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap()
    });
    // The complete Journal, every Snapshot occurrence and actual hot Tick
    // originals form the durable prefix before the first retained Audit refusal.
    inspection.check(&stores, &world, &journal_expected, journal_expected.len());
    inspect_snapshots(
        &mut inspection,
        &stores,
        &world,
        &snapshot_expected,
        snapshot_expected.len(),
    );
    inspect_hot_ticks(
        &mut inspection,
        &stores,
        &world,
        &tick_expected,
        tick_expected.len(),
    );
    inspect_audits(
        &mut inspection,
        &stores,
        &world,
        &expected,
        if first_count_failure {
            0
        } else {
            expected.len()
        },
    );
    assert_eq!(object_lanes(&transfer), after_lanes);
    assert_eq!(
        (core.remaining_steps(), core.remaining_read_bytes()),
        settled
    );
    drop(material);
    assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
    assert!(!core.is_child_active());
    drop(stores);
    drop(world);
    drop(expected);
    drop(journal_expected);
    drop(snapshot_expected);
    drop(tick_expected);
    drop(sequence);
    drop(input);
    assert_eq!(memory.each_ref().map(MemoryBudget::used), [0; 3]);
}

#[test]
fn complete_projected_genesis_retained_audit_results_cold_read_from_three_pools() {
    retained_audit_fanout(false);
}

#[test]
fn first_retained_audit_count_failure_preserves_actual_journal_snapshot_tick_prefix_without_refund()
{
    retained_audit_fanout(true);
}

#[test]
fn actual_retained_audit_codec_pays_both_passes_and_preserves_legacy_envelope() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = audit_source(&cfg);
    let audit = &world.tick_consensus_rejection_audit_events()[0];
    let expected = audit_wire(audit);
    let core = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
    let mut canonical = canonical_work();
    let before = (canonical.visits_remaining, canonical.bytes_remaining);
    let totals = Cell::new([0u64; 2]);
    let comparisons = Cell::new([0u64; 2]);
    let emitting = Cell::new(false);
    let debit = |steps, bytes| {
        core.charge(steps, bytes, &mut || false)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        let old = totals.get();
        totals.set([
            old[0].checked_add(steps).unwrap(),
            old[1].checked_add(bytes).unwrap(),
        ]);
        if steps > 1 && bytes > 0 {
            let mut phases = comparisons.get();
            phases[usize::from(emitting.get())] += 1;
            comparisons.set(phases);
        }
        Ok(())
    };
    let baseline = memory.used();
    let mut actual = Vec::with_capacity(8192);
    let reference = persist_tick_consensus_rejection_audit_original_with_work_v1(
        audit,
        &memory,
        &mut canonical,
        &mut || false,
        8192,
        &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
            emitting.set(true);
            stream.write_to(&mut actual)
        },
        &debit,
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(reference.id, *blake3::hash(&actual).as_bytes());
    assert_eq!(reference.byte_len, actual.len() as u64);
    assert!(comparisons.get().into_iter().all(|count| count > 0));
    // One separate fixed envelope admission, plus every actual codec request.
    assert_eq!(totals.get()[0], 1 + before.0 - canonical.visits_remaining);
    assert!(totals.get()[1] > before.1 - canonical.bytes_remaining);
    assert_eq!(STEPS - core.remaining_steps(), totals.get()[0]);
    assert_eq!(BYTES - core.remaining_read_bytes(), totals.get()[1]);
    assert_eq!(memory.used(), baseline);
    assert!(!core.is_child_active());
    let paid_before = (
        core.remaining_steps(),
        core.remaining_read_bytes(),
        totals.get(),
    );
    let legacy_before = (canonical.visits_remaining, canonical.bytes_remaining);
    actual.clear();
    let legacy = with_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::RejectionAudit(audit),
        &memory,
        &mut canonical,
        &mut || false,
        8192,
        |producer| {
            let reference = OriginalArtifactRefV1 {
                id: producer.original_id(),
                byte_len: producer.original_bytes(),
            };
            producer.persist_to(
                &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                    stream.write_to(&mut actual)
                },
            )?;
            Ok(reference)
        },
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(legacy, reference);
    assert!(canonical.visits_remaining < legacy_before.0);
    assert!(canonical.bytes_remaining < legacy_before.1);
    assert_eq!(
        (
            core.remaining_steps(),
            core.remaining_read_bytes(),
            totals.get()
        ),
        paid_before
    );
    assert_eq!(memory.used(), baseline);
    drop(actual);
    drop(expected);
    drop(world);
    drop(input);
    assert_eq!(memory.used(), 0);
}

#[test]
fn metered_retained_audit_preserves_one_shot_and_typed_second_pass_failure() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = audit_source(&cfg);
    let audit = &world.tick_consensus_rejection_audit_events()[0];
    let core = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
    let mut canonical = canonical_work();
    let baseline = memory.used();
    for mode in 0..4 {
        let before = (core.remaining_steps(), core.remaining_read_bytes());
        let before_visits = canonical.visits_remaining;
        let emitting = Cell::new(false);
        let debit = |steps, bytes| {
            core.charge(steps, bytes, &mut || false)
                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
            if mode == 3 && emitting.get() && steps > 1 && bytes > 0 {
                return Err(BodyMutationError::Original(Box::new(FormalError::Invalid(
                    "test retained Audit second pass refusal".into(),
                ))));
            }
            Ok(())
        };
        let result = persist_tick_consensus_rejection_audit_original_with_work_v1(
            audit,
            &memory,
            &mut canonical,
            &mut || false,
            8192,
            &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                if mode == 0 {
                    return Ok(());
                }
                emitting.set(true);
                stream.write_to(&mut std::io::sink())?;
                if mode == 1 {
                    assert!(stream.write_to(&mut std::io::sink()).is_err());
                }
                if mode == 2 {
                    return Err(BodyMutationError::Invalid(
                        "test retained Audit finish refused",
                    ));
                }
                Ok(())
            },
            &debit,
        );
        let error = result.unwrap_err();
        if mode == 3 {
            assert!(matches!(error, BodyMutationError::Original(ref cause)
                if matches!(cause.downcast_ref::<FormalError>(), Some(FormalError::Invalid(message))
                    if message == "test retained Audit second pass refusal")));
        } else if mode == 2 {
            assert!(matches!(
                error,
                BodyMutationError::Invalid("test retained Audit finish refused")
            ));
        } else {
            assert!(matches!(
                error,
                BodyMutationError::Invalid("agent original stream incomplete")
            ));
        }
        assert!(core.remaining_steps() < before.0);
        assert!(core.remaining_read_bytes() < before.1);
        assert!(canonical.visits_remaining < before_visits);
        assert_eq!(memory.used(), baseline);
        assert!(!core.is_child_active());
    }
    drop(world);
    drop(input);
    assert_eq!(memory.used(), 0);
}
