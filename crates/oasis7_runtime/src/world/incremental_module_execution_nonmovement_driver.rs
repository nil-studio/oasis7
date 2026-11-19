//! Selected nonmovement families: real Rule lane followed by the typed owner.
#[path = "incremental_module_execution_effective_dispatch.rs"]
mod effective_dispatch;
use super::CoreDriverErrorV1;
use super::normal_extension_driver;
use super::*;
use super::{ordinary_settlement::OrdinaryDriverErrorV1, selected_rule::SelectedRuleLaneErrorV1};
use crate::world::{
    incremental_action_data::main_token_transfer::{CompletedMainTokenTransferV1, MainTokenTransferWorkV1},
    incremental_action_body::{BodyActionWorkV1, CompletedBodyActionV1},
    incremental_action_data::data_access::{CompletedDataAccessV1, DataAccessWorkV1},
    incremental_action_data::transfer_resource::{
        CompletedTransferResourceV1, TransferResourceWorkV1,
    },
    incremental_action_data::{CollectDataWorkV1, CompletedCollectDataV1},
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
};
pub(in crate::world) use effective_dispatch::{EffectiveActionFamilyV1, EffectiveActionWorkBankV1};
pub(in crate::world) enum NonmovementSelectedWorkV1<'a> {
    ModuleLifecycle(
        crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1<'a>,
        &'a crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleSourcesV1<'a>,
        &'a mut crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
        &'a oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1,
    ),
    FactorySchedule(&'a mut crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,&'a crate::world::incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1<'a>,&'a oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1),
    ContractLifecycle(&'a mut crate::world::incremental_normal_action_diplomacy::contracts::ContractWorkV1,&'a crate::world::incremental_normal_action_diplomacy::contracts::ContractSourcesV1<'a,'a>,&'a oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1),
    Alliance(&'a mut crate::world::incremental_normal_action_diplomacy::AllianceWorkV1,&'a crate::world::incremental_normal_action_diplomacy::AllianceSourcesV1<'a,'a>,&'a oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1),
    LogisticsRegistration(&'a mut crate::world::incremental_normal_action_logistics::LogisticsRegistrationWorkV1,
        &'a ReservedValue<oasis7_world_state::incremental_logistics_records::LogisticsFamilyQualificationV1>,
        &'a oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1),
    NormalExtension(normal_extension_driver::NormalCoreExtensionWorkViewV1<'a>),
    Core,
    Crisis(&'a mut crate::world::incremental_action_crisis::CrisisWorkV1, crate::world::incremental_action_crisis::CrisisSourcesV1<'a>),
    ContractAccept(&'a mut crate::world::incremental_action_contract_accept::ContractAcceptWorkV1, crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1<'a>),
    MetaProgress(&'a mut crate::world::incremental_action_meta_progress::MetaProgressWorkV1,
        crate::world::incremental_action_meta_progress::MetaProgressSourcesV1<'a>),
    ProfileGovernance(&'a mut crate::world::incremental_normal_action_profile::ProfileGovernanceWorkV1,crate::world::incremental_normal_action_profile::ProfileGovernanceSourcesV1<'a>),
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
    MainTokenTransfer(&'a mut MainTokenTransferWorkV1, &'a ReservedValue<oasis7_world_state::incremental_owner_closure::WorldPrimitiveQualificationV1>, &'a oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1),
    DispatchBank(&'a mut EffectiveActionWorkBankV1<'a>),
}
pub(in crate::world) enum CompletedNonmovementActionV1 {
    ModuleLifecycle(crate::world::incremental_normal_action_module_lifecycle::CompletedModuleLifecycleActionV1),
    FactorySchedule(
        crate::world::incremental_normal_action_factory_schedule::CompletedFactoryScheduleActionV1,
    ),
    ContractLifecycle(crate::world::incremental_normal_action_diplomacy::CompletedAllianceActionV1),
    Alliance(crate::world::incremental_normal_action_diplomacy::CompletedAllianceActionV1),
    LogisticsRegistration(
        crate::world::incremental_normal_action_logistics::CompletedLogisticsRegistrationV1,
    ),
    NormalExtension(normal_extension_driver::CompletedNormalExtensionV1),
    Core(crate::world::incremental_action_execution_core::CompletedCoreActionV1),
    Crisis(crate::world::incremental_action_crisis::CompletedCrisisV1),
    ContractAccept(crate::world::incremental_action_contract_accept::CompletedContractAcceptV1),
    MetaProgress(crate::world::incremental_action_meta_progress::CompletedMetaProgressV1),
    ProfileGovernance(
        crate::world::incremental_normal_action_profile::CompletedProfileGovernanceV1,
    ),
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
    MainTokenTransfer(CompletedMainTokenTransferV1),
}

impl<'a> NonmovementSelectedWorkV1<'a> {
    fn family(&self) -> Option<EffectiveActionFamilyV1> {
        Some(match self {
            Self::Core => EffectiveActionFamilyV1::Core,
            Self::FactorySchedule(..) => EffectiveActionFamilyV1::FactorySchedule,
            Self::ModuleLifecycle(..) => EffectiveActionFamilyV1::ModuleLifecycle,
            Self::ContractLifecycle(..) => EffectiveActionFamilyV1::ContractLifecycle,
            Self::Alliance(..) => EffectiveActionFamilyV1::Alliance,
            Self::LogisticsRegistration(..) => EffectiveActionFamilyV1::LogisticsRegistration,
            Self::NormalExtension(..) => EffectiveActionFamilyV1::NormalExtension,
            Self::Crisis(..) => EffectiveActionFamilyV1::Crisis,
            Self::ContractAccept(..) => EffectiveActionFamilyV1::ContractAccept,
            Self::MetaProgress(..) => EffectiveActionFamilyV1::MetaProgress,
            Self::ProfileGovernance(..) => EffectiveActionFamilyV1::ProfileGovernance,
            Self::FactoryLifecycle(..) => EffectiveActionFamilyV1::FactoryLifecycle,
            Self::RouteAvailability(..) => EffectiveActionFamilyV1::RouteAvailability,
            Self::MaterialTransfer(..) => EffectiveActionFamilyV1::MaterialTransfer,
            Self::Body(..) => EffectiveActionFamilyV1::Body,
            Self::DataAccess(..) => EffectiveActionFamilyV1::DataAccess,
            Self::CollectData(..) => EffectiveActionFamilyV1::CollectData,
            Self::AuthenticatedCollectData(..) => EffectiveActionFamilyV1::AuthenticatedCollectData,
            Self::TransferResource(..) => EffectiveActionFamilyV1::TransferResource,
            Self::MainTokenTransfer(..) => EffectiveActionFamilyV1::MainTokenTransfer,
            Self::DispatchBank(..) => return None,
        })
    }

    pub(super) fn factory_module_parts_v1(
        &mut self,
    ) -> Option<(
        &mut crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
        &crate::world::incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1<'a>,
        &oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1,
    )> {
        match self {
            Self::FactorySchedule(work, sources, common) => Some((&mut **work, *sources, *common)),
            Self::DispatchBank(bank) => bank.factory_module_parts_v1(),
            _ => None,
        }
    }

    fn resolve_effective(
        self,
        original: &crate::Action,
        action: &crate::Action,
    ) -> Option<(Self, Option<&'a mut usize>)> {
        let family = EffectiveActionFamilyV1::for_action(action)?;
        match self {
            Self::DispatchBank(bank) => bank.select_for_original(original, action),
            work if work.family() == Some(family) => Some((work, None)),
            _ => None,
        }
    }
}
#[derive(Debug)]
pub(in crate::world) enum NonmovementSelectedErrorV1<OE> {
    Rules(SelectedRuleLaneErrorV1<OE>),
    Business(OrdinaryDriverErrorV1<OE>),
}
#[cfg(any(test, all(feature = "test_tier_required", feature = "test-support")))]
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
            let original_family = EffectiveActionFamilyV1::for_action(&selected.envelope().action);
            let compatible = original_family.is_some()
                && (matches!(&work, NonmovementSelectedWorkV1::DispatchBank(_))
                    || work.family() == original_family);
            if !compatible {
                return Err(NonmovementSelectedErrorV1::Business(
                    OrdinaryDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                        BodyMutationError::Invalid("selected original family unsupported"),
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
            self.execute_nonmovement_after_rules_v1(
                selected,
                outcome,
                &admission,
                admission_read,
                policy_scan,
                policy_work,
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
    #[cfg(any(test, all(feature = "test_tier_required", feature = "test-support")))]
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
            let Some(selected) =
                self.take_nonmovement_input_v1(pending, pending_work, parent_read, live_owner)?
            else {
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
    pub(super) fn take_nonmovement_input_v1<P, PE, PC>(
        &mut self,
        pending: &mut ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        pending_work:&mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
        parent_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            PE,
            PC,
        >,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<SelectedPendingActionV1>, NonmovementSelectedErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
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
            Ok(Some(selected))
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

#[path = "incremental_module_execution_nonmovement_after_rules.rs"]
mod after_rules;
