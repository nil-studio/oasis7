//! Complete actual GEN Journal originals, without mailbox prepublication.
//! Codec tests below prove work/completion only; no test creates acceptance.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
};
use oasis7_runtime::incremental_action_original::persist_world_event_original_with_work_v1;
use oasis7_world_state::{
    DomainEvent, WorldEvent,
    incremental_cognition_retention_ledger::CognitionRetentionWorkV1,
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1, JsonWriteWorkV1},
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_format::complete_world_format_profile_v1,
};
use std::{cell::Cell, mem::size_of};

const STEPS: u64 = 256_000_000;
const BYTES: u64 = 4 << 30;
const PATHS: [&str; 3] = ["source-pages", "primary-pages", "replica-pages"];

fn journal_source(cfg: &GenesisConfigV1) -> World {
    // Same public initial identity recipe as business::identity_bound_seed.
    // This is fixture construction before production, never formal material.
    let mut original = World::new_with_state(WorldState::default());
    original
        .bind_cognition_runtime(&cfg.world_id, "main", 0, None, "pending", 0)
        .unwrap();
    original.initialize_empty_formal_history().unwrap();
    let genesis = hex::encode(cfg.digest().unwrap());
    let snapshot = original.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: &cfg.world_id,
            chain_id: "projected-genesis-chain",
            genesis_ref: Some(&genesis),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: original.state().time,
        },
        hex::encode(cfg.initial_content),
        hex::encode(cfg.format_profile),
    );
    let mut world = World::from_snapshot(snapshot, original.journal().clone()).unwrap();
    world.initialize_empty_formal_history().unwrap();
    assert!(world.state().agents.is_empty());
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
    assert!(
        world.journal().events.is_empty(),
        "fresh GEN fixture Journal: {:?}",
        world.journal().events
    );
    let missing_agents = ["missing-gen-event-a", "missing-gen-event-b"];
    let actions = missing_agents.map(|agent_id| {
        world.submit_action(Action::MoveAgent {
            agent_id: agent_id.into(),
            to: GeoPos::new(1, 2, 3),
        })
    });
    world.step().unwrap();
    assert!(world.state().agents.is_empty());
    assert_eq!(world.pending_actions_len(), 0);
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
    assert!(
        (actions.len()..=64).contains(&world.journal().events.len()),
        "GEN submitted {actions:?}; actual Journal length {}: {:?}",
        world.journal().events.len(),
        world.journal().events
    );
    for (id, missing_agent) in actions.into_iter().zip(missing_agents) {
        // MoveAgent is outside the actual ActionAccepted whitelist.
        assert!(
            !world.journal().events.iter().any(|event| matches!(
                &event.body, WorldEventBody::Domain(DomainEvent::ActionAccepted { action_id, .. })
                if *action_id == id
            )),
            "MoveAgent {id} unexpectedly accepted: {:?}",
            world.journal().events
        );
        assert_eq!(
            world
                .journal()
                .events
                .iter()
                .filter(|event| matches!(
                    &event.body, WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: oasis7_world_state::RejectReason::AgentNotFound { agent_id },
                    }) if *action_id == id && agent_id == missing_agent
                ) && matches!(
                    &event.caused_by, Some(oasis7_world_state::CausedBy::Action(action_id))
                    if *action_id == id
                ))
                .count(),
            1,
            "MoveAgent {id} for {missing_agent} needs exactly one causal rejection: {:?}",
            world.journal().events
        );
    }
    world
}

fn json_read_work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}

fn object_lanes(work: &IwsGenesisTransferWorkV1) -> [[u64; 7]; 3] {
    std::array::from_fn(|i| {
        [
            work.objects_physical[i].reads_remaining,
            work.objects_physical[i].writes_remaining,
            work.objects_logical[i].nodes_remaining,
            work.objects_physical[i].read_bytes_remaining,
            work.objects_physical[i].write_bytes_remaining,
            work.objects_logical[i].read_bytes_remaining,
            work.objects_logical[i].write_bytes_remaining,
        ]
    })
}

