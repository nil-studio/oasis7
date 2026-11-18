//! Actual projected GEN producer -> durable business originals -> cold reads.
//! The test never writes business bytes. No acceptance, Qualified-parent or
//! normal execution authority is created by these DATA-only checks.
use super::*;
use oasis7::world_service::formal_result::decode_formal_business_descriptor_bounded_v1;
use oasis7_distfs::{
    controlled_authority::formal::endpoint::FormalArchiveReadWorkV1,
    incremental_page_store::PageStoreError,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalError,
    bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1, WorkIdentityV1},
    business_index::{
        BUSINESS_INDEX_MAX_LEAF_BYTES, PaidBusinessIndexNodeReaderV1,
        canonical_business_index_root_bounded_v1, decode_business_index_delta_bounded_v1,
        empty_business_index_root_bounded_v1, verify_business_index_delta_bounded_v1,
    },
};
use oasis7_formal_protocol::incremental_retention::RetentionLedgerManifestV1;
use oasis7_proto::memory_budget::MemoryReservation;
use oasis7_world_state::{
    incremental_cognition_retention_ledger::CognitionRetentionWorkV1,
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1, JsonWriteWorkV1},
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_world_format::complete_world_format_profile_v1,
};
use std::{cell::Cell, mem::size_of};

#[derive(Debug)]
enum MeterError {
    Context(FormalError),
    Store(PageStoreError),
}

fn invalid(note: &'static str) -> FormalError {
    FormalError::Invalid(note.into())
}

fn counters(io: &ObjectIo<'_>) -> [u64; 7] {
    [
        io.physical.reads_remaining,
        io.physical.writes_remaining,
        io.logical.nodes_remaining,
        io.physical.read_bytes_remaining,
        io.physical.write_bytes_remaining,
        io.logical.read_bytes_remaining,
        io.logical.write_bytes_remaining,
    ]
}

fn install(io: &mut ObjectIo<'_>, v: [u64; 7]) {
    io.physical.reads_remaining = v[0];
    io.physical.writes_remaining = v[1];
    io.logical.nodes_remaining = v[2];
    io.physical.read_bytes_remaining = v[3];
    io.physical.write_bytes_remaining = v[4];
    io.logical.read_bytes_remaining = v[5];
    io.logical.write_bytes_remaining = v[6];
}

// Loan the ORIGINAL accounts under one exclusive child. Never replace them
// with a fresh allowance, and never refund any lane's actual consumption.
fn metered<T>(
    common: &mut FormalBoundedContextV1<'_>,
    io: &mut ObjectIo<'_>,
    run: impl FnOnce(&mut ObjectIo<'_>) -> Result<T, PageStoreError>,
) -> Result<T, MeterError> {
    use MeterError::{Context, Store};
    common.charge(56, 4096).map_err(Context)?;
    let _frame = common
        .memory()
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(|e| Store(PageStoreError::Memory(e)))?;
    let before = counters(io);
    let lease = common.work().begin_child().map_err(Context)?;
    let steps = lease.remaining_steps() / 3;
    let bytes = lease.remaining_read_bytes() / 4;
    let capped = std::array::from_fn(|i| before[i].min(if i < 3 { steps } else { bytes }));
    install(io, capped);
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(io))) {
        Ok(result) => result,
        Err(payload) => {
            install(io, [0; 7]);
            drop(lease);
            std::panic::resume_unwind(payload)
        }
    };
    let after = counters(io);
    if (0..7).any(|i| after[i] > capped[i]) {
        install(io, [0; 7]);
        drop(lease);
        return Err(Store(PageStoreError::Corrupt(
            "GEN original lane increased",
        )));
    }
    let used: [u64; 7] = std::array::from_fn(|i| capped[i] - after[i]);
    install(io, std::array::from_fn(|i| before[i] - used[i]));
    let debit = lease
        .consume(used[..3].iter().sum(), used[3..].iter().sum())
        .map_err(Context);
    lease.finish();
    match result {
        Err(error) => Err(Store(error)),
        Ok(value) => debit.map(|()| value),
    }
}

struct NoPriorNodes;

impl PaidBusinessIndexNodeReaderV1 for NoPriorNodes {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        common.charge(1, digest.len() as u64)?;
        Err(invalid(
            "fresh GEN unexpectedly requested a prior business node",
        ))
    }
}

// Read index nodes from the actual endpoint object store, not delta.node_writes.
struct ActualNodes<'a, 'pages, 'io> {
    objects: &'a IncrementalObjectStore<'pages>,
    io: RefCell<ObjectIo<'io>>,
    core: WorkIdentityV1,
    fee: MemoryReservation,
    reads: Cell<u64>,
    first: RefCell<Option<PageStoreError>>,
}

