//! One fixed assembler joins all actually captured auxiliary source owners.
//! No external root, graph, status or completion flag enters this constructor.
use super::*;
use crate::world::{
    incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    incremental_cognition_projection_aux_state::*,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    incremental_normal_event_commitment::NormalEventCommitmentV1,
    incremental_normal_final_sources::NormalCognitionProjectionInputsV1,
    incremental_normal_projection_commitment::{
        AcceptedNormalProjectionCacheV1, NormalProjectionCurrentWorkV1,
        prepare_current_journal_projection_v1,
    },
};
use oasis7_world_state::incremental_cognition_projection_aux::CognitionProjectionAuxWorkV1;
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn prepare_current_normal_projection_aux_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        cache: &AcceptedNormalProjectionCacheV1,
        revocation: &RuntimeRevocationCurrentV1,
        terminal: &NormalTerminalReadMetadataV1,
        events: &NormalEventCommitmentV1,
        cognition_inputs: &NormalCognitionProjectionInputsV1<'_>,
        authorization: &FrozenNormalAuthorizationProjectionV1,
        policy: &PaidCurrentBootstrapPolicyV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalProjectionCurrentWorkV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ActualCurrentNormalProjectionAuxV1, RootModuleExecutionErrorV1<O::Error>> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cache.validate_parent_v1(parent, common)?;
            let live = live_owner();
            terminal.validate_provenance(self.ctx.transaction, common, &live)?;
            cognition_inputs.validate(self.ctx.transaction, common, &live)?;
            metadata::validate_runtime_projection_coverage_v1(
                parent,
                self.ctx.transaction,
                work.pending_entries,
                common,
            )?;
            let revocation = self.capture_current_revocation_projection_v1(
                parent, revocation, read, work, common, live_owner,
            )?;
            let journal = prepare_current_journal_projection_v1(
                parent,
                cache,
                terminal,
                events,
                self.ctx.transaction,
                work.max_json_output_bytes,
                common,
                &live_owner(),
            )?;
            let mut state_work = CognitionProjectionAuxWorkV1 {
                pending_entries: &mut *work.pending_entries,
                json: &mut *work.cognition_json,
                processing: &mut *work.aux_processing,
                controls_remaining: &mut *work.cognition_controls_remaining,
            };
            let state = capture_current_cognition_state_projection_aux_bounded_v1(
                parent,
                self.ctx,
                &mut state_work,
                common,
                &live_owner(),
            )?;
            let proof = capture_current_cognition_proof_projection_aux_bounded_v1(
                parent,
                cognition_inputs,
                self.ctx,
                &mut state_work,
                common,
                &live_owner(),
            )?;
            let mut history_work = CognitionHistoryProjectionAuxWorkV1 {
                index: &mut *work.cognition_history_index,
                cbor: &mut *work.cognition_history_cbor,
                originals: &mut *work.originals,
            };
            let history = capture_current_cognition_history_projection_aux_bounded_v1(
                parent,
                cognition_inputs,
                self.ctx,
                self.originals,
                &mut state_work,
                &mut history_work,
                common,
                &live_owner(),
            )
            .map_err(|error| match error {
                CognitionHistoryProjectionErrorV1::Body(error) => {
                    RootModuleExecutionErrorV1::Body(error)
                }
                CognitionHistoryProjectionErrorV1::Original(error) => {
                    RootModuleExecutionErrorV1::ArtifactOriginalStorage(error)
                }
            })?;
            let retention = self.capture_current_normal_retention_projection_v1(
                parent,
                cognition_inputs.retention(),
                work.cognition_json,
                work.aux_processing,
                common,
                live_owner,
            )?;
            let mut runtime_work = oasis7_world_state::incremental_runtime_projection_aux::RuntimeProjectionAuxWorkV1 {
                pending_entries: &mut *work.pending_entries,
                outer_scan: &mut *work.scan,
                inner_scan: &mut *work.runtime_inner_scan,
                config: &mut *work.runtime_config,
                resource_scans: &mut *work.runtime_resource_scans,
                resource_cells: &mut *work.runtime_resource_cells,
                governance: &mut *work.runtime_governance,
                json: &mut *work.cognition_json,
                authorization_cells: &mut *work.runtime_authorization_cells,
                controls_cells: &mut *work.runtime_controls_cells,
                processing: &mut *work.aux_processing,
                selected_heap: work.max_json_output_bytes,
            };
            let runtime = crate::world::incremental_normal_projection_runtime_sources::capture_current_normal_projection_runtime_sources_v1(
                parent, read, self.ctx, &mut runtime_work, common, &live_owner(),
            )?;
            runtime.validate(self.ctx.transaction, common, &live_owner())?;
            let mut columns = cache.auxiliary_columns_v1(parent, common)?;
            columns = fields::fold_current_runtime_fields_projection_v1(
                &columns,
                runtime.source(),
                self.ctx.transaction,
                common,
                &live_owner(),
            )?;
            columns = fold::fold_current_revocation_projection_v1(
                &columns,
                &revocation,
                self.ctx.transaction,
                common,
                &live_owner(),
            )?;
            columns = cognition::fold_current_cognition_state_projection_v1(
                &columns,
                &state,
                self.ctx.transaction,
                common,
                &live_owner(),
            )?;
            columns = cognition::fold_current_cognition_proof_projection_v1(
                &columns,
                &proof,
                self.ctx.transaction,
                common,
                &live_owner(),
            )?;
            columns = history::fold_current_cognition_history_projection_v1(
                &columns,
                &history,
                self.ctx.transaction,
                common,
                &live_owner(),
            )?;
            columns=crate::world::incremental_normal_retention::projection::fold_current_normal_retention_projection_v1(
                &columns,&retention,self.ctx.transaction,common,&live_owner(),
            )?;
            columns = metadata::fold_current_runtime_metadata_projection_v1(
                &columns,
                parent,
                self.ctx.transaction,
                terminal,
                &journal,
                cognition_inputs,
                authorization,
                policy,
                common,
                &live_owner(),
            )?;
            let bytes = std::mem::size_of::<ActualCurrentNormalProjectionAuxV1>();
            common.charge(1, bytes as u64).map_err(error)?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let output = ActualCurrentNormalProjectionAuxV1 {
                columns,
                parent_record: parent.refs().head.verified_record_id(),
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                memory: common.memory().clone(),
                fee,
            };
            output.validate(parent, self.ctx.transaction, common, &live_owner())?;
            Ok(output)
        }));
        match attempt {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(payload) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(payload)
            }
        }
    }
}