struct Inspection {
    physical: [PageIoBudget; 3],
    logical: [ValueIoBudget; 3],
    stop: AtomicBool,
}
impl Inspection {
    fn new() -> Self {
        Self {
            physical: std::array::from_fn(|_| PageIoBudget {
                reads_remaining: 100_000,
                read_bytes_remaining: 16 << 20,
                writes_remaining: 0,
                write_bytes_remaining: 0,
            }),
            logical: std::array::from_fn(|_| ValueIoBudget {
                nodes_remaining: 100_000,
                read_bytes_remaining: 8 << 20,
                write_bytes_remaining: 8 << 20,
            }),
            stop: AtomicBool::new(false),
        }
    }
    fn check_fresh(&mut self, stores: &[IncrementalPageStore; 3]) {
        for (i, store) in stores.iter().enumerate() {
            let error = match IncrementalObjectStore::open(
                store,
                MemoryClass::Recovery,
                &mut self.physical[i],
                &self.stop,
            ) {
                Err(error) => error,
                Ok(_) => panic!("fresh GEN namespace {i} already has an object capability"),
            };
            assert!(
                matches!(&error, PageStoreError::Io(cause) if cause.kind() == std::io::ErrorKind::NotFound),
                "fresh GEN namespace {i} must lack its object capability: {error:?}"
            );
            assert_eq!(self.physical[i].writes_remaining, 0);
            assert_eq!(self.physical[i].write_bytes_remaining, 0);
        }
    }
    fn check(
        &mut self,
        stores: &[IncrementalPageStore; 3],
        world: &World,
        expected: &[Vec<u8>],
        visible: usize,
    ) {
        for (i, store) in stores.iter().enumerate() {
            let objects = IncrementalObjectStore::open(
                store,
                MemoryClass::Recovery,
                &mut self.physical[i],
                &self.stop,
            )
            .unwrap();
            let before_output = self.logical[i].write_bytes_remaining;
            let mut io = ObjectIo {
                physical: &mut self.physical[i],
                logical: &mut self.logical[i],
                class: MemoryClass::Recovery,
                cancelled: &self.stop,
            };
            let mut output = 0;
            for (ordinal, bytes) in expected.iter().enumerate() {
                let id = *blake3::hash(bytes).as_bytes();
                let actual = objects.read_object_owned(&id, bytes.len(), &mut io);
                if ordinal < visible {
                    let actual = actual.unwrap();
                    assert_eq!(actual.as_slice(), bytes.as_slice());
                    assert_eq!(*blake3::hash(&actual).as_bytes(), id);
                    let decoded: WorldEvent = serde_cbor::from_slice(&actual).unwrap();
                    assert_eq!(&decoded, &world.journal().events[ordinal]);
                    output += bytes.len() as u64;
                } else {
                    assert!(
                        matches!(actual, Err(PageStoreError::Missing(missing)) if missing == id)
                    );
                }
            }
            assert_eq!(
                before_output - self.logical[i].write_bytes_remaining,
                output
            );
            assert_eq!(self.physical[i].writes_remaining, 0);
            assert_eq!(self.physical[i].write_bytes_remaining, 0);
        }
    }
}