impl PaidBusinessIndexNodeReaderV1 for ActualNodes<'_, '_, '_> {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        common.charge(1, digest.len() as u64)?;
        if self.core != common.work().identity()
            || !self.fee.uses_budget(common.memory())
            || !self.objects.uses_budget(common.memory())
            || self.first.borrow().is_some()
        {
            return Err(invalid("GEN node reader context or prior failure"));
        }
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("GEN node digest"));
        }
        let mut id = [0; 32];
        hex::decode_to_slice(digest, &mut id).map_err(|_| invalid("GEN node digest"))?;
        let mut io = self
            .io
            .try_borrow_mut()
            .map_err(|_| invalid("GEN node reader reentry"))?;
        self.reads.set(self.reads.get() + 1);
        let raw = match metered(common, &mut io, |io| {
            self.objects
                .read_object_owned(&id, BUSINESS_INDEX_MAX_LEAF_BYTES + 128, io)
        }) {
            Ok(raw) => raw,
            Err(MeterError::Context(error)) => return Err(error),
            Err(MeterError::Store(PageStoreError::Missing(missing))) if missing == id => {
                return Ok(None);
            }
            Err(MeterError::Store(error)) => {
                *self.first.borrow_mut() = Some(error);
                return Err(invalid("GEN node physical read failed"));
            }
        };
        drop(io);
        if !raw.uses_budget(common.memory()) {
            return Err(invalid("GEN node raw pool"));
        }
        // Keep the Arc's complete original owner until the separately admitted
        // exact-capacity copy is complete.
        let n = raw.len();
        common.charge(1, (n + size_of::<Vec<u8>>()) as u64)?;
        let fee = common
            .memory()
            .reserve(n + size_of::<Vec<u8>>(), MemoryClass::Ordinary)
            .map_err(|_| invalid("GEN node output admission"))?;
        let mut copied = Vec::new();
        copied
            .try_reserve_exact(n)
            .map_err(|_| invalid("GEN node output allocation"))?;
        if copied.capacity() != n {
            return Err(invalid("GEN node output capacity"));
        }
        for chunk in raw.chunks(8192) {
            common.charge(1, chunk.len() as u64)?;
            copied.extend_from_slice(chunk);
        }
        Ok(Some(fee.retain(copied)))
    }
}

fn digest(bytes: &[u8], common: &mut FormalBoundedContextV1<'_>) -> [u8; 32] {
    common
        .charge(1, size_of::<blake3::Hasher>() as u64)
        .unwrap();
    let _fee = common
        .memory()
        .reserve(size_of::<blake3::Hasher>(), MemoryClass::Ordinary)
        .unwrap();
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.chunks(8192) {
        common.charge(1, chunk.len() as u64).unwrap();
        hash.update(chunk);
    }
    common.charge(1, 32).unwrap();
    *hash.finalize().as_bytes()
}

fn json_read_work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}

fn identity_bound_seed(cfg: &GenesisConfigV1) -> World {
    let mut original = World::new_with_state(WorldState::default());
    original
        .bind_cognition_runtime(&cfg.world_id, "main", 0, None, "pending", 0)
        .unwrap();
    original.initialize_empty_formal_history().unwrap();
    let genesis = hex::encode(cfg.digest().unwrap());
    // Initial test input construction only; this Snapshot is never formal
    // material, an accepted-parent substitute or a normal execution source.
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
    world
}

