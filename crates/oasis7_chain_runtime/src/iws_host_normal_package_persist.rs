//! Actual dual-namespace persistence before Ready staging. Every write uses the
//! existing paired original writer's finish/readback/fanout kernel. No durable
//! accepted head or replay permission is issued by this data owner.
use super::*;
use crate::iws_host_runtime::{IwsHostRuntimeErrorV1, IwsHostRuntimeV1};
use oasis7_distfs::{
    controlled_authority::formal::endpoint::FormalArchiveReadWorkV1,
    incremental_page_store::PageStoreError,
};
use oasis7_formal_protocol::incremental_material::{
    OriginalObjectStreamReader, OriginalObjectStreamWriter,
};
use oasis7_proto::memory_budget::MemoryBudget;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
const SCOPE_BYTES: u64 = (size_of::<((u64, u64), (u64, u64))>()
    + size_of::<oasis7_formal_protocol::incremental_execution::ExecutionFence>())
    as u64;

pub(crate) struct PersistedNormalCandidatePackageV1 {
    package: PaidNormalCandidatePackageV1,
    core: WorkIdentityV1,
    primary: ((u64, u64), (u64, u64)),
    replica: ((u64, u64), (u64, u64)),
    fence: oasis7_formal_protocol::incremental_execution::ExecutionFence,
    fee: MemoryReservation,
}
impl PersistedNormalCandidatePackageV1 {
    /// Only the private fixed publication state machine consumes this DATA;
    /// its entry validates the real namespace receipt and exact input first.
    pub(in crate::iws_host_runtime) fn material_data(&self) -> &FormalCandidateMaterialV1 {
        &self.package.candidate
    }

