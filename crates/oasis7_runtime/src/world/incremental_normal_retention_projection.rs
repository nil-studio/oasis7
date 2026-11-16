//! Ordinary retention projection from the real before/after side-ledger owners.
//! Only actual changed JSON branches are visited; no complete retention scan.
use super::*;
use crate::world::{
    incremental_body_event::AgentEventOriginalV1,
    incremental_factory_phase::io::{FactoryPhaseMeterLaneV1, FactoryPhaseMeterLanesV1},
    incremental_module_invocation::{RootModuleExecutionContextV1, RootModuleExecutionErrorV1},
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_world_state::{
    incremental_cognition_retention_ledger::{
        CognitionRetentionProjectionAuxV1, capture_current_cognition_retention_projection_aux_v1,
    },
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_json_records::JsonChangeWorkV1,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
};
pub(in crate::world) struct PaidNormalRetentionProjectionV1 {
    source: CognitionRetentionProjectionAuxV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    fee: MemoryReservation,
}
impl PaidNormalRetentionProjectionV1 {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(formal)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.source.validate(tx, common.memory(), live)?;
        Ok(())
    }
}
struct Loan<'a> {
    json: &'a mut JsonChangeWorkV1,
    processing: &'a mut BusinessDiffProcessingWorkV1,
}
impl FactoryPhaseMeterLanesV1 for Loan<'_> {
    fn visit_lanes(&mut self, v: &mut dyn for<'a> FnMut(FactoryPhaseMeterLaneV1<'a>)) {
        v(FactoryPhaseMeterLaneV1::Steps(
            &mut self.json.json.cells_remaining,
        ));
        v(FactoryPhaseMeterLaneV1::Steps(
            &mut self.json.json.json_nodes_remaining,
        ));
        v(FactoryPhaseMeterLaneV1::Steps(
            &mut self.json.paths_remaining,
        ));
        v(FactoryPhaseMeterLaneV1::Steps(
            &mut self.processing.steps_remaining,
        ));
        v(FactoryPhaseMeterLaneV1::Bytes(
            &mut self.processing.bytes_remaining,
        ));
    }
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => formal(e),
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn capture_current_normal_retention_projection_v1(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        after: &FrozenNormalRetentionV1,
        json: &mut JsonChangeWorkV1,
        processing: &mut BusinessDiffProcessingWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidNormalRetentionProjectionV1, RootModuleExecutionErrorV1<O::Error>> {
        parent.validate_context(common).map_err(formal)?;
        after.validate_current(self.ctx.transaction, common, &live_owner())?;
        let before = parent.refs().retention.ok_or(BodyMutationError::Invalid(
            "normal retention projection signed source missing",
        ))?;
        before.validate_binding_v1(parent.refs().head, parent.refs().roots, common)?;
        let bytes = std::mem::size_of::<PaidNormalRetentionProjectionV1>();
        common.charge(1, bytes as u64).map_err(formal)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let core = common.work().identity();
        let memory = common.memory().clone();
        let mut loan = Loan { json, processing };
        let source = self.with_factory_prologue_external_meter_v1(
            common,
            live_owner,
            &mut loan,
            |root, live, loan| {
                capture_current_cognition_retention_projection_aux_v1(
                    before,
                    after.source(),
                    root.ctx,
                    loan.json,
                    loan.processing,
                    &live(),
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(overlay(e)))
            },
        )?;
        Ok(PaidNormalRetentionProjectionV1 {
            source,
            core,
            memory,
            fee,
        })
    }
}
pub(in crate::world) fn fold_current_normal_retention_projection_v1<M: MerkleNodeReader>(
    before: &oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    source: &PaidNormalRetentionProjectionV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<
    oasis7_world_state::observation_projection::current_commitment::ProjectionColumnDataV1,
    BodyMutationError,
> {
    use crate::world::incremental_normal_projection_runtime as cognition;
    source.validate(tx, common, live)?;
    let memory = common.memory().clone();
    let mut columns = before
        .share_data(&memory, &mut |s, b| common.charge(s, b))
        .map_err(|e| match e {
            oasis7_world_state::observation_projection::current_commitment::ProjectionColumnErrorV1::Work(e) => {
                formal(e)
            }
            oasis7_world_state::observation_projection::current_commitment::ProjectionColumnErrorV1::Memory(e) => {
                ValueTreeError::Memory(e).into()
            }
            oasis7_world_state::observation_projection::current_commitment::ProjectionColumnErrorV1::Invalid(e) => {
                BodyMutationError::Invalid(e)
            }
        })?;
    let cognition = columns
        .select_path_data_v1(
            &[
                oasis7_world_state::observation_projection::current_commitment::ProjectionDataPathRefV1::Field(
                    "cognition",
                ),
            ],
            &memory,
            &mut |s, b| common.charge(s, b),
        )
        .map_err(|e| match e {
            oasis7_world_state::observation_projection::current_commitment::ProjectionColumnErrorV1::Work(e) => {
                formal(e)
            }
            oasis7_world_state::observation_projection::current_commitment::ProjectionColumnErrorV1::Memory(e) => {
                ValueTreeError::Memory(e).into()
            }
            oasis7_world_state::observation_projection::current_commitment::ProjectionColumnErrorV1::Invalid(e) => {
                BodyMutationError::Invalid(e)
            }
        })?
        .ok_or_else(bad)?;
    if cognition.root_kind() == "null" {
        if source.source.present() {
            return Err(bad());
        }
        source.validate(tx, common, live)?;
        return Ok(columns);
    }
    let common = std::cell::RefCell::new(common);
    let pay = |s, b| common.borrow_mut().charge(s, b).map_err(formal);
    cognition::retention_path_presence_v1(
        &mut columns,
        "retention_state",
        None,
        source.source.present(),
        &mut common.borrow_mut(),
    )?;
    source.source.visit_paths_with_work_v1(pay, |patch| {
        cognition::retention_path_apply_v1(
            &mut columns,
            "retention_state",
            None,
            patch,
            &mut common.borrow_mut(),
        )
    })?;
    source.source.visit_paths_with_work_v1(pay, |patch| {
        cognition::retention_path_check_v1(
            &columns,
            "retention_state",
            None,
            patch,
            &mut common.borrow_mut(),
        )
    })?;
    source.validate(tx, &mut common.borrow_mut(), live)?;
    Ok(columns)
}