#[test]
fn real_projected_genesis_business_output_roundtrips_both_independent_endpoints() {
    const STEPS: u64 = 256_000_000;
    const BYTES: u64 = 4 << 30;
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = identity_bound_seed(&cfg);
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
    let expected_genesis = hex::encode(cfg.digest().unwrap());
    let expected_binding = world.current_cognition_runtime_binding().unwrap();
    let governing_wire = expected_binding.runtime_manifest_hash.to_string();
    let expected_governing = governing_wire.strip_prefix("blake3:").unwrap();
    let expected_side = serde_json::to_vec(&world.state().retained_history_root).unwrap();
    let cancelled = AtomicBool::new(false);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::create(
            &fixture
                .0
                .join(["source-pages", "primary-pages", "replica-pages"][i]),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let archive = FormalArchiveReadWorkV1::new(STEPS, BYTES).unwrap();
    let mut cancelled_common = || cancelled.load(Ordering::Acquire);
    let mut common = archive.bounded_context(&memory, &mut cancelled_common);

    // Every role has one initial finite account, before production starts.
    let mut transfer = transfer_work();
    let mut producer = work::work();
    let physical = work::physical();
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
    let live_before_owner = memory.used();
    let material = prepare_projected_iws_genesis_for_endpoints_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory,
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut producer,
        physical,
        &mut transfer,
        &mut CognitionRetentionWorkV1 {
            json: &mut json,
            producer: &mut json_producer,
            changed: &mut changed,
            processing: &mut processing,
        },
        &mut common,
    )
    .unwrap();
    // The producer's borrowed update has ended. Its exact projected source,
    // typed graph, encoded originals and all their original fees moved here.
    material.validate_context(&mut common).unwrap();
    let projected = material.projected();
    assert_eq!(projected.roots().height, 0);
    common.charge(32, 1024).unwrap();
    let roots_digest = projected.roots().digest().unwrap();
    assert_eq!(projected.retention_manifest().roots, roots_digest);
    let source = projected.business_source_bounded_v1(&mut common).unwrap();
    assert_eq!(source.world_result_count(), 0);
    drop(source);
    let business = material.descriptor();
    let business_delta = material.delta().delta();
    assert_eq!(business.tree().index_root().leaf_count, 1);
    let metadata = business.tree().metadata();
    assert_eq!(metadata.world_id, cfg.world_id);
    assert_eq!(metadata.chain_id, "projected-genesis-chain");
    assert_eq!(metadata.genesis_digest, expected_genesis);
    assert_eq!(metadata.formal_position, 1);
    assert_eq!(metadata.after_state_root, hex::encode(roots_digest));
    assert_eq!(metadata.governing_manifest_digest, expected_governing);
    let side = projected
        .projected()
        .retained_history_root_json_bounded_v1(&mut common)
        .unwrap();
    assert_eq!(side.as_slice(), expected_side.as_slice());

    let result_root = business.root_bounded(&mut common).unwrap();
    let (descriptor_ref, descriptor) = material.descriptor_original();
    let (delta_ref, delta) = material.delta_original();
    let (retention_ref, retention) = material.retention_original();
    let (catalog_ref, catalog_wire) = material.wasm_catalog_original();
    assert_eq!(material.wasm_catalog().count, 0);
    assert_eq!(
        material.wasm_catalog().kind,
        oasis7_formal_protocol::incremental_artifacts::ArtifactKindV1::Wasm
    );
    assert!(catalog_wire.uses_budget(&memory));
    assert_eq!(catalog_ref.byte_len, catalog_wire.len() as u64);
    assert_eq!(catalog_ref.id, digest(catalog_wire, &mut common));
    assert_eq!(retention.reserved_bytes(), 128 * 1024);
    assert_eq!(retention_ref.id, digest(retention, &mut common));
    assert_eq!(retention_ref.byte_len, retention.len() as u64);
    root_set::assert_original(&material, &cfg, roots_digest, &mut common);
    let descriptor_id = digest(descriptor, &mut common);
    let delta_id = digest(delta, &mut common);
    assert_eq!(descriptor_ref.id, descriptor_id);
    assert_eq!(descriptor_ref.byte_len, descriptor.len() as u64);
    assert_eq!(delta_ref.id, delta_id);
    assert_eq!(delta_ref.byte_len, delta.len() as u64);
    assert_eq!(business_delta.new_root.as_str(), result_root.as_str());
    assert!(!business_delta.node_writes.is_empty());
    for node in &business_delta.node_writes {
        let actual = digest(&node.bytes, &mut common);
        assert_eq!(hex::encode(actual), node.digest);
    }
    let originals = || {
        [descriptor.as_slice(), delta.as_slice()]
            .into_iter()
            .chain(
                business_delta
                    .node_writes
                    .iter()
                    .map(|node| node.bytes.as_slice()),
            )
            .chain(std::iter::once(retention.as_slice()))
    };

    // Every original page/object handle from the producer is closed before
    // readback. No test-side write or delta-node cache can satisfy this read.
    drop(stores);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::open(
            &fixture
                .0
                .join(["source-pages", "primary-pages", "replica-pages"][i]),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .unwrap()
    });
    let empty = empty_business_index_root_bounded_v1(&mut common).unwrap();
    let empty_root = canonical_business_index_root_bounded_v1(&empty, &mut common).unwrap();

    for i in 0..3 {
        let mut io = ObjectIo {
            physical: &mut transfer.objects_physical[i],
            logical: &mut transfer.objects_logical[i],
            class: MemoryClass::Ordinary,
            cancelled: &cancelled,
        };
        let objects = metered(&mut common, &mut io, |io| {
            IncrementalObjectStore::open(&stores[i], io.class, io.physical, io.cancelled)
        })
        .unwrap();
        // On the old producer this must fail with the real Missing(id), not
        // pass after a fixture has populated descriptor/delta/node objects.
        let descriptor_raw = metered(&mut common, &mut io, |io| {
            objects.read_object_owned(&descriptor_id, descriptor.len(), io)
        })
        .unwrap_or_else(|error| {
            panic!("cold namespace {i}: producer business descriptor read: {error:?}")
        });
        for expected in originals() {
            let id = digest(expected, &mut common);
            let raw = metered(&mut common, &mut io, |io| {
                objects.read_object_owned(&id, expected.len(), io)
            })
            .unwrap();
            common.charge(1, raw.len() as u64).unwrap();
            assert_eq!(raw.as_slice(), expected);
            assert_eq!(digest(&raw, &mut common), id);
            if id == retention_ref.id {
                let decoded = RetentionLedgerManifestV1::decode_paid_with_work(
                    &raw,
                    &memory,
                    &mut |steps, bytes| {
                        common.charge(steps, bytes).unwrap();
                        Ok(())
                    },
                )
                .unwrap();
                common.charge(1, 256).unwrap();
                assert_eq!(&*decoded, &**projected.retention_manifest());
                assert_eq!(decoded.roots, roots_digest);
                assert_eq!(decoded.previous_record, None);
            }
        }
        root_set::assert_cold_original(&material, &cfg, &objects, &mut io, &mut common);
        // Empty P still produces a same-source final Catalog descriptor. Read
        // its actual object from each reopened namespace before decoding it.
        let catalog_raw = metered(&mut common, &mut io, |io| {
            objects.read_object_owned(&catalog_ref.id, catalog_wire.len(), io)
        })
        .unwrap();
        common.charge(2, 1024 + catalog_wire.len() as u64).unwrap();
        let catalog_phase = memory.reserve(1024, MemoryClass::Ordinary).unwrap();
        assert_eq!(catalog_raw.as_slice(), catalog_wire.as_slice());
        let decoded_catalog = oasis7_formal_protocol::incremental_artifacts::ArtifactCatalogV1
            ::from_canonical_bytes_with_work(&catalog_raw, &mut |steps, bytes| {
                common.charge(steps, bytes).unwrap();
                Ok(())
            }).unwrap();
        assert_eq!(&decoded_catalog, material.wasm_catalog());
        assert_eq!(digest(&catalog_raw, &mut common), catalog_ref.id);
        drop(decoded_catalog);
        drop(catalog_phase);
        drop(catalog_raw);
        let delta_raw = metered(&mut common, &mut io, |io| {
            objects.read_object_owned(&delta_id, delta.len(), io)
        })
        .unwrap();
        let decoded =
            decode_formal_business_descriptor_bounded_v1(descriptor_raw.as_slice(), &mut common)
                .unwrap();
        let decoded_delta =
            decode_business_index_delta_bounded_v1(delta_raw.as_slice(), &mut common).unwrap();
        let verified = verify_business_index_delta_bounded_v1(
            &decoded_delta,
            &empty_root,
            &NoPriorNodes,
            &mut common,
        )
        .unwrap();
        assert_eq!(&*verified, decoded.tree().index_root());
        let recovered_root = decoded.root_bounded(&mut common).unwrap();
        assert_eq!(recovered_root.as_str(), result_root.as_str());
        assert_eq!(decoded.metadata(), metadata);

        common.charge(1, 4096).unwrap();
        let nodes = ActualNodes {
            objects: &objects,
            io: RefCell::new(io),
            core: common.work().identity(),
            fee: common
                .memory()
                .reserve(4096, MemoryClass::Ordinary)
                .unwrap(),
            reads: Cell::new(0),
            first: RefCell::new(None),
        };
        let result = decoded.validate_metadata_leaf_bounded_v1(&nodes, &mut common);
        assert!(
            result.is_ok(),
            "endpoint {i}: metadata={result:?}; physical={:?}",
            nodes.first.borrow()
        );
        assert!(nodes.first.borrow().is_none());
        assert!(nodes.reads.get() > 0);
        drop(nodes);
    }
    business.validate_context(&mut common).unwrap();
    assert!(!common.work().is_child_active());
    assert!(archive.remaining_steps() < STEPS);
    assert!(archive.remaining_read_bytes() < BYTES);
    assert_owned_genesis_business_data(&material, &mut common, &mut transfer.canonical);
    drop(empty_root);
    drop(empty);
    drop(result_root);
    drop(side);
    assert!(memory.used() > live_before_owner);
    drop(material);
    assert_eq!(memory.used(), live_before_owner);
}

