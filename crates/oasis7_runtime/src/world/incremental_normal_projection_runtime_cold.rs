//! Complete actual accepted-parent auxiliary capture without a resident cache.
//! Every column is built from its qualified source or original explicit role
//! profile. No ordinary successor, default World or replacement meter enters.
use super::*;
use crate::world::{
    incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    incremental_cognition_projection_aux_state::*,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    incremental_normal_industry_due::delivery::ProductValidationDeliveryWorkV1,
    incremental_normal_projection_commitment::{
        NormalProjectionCurrentWorkV1, capture_projection_json_data_v1,
        prepare_cold_journal_projection_v1,
    },
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::incremental_cognition_projection_aux::CognitionProjectionAuxWorkV1;
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1, ProjectionDataPathRefV1 as Path,
};
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
    pub(in crate::world) fn prepare_qualified_normal_projection_aux_v1<
        P: MerkleNodeReader,
        V: ValueNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        _revocation: &RuntimeRevocationCurrentV1,
        terminal: &NormalTerminalReadMetadataV1,
        accepted_ledger: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
        accepted_retention: &ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>,
        policy: &PaidCurrentBootstrapPolicyV1,
        authorization: &FrozenNormalAuthorizationProjectionV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        work: &mut NormalProjectionCurrentWorkV1<'_>,
        journal_work: &mut ProductValidationDeliveryWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ActualCurrentNormalProjectionAuxV1, RootModuleExecutionErrorV1<O::Error>> {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent.validate_context(common).map_err(error)?;
            let live = live_owner();
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live)?;
            terminal.validate_provenance(self.ctx.transaction, common, &live)?;
            let revocation=with_business_source_read_work_v1(parent.refs().roots,self.ctx,read,work.aux_authorization,work.aux_processing,common,|_ctx,read,w,_processing|{
    let mut selected=WorldFieldReadContextV1{root:parent.refs().roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),state:read.state,values:oasis7_formal_protocol::incremental_value::ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled};
    oasis7_world_state::incremental_runtime_revocation::read_qualified_runtime_revocation_projection_columns_v1(parent.refs().storage.current().runtime().revocation(),parent.refs().roots,&mut selected,&mut w.scan,&mut w.typed).map_err(SourceError::from)
   }).map_err(|e|error(ReadCause(e)))?;
            revocation.validate_binding(
                parent.refs().storage.current().runtime().revocation(),
                parent.refs().roots,
                common.memory(),
            )?;
            let journal = prepare_cold_journal_projection_v1(
                parent,
                terminal,
                read,
                self.originals,
                journal_work,
                work.max_json_output_bytes,
                self.ctx.transaction,
                common,
                &mut self.ctx.cancelled,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::ProductDelivery)?;
            let mut state_work = CognitionProjectionAuxWorkV1 {
                pending_entries: &mut *work.pending_entries,
                json: &mut *work.cognition_json,
                processing: &mut *work.aux_processing,
                controls_remaining: &mut *work.cognition_controls_remaining,
            };
            let state = capture_qualified_cognition_state_projection_aux_bounded_v1(
                parent,
                self.ctx,
                &mut state_work,
                common,
                &live_owner(),
            )?;
            let proof = match accepted_ledger {
                Some(source) => capture_qualified_cognition_proof_projection_aux_bounded_v1(
                    parent,
                    source,
                    self.ctx,
                    &mut state_work,
                    common,
                    &live_owner(),
                )?,
                None => capture_qualified_absent_cognition_proof_projection_aux_bounded_v1(
                    parent,
                    self.ctx.transaction,
                    common,
                    &live_owner(),
                )?,
            };
            let mut history_work = CognitionHistoryProjectionAuxWorkV1 {
                index: &mut *work.cognition_history_index,
                cbor: &mut *work.cognition_history_cbor,
                originals: &mut *work.originals,
            };
            let history = capture_qualified_cognition_history_projection_aux_bounded_v1(
                parent,
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
            accepted_retention.validate_binding_v1(
                parent.refs().head,
                parent.refs().roots,
                common,
            )?;
            let retention = work.with_source_io_v1(self.ctx,read,common,|_,read,work|{
                oasis7_world_state::incremental_cognition_retention_ledger::materialize_qualified_cognition_retention_v1(accepted_retention,read,&mut work.cognition_json.json).map_err(Into::into)
            })?;
            let mut runtime_work=oasis7_world_state::incremental_runtime_projection_aux::RuntimeProjectionAuxWorkV1{pending_entries:&mut *work.pending_entries,outer_scan:&mut *work.scan,inner_scan:&mut *work.runtime_inner_scan,config:&mut *work.runtime_config,resource_scans:&mut *work.runtime_resource_scans,resource_cells:&mut *work.runtime_resource_cells,governance:&mut *work.runtime_governance,json:&mut *work.cognition_json,authorization_cells:&mut *work.runtime_authorization_cells,controls_cells:&mut *work.runtime_controls_cells,processing:&mut *work.aux_processing,selected_heap:work.max_json_output_bytes};
            let runtime=crate::world::incremental_normal_projection_runtime_sources::capture_qualified_normal_projection_runtime_sources_v1(parent,read,self.ctx,&mut runtime_work,common,&live_owner())?;
            runtime.validate(self.ctx.transaction, common, &live_owner())?;
            let mut columns =
                cold_cognition_profile::prepare_qualified_cognition_projection_profile_v1(
                    parent, common,
                )?;
            let raw = capture_projection_json_data_v1(revocation.value(), common)?;
            let value = fold::value(&raw, common)?;
            let memory = common.memory().clone();
            columns = columns
                .with_path_data_v1(
                    &[Path::Field("capability_revocation_state")],
                    Some(&value),
                    &memory,
                    &mut |s, b| common.charge(s, b),
                )
                .map_err(fold::graph)?;
            columns = fold::initialize_revocation_set_projection_v1(&columns, common)?;
            columns = fields::fold_current_runtime_fields_projection_v1(
                &columns,
                runtime.source(),
                self.ctx.transaction,
                common,
                &live_owner(),
            )?;
            if parent.refs().storage.current().cognition().presence()!=oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3::Disabled{
   columns=cognition::fold_current_cognition_state_projection_v1(&columns,&state,self.ctx.transaction,common,&live_owner())?;
   columns=cognition::fold_current_cognition_proof_projection_v1(&columns,&proof,self.ctx.transaction,common,&live_owner())?;
   columns=history::fold_current_cognition_history_projection_v1(&columns,&history,self.ctx.transaction,common,&live_owner())?;
   }
            if let Some(retention) = retention.as_ref() {
                if parent.refs().storage.current().cognition().presence()==oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3::Disabled {
                    return Err(BodyMutationError::Invalid("disabled Cognition has retained optional state").into());
                }
                let raw = capture_projection_json_data_v1(&**retention, common)?;
                let value = fold::value(&raw, common)?;
                columns = columns
                    .with_path_data_v1(
                        &[Path::Field("cognition"), Path::Field("retention_state")],
                        Some(&value),
                        &memory,
                        &mut |s, b| common.charge(s, b),
                    )
                    .map_err(fold::graph)?;
            }
            columns = metadata::fold_qualified_runtime_metadata_projection_v1(
                &columns,
                parent,
                self.ctx.transaction,
                terminal,
                &journal,
                authorization,
                policy,
                common,
                &live_owner(),
            )?;
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live_owner())?;
            terminal.validate_provenance(self.ctx.transaction, common, &live_owner())?;
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
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
}

use oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1;
