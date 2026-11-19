//! A second accepted GEN endpoint rebuilds its own physical resident owners.
//! Only immutable, already authenticated projection DATA is shared; source
//! qualifications, authorization and directory owners are never copied.
use super::*;

fn initial_parent(
    parent: &BoundNormalTickParentV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    use oasis7_formal_protocol::controlled_authority::formal::{
        FormalRecordKindV1, incremental::is_iws_record,
    };
    parent.validate_context(common).map_err(formal)?;
    common.consume_steps(4).map_err(formal)?;
    let refs = parent.refs();
    let record = refs.head.record();
    if record.kind != FormalRecordKindV1::InitialActivation
        || record.position != 1
        || !is_iws_record(record.schema_version)
        || refs.core.body().height != 0
        || refs.core.body().transition.is_some()
    {
        return Err(BodyMutationError::Invalid(
            "replica actual accepted GEN profile",
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn bootstrap_authenticated_genesis_replica_resident_v1<'s, M, R, W, O, E, C>(
    source_parent: &BoundNormalTickParentV1<'_>,
    source_resident: &ReservedValue<NormalHostResidentSourcesV1>,
    replica_parent: &BoundNormalTickParentV1<'_>,
    transaction: &mut ExecutionTransaction<'s, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    originals: &mut O,
    projection: ProvisionalAuthorizationProjectionV1,
    genesis: &RuntimeGenesisSummaryV1,
    authority: NormalHostGenesisAuthorityV1<'_>,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
    live: &mut dyn FnMut() -> ExecutionFence,
) -> Result<ReservedValue<NormalHostResidentSourcesV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        initial_parent(source_parent, common)?;
        initial_parent(replica_parent, common)?;
        work.validate_context(common)?;
        source_resident.validate_live_binding_v1(
            source_parent.refs().head.verified_record_id(),
            &source_parent.fence(),
            common,
        )?;
        if !source_resident.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "replica GEN source resident pool",
            ));
        }
        // Each call validates the corresponding genuine accepted parent. The
        // cache compares exact signed record, body and roots, never root alone.
        source_resident
            .projection_cache
            .validate_parent_v1(source_parent, common)?;
        source_resident
            .projection_cache
            .validate_parent_v1(replica_parent, common)?;
        transaction.validate_bootstrap_read_only_v1(&live())?;
        if transaction.parent_fence() != replica_parent.fence() {
            return Err(BodyMutationError::Invalid("replica GEN actual transaction"));
        }
        let cache = source_resident.projection_cache.share_ordinary_v1(common)?;
        let retention = source_resident
            .retention
            .retain_normal_successor_v1(common)?;
        let (base, _now) = {
            let (io, mut startup) = work.startup_loans();
            prepare_authenticated_normal_host_base_v1(
                replica_parent,
                transaction,
                state,
                values,
                member_sink,
                originals,
                projection,
                authority.source(genesis),
                retention,
                io,
                &mut startup,
                take_state_error,
                cancelled,
                common,
            )?
        };
        base.process_source_v1()
            .same_configuration_v1(source_resident.process_source_v1(), common)?;
        let resident = base.finish_projection_cache_v1(replica_parent, cache, common)?;
        transaction.validate_bootstrap_read_only_v1(&live())?;
        Ok(resident)
    }));
    match attempt {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            Err(error)
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