// Exercise the consuming public boundary using this same genuine GEN source
// and continuing Common. No fixture, physical read or producer allowance is
// recreated; only refusal contexts use a foreign Core/pool.
fn assert_owned_genesis_business_data(
    material: &business_originals::ProvisionalProjectedGenesisBusinessV1,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) {
    let memory = common.memory().clone();
    let before = memory.used();
    let business = material
        .projected()
        .prepare_business_output_bounded_v1(&NoPriorNodes, common, canonical)
        .unwrap();
    let descriptor_allocation = business.tree().metadata().world_id.as_ptr();
    let nodes_allocation = business.delta().node_writes.as_ptr();
    let first_node_allocation = business.delta().node_writes[0].bytes.as_ptr();
    let before_move = memory.used();
    let (descriptor, delta) = business.into_owned_data_bounded_v1(common).unwrap();
    assert_eq!(
        descriptor.tree().metadata().world_id.as_ptr(),
        descriptor_allocation
    );
    assert_eq!(delta.delta().node_writes.as_ptr(), nodes_allocation);
    assert_eq!(
        delta.delta().node_writes[0].bytes.as_ptr(),
        first_node_allocation
    );
    assert_eq!(descriptor.tree().index_root(), &delta.delta().new);
    // The separately built original kernel output must still match the
    // producer's retained result, not only its own encoded roundtrip.
    common
        .charge(
            2,
            (material.descriptor_original().1.len() + material.delta_original().1.len()) as u64,
        )
        .unwrap();
    assert_eq!(descriptor.tree(), material.descriptor().tree());
    assert_eq!(delta.delta(), material.delta().delta());
    descriptor.validate_context(common).unwrap();
    delta.validate_context(common).unwrap();
    assert!(memory.used() < before_move);
    assert!(memory.used() > before);

    // The descriptor may end first. The last live delta retains its entire
    // actual node graph and original phase independently of the source fee.
    drop(descriptor);
    assert_eq!(memory.used(), before + delta.reserved_bytes());
    let node = &delta.delta().node_writes[0];
    assert_eq!(hex::encode(digest(&node.bytes, common)), node.digest);
    drop(delta);
    assert_eq!(memory.used(), before);

    let original = common.work();
    let foreign =
        FormalBoundedWorkV1::new(original.remaining_steps(), original.remaining_read_bytes())
            .unwrap();
    let foreign_memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    for (foreign_core, foreign_pool, cancelled) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let before = memory.used();
        let business = material
            .projected()
            .prepare_business_output_bounded_v1(&NoPriorNodes, common, canonical)
            .unwrap();
        let core = if foreign_core { &foreign } else { original };
        let pool = if foreign_pool {
            &foreign_memory
        } else {
            &memory
        };
        let mut stop = || cancelled;
        let mut refused = FormalBoundedContextV1::new(pool, core, &mut stop);
        assert!(material.validate_context(&mut refused).is_err());
        let before_steps = original.remaining_steps();
        let before_bytes = original.remaining_read_bytes();
        let error = match business.into_owned_data_bounded_v1(&mut refused) {
            Err(error) => error,
            Ok(_) => panic!("foreign or cancelled GEN source returned owned DATA"),
        };
        if cancelled {
            assert!(
                matches!(error.body_error(), Some(BodyMutationError::Original(cause))
                if cause.to_string().contains("bounded archive read cancelled"))
            );
        } else {
            assert!(matches!(
                error.body_error(),
                Some(BodyMutationError::Invalid(
                    "actual normal business commitment source"
                ))
            ));
        }
        if foreign_core || cancelled {
            assert_eq!(original.remaining_steps(), before_steps);
        } else {
            assert!(original.remaining_steps() < before_steps);
        }
        assert_eq!(original.remaining_read_bytes(), before_bytes);
        drop(error);
        assert_eq!(memory.used(), before);
        assert_eq!(foreign_memory.used(), 0);
        assert!(!original.is_child_active());
    }
    material.validate_context(common).unwrap();
}

