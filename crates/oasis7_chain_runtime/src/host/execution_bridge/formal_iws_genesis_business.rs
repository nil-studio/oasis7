//! Persist the actual projected GEN business output as provisional originals.
//! This is not a receipt, accepted sidecar, cache promotion, or normal permit.
use super::super::incremental_genesis::{
    ProvisionalProjectedRuntimeGenesisIndexV1, RuntimeGenesisPhysicalErrorV1,
};
use super::{IwsGenesisTransferErrorV1 as Error, IwsGenesisTransferWorkV1};
use oasis7::world_service::formal_result::{
    PaidFormalBusinessDescriptorV1, PaidFormalBusinessErrorV1,
};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::FormalBoundedContextV1,
        business_index::{PaidBusinessIndexDeltaV1, PaidBusinessIndexNodeReaderV1},
    },
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionError,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_runtime::BodyMutationError;
use std::{
    mem::size_of,
    sync::atomic::{AtomicBool, Ordering},
};

type Result<T> = std::result::Result<T, Error>;

/// The exact projected owner and its completed original business DATA. Only
/// this module's successful fanout can construct the pairing; it is not an
/// accepted source, namespace lease, output locator or publication permit.
pub(crate) struct ProvisionalProjectedGenesisBusinessV1 {
    projected: ProvisionalProjectedRuntimeGenesisIndexV1,
    descriptor: PaidFormalBusinessDescriptorV1,
    delta: PaidBusinessIndexDeltaV1,
    descriptor_wire: ReservedValue<Vec<u8>>,
    delta_wire: ReservedValue<Vec<u8>>,
    retention_wire: ReservedValue<Vec<u8>>,
    descriptor_ref: OriginalArtifactRefV1,
    delta_ref: OriginalArtifactRefV1,
    retention_ref: OriginalArtifactRefV1,
    allocation: MemoryReservation,
}

impl ProvisionalProjectedGenesisBusinessV1 {
    pub(crate) fn projected(&self) -> &ProvisionalProjectedRuntimeGenesisIndexV1 {
        &self.projected
    }

    pub(crate) fn descriptor(&self) -> &PaidFormalBusinessDescriptorV1 {
        &self.descriptor
    }

    pub(crate) fn delta(&self) -> &PaidBusinessIndexDeltaV1 {
        &self.delta
    }

    pub(crate) fn descriptor_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.descriptor_ref, &self.descriptor_wire)
    }

    pub(crate) fn delta_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.delta_ref, &self.delta_wire)
    }

    pub(crate) fn retention_original(&self) -> (OriginalArtifactRefV1, &ReservedValue<Vec<u8>>) {
        (self.retention_ref, &self.retention_wire)
    }

    pub(crate) fn validate_context(&self, common: &mut FormalBoundedContextV1<'_>) -> Result<()> {
        self.projected
            .projected()
            .validate_context_bounded_v1(common)
            .map_err(Error::Body)?;
        self.descriptor
            .validate_context(common)
            .map_err(context_error)?;
        self.delta.validate_context(common).map_err(context_error)?;
        common.consume_steps(1).map_err(context_error)?;
        if !self.allocation.uses_budget(common.memory())
            || !self.descriptor_wire.uses_budget(common.memory())
            || !self.delta_wire.uses_budget(common.memory())
            || !self.retention_wire.uses_budget(common.memory())
            || !self
                .projected
                .retention_manifest()
                .uses_budget(common.memory())
        {
            return Err(PageStoreError::Corrupt("GEN business retained originals pool").into());
        }
        Ok(())
    }
}

fn context_error(error: FormalError) -> Error {
    Error::Body(BodyMutationError::Original(Box::new(error)))
}

fn business_error(error: PaidFormalBusinessErrorV1) -> Error {
    Error::Genesis(RuntimeGenesisPhysicalErrorV1::Business(error))
}

fn cancelled(stop: &AtomicBool) -> Result<()> {
    if stop.load(Ordering::Acquire) {
        Err(PageStoreError::Cancelled.into())
    } else {
        Ok(())
    }
}

struct NoPriorNodes;

