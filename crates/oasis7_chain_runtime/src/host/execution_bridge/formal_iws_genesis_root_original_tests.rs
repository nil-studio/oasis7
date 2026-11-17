//! Real RootSet original readback and the first post-manifest refusal.
use super::*;
use oasis7_formal_protocol::{incremental_execution::RootSetV1, incremental_state::StateRootV1};

pub(super) fn assert_original(
    material: &business_originals::ProvisionalProjectedGenesisBusinessV1,
    roots_digest: [u8; 32],
    common: &mut FormalBoundedContextV1<'_>,
) {
    let (reference, wire) = material.roots_original();
    common.charge(1, 32).unwrap();
    assert_eq!(reference.id, roots_digest);
    assert_eq!(reference.byte_len, wire.len() as u64);
    assert_eq!(wire.reserved_bytes(), wire.capacity());
    assert!(wire.uses_budget(common.memory()));
    assert!(wire.len() <= 759);
    assert_eq!(digest(wire, common), reference.id);
}

pub(super) fn assert_cold_original(
    material: &business_originals::ProvisionalProjectedGenesisBusinessV1,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    common: &mut FormalBoundedContextV1<'_>,
) {
    let (reference, expected) = material.roots_original();
    let raw = metered(common, io, |io| {
        objects.read_object_owned(&reference.id, expected.len(), io)
    })
    .unwrap();
    common.charge(1, raw.len() as u64).unwrap();
    assert_eq!(raw.as_slice(), expected.as_slice());
    assert_eq!(digest(&raw, common), reference.id);
    // Same fixed decode admission as the actual cold-root reader, without
    // manufacturing the VerifiedFormalCommit that its accepted entry needs.
    let admission = 2048 + size_of::<RootSetV1>() + 6 * (size_of::<StateRootV1>() + 128);
    common.charge(32, admission as u64 + 1024).unwrap();
    let fee = common
        .memory()
        .reserve(admission, MemoryClass::Ordinary)
        .unwrap();
    let decoded = fee.retain(RootSetV1::from_canonical_bytes(&raw).unwrap());
    assert_eq!(&*decoded, material.projected().roots());
    assert_eq!(decoded.height, 0);
    assert_eq!(
        reference.id,
        material.projected().retention_manifest().roots
    );
}

#[test]
fn projected_genesis_business_root_set_count_limit_returns_no_owner_or_refund() {
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
    let projected = match projected {
        GenesisEndpointOutputV1::Projected(projected) => projected,
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
    common.charge(1, 32).unwrap();
    let roots_id = projected.retention_manifest().roots;
    let count = 3 + u64::try_from(expected_delta.delta().node_writes.len()).unwrap();
    let originals = || {
        [descriptor.as_slice(), delta.as_slice()]
            .into_iter()
            .chain(
                expected_delta
                    .delta()
                    .node_writes
                    .iter()
                    .map(|node| node.bytes.as_slice()),
            )
            .chain(std::iter::once(retention.as_slice()))
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
            objects.read_object_owned(&roots_id, 1024, &mut io),
            Err(PageStoreError::Missing(missing)) if missing == roots_id
        ));
    }
    // Narrow only the original count already left by physical GEN. N real
    // business/manifest writes must succeed; the first roots attempt is N + 1.
    assert!(transfer.originals_remaining >= count);
    transfer.originals_remaining = count;
    let before_lanes = remaining_object_work(&transfer);
    let before_steps = archive.remaining_steps();
    let before_bytes = archive.remaining_read_bytes();
    let before_canonical = (
        transfer.canonical.visits_remaining,
        transfer.canonical.bytes_remaining,
    );
    let error = match business_originals::persist_projected_genesis_business_v1(
        projected,
        &stores[0],
        [&stores[1], &stores[2]],
        &mut transfer,
        &mut common,
        &cancelled,
    ) {
        Err(error) => error,
        Ok(_) => panic!("GEN RootSet count failure returned a completed owner"),
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
    assert!(transfer.canonical.visits_remaining <= before_canonical.0);
    assert!(transfer.canonical.bytes_remaining <= before_canonical.1);
    let after_lanes = remaining_object_work(&transfer);
    for i in 0..3 {
        assert!(after_lanes[i][2] < before_lanes[i][2]);
        for lane in 0..7 {
            assert!(after_lanes[i][lane] <= before_lanes[i][lane]);
        }
    }
    // This is the first RootSet-original attempt. The complete real business
    // prefix and manifest remain durable after closing all production handles.
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
            objects.read_object_owned(&roots_id, 1024, &mut io),
            Err(PageStoreError::Missing(missing)) if missing == roots_id
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