fn remaining_object_work(work: &IwsGenesisTransferWorkV1) -> [[u64; 7]; 3] {
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

// Run the same private physical stage as the complete wrapper, then fail the
// first business persistence attempt. No earlier successful business write can
// turn the failure into an immutable-object hit or conceal partial publication.
fn business_stage_failure_drops_owner_without_refunding_work(cancel: bool) {
    const STEPS: u64 = 256_000_000;
    const BYTES: u64 = 4 << 30;
    const INSPECT_LOGICAL_BYTES: u64 = 16 << 20;
    const INSPECT_OUTPUT_BYTES: u64 = INSPECT_LOGICAL_BYTES / 2;
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = identity_bound_seed(&cfg);
    let cancelled = AtomicBool::new(false);
    // Independent, finite read-only inspection accounts are admitted before
    // production. They never replenish or retry the failed producer accounts,
    // and their separate signal lets us inspect a cancelled producer's disk.
    let inspect_cancelled = AtomicBool::new(false);
    let mut inspect_physical: [PageIoBudget; 3] = std::array::from_fn(|_| PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    });
    let mut inspect_logical: [ValueIoBudget; 3] = std::array::from_fn(|_| ValueIoBudget {
        nodes_remaining: 100_000,
        // read_range charges bytes emitted to memory as logical output.
        // Split the original total allowance; physical writes remain zero.
        read_bytes_remaining: INSPECT_LOGICAL_BYTES - INSPECT_OUTPUT_BYTES,
        write_bytes_remaining: INSPECT_OUTPUT_BYTES,
    });
    for io in &inspect_logical {
        assert_eq!(
            io.read_bytes_remaining + io.write_bytes_remaining,
            INSPECT_LOGICAL_BYTES
        );
    }
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::create(
            &fixture
                .0
                .join(["source-pages", "primary-pages", "replica-pages"][i]),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let archive = FormalArchiveReadWorkV1::new(STEPS, BYTES).unwrap();
    let mut cancelled_common = || cancelled.load(Ordering::Acquire);
    let mut common = archive.bounded_context(&memory, &mut cancelled_common);
    let original_core = common.work().identity();
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
    let live_before_owner = memory.used();
    let projected = prepare_iws_genesis_for_endpoints_inner_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory,
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut producer,
        work::physical(),
        &mut transfer,
        Some((
            &mut common,
            &mut CognitionRetentionWorkV1 {
                json: &mut json,
                producer: &mut json_producer,
                changed: &mut changed,
                processing: &mut processing,
            },
        )),
    )
    .unwrap();
    let (projected, catalog) = match projected {
        GenesisEndpointOutputV1::Projected(projected, catalog) => (projected, catalog),
        GenesisEndpointOutputV1::Original(_) => panic!("projected GEN returned original mode"),
    };
    assert_eq!(projected.roots().height, 0);
    assert_eq!(
        projected.retention_manifest().roots,
        projected.roots().digest().unwrap()
    );
    assert!(memory.used() > live_before_owner);
    assert!(archive.remaining_steps() < STEPS);
    assert!(archive.remaining_read_bytes() < BYTES);

    // Derive expectations from this actual projected owner, without writing
    // them. All three namespaces must lack both originals before the attempt.
    let (descriptor, delta) = {
        let business = projected
            .prepare_business_output_bounded_v1(&NoPriorNodes, &mut common, &mut transfer.canonical)
            .unwrap();
        (
            business.descriptor_cbor_bounded(&mut common).unwrap(),
            business.delta_cbor_bounded(&mut common).unwrap(),
        )
    };
    let descriptor_id = digest(&descriptor, &mut common);
    let delta_id = digest(&delta, &mut common);
    assert_ne!(descriptor_id, delta_id);
    for i in 0..3 {
        let objects = IncrementalObjectStore::open(
            &stores[i],
            MemoryClass::Recovery,
            &mut inspect_physical[i],
            &inspect_cancelled,
        )
        .unwrap();
        let mut io = ObjectIo {
            physical: &mut inspect_physical[i],
            logical: &mut inspect_logical[i],
            class: MemoryClass::Recovery,
            cancelled: &inspect_cancelled,
        };
        for (id, expected) in [(&descriptor_id, &descriptor), (&delta_id, &delta)] {
            assert!(matches!(
                objects.read_object_owned(id, expected.len(), &mut io),
                Err(PageStoreError::Missing(missing)) if missing == *id
            ));
        }
    }

    // Only the first business phase is invoked below. No preparation accounts
    // are recreated; the genuine physical owner is moved, never cloned.
    if cancel {
        cancelled.store(true, Ordering::Release);
    } else {
        transfer.objects_logical[2].nodes_remaining = 0;
    }
    let before_lanes = remaining_object_work(&transfer);
    let before_steps = archive.remaining_steps();
    let before_bytes = archive.remaining_read_bytes();
    let before_originals = transfer.originals_remaining;
    let before_canonical = (
        transfer.canonical.visits_remaining,
        transfer.canonical.bytes_remaining,
    );
    let result = business_originals::persist_projected_genesis_business_v1(
        (projected, catalog),
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut transfer,
        &mut common,
        &cancelled,
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("failed GEN business stage returned a projected owner"),
    };
    assert_eq!(common.work().identity(), original_core);
    assert!(!common.work().is_child_active());
    assert!(archive.remaining_steps() <= before_steps);
    assert!(archive.remaining_read_bytes() <= before_bytes);
    let after_lanes = remaining_object_work(&transfer);
    for i in 0..3 {
        for lane in 0..7 {
            assert!(
                after_lanes[i][lane] <= before_lanes[i][lane],
                "namespace {i}, lane {lane} was replenished"
            );
        }
    }
    if cancel {
        assert!(matches!(
            error,
            IwsGenesisTransferErrorV1::Store(PageStoreError::Cancelled)
        ));
        assert_eq!(after_lanes, before_lanes);
        assert_eq!(archive.remaining_steps(), before_steps);
        assert_eq!(archive.remaining_read_bytes(), before_bytes);
        assert_eq!(transfer.originals_remaining, before_originals);
        assert_eq!(
            (
                transfer.canonical.visits_remaining,
                transfer.canonical.bytes_remaining
            ),
            before_canonical
        );
    } else {
        assert!(matches!(
            error,
            IwsGenesisTransferErrorV1::Store(PageStoreError::ResourceLimited("node/IO allowance"))
        ));
        assert_eq!(transfer.objects_logical[2].nodes_remaining, 0);
        assert_eq!(before_originals - transfer.originals_remaining, 1);
        assert!(after_lanes[0][2] < before_lanes[0][2]);
        assert!(after_lanes[1][2] < before_lanes[1][2]);
        assert!(after_lanes[2][0] < before_lanes[2][0]);
        assert!(archive.remaining_steps() < before_steps);
        assert!(archive.remaining_read_bytes() < before_bytes);
        assert!(transfer.canonical.visits_remaining <= before_canonical.0);
        assert!(transfer.canonical.bytes_remaining <= before_canonical.1);
    }
    // Close every production page handle before reading the actual durable
    // partial result. Cancellation leaves no descriptor; the replica node
    // failure leaves the full descriptor only in source and primary. Delta
    // was not attempted anywhere. This does not retry publication.
    drop(stores);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::open(
            &fixture
                .0
                .join(["source-pages", "primary-pages", "replica-pages"][i]),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap()
    });
    for i in 0..3 {
        let objects = IncrementalObjectStore::open(
            &stores[i],
            MemoryClass::Recovery,
            &mut inspect_physical[i],
            &inspect_cancelled,
        )
        .unwrap();
        let mut io = ObjectIo {
            physical: &mut inspect_physical[i],
            logical: &mut inspect_logical[i],
            class: MemoryClass::Recovery,
            cancelled: &inspect_cancelled,
        };
        let raw = objects.read_object_owned(&descriptor_id, descriptor.len(), &mut io);
        if !cancel && i < 2 {
            assert_eq!(raw.unwrap().as_slice(), descriptor.as_slice());
        } else {
            assert!(
                matches!(raw, Err(PageStoreError::Missing(missing)) if missing == descriptor_id)
            );
        }
        assert!(matches!(
            objects.read_object_owned(&delta_id, delta.len(), &mut io),
            Err(PageStoreError::Missing(missing)) if missing == delta_id
        ));
        assert_eq!(inspect_physical[i].writes_remaining, 0);
        assert_eq!(inspect_physical[i].write_bytes_remaining, 0);
        let expected_output = if !cancel && i < 2 {
            u64::try_from(descriptor.len()).unwrap()
        } else {
            0
        };
        assert_eq!(
            INSPECT_OUTPUT_BYTES - inspect_logical[i].write_bytes_remaining,
            expected_output
        );
    }
    assert_eq!(cancelled.load(Ordering::Acquire), cancel);
    assert_eq!(remaining_object_work(&transfer), after_lanes);
    // No successful owner escapes. Equivalent reopened stores/input remain
    // charged while the consumed projection and failed-stage temporaries,
    // including these separately retained expected originals, all release.
    drop(descriptor);
    drop(delta);
    assert_eq!(memory.used(), live_before_owner);
}