impl PaidBusinessIndexNodeReaderV1 for NoPriorNodes {
    fn read_node_bounded(
        &self,
        digest: &str,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> std::result::Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
        common.charge(1, digest.len() as u64)?;
        Err(FormalError::Invalid(
            "fresh GEN unexpectedly requested a prior business node".into(),
        ))
    }
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

/// Temporarily lend the actual seven IO lanes under the original Core. The
/// reserved frame covers counters and child state. Only unused loan allowance
/// returns to the caller; actual debit remains spent on success and failure.
fn metered<T>(
    common: &mut FormalBoundedContextV1<'_>,
    io: &mut ObjectIo<'_>,
    run: impl FnOnce(&mut ObjectIo<'_>) -> std::result::Result<T, PageStoreError>,
) -> Result<T> {
    common.charge(56, 4096).map_err(context_error)?;
    let _frame = common
        .memory()
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let before = counters(io);
    let lease = common.work().begin_child().map_err(context_error)?;
    let steps = lease.remaining_steps() / 3;
    let bytes = lease.remaining_read_bytes() / 4;
    let capped = std::array::from_fn(|i| before[i].min(if i < 3 { steps } else { bytes }));
    install(io, capped);
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(io))) {
        Ok(result) => result,
        Err(payload) => {
            install(io, [0; 7]);
            // An abandoned lease must drain the original Core, not refund it.
            drop(lease);
            std::panic::resume_unwind(payload)
        }
    };
    let after = counters(io);
    if (0..7).any(|i| after[i] > capped[i]) {
        install(io, [0; 7]);
        drop(lease);
        return Err(PageStoreError::Corrupt("GEN business original lane increased").into());
    }
    let used: [u64; 7] = std::array::from_fn(|i| capped[i] - after[i]);
    install(io, std::array::from_fn(|i| before[i] - used[i]));
    // Three step lanes and four byte lanes are individually capped by the
    // corresponding fraction of this same lease, so neither sum can overflow.
    let debit = lease
        .consume(used[..3].iter().sum(), used[3..].iter().sum())
        .map_err(context_error);
    lease.finish();
    match result {
        Err(error) => Err(error.into()),
        Ok(value) => debit.map(|()| value),
    }
}

