//! One typed effective-action dispatcher shared by ordinary and pre-resolved module input.
use super::*;
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(in crate::world) fn execute_nonmovement_after_rules_v1<P, ER, CR, PE, PC, AP, AE, AC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        outcome: ReservedValue<RootRulePhaseOutcomeV1>,
        admission: &crate::world::incremental_action_admission::UnboundActionAdmissionV1<'_>,
        admission_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            AP,
            R,
            AE,
            AC,
        >,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        parent_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            PE,
            PC,
        >,
        work: NonmovementSelectedWorkV1<'_>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (
            ReservedValue<RootRulePhaseOutcomeV1>,
            CompletedNonmovementActionV1,
        ),
        NonmovementSelectedErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
    {
        let result = (|| {
            let effective = outcome
                .override_action_for_selected_v1(selected, self.ctx.transaction)
                .map_err(|error| {
                    NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                        RootModuleExecutionErrorV1::Body(error),
                    ))
                })?
                .unwrap_or(&selected.envelope().action);
            let (work, accepted_entries) = work
                .resolve_effective(&selected.envelope().action, effective)
                .ok_or_else(|| {
                    NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                            "effective action work slot unavailable",
                        )),
                    ))
                })?;
            let completed = match work {
                NonmovementSelectedWorkV1::DispatchBank(_) => return Err(
                    NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid("unresolved effective action work bank"))
                    ))),
                NonmovementSelectedWorkV1::NormalExtension(mut work) => {
                    CompletedNonmovementActionV1::NormalExtension(self.execute_normal_extension_after_rules_v1(
                        selected,admission,&outcome,materials,read,parent_read,&mut work,accepted_entries,audit_work,
                        directory,enumeration,tail_work,live_owner,
                    ).map_err(NonmovementSelectedErrorV1::Business)?)
                }
                NonmovementSelectedWorkV1::ModuleLifecycle(mut work,sources,shared,common) => {
                    CompletedNonmovementActionV1::ModuleLifecycle(self.execute_module_lifecycle_after_rules_v1(
                        selected,admission,&outcome,materials,read,parent_read,&mut work,sources,shared,common,
                        accepted_entries,audit_work,directory,enumeration,tail_work,live_owner,
                    ).map_err(NonmovementSelectedErrorV1::Business)?)
                }
                NonmovementSelectedWorkV1::FactorySchedule(work,sources,common)=>{
                    CompletedNonmovementActionV1::FactorySchedule(self.execute_factory_schedule_after_rules_v1(selected,admission,&outcome,materials,read,parent_read,work,sources,common,accepted_entries,audit_work,directory,enumeration,tail_work,live_owner).map_err(NonmovementSelectedErrorV1::Business)?)
                }
                NonmovementSelectedWorkV1::ContractLifecycle(work,sources,common)=>{
                    CompletedNonmovementActionV1::ContractLifecycle(self.execute_contract_after_rules_v1(selected,admission,&outcome,materials,read,parent_read,work,sources,common,accepted_entries,audit_work,directory,enumeration,tail_work,live_owner).map_err(NonmovementSelectedErrorV1::Business)?)
                }
                NonmovementSelectedWorkV1::Alliance(work,sources,common)=>{
                    CompletedNonmovementActionV1::Alliance(self.execute_alliance_after_rules_v1(
                        selected,admission,&outcome,materials,read,parent_read,work,sources,common,accepted_entries,audit_work,
                        directory,enumeration,tail_work,live_owner,
                    ).map_err(NonmovementSelectedErrorV1::Business)?)
                }
                NonmovementSelectedWorkV1::LogisticsRegistration(work,logistics,common)=>{
                    CompletedNonmovementActionV1::LogisticsRegistration(self.execute_logistics_registration_after_rules_v1(
                        selected,admission,&outcome,materials,read,parent_read,work,logistics,common,accepted_entries,audit_work,
                        directory,enumeration,tail_work,live_owner,
                    ).map_err(NonmovementSelectedErrorV1::Business)?)
                }
                NonmovementSelectedWorkV1::Core => {
                    CompletedNonmovementActionV1::Core(
                        self.execute_core_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            Some(parent_read),
                            accepted_entries,
                            policy_scan,
                            policy_work,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(|error| match error {
                            CoreDriverErrorV1::Module(error) => {
                                NonmovementSelectedErrorV1::Business(
                                    OrdinaryDriverErrorV1::Module(error),
                                )
                            }
                            CoreDriverErrorV1::Settlement(
                                crate::world::incremental_action_execution_core::CoreSettlementErrorV1::Body(error),
                            ) => NonmovementSelectedErrorV1::Business(
                                OrdinaryDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(error)),
                            ),
                            CoreDriverErrorV1::Settlement(
                                crate::world::incremental_action_execution_core::CoreSettlementErrorV1::Resources(error),
                            ) => NonmovementSelectedErrorV1::Business(
                                OrdinaryDriverErrorV1::Settlement(super::ordinary_settlement::OrdinarySettlementErrorV1::Resources(error)),
                            ),
                            CoreDriverErrorV1::SelectedRules(error) => {
                                NonmovementSelectedErrorV1::Rules(error)
                            }
                        })?,
                    )
                }
                NonmovementSelectedWorkV1::Crisis(work, sources) => {
                    CompletedNonmovementActionV1::Crisis(
                        self.execute_crisis_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::ContractAccept(work, sources) => {
                    CompletedNonmovementActionV1::ContractAccept(
                        self.execute_contract_accept_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::MetaProgress(work, sources) => {
                    CompletedNonmovementActionV1::MetaProgress(
                        self.execute_meta_progress_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::ProfileGovernance(work, sources) => {
                    CompletedNonmovementActionV1::ProfileGovernance(
                        self.execute_profile_governance_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::FactoryLifecycle(work, sources) => {
                    CompletedNonmovementActionV1::FactoryLifecycle(
                        self.execute_factory_lifecycle_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::RouteAvailability(work, logistics) => {
                    CompletedNonmovementActionV1::RouteAvailability(
                        self.execute_route_availability_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            logistics,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::MaterialTransfer(work, family) => {
                    CompletedNonmovementActionV1::MaterialTransfer(
                        self.execute_material_transfer_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            accepted_entries,
                            family,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::AuthenticatedCollectData(
                    data_work,
                    auth_work,
                    index,
                    controls,
                ) => {
                    let mut auth_read=oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                        root:self.families.roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),
                        state:admission_read.state,values:ValueReadContext {source:admission_read.values.source,
                            memory:admission_read.values.memory,class:admission_read.values.class,io:&mut *admission_read.values.io},
                        key_work:&mut *admission_read.key_work,semantic_bytes:admission_read.semantic_bytes,record_bytes:admission_read.record_bytes,
                        take_state_error:&mut admission_read.take_state_error,cancelled:&mut admission_read.cancelled};
                    CompletedNonmovementActionV1::CollectData(
                        self.execute_authenticated_collect_data_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            &mut auth_read,
                            auth_work,
                            index,
                            controls,
                            data_work,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }

                NonmovementSelectedWorkV1::DataAccess(work, controls) => {
                    CompletedNonmovementActionV1::DataAccess(
                        self.execute_data_access_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            controls,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::Body(work) => CompletedNonmovementActionV1::Body(
                    self.execute_body_after_rules_v1(
                        selected,
                        admission,
                        &outcome,
                        materials,
                        read,
                        parent_read,
                        work,
                        accepted_entries,
                        audit_work,
                        directory,
                        enumeration,
                        tail_work,
                        live_owner,
                    )
                    .map_err(NonmovementSelectedErrorV1::Business)?,
                ),
                NonmovementSelectedWorkV1::CollectData(work) => {
                    CompletedNonmovementActionV1::CollectData(
                        self.execute_collect_data_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::TransferResource(work) => {
                    CompletedNonmovementActionV1::TransferResource(
                        self.execute_transfer_resource_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
                NonmovementSelectedWorkV1::MainTokenTransfer(work, primitive, common) => {
                    CompletedNonmovementActionV1::MainTokenTransfer(
                        self.execute_main_token_transfer_after_rules_v1(
                            selected,
                            admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            primitive,
                            common,
                            accepted_entries,
                            audit_work,
                            directory,
                            enumeration,
                            tail_work,
                            live_owner,
                        )
                        .map_err(NonmovementSelectedErrorV1::Business)?,
                    )
                }
            };
            Ok((outcome, completed))
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
