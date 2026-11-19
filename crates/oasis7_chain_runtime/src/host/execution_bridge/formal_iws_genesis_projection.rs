//! Original retained-history bytes from P's real object namespace. This DATA
//! reader does not grant accepted-parent or Current transaction qualification.
use super::*;
use oasis7_proto::memory_budget::MemoryBudget;
use oasis7_runtime::{
    PaidProjectionRetainedOriginalReaderV1,
    incremental_genesis_projection::{
        PaidGenesisProjectionCommitmentV1, RuntimeProjectedGenesisV1,
    },
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::sync::Arc;

struct GenesisRetainedOriginalsV1<'a, 'pages, 'io> {
    objects: &'a IncrementalObjectStore<'pages>,
    memory: &'a MemoryBudget,
    io: ObjectIo<'io>,
    core: WorkIdentityV1,
    fee: MemoryReservation,
    failed: bool,
}

fn body(error: Error) -> BodyMutationError {
    match error {
        Error::Body(error) => error,
        Error::Store(error) => BodyMutationError::Original(Box::new(error)),
        error => BodyMutationError::Original(Box::new(error)),
    }
}

impl PaidProjectionRetainedOriginalReaderV1 for GenesisRetainedOriginalsV1<'_, '_, '_> {
    type RetainedError = BodyMutationError;

    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }

    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }

    fn read_retained_original_bounded_v1(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> std::result::Result<Arc<ReservedValue<Vec<u8>>>, BodyMutationError> {
        let failed = std::mem::replace(&mut self.failed, true);
        common
            .charge(1, 32)
            .map_err(|error| body(context_error(error)))?;
        if failed
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.objects.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "GEN retained original context or prior failure",
            ));
        }
        // This consumes the exact continuing producer lanes. Missing/corrupt
        // originals remain the original typed store failure, never empty data.
        let raw = metered(common, &mut self.io, |io| {
            self.objects.read_object_owned(id, max_bytes, io)
        })
        .map_err(body)?;
        if !raw.uses_budget(common.memory())
            || raw.len() > max_bytes
            || digest(&raw, common).map_err(body)? != *id
        {
            return Err(BodyMutationError::Invalid(
                "GEN retained original pool/length/hash",
            ));
        }
        self.failed = false;
        Ok(raw)
    }
}

pub(super) fn capture(
    projected: &RuntimeProjectedGenesisV1,
    objects: &IncrementalObjectStore<'_>,
    memory: &MemoryBudget,
    io: ObjectIo<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<PaidGenesisProjectionCommitmentV1> {
    let n = size_of::<GenesisRetainedOriginalsV1<'_, '_, '_>>();
    common.charge(1, n as u64).map_err(context_error)?;
    let fee = common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(PageStoreError::Memory)?;
    let mut reader = GenesisRetainedOriginalsV1 {
        objects,
        memory,
        io,
        core: common.work().identity(),
        fee,
        failed: false,
    };
    projected
        .prepare_formal_projection_commitment_bounded_v1(&mut reader, common, canonical)
        .map_err(Error::Body)
}