fn journal_fanout(first_count_failure: bool) {
    let fixture = Fixture::new();
    // Deliberately independent pools: only source ownership is tied to Common.
    let memory: [MemoryBudget; 3] =
        std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
    let input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = journal_source(&cfg);
    let expected: Vec<_> = world
        .journal()
        .events
        .iter()
        .map(|event| {
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
            assert!(bytes.len() <= 8192);
            bytes
        })
        .collect();
    assert_ne!(blake3::hash(&expected[0]), blake3::hash(&expected[1]));
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
        assert!(transfer.originals_remaining >= 1);
        transfer.originals_remaining = 1;
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
            Ok(_) => panic!("first Journal count refusal returned an owner"),
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
            (event_root, expected.len())
        );
        let refs = projected.journal_originals_bounded_v1(&mut common).unwrap();
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
        // Five business originals and the complete empty Wasm catalog root.
        let business_count = 6 + material.delta().delta().node_writes.len() as u64;
        assert_eq!(
            before_count - transfer.originals_remaining,
            expected.len() as u64 + world.tick_consensus_records().len() as u64 + business_count
        );
        let source = projected.business_source_bounded_v1(&mut common).unwrap();
        assert_eq!(source.world_result_count(), 0);
        drop(source);
        // Readonly getters keep the original Core/pool contract.
        let foreign = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
        let mut never = || false;
        assert!(
            projected
                .journal_originals_bounded_v1(&mut FormalBoundedContextV1::new(
                    &memory[0], &foreign, &mut never
                ),)
                .is_err()
        );
        assert!(
            projected
                .journal_originals_bounded_v1(&mut FormalBoundedContextV1::new(
                    &memory[1], &core, &mut never
                ),)
                .is_err()
        );
        let mut stopped = || true;
        assert!(
            projected
                .journal_originals_bounded_v1(&mut FormalBoundedContextV1::new(
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
    inspection.check(
        &stores,
        &world,
        &expected,
        if first_count_failure {
            1
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
    drop(sequence);
    drop(input);
    assert_eq!(memory.each_ref().map(MemoryBudget::used), [0; 3]);
}

#[test]
fn complete_projected_genesis_journal_originals_cold_read_in_order_from_three_pools() {
    journal_fanout(false);
}

#[test]
fn first_journal_count_failure_retains_only_real_prefix_without_owner_or_refund() {
    journal_fanout(true);
}

#[test]
fn actual_event_codec_hook_charges_comparison_bytes_on_both_original_passes() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = journal_source(&cfg);
    let event = &world.journal().events[0];
    let expected = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
    assert!(expected.len() <= 8192);
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
    let receipt = persist_world_event_original_with_work_v1(
        event,
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
    assert_eq!(receipt.original_id(), *blake3::hash(&actual).as_bytes());
    assert_eq!(receipt.original_bytes(), actual.len() as u64);
    assert!(comparisons.get().into_iter().all(|count| count > 0));
    assert_eq!(totals.get()[0], before.0 - canonical.visits_remaining);
    assert!(totals.get()[1] > before.1 - canonical.bytes_remaining);
    assert_eq!(STEPS - core.remaining_steps(), totals.get()[0]);
    assert_eq!(BYTES - core.remaining_read_bytes(), totals.get()[1]);
    assert_eq!(memory.used(), baseline);
    assert!(!core.is_child_active());
    // The old public concrete entry still takes no hook, consumes the SAME
    // continuing canonical account and produces the exact existing wire.
    let paid_before = (
        core.remaining_steps(),
        core.remaining_read_bytes(),
        totals.get(),
    );
    let legacy_before = (canonical.visits_remaining, canonical.bytes_remaining);
    actual.clear();
    let legacy = persist_world_event_original_v1(
        event,
        &memory,
        &mut canonical,
        &mut || false,
        8192,
        &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
            stream.write_to(&mut actual)
        },
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(legacy.original_id(), receipt.original_id());
    assert_eq!(legacy.original_bytes(), receipt.original_bytes());
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
fn metered_event_original_preserves_finish_one_shot_and_typed_second_pass_error() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = journal_source(&cfg);
    let event = &world.journal().events[0];
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
                    "test metered second pass refusal".into(),
                ))));
            }
            Ok(())
        };
        let result = persist_world_event_original_with_work_v1(
            event,
            &memory,
            &mut canonical,
            &mut || false,
            8192,
            &mut |stream: &mut dyn ActionOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                if mode == 0 {
                    return Ok(()); // an unconsumed ACK must never return a receipt
                }
                emitting.set(true);
                stream.write_to(&mut std::io::sink())?;
                if mode == 1 {
                    assert!(stream.write_to(&mut std::io::sink()).is_err());
                }
                if mode == 2 {
                    return Err(BodyMutationError::Invalid("test finish refused"));
                }
                Ok(())
            },
            &debit,
        );
        let error = result.unwrap_err();
        if mode == 3 {
            assert!(matches!(error, BodyMutationError::Original(ref cause)
                if matches!(cause.downcast_ref::<FormalError>(), Some(FormalError::Invalid(message))
                    if message == "test metered second pass refusal")));
        } else if mode == 2 {
            assert!(matches!(
                error,
                BodyMutationError::Invalid("test finish refused")
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
    }
    drop(world);
    drop(input);
    assert_eq!(memory.used(), 0);
}

#[test]
fn journal_original_child_settles_codec_refusal_cancellation_and_unwind() {
    for mode in 0..3 {
        let fixture = Fixture::new();
        let memory: [MemoryBudget; 3] =
            std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
        let input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
        let mut cfg = config();
        cfg.format_profile = complete_world_format_profile_v1();
        let world = journal_source(&cfg);
        let event = &world.journal().events[0];
        let stop = AtomicBool::new(false);
        let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
            IncrementalPageStore::create(
                &fixture.0.join(PATHS[i]),
                memory[i].clone(),
                PageCompression::Raw,
            )
            .unwrap()
        });
        let mut transfer = transfer_work();
        let objects: [IncrementalObjectStore<'_>; 3] = std::array::from_fn(|i| {
            IncrementalObjectStore::create(&stores[i], &mut transfer.objects_physical[i], &stop)
                .unwrap()
        });
        let before_lanes = object_lanes(&transfer);
        let before_canonical = (
            transfer.canonical.visits_remaining,
            transfer.canonical.bytes_remaining,
        );
        let before_count = transfer.originals_remaining;
        // Narrow this independent failure fixture's one original Core. A
        // one-MiB total cannot admit even the first real map comparison share.
        let core = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
        if mode == 0 {
            core.charge(0, BYTES - (1 << 20), &mut || false).unwrap();
        }
        let calls = Cell::new(0);
        let mut cancelled = || {
            calls.set(calls.get() + 1);
            if mode == 2 && calls.get() == 20 {
                panic!("actual GEN codec cancellation poll unwind");
            }
            mode == 1 && calls.get() == 20
        };
        let first = RefCell::new(None);
        let baseline = memory.each_ref().map(MemoryBudget::used);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut common = FormalBoundedContextV1::new(&memory[0], &core, &mut cancelled);
            originals::Originals {
                stores: &objects,
                physical: &mut transfer.objects_physical,
                logical: &mut transfer.objects_logical,
                occurrences: &mut transfer.originals_remaining,
                max_bytes: transfer.max_original_bytes,
                cancelled: &stop,
                error: &first,
            }
            .event_bounded(event, &mut transfer.canonical, &mut common)
        }));
        assert!(!core.is_child_active());
        assert!(first.borrow().is_none());
        assert_eq!(transfer.originals_remaining, before_count);
        assert!(transfer.canonical.visits_remaining < before_canonical.0);
        assert!(transfer.canonical.visits_remaining > 0); // restored actual work, not placeholder
        assert!(transfer.canonical.bytes_remaining <= before_canonical.1);
        assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
        if mode == 2 {
            assert!(result.is_err());
            assert_eq!(core.remaining_steps(), 0);
            assert_eq!(core.remaining_read_bytes(), 0);
            assert_eq!(object_lanes(&transfer), [[0; 7]; 3]);
        } else {
            let error = result.unwrap().unwrap_err();
            let BodyMutationError::Original(error) = error else {
                panic!("lost actual Common cause: {error:?}");
            };
            let error = error.downcast_ref::<FormalError>().unwrap();
            assert!(
                matches!(error, FormalError::Invalid(message) if message == if mode == 0 {
                    "bounded archive read bytes"
                } else {
                    "bounded archive read cancelled"
                })
            );
            assert_eq!(object_lanes(&transfer), before_lanes);
            assert!(core.remaining_steps() < STEPS);
            assert!(core.remaining_read_bytes() > 0);
        }
        drop(objects);
        drop(stores);
        drop(world);
        drop(input);
        assert_eq!(memory.each_ref().map(MemoryBudget::used), [0; 3]);
    }
}

#[path = "formal_iws_genesis_snapshot_original_tests.rs"]
mod snapshots;