fn digest(bytes: &[u8], common: &mut FormalBoundedContextV1<'_>) -> Result<[u8; 32]> {
    common
        .charge(1, size_of::<blake3::Hasher>() as u64)
        .map_err(context_error)?;
    let _fee = common
        .memory()
        .reserve(size_of::<blake3::Hasher>(), MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.chunks(8192) {
        common
            .charge(1, chunk.len() as u64)
            .map_err(context_error)?;
        hash.update(chunk);
    }
    common.charge(1, 32).map_err(context_error)?;
    Ok(*hash.finalize().as_bytes())
}

fn write_original(
    objects: &[IncrementalObjectStore<'_>; 3],
    bytes: &[u8],
    expected: Option<[u8; 32]>,
    work: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<OriginalArtifactRefV1> {
    cancelled(stop)?;
    common.consume_steps(1).map_err(context_error)?;
    if bytes.len() > work.max_original_bytes {
        return Err(PageStoreError::ResourceLimited("GEN business original byte limit").into());
    }
    work.originals_remaining =
        work.originals_remaining
            .checked_sub(1)
            .ok_or(PageStoreError::ResourceLimited(
                "GEN business original count",
            ))?;
    let id = digest(bytes, common)?;
    if expected.is_some_and(|expected| expected != id) {
        return Err(PageStoreError::Corrupt("GEN business node digest").into());
    }
    for (i, object) in objects.iter().enumerate() {
        cancelled(stop)?;
        let mut io = ObjectIo {
            physical: &mut work.objects_physical[i],
            logical: &mut work.objects_logical[i],
            class: MemoryClass::Ordinary,
            cancelled: stop,
        };
        let written = metered(common, &mut io, |io| {
            // write_object includes complete winning-locator readback before
            // returning, even when an immutable original already exists.
            object.write_object(
                &mut std::io::Cursor::new(bytes),
                bytes.len() as u64,
                Some(id),
                io,
            )
        })?;
        if written != id {
            return Err(PageStoreError::Corrupt("GEN business original write identity").into());
        }
    }
    cancelled(stop)?;
    common.consume_steps(1).map_err(context_error)?;
    Ok(OriginalArtifactRefV1 {
        id,
        byte_len: bytes.len() as u64,
    })
}

fn open_store<'pages>(
    pages: &'pages IncrementalPageStore,
    index: usize,
    work: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<IncrementalObjectStore<'pages>> {
    // Lease/directory checks, path copies and marker fsync are not PageIo
    // lanes. Admit their work before entering the exclusive object IO loan,
    // as the normal Host object namespace opener does.
    let path_bytes = u64::try_from(pages.root().as_os_str().as_encoded_bytes().len())
        .map_err(|_| PageStoreError::ResourceLimited("GEN business object namespace path"))?;
    let steps = path_bytes
        .checked_add(64)
        .ok_or(PageStoreError::ResourceLimited(
            "GEN business object namespace work",
        ))?;
    let bytes = path_bytes
        .checked_mul(8)
        .and_then(|n| n.checked_add(4096))
        .ok_or(PageStoreError::ResourceLimited(
            "GEN business object namespace bytes",
        ))?;
    common.charge(steps, bytes).map_err(context_error)?;
    let mut io = ObjectIo {
        physical: &mut work.objects_physical[index],
        logical: &mut work.objects_logical[index],
        class: MemoryClass::Ordinary,
        cancelled: stop,
    };
    metered(common, &mut io, |io| {
        IncrementalObjectStore::open(pages, io.class, io.physical, io.cancelled)
    })
}

/// Consume the genuine producer output while its business source borrows it.
/// Every error drops that provisional output. Only successful fanout returns
/// the same owner together with its original paid business graph and wires;
/// it still grants no Ready/Accepted or durable closure proof.
/// As with the original GEN fanout, each target pays for its actual local copy
/// from its own PageStore pool. The projection keeps the original source pool.
pub(crate) fn persist_projected_genesis_business_v1(
    projected: ProvisionalProjectedRuntimeGenesisIndexV1,
    source: &IncrementalPageStore,
    targets: [&IncrementalPageStore; 2],
    work: &mut IwsGenesisTransferWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    stop: &AtomicBool,
) -> Result<ProvisionalProjectedGenesisBusinessV1> {
    cancelled(stop)?;
    if !common.memory().same_pool(source.memory_budget()) {
        return Err(PageStoreError::Corrupt("GEN business source pool").into());
    }
    let bytes = size_of::<ProvisionalProjectedGenesisBusinessV1>();
    common.charge(1, bytes as u64).map_err(context_error)?;
    let allocation = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let (
        descriptor,
        delta,
        descriptor_wire,
        delta_wire,
        retention_wire,
        descriptor_ref,
        delta_ref,
        retention_ref,
    ) = {
        let business = projected
            .prepare_business_output_bounded_v1(&NoPriorNodes, common, &mut work.canonical)
            .map_err(Error::Genesis)?;
        let descriptor = business
            .descriptor_cbor_bounded(common)
            .map_err(business_error)?;
        let delta = business
            .delta_cbor_bounded(common)
            .map_err(business_error)?;
        let objects = [
            open_store(source, 0, work, common, stop)?,
            open_store(targets[0], 1, work, common, stop)?,
            open_store(targets[1], 2, work, common, stop)?,
        ];
        let descriptor_ref = write_original(&objects, &descriptor, None, work, common, stop)?;
        let delta_ref = write_original(&objects, &delta, None, work, common, stop)?;
        for node in &business.delta().node_writes {
            common
                .charge(1, node.digest.len() as u64)
                .map_err(context_error)?;
            let mut expected = [0; 32];
            hex::decode_to_slice(&node.digest, &mut expected)
                .map_err(|_| PageStoreError::Corrupt("GEN business node ID"))?;
            write_original(&objects, &node.bytes, Some(expected), work, common, stop)?;
        }
        // The source already computed this exact index-finish RootSet ID under
        // the same Core. Bind its actual GEN retention DATA before encoding;
        // neither a Header nor caller-supplied replacement roots are involved.
        business.validate_context(common).map_err(business_error)?;
        let actual_source = business.actual_source();
        common
            .charge(2, actual_source.after_state_root().len() as u64)
            .map_err(context_error)?;
        let mut roots = [0; 32];
        hex::decode_to_slice(actual_source.after_state_root(), &mut roots)
            .map_err(|_| PageStoreError::Corrupt("GEN retention roots ID"))?;
        let retention = projected.retention_manifest();
        if actual_source.final_roots().height != 0 || !retention.uses_budget(common.memory()) {
            return Err(PageStoreError::Corrupt("GEN retention source binding").into());
        }
        let memory = common.memory().clone();
        let mut first = None;
        let mut pay = |steps, bytes| {
            common.charge(steps, bytes).map_err(|error| {
                if first.is_none() {
                    first = Some(error);
                }
                ExecutionError::Aborted
            })
        };
        let retention_wire = retention
            .validate_binding_v1(roots, None, &mut pay)
            .and_then(|()| retention.canonical_bytes_with_work(&memory, &mut pay));
        if let Some(error) = first {
            return Err(context_error(error));
        }
        // Keep the codec's entire 128 KiB reservation with its returned wire.
        let retention_wire = retention_wire
            .map_err(|error| Error::Body(BodyMutationError::Original(Box::new(error))))?;
        let retention_ref = write_original(&objects, &retention_wire, None, work, common, stop)?;
        // Consume the same update at the original final source-check boundary.
        // This ends its borrow of projected without releasing either graph.
        let (owned_descriptor, owned_delta) = business
            .into_owned_data_bounded_v1(common)
            .map_err(business_error)?;
        (
            owned_descriptor,
            owned_delta,
            descriptor,
            delta,
            retention_wire,
            descriptor_ref,
            delta_ref,
            retention_ref,
        )
    };
    cancelled(stop)?;
    common.consume_steps(1).map_err(context_error)?;
    Ok(ProvisionalProjectedGenesisBusinessV1 {
        projected,
        descriptor,
        delta,
        descriptor_wire,
        delta_wire,
        retention_wire,
        descriptor_ref,
        delta_ref,
        retention_ref,
        allocation,
    })
}