#[test]
fn projected_genesis_business_stage_cancelled_returns_no_owner_or_refund() {
    business_stage_failure_drops_owner_without_refunding_work(true);
}

#[test]
fn projected_genesis_business_stage_replica_node_limit_returns_no_owner_or_refund() {
    business_stage_failure_drops_owner_without_refunding_work(false);
}

#[test]
fn projected_genesis_business_retention_count_limit_returns_no_owner_or_refund() {
    const STEPS: u64 = 256_000_000;
    const BYTES: u64 = 4 << 30;
    const INSPECT_LOGICAL_BYTES: u64 = 16 << 20;
    const INSPECT_OUTPUT_BYTES: u64 = INSPECT_LOGICAL_BYTES / 2;
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _input = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = identity_bound_seed(&cfg);
    let cancelled = AtomicBool::new(false);
    // Independent, finite read-only inspection accounts are admitted before
    // production. They never replenish or retry the failed producer accounts,
    // and their separate signal lets us inspect a cancelled producer's disk.
    let inspect_cancelled = AtomicBool::new(false);
    let mut inspect_physical: [PageIoBudget; 3] = std::array::from_fn(|_| PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 16 << 20,
        writes_remaining: 0,
        write_bytes_remaining: 0,
    });
    let mut inspect_logical: [ValueIoBudget; 3] = std::array::from_fn(|_| ValueIoBudget {
        nodes_remaining: 100_000,
        // read_range charges bytes emitted to memory as logical output.
        // Split the original total allowance; physical writes remain zero.
        read_bytes_remaining: INSPECT_LOGICAL_BYTES - INSPECT_OUTPUT_BYTES,
        write_bytes_remaining: INSPECT_OUTPUT_BYTES,
    });
    for io in &inspect_logical {
        assert_eq!(
            io.read_bytes_remaining + io.write_bytes_remaining,
            INSPECT_LOGICAL_BYTES
        );
    }
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::create(
            &fixture
                .0
                .join(["source-pages", "primary-pages", "replica-pages"][i]),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let archive = FormalArchiveReadWorkV1::new(STEPS, BYTES).unwrap();
    let mut cancelled_common = || cancelled.load(Ordering::Acquire);
    let mut common = archive.bounded_context(&memory, &mut cancelled_common);
    let original_core = common.work().identity();
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
    let live_before_owner = memory.used();
    let projected = prepare_iws_genesis_for_endpoints_inner_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory,
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut producer,
        work::physical(),
        &mut transfer,
        Some((
            &mut common,
            &mut CognitionRetentionWorkV1 {
                json: &mut json,
                producer: &mut json_producer,
                changed: &mut changed,
                processing: &mut processing,
            },
        )),
    )
    .unwrap();
    let (projected, catalog) = match projected {
        GenesisEndpointOutputV1::Projected(projected, catalog) => (projected, catalog),
        GenesisEndpointOutputV1::Original(_) => panic!("projected GEN returned original mode"),
    };
    assert_eq!(projected.roots().height, 0);
    assert_eq!(
        projected.retention_manifest().roots,
        projected.roots().digest().unwrap()
    );
    assert!(memory.used() > live_before_owner);
    assert!(archive.remaining_steps() < STEPS);
    assert!(archive.remaining_read_bytes() < BYTES);

    // Compute the prefix count and expected bytes from this same real owner.
    // No original is written and no producer account is recreated here.
    let (descriptor, delta, expected_delta) = {
        let business = projected
            .prepare_business_output_bounded_v1(&NoPriorNodes, &mut common, &mut transfer.canonical)
            .unwrap();
        let descriptor = business.descriptor_cbor_bounded(&mut common).unwrap();
        let delta = business.delta_cbor_bounded(&mut common).unwrap();
        let (_, expected_delta) = business.into_owned_data_bounded_v1(&mut common).unwrap();
        (descriptor, delta, expected_delta)
    };
    let retention = projected
        .retention_manifest()
        .canonical_bytes_with_work(&memory, &mut |steps, bytes| {
            common.charge(steps, bytes).unwrap();
            Ok(())
        })
        .unwrap();
    assert_eq!(retention.reserved_bytes(), 128 * 1024);
    let retention_id = digest(&retention, &mut common);
    let count = 2 + u64::try_from(expected_delta.delta().node_writes.len()).unwrap();
    let originals = || {
        [descriptor.as_slice(), delta.as_slice()].into_iter().chain(
            expected_delta
                .delta()
                .node_writes
                .iter()
                .map(|node| node.bytes.as_slice()),
        )
    };
    for i in 0..3 {
        let objects = IncrementalObjectStore::open(
            &stores[i],
            MemoryClass::Recovery,
            &mut inspect_physical[i],
            &inspect_cancelled,
        )
        .unwrap();
        let mut io = ObjectIo {
            physical: &mut inspect_physical[i],
            logical: &mut inspect_logical[i],
            class: MemoryClass::Recovery,
            cancelled: &inspect_cancelled,
        };
        assert!(matches!(
            objects.read_object_owned(&retention_id, retention.len(), &mut io),
            Err(PageStoreError::Missing(missing)) if missing == retention_id
        ));
    }
    // Narrow only the original count already left by physical GEN. N real
    // business writes must succeed; the first manifest attempt is N + 1.
    assert!(transfer.originals_remaining >= count);
    transfer.originals_remaining = count;
    let before_lanes = remaining_object_work(&transfer);
    let before_steps = archive.remaining_steps();
    let before_bytes = archive.remaining_read_bytes();
    let error = match business_originals::persist_projected_genesis_business_v1(
        (projected, catalog),
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut transfer,
        &mut common,
        &cancelled,
    ) {
        Err(error) => error,
        Ok(_) => panic!("GEN retention count failure returned a completed owner"),
    };
    assert!(matches!(
        error,
        IwsGenesisTransferErrorV1::Store(PageStoreError::ResourceLimited(
            "GEN business original count"
        ))
    ));
    assert_eq!(transfer.originals_remaining, 0);
    assert_eq!(common.work().identity(), original_core);
    assert!(!common.work().is_child_active());
    assert!(archive.remaining_steps() < before_steps);
    assert!(archive.remaining_read_bytes() < before_bytes);
    let after_lanes = remaining_object_work(&transfer);
    for i in 0..3 {
        assert!(after_lanes[i][2] < before_lanes[i][2]);
        for lane in 0..7 {
            assert!(after_lanes[i][lane] <= before_lanes[i][lane]);
        }
    }
    // No successful manifest write preceded this failure. Inspect the complete
    // durable business prefix after closing every production handle.
    drop(stores);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::open(
            &fixture
                .0
                .join(["source-pages", "primary-pages", "replica-pages"][i]),
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap()
    });
    for i in 0..3 {
        let objects = IncrementalObjectStore::open(
            &stores[i],
            MemoryClass::Recovery,
            &mut inspect_physical[i],
            &inspect_cancelled,
        )
        .unwrap();
        let mut io = ObjectIo {
            physical: &mut inspect_physical[i],
            logical: &mut inspect_logical[i],
            class: MemoryClass::Recovery,
            cancelled: &inspect_cancelled,
        };
        let mut output = 0u64;
        let mut found = 0u64;
        for expected in originals() {
            let id = digest(expected, &mut common);
            let raw = objects
                .read_object_owned(&id, expected.len(), &mut io)
                .unwrap();
            common.charge(1, raw.len() as u64).unwrap();
            assert_eq!(raw.as_slice(), expected);
            assert_eq!(digest(&raw, &mut common), id);
            output += u64::try_from(raw.len()).unwrap();
            found += 1;
        }
        assert_eq!(found, count);
        assert!(matches!(
            objects.read_object_owned(&retention_id, retention.len(), &mut io),
            Err(PageStoreError::Missing(missing)) if missing == retention_id
        ));
        assert_eq!(inspect_physical[i].writes_remaining, 0);
        assert_eq!(inspect_physical[i].write_bytes_remaining, 0);
        assert_eq!(
            INSPECT_OUTPUT_BYTES - inspect_logical[i].write_bytes_remaining,
            output
        );
    }
    assert_eq!(remaining_object_work(&transfer), after_lanes);
    assert!(!cancelled.load(Ordering::Acquire));
    assert!(!common.work().is_child_active());
    drop(expected_delta);
    drop(descriptor);
    drop(delta);
    drop(retention);
    assert_eq!(memory.used(), live_before_owner);
}

#[path = "formal_iws_genesis_root_original_tests.rs"]
mod root_set;