    pub(crate) fn candidate_bounded_v1<'a>(
        &'a self,
        prepared: &PreparedNormalHostRuntimeV1,
        input: &PaidVerifiedNormalTickInputV1<'_>,
        memory: &MemoryBudget,
        archive: &FormalArchiveReadWorkV1,
        cancelled: &AtomicBool,
    ) -> Result<&'a FormalCandidateMaterialV1, NormalPackageErrorV1> {
        let frozen = input.frozen_sources();
        for replica in [false, true] {
            let check=|parent:oasis7_distfs::controlled_authority::formal::endpoint::HostAcceptedParentReadViewV1<'_>|{
                archive.charge(1,SCOPE_BYTES,&mut||cancelled.load(Ordering::Acquire))?;
                let expected=if replica{self.replica}else{self.primary};
                if parent.fence()!=self.fence || parent.namespace_identities()!=expected{
                    return Err(NormalPackageErrorV1::Invalid("persisted normal package actual namespace/fence"))
                }Ok(())
            };
            if replica {
                frozen.with_replica_parent_read_view_bounded(
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                    check,
                )?;
            } else {
                frozen.with_primary_parent_read_view_bounded(
                    memory,
                    archive,
                    &mut || cancelled.load(Ordering::Acquire),
                    check,
                )?;
            }
        }
        let mut stop = || cancelled.load(Ordering::Acquire);
        let mut common = archive.bounded_context(memory, &mut stop);
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(memory) {
            return Err(invalid());
        }
        self.package.candidate(prepared, input, &mut common)
    }
}
struct Compare<'a> {
    expected: &'a [u8],
    at: usize,
    cancelled: &'a AtomicBool,
    mismatch: bool,
}
impl Write for Compare<'_> {
    fn write(&mut self, raw: &[u8]) -> io::Result<usize> {
        if self.mismatch {
            return Err(io::ErrorKind::Other.into());
        }
        for chunk in raw.chunks(8192) {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::ErrorKind::Other.into());
            }
            let Some(end) = self.at.checked_add(chunk.len()) else {
                self.mismatch = true;
                return Err(io::ErrorKind::Other.into());
            };
            if self.expected.get(self.at..end) != Some(chunk) {
                self.mismatch = true;
                return Err(io::ErrorKind::Other.into());
            }
            self.at = end;
        }
        Ok(raw.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn put(
    object: &ArtifactObject,
    originals: &crate::iws_host_runtime::normal::originals::HostNormalOriginalsV1<'_, '_, '_>,
    common: &mut FormalBoundedContextV1<'_>,
    cancelled: &AtomicBool,
) -> Result<(), IwsHostRuntimeErrorV1> {
    common.charge(1, object.content_hash.len() as u64)?;
    let mut expected = [0u8; 32];
    hex::decode_to_slice(&object.content_hash, &mut expected).map_err(|_| {
        IwsHostRuntimeErrorV1::Move(ExecutionError::Invalid(
            "normal package original hash encoding",
        ))
    })?;
    let mut hash = blake3::Hasher::new();
    for chunk in object.bytes.chunks(8192) {
        common.charge(1, chunk.len() as u64)?;
        hash.update(chunk);
    }
    common.charge(1, 32)?;
    if hash.finalize().as_bytes() != &expected {
        return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::Invalid(
            "normal package original bytes/hash",
        )));
    }
    let length = u64::try_from(object.bytes.len()).map_err(|_| {
        IwsHostRuntimeErrorV1::Move(ExecutionError::ResourceLimited(
            "normal package original length",
        ))
    })?;
    // Resident payload traversal/copy and exact byte comparison are bounded by
    // two passes of this known length, even if the store writes one byte at a
    // time. Pay that bound before the IO child; its callback cannot reenter
    // Common. Physical and logical tree work is debited by the actual IO lanes.
    let payload = length.checked_mul(2).ok_or_else(|| {
        IwsHostRuntimeErrorV1::Move(ExecutionError::ResourceLimited(
            "normal package payload work",
        ))
    })?;
    let byte_work = length.checked_mul(3).ok_or_else(|| {
        IwsHostRuntimeErrorV1::Move(ExecutionError::ResourceLimited(
            "normal package comparison bytes",
        ))
    })?;
    common.charge(
        payload.checked_add(4).ok_or_else(|| {
            IwsHostRuntimeErrorV1::Move(ExecutionError::ResourceLimited(
                "normal package payload steps",
            ))
        })?,
        byte_work.checked_add(64).ok_or_else(|| {
            IwsHostRuntimeErrorV1::Move(ExecutionError::ResourceLimited(
                "normal package payload bytes",
            ))
        })?,
    )?;
    let memory = common.memory().clone();
    originals.with_metered_paired_reader(common,|original|{
        let id=original.write_original(&mut io::Cursor::new(object.bytes.as_slice()),length,expected,&memory)?;
        if id!=expected{return Err(PageStoreError::Corrupt("normal package persisted identity"))}
        // Both endpoints have already passed the paired writer's finish /
        // readback kernels. Also compare primary's true retained byte stream.
        let mut compare=Compare{expected:&object.bytes,at:0,cancelled,mismatch:false};
        let result=original.stream_original(&expected,length,&memory,&mut compare);
        if cancelled.load(Ordering::Acquire){return Err(PageStoreError::Cancelled)}
        let read=result?;
        if compare.mismatch||compare.at!=object.bytes.len()||read!=length{return Err(PageStoreError::Corrupt("normal package persisted byte readback"))}
        Ok(())
    }).map_err(|error|match error{
        crate::iws_host_runtime::normal::originals::PairedNormalOriginalMeterErrorV1::Context(e)=>IwsHostRuntimeErrorV1::Formal(e),
        crate::iws_host_runtime::normal::originals::PairedNormalOriginalMeterErrorV1::Store(e)=>IwsHostRuntimeErrorV1::Physical(e),
    })
}
impl IwsHostRuntimeV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn persist_normal_candidate_package_bounded_v1(
        &self,
        package: PaidNormalCandidatePackageV1,
        input: &PaidVerifiedNormalTickInputV1<'_>,
        prepared: &PreparedNormalHostRuntimeV1,
        input_limits: &crate::iws_normal_input_decode::NormalInputDecodeLimitsV1,
        physical: &mut crate::iws_host_runtime::normal::HostNormalIoWorkV1,
        archive: &FormalArchiveReadWorkV1,
        memory: &MemoryBudget,
        cancelled: &AtomicBool,
    ) -> Result<
        PersistedNormalCandidatePackageV1,
        (IwsHostRuntimeErrorV1, PaidNormalCandidatePackageV1),
    > {
        let receipt = (|| -> Result<_, IwsHostRuntimeErrorV1> {
            // Snapshot only data from the genuine borrowed holders, never caller
            // namespace IDs. Both immutable Frozen holders remain in input's source.
            let frozen = input.frozen_sources();
            let (primary, fence) = frozen.with_primary_parent_read_view_bounded(
                memory,
                archive,
                &mut || cancelled.load(Ordering::Acquire),
                |parent| {
                    archive.charge(1, SCOPE_BYTES, &mut || cancelled.load(Ordering::Acquire))?;
                    Ok::<_, IwsHostRuntimeErrorV1>((parent.namespace_identities(), parent.fence()))
                },
            )?;
            let replica = frozen.with_replica_parent_read_view_bounded(
                memory,
                archive,
                &mut || cancelled.load(Ordering::Acquire),
                |parent| {
                    archive.charge(1, SCOPE_BYTES, &mut || cancelled.load(Ordering::Acquire))?;
                    if parent.fence() != fence {
                        return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::ParentConflict));
                    }
                    Ok::<_, IwsHostRuntimeErrorV1>(parent.namespace_identities())
                },
            )?;
            self.with_normal_package_originals_bounded(
                input,
                prepared,
                input_limits,
                physical,
                archive,
                memory,
                cancelled,
                |originals, common| {
                    // The enclosing bridge retains both original Frozen FD holders,
                    // actual namespace guards and the same caller work loans.
                    input.validate_context(common)?;
                    prepared
                        .validate_input_source_bounded_v1(input.data(), common)
                        .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
                    common.charge(1, 64)?;
                    if package.core != common.work().identity()
                        || !package.fee.uses_budget(common.memory())
                        || package.commit != prepared.candidate().commit_id()
                        || package.material != input.data().checked().material_id()
                    {
                        return Err(IwsHostRuntimeErrorV1::Move(ExecutionError::Invalid(
                            "normal package persistence source",
                        )));
                    }
                    let fee = common
                        .memory()
                        .reserve(
                            size_of::<PersistedNormalCandidatePackageV1>(),
                            MemoryClass::Ordinary,
                        )
                        .map_err(|e| IwsHostRuntimeErrorV1::Value(ValueTreeError::Memory(e)))?;
                    let _phase = common
                        .memory()
                        .reserve(
                            size_of::<Compare<'_>>() + size_of::<blake3::Hasher>() + 1024,
                            MemoryClass::Ordinary,
                        )
                        .map_err(|e| IwsHostRuntimeErrorV1::Value(ValueTreeError::Memory(e)))?;
                    for object in &package.candidate.material.objects {
                        common.consume_steps(1)?;
                        put(object, originals, common, cancelled)?;
                    }
                    let frozen = package.candidate.frozen_material.as_ref().ok_or_else(|| {
                        IwsHostRuntimeErrorV1::Move(ExecutionError::Invalid(
                            "normal package Frozen original closure",
                        ))
                    })?;
                    for object in &frozen.objects {
                        common.consume_steps(1)?;
                        put(object, originals, common, cancelled)?;
                    }
                    prepared
                        .validate_input_source_bounded_v1(input.data(), common)
                        .map_err(IwsHostRuntimeErrorV1::NormalSource)?;
                    Ok((common.work().identity(), primary, replica, fence, fee))
                },
            )
        })();
        match receipt {
            Ok((core, primary, replica, fence, fee)) => Ok(PersistedNormalCandidatePackageV1 {
                package,
                core,
                primary,
                replica,
                fence,
                fee,
            }),
            Err(error) => Err((error, package)),
        }
    }
}
