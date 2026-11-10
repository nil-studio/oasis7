//! Selected nonmovement families: real Rule lane followed by the typed owner.
use super::*;
use super::{ordinary_settlement::OrdinaryDriverErrorV1, selected_rule::SelectedRuleLaneErrorV1};
use crate::world::{
    incremental_action_body::{BodyActionWorkV1, CompletedBodyActionV1},
    incremental_action_data::data_access::{CompletedDataAccessV1, DataAccessWorkV1},
    incremental_action_data::transfer_resource::{
        CompletedTransferResourceV1, TransferResourceWorkV1,
    },
    incremental_action_data::{CollectDataWorkV1, CompletedCollectDataV1},
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
};
pub(in crate::world) enum NonmovementSelectedWorkV1<'a> {
    Crisis(&'a mut crate::world::incremental_action_crisis::CrisisWorkV1, crate::world::incremental_action_crisis::CrisisSourcesV1<'a>),
    ContractAccept(&'a mut crate::world::incremental_action_contract_accept::ContractAcceptWorkV1, crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1<'a>),
    MetaProgress(&'a mut crate::world::incremental_action_meta_progress::MetaProgressWorkV1,
        crate::world::incremental_action_meta_progress::MetaProgressSourcesV1<'a>),
    FactoryLifecycle(&'a mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
        crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1<'a>),
    MaterialTransfer(&'a mut crate::world::incremental_material_normalization::transfer::MaterialTransferWorkV1,
        &'a ReservedValue<oasis7_world_state::incremental_material_records::MaterialFamilyQualificationV1>),
    RouteAvailability(&'a mut crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1,
        &'a ReservedValue<oasis7_world_state::incremental_logistics_records::LogisticsFamilyQualificationV1>),
    Body(&'a mut BodyActionWorkV1),
    DataAccess(&'a mut DataAccessWorkV1, &'a ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>),
    CollectData(&'a mut CollectDataWorkV1),
    AuthenticatedCollectData(&'a mut CollectDataWorkV1,
        &'a mut crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1,
        &'a ReservedValue<oasis7_world_state::incremental_world_native::StarterBindingIndexV1>,
        &'a ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>),
    TransferResource(&'a mut TransferResourceWorkV1),
}
pub(in crate::world) enum CompletedNonmovementActionV1 {
    Crisis(crate::world::incremental_action_crisis::CompletedCrisisV1),
    ContractAccept(crate::world::incremental_action_contract_accept::CompletedContractAcceptV1),
    MetaProgress(crate::world::incremental_action_meta_progress::CompletedMetaProgressV1),
    FactoryLifecycle(
        crate::world::incremental_action_factory_lifecycle::CompletedFactoryLifecycleV1,
    ),
    MaterialTransfer(
        crate::world::incremental_material_normalization::transfer::CompletedMaterialTransferV1,
    ),
    RouteAvailability(
        crate::world::incremental_action_route_availability::CompletedRouteAvailabilityV1,
    ),
    Body(CompletedBodyActionV1),
    DataAccess(CompletedDataAccessV1),
    CollectData(CompletedCollectDataV1),
    TransferResource(CompletedTransferResourceV1),
}
#[derive(Debug)]
pub(in crate::world) enum NonmovementSelectedErrorV1<OE> {
    Rules(SelectedRuleLaneErrorV1<OE>),
    Business(OrdinaryDriverErrorV1<OE>),
}
#[cfg(test)]
impl<OE> NonmovementSelectedErrorV1<OE> {
    /// Observe the actual nested cause without replacing the production error.
    pub(in crate::world) fn body_error_for_fixture(&self) -> Option<&BodyMutationError> {
        match self {
            Self::Business(OrdinaryDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                error,
            ))) => Some(error),
            _ => None,
        }
    }
}
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
    /// Run genuine admission and Rule here, then re-read Current admission
    /// after Rule/Modify before invoking the selected typed business owner.
    /// This entry consumes only the named typed families. It does not mint a
    /// completed tick or handle protected governance, safe alternatives or
    /// economy-dependent families on their behalf.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_selected_nonmovement_v1<P, ER, CR, PE, PC, AP, AE, AC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
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
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
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
            // Check the original family before Rule can write. The afterRules
            // owners independently authenticate the final family and receipt.
            let compatible = match &work {
                NonmovementSelectedWorkV1::Crisis(..) => matches!(
                    selected.envelope().action,
                    crate::Action::ResolveCrisis { .. }
                ),
                NonmovementSelectedWorkV1::ContractAccept(..) => matches!(
                    selected.envelope().action,
                    crate::Action::AcceptEconomicContract { .. }
                ),
                NonmovementSelectedWorkV1::MetaProgress(..) => matches!(
                    selected.envelope().action,
                    crate::Action::GrantMetaProgress { .. }
                ),
                NonmovementSelectedWorkV1::FactoryLifecycle(..) => matches!(
                    selected.envelope().action,
                    crate::Action::MaintainFactory { .. } | crate::Action::RecycleFactory { .. }
                ),
                NonmovementSelectedWorkV1::RouteAvailability(..) => matches!(
                    selected.envelope().action,
                    crate::Action::SetLogisticsRouteAvailability { .. }
                ),
                NonmovementSelectedWorkV1::MaterialTransfer(..) => matches!(
                    selected.envelope().action,
                    crate::Action::TransferMaterial { .. }
                ),
                NonmovementSelectedWorkV1::DataAccess(..) => matches!(
                    selected.envelope().action,
                    crate::Action::GrantDataAccess { .. } | crate::Action::RevokeDataAccess { .. }
                ),
                NonmovementSelectedWorkV1::Body(_) => matches!(
                    selected.envelope().action,
                    crate::Action::EmitBodyAttributes { .. }
                        | crate::Action::QueryObservation { .. }
                        | crate::Action::BodyAction { .. }
                ),
                NonmovementSelectedWorkV1::AuthenticatedCollectData(..) => matches!(
                    selected.envelope().action,
                    crate::Action::CollectDataAuthenticated { .. }
                ),
                NonmovementSelectedWorkV1::CollectData(_) => matches!(
                    selected.envelope().action,
                    crate::Action::CollectData { .. }
                ),
                NonmovementSelectedWorkV1::TransferResource(_) => matches!(
                    selected.envelope().action,
                    crate::Action::TransferResource { .. }
                        | crate::Action::EmitResourceTransfer { .. }
                ),
            };
            if !compatible {
                return Err(NonmovementSelectedErrorV1::Business(
                    OrdinaryDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                        BodyMutationError::Invalid("selected nonmovement original family"),
                    )),
                ));
            }
            let (outcome, admission) = self
                .prepare_selected_rule_lane_v1(
                    selected,
                    qualification,
                    current,
                    admission_read,
                    policy_scan,
                    policy_work,
                    audit_work,
                    directory,
                    rule_work,
                    live_owner,
                )
                .map_err(NonmovementSelectedErrorV1::Rules)?;
            let completed = match work {
                NonmovementSelectedWorkV1::Crisis(work, sources) => {
                    CompletedNonmovementActionV1::Crisis(
                        self.execute_crisis_after_rules_v1(
                            selected,
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            &sources,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            logistics,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            &mut auth_read,
                            auth_work,
                            index,
                            controls,
                            data_work,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
                            controls,
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
                        &admission,
                        &outcome,
                        materials,
                        read,
                        parent_read,
                        work,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
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
                            &admission,
                            &outcome,
                            materials,
                            read,
                            parent_read,
                            work,
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
    /// Consume at most one actual FIFO input and finish its named typed lane.
    /// Queue exhaustion yields None; another family fails this transaction and
    /// cannot be silently skipped. This is not a complete tick executor.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_next_nonmovement_v1<P, ER, CR, PE, PC, AP, AE, AC>(
        &mut self,
        prologue: &crate::world::incremental_factory_phase::FactoryPhaseOutcomeV1,
        pending: &mut ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
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
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        Option<(
            ReservedValue<RootRulePhaseOutcomeV1>,
            CompletedNonmovementActionV1,
        )>,
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
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let result = (|| {
            let live = live_owner();
            prologue
                .validate_for_queue(
                    self.ctx.transaction,
                    self.events.world_events,
                    self.now,
                    self.ctx.values.memory,
                    &live,
                )
                .map_err(|error| {
                    NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                        RootModuleExecutionErrorV1::Body(error),
                    ))
                })?;
            let guard = self.ctx.transaction.guard();
            let step = crate::world::incremental_step::IwsStepContext::borrow_counted_at(
                self.ctx.transaction,
                self.now,
                &guard,
                &live,
                self.events.values,
                &mut self.events.mutation,
                &mut self.events.original_work,
                self.events.world_events,
            )
            .map_err(|error| {
                NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(error),
                ))
            })?;
            drop(step);
            self.take_next_nonmovement_inner_v1(
                pending,
                pending_work,
                qualification,
                current,
                admission_read,
                policy_scan,
                policy_work,
                materials,
                read,
                parent_read,
                work,
                audit_work,
                directory,
                rule_work,
                enumeration,
                tail_work,
                live_owner,
            )
        })();
        if result.is_err() {
            let _ = self
                .ctx
                .transaction
                .contribute::<(), oasis7_formal_protocol::incremental_execution::ExecutionError>(
                |_| Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted),
            );
        }
        result
    }

    /// Component fixtures exercise the same queue kernel without claiming a
    /// completed production prologue. Production has no bypass entrypoint.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_next_nonmovement_for_component_v1<
        P,
        ER,
        CR,
        PE,
        PC,
        AP,
        AE,
        AC,
    >(
        &mut self,
        pending: &mut ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
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
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        Option<(
            ReservedValue<RootRulePhaseOutcomeV1>,
            CompletedNonmovementActionV1,
        )>,
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
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        self.take_next_nonmovement_inner_v1(
            pending,
            pending_work,
            qualification,
            current,
            admission_read,
            policy_scan,
            policy_work,
            materials,
            read,
            parent_read,
            work,
            audit_work,
            directory,
            rule_work,
            enumeration,
            tail_work,
            live_owner,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn take_next_nonmovement_inner_v1<P, ER, CR, PE, PC, AP, AE, AC>(
        &mut self,
        pending: &mut ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
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
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        Option<(
            ReservedValue<RootRulePhaseOutcomeV1>,
            CompletedNonmovementActionV1,
        )>,
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
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let result = (|| {
            let live = live_owner();
            // The same authenticated queue owner verifies the exact original,
            // removes the FIFO head and appends its Input fact in this sole tx.
            // Parent read metering is shared with the subsequent business phase.
            let selected = crate::world::incremental_action_execution::take_pending_action(
                pending,
                self.ctx.transaction,
                parent_read,
                pending_work,
                self.originals,
                &live,
            )
            .map_err(|error| {
                NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(error),
                ))
            })?;
            let Some(selected) = selected else {
                // An empty index performs no traversal, so peek cannot poll
                // cancellation for us. Exhaustion is still a live boundary.
                let cancelled = (parent_read.cancelled)()
                    || (self.ctx.cancelled)()
                    || (self.events.mutation.cancelled)();
                let source_error =
                    (parent_read.take_state_error)().or_else(|| (self.ctx.take_state_error)());
                let value_error = if cancelled {
                    Some(ValueTreeError::Cancelled)
                } else {
                    source_error
                };
                if let Some(error) = value_error {
                    return Err(NonmovementSelectedErrorV1::Business(
                        OrdinaryDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                            BodyMutationError::Value(error),
                        )),
                    ));
                }
                let guard = self.ctx.transaction.guard();
                self.ctx
                    .transaction
                    .validate_guard(&guard, &live_owner())
                    .map_err(RootModuleExecutionErrorV1::from)
                    .map_err(|error| {
                        NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(error))
                    })?;
                return Ok(None);
            };
            self.execute_selected_nonmovement_v1(
                &selected,
                qualification,
                current,
                admission_read,
                policy_scan,
                policy_work,
                materials,
                read,
                parent_read,
                work,
                audit_work,
                directory,
                rule_work,
                enumeration,
                tail_work,
                live_owner,
            )
            .map(Some)
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
