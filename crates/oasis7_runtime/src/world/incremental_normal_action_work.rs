//! Original normal-tick work is reborrowed for every selected action.
//! The same counter values continue into the tail; no bank refills an allowance.
use crate::world::incremental_module_invocation::EffectiveActionWorkBankV1;
use oasis7_proto::memory_budget::ReservedValue;
/// Explicit original allowances; `view` only reborrows the remaining values.
pub(in crate::world) struct NormalModuleLifecycleWorkSetV1 {
    pub controls: oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsWorkV1,
    pub capability_authorization: crate::world::incremental_authorization_root_current::AuthorizationRootSourceReadWorkV1,
    pub config_write: oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigWriteWorkV1,
    pub proposal_read: oasis7_world_state::incremental_runtime_proposal::RuntimeProposalReadWorkV1,
    pub proposal_write:
        oasis7_world_state::incremental_runtime_proposal::RuntimeProposalWriteWorkV1,
    pub proposal_scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub governance: oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceWorkV1,
    pub artifacts:
        oasis7_world_state::incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1,
    pub content:
        oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1,
    pub marketplace:
        oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFamilyWorkV1,
    pub marketplace_scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub instance_scan: oasis7_formal_protocol::incremental_state::StateScanBudget,
    pub instance_cells: u64,
    pub pending_entries: usize,
    pub max_selected_heap: usize,
    pub max_original_bytes: usize,
}
impl NormalModuleLifecycleWorkSetV1 {
    pub(in crate::world) fn view(
        &mut self,
    ) -> crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1<'_> {
        crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleWorkV1 {
            controls: &mut self.controls,
            capability_authorization: &mut self.capability_authorization,
            config_write: &mut self.config_write,
            proposal_read: &mut self.proposal_read,
            proposal_write: &mut self.proposal_write,
            proposal_scan: &mut self.proposal_scan,
            governance: &mut self.governance,
            artifacts: &mut self.artifacts,
            content: &mut self.content,
            marketplace: &mut self.marketplace,
            marketplace_scan: &mut self.marketplace_scan,
            instance_scan: &mut self.instance_scan,
            instance_cells: &mut self.instance_cells,
            pending_entries: &mut self.pending_entries,
            max_selected_heap: self.max_selected_heap,
            max_original_bytes: self.max_original_bytes,
        }
    }
}
pub(in crate::world) struct NormalActionWorkSetV1<'common> {
    pub module_lifecycle: NormalModuleLifecycleWorkSetV1,
    pub crisis: crate::world::incremental_action_crisis::CrisisWorkV1,
    pub contract_accept: crate::world::incremental_action_contract_accept::ContractAcceptWorkV1,
    pub meta_progress: crate::world::incremental_action_meta_progress::MetaProgressWorkV1,
    pub factory_lifecycle:
        crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
    pub route_availability:
        crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1,
    pub material_transfer:
        crate::world::incremental_material_normalization::transfer::MaterialTransferWorkV1,
    pub body: crate::world::incremental_action_body::BodyActionWorkV1,
    pub data_access: crate::world::incremental_action_data::data_access::DataAccessWorkV1,
    pub collect_data: crate::world::incremental_action_data::CollectDataWorkV1,
    pub authenticated_data: crate::world::incremental_action_data::CollectDataWorkV1,
    pub authenticated_work:
        crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1,
    pub transfer_resource:
        crate::world::incremental_action_data::transfer_resource::TransferResourceWorkV1,
    pub factory_schedule:
        crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
    pub contracts: crate::world::incremental_normal_action_diplomacy::contracts::ContractWorkV1,
    pub alliance: crate::world::incremental_normal_action_diplomacy::AllianceWorkV1,
    pub logistics_registration:
        crate::world::incremental_normal_action_logistics::LogisticsRegistrationWorkV1,
    pub profile: crate::world::incremental_normal_action_profile::ProfileGovernanceWorkV1,
    pub extension: crate::world::incremental_module_invocation::NormalCoreExtensionWorkV1<'common>,
}
pub(in crate::world) struct NormalActionSourcesV1<'p> {
    pub module_lifecycle: &'p crate::world::incremental_normal_action_module_lifecycle::ModuleLifecycleSourcesV1<'p>,
    pub crisis_sources: &'p crate::world::incremental_action_crisis::CrisisSourcesV1<'p>,
    pub contract_sources: &'p crate::world::incremental_action_contract_accept::ContractAcceptSourcesV1<'p>,
    pub meta_sources: &'p crate::world::incremental_action_meta_progress::MetaProgressSourcesV1<'p>,
    pub factory_sources: &'p crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1<'p>,
    pub logistics: &'p ReservedValue<
            oasis7_world_state::incremental_logistics_records::LogisticsFamilyQualificationV1,
        >,
    pub materials: &'p ReservedValue<
            oasis7_world_state::incremental_material_records::MaterialFamilyQualificationV1,
        >,
    pub controls: &'p ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
    pub starter_bindings: &'p ReservedValue<
            oasis7_world_state::incremental_world_native::StarterBindingIndexV1,
        >,
    pub factory_schedule: &'p crate::world::incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1<'p>,
    pub contracts: &'p crate::world::incremental_normal_action_diplomacy::contracts::ContractSourcesV1<'p,'p>,
    pub alliance: &'p crate::world::incremental_normal_action_diplomacy::AllianceSourcesV1<'p,'p>,
    pub profile: &'p crate::world::incremental_normal_action_profile::ProfileGovernanceSourcesV1<'p>,
}
impl NormalActionWorkSetV1<'_> {
    pub(in crate::world) fn common_work(
        &self,
    ) -> &oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1
    {
        self.extension.common
    }
    pub(in crate::world) fn bank<'borrow>(
        &'borrow mut self,
        sources: &'borrow NormalActionSourcesV1<'_>,
    ) -> EffectiveActionWorkBankV1<'borrow> {
        EffectiveActionWorkBankV1::new(
            &mut self.crisis,
            sources.crisis_sources,
            &mut self.contract_accept,
            sources.contract_sources,
            &mut self.meta_progress,
            sources.meta_sources,
            &mut self.factory_lifecycle,
            sources.factory_sources,
            &mut self.route_availability,
            sources.logistics,
            &mut self.material_transfer,
            sources.materials,
            &mut self.body,
            &mut self.data_access,
            sources.controls,
            &mut self.collect_data,
            &mut self.authenticated_data,
            &mut self.authenticated_work,
            sources.starter_bindings,
            &mut self.transfer_resource,
        )
        .with_normal_extension_view(self.extension.view())
        .with_factory_schedule(&mut self.factory_schedule, sources.factory_schedule)
        .with_contract_lifecycle(&mut self.contracts, sources.contracts)
        .with_alliance(&mut self.alliance, sources.alliance)
        .with_logistics_registration(&mut self.logistics_registration)
        .with_profile_governance(&mut self.profile, sources.profile)
        .with_module_lifecycle(self.module_lifecycle.view(), sources.module_lifecycle)
    }
}
