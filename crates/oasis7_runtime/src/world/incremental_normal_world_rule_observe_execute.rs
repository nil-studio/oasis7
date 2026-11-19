//! Fixed complete Observe command: actual registered retry or complete
//! Current coverage, genuine cut/original publication, then actual registry CAS.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_world_rule_observation_v2::{
        ObservationCurrentSourceErrorV2, ObservationJsonOriginalSinkV2,
        ObservationOriginalPersistenceErrorV2, prepare_observation_current_source_v2,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter, incremental_value::ValueNodeWriter,
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_world_rule_requests_current::{
        CurrentObservationRegistryEntryV2, read_current_observation_registry_v2,
    },
    incremental_world_rules_family::WorldRulesFamilyWorkV1,
};
use std::fmt::Debug;

enum Prepared {
    Retained(RetainedCurrentObservationV2),
    Fresh(
        ReservedValue<CurrentObservationRegistryEntryV2>,
        PreparedCurrentObservationCoverageV2,
    ),
}
#[derive(Debug)]
struct PersistenceFailure<E: Debug>(ObservationOriginalPersistenceErrorV2<E>);
impl<E: Debug> std::fmt::Display for PersistenceFailure<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Current observation original persistence: {:?}", self.0)
    }
}
impl<E: Debug> std::error::Error for PersistenceFailure<E> {}
#[derive(Debug)]
struct SourceFailure(ObservationCurrentSourceErrorV2);
impl std::fmt::Display for SourceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Current observation source: {:?}", self.0)
    }
}
impl std::error::Error for SourceFailure {}
fn source_error(error: ObservationCurrentSourceErrorV2) -> IndustrialDeliveryErrorV1 {
    BodyMutationError::Original(Box::new(SourceFailure(error))).into()
}
fn persistence_error<E: Debug + Send + Sync + 'static>(
    error: ObservationOriginalPersistenceErrorV2<E>,
) -> IndustrialDeliveryErrorV1 {
    BodyMutationError::Original(Box::new(PersistenceFailure(error))).into()
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_lifecycle_observe_v2<M, P, V, R, W, E, C, PE, PC, CW, ME, S>(
    header: header::PreparedFreshLifecycleHeaderV1,
    normal_parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
    sources: &super::super::emergency_close::EmergencyCloseSourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    extra: &mut EmergencyServiceSourceWorkV1<'_>,
    rules_work: &mut WorldRulesFamilyWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
    sink: &mut S,
) -> Result<super::super::written::FreshWrittenLifecycleCommandV1, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    S: ObservationJsonOriginalSinkV2,
    S::Error: Debug + Send + Sync + 'static,
{
    let result = (|| {
        header.validate(
            ctx.transaction,
            common.memory(),
            now,
            common.work().identity(),
            live,
        )?;
        let crate::DomainEvent::WorldRuleCommitted {
            command:
                WorldRuleCommandV1::EmergencySupply {
                    command: EmergencySupplyCommandV1::Observe { region_id },
                },
            ..
        } = header.event()
        else {
            return Err(BodyMutationError::Invalid("Observe lifecycle command").into());
        };
        if now == 0 || !now.is_multiple_of(60) {
            return Err(crate::world_rules::invalid("missing_authority").into());
        }
        let prepared = super::super::delivery::paid_io::run(
            ctx,
            parent,
            write,
            materials,
            work,
            common,
            |ctx, parent, _write, _materials, work, tail| {
                let guard = ctx.transaction.guard();
                let registry = read_current_observation_registry_v2(
                    region_id,
                    now - 60,
                    sources.rules.requests(),
                    sources.delivery.eligibility.roots,
                    state,
                    ctx,
                    &mut work.fixed_scan,
                    &mut work.composite_read,
                    &guard,
                    live,
                )?;
                if registry.registered().is_some() {
                    return Ok(Prepared::Retained(retain_current_observation_v2(
                        registry,
                        &header,
                        ctx.transaction,
                        tail,
                        now,
                        live,
                    )?));
                }
                let directory = super::super::top::open_top_in_child_v1(
                    sources.rules,
                    sources.delivery.eligibility.roots,
                    state,
                    ctx,
                    work,
                    rules_work,
                    tail,
                    live,
                )?;
                let coverage = prepare_current_observation_coverage_v2(
                    region_id, &directory, sources, state, ctx, parent, work, extra, tail, now,
                    live,
                )?;
                Ok(Prepared::Fresh(registry, coverage))
            },
        )?;
        let source = match prepared {
            Prepared::Retained(source) => {
                super::super::written::LifecycleWrittenSourceV1::RetainedObserve(source)
            }
            Prepared::Fresh(registry, coverage) => {
                let captured = prepare_observation_current_source_v2(
                    normal_parent,
                    ctx.transaction,
                    common,
                    live,
                )
                .map_err(source_error)?;
                let cut = captured
                    .persist_json_original_v2(normal_parent, ctx.transaction, common, live, sink)
                    .map_err(persistence_error)?;
                let published = publish_current_observation_v2(
                    coverage,
                    cut,
                    normal_parent,
                    ctx.transaction,
                    common,
                    now,
                    live,
                    sink,
                )
                .map_err(|error| match error {
                    CurrentObservationPublishErrorV2::Coverage(error) => error,
                    CurrentObservationPublishErrorV2::Source(error) => source_error(error),
                    CurrentObservationPublishErrorV2::Persistence(error) => {
                        persistence_error(error)
                    }
                })?;
                let committed = commit_current_observation_v2(
                    published,
                    registry,
                    &header,
                    normal_parent,
                    sources,
                    state,
                    ctx,
                    parent,
                    write,
                    materials,
                    work,
                    common,
                    now,
                    live,
                )?;
                super::super::written::LifecycleWrittenSourceV1::Observe(committed)
            }
        };
        Ok(super::super::written::FreshWrittenLifecycleCommandV1 { header, source })
    })();
    // Only read-only policy rejections may leave this command retryable. All IO,
    // budget, cancellation and partial-write failures abort the enclosing tick.
    if result
        .as_ref()
        .is_err_and(|error| !matches!(error, IndustrialDeliveryErrorV1::World(_)))
    {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
