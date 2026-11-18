//! Finite, explicit assembly of existing work lanes. These limits are local
//! ceilings only; they do not copy, reserve or replace the original Common
//! balance. Every real producer retains its existing Common admission.
use super::*;
use crate::world::incremental_action_body::BodyActionWorkV1;
use crate::world::incremental_action_contract_accept::ContractAcceptWorkV1;
use crate::world::incremental_action_crisis::CrisisWorkV1;
use crate::world::incremental_action_data::CollectDataWorkV1;
use crate::world::incremental_action_data::auth_current::CollectDataAuthorizationWorkV1;
use crate::world::incremental_action_data::auth_signature::CollectDataSignatureWorkV1;
use crate::world::incremental_action_data::data_access::DataAccessWorkV1;
use crate::world::incremental_action_data::main_token_transfer::MainTokenTransferWorkV1;
use crate::world::incremental_action_data::transfer_resource::TransferResourceWorkV1;
use crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1;
use crate::world::incremental_action_meta_progress::MetaProgressWorkV1;
use crate::world::incremental_action_route_availability::RouteAvailabilityWorkV1;
use crate::world::incremental_authorization_root_current::AuthorizationRootSourceReadWorkV1;
use crate::world::incremental_factory_depreciation::FactoryDepreciationWorkV1;
use crate::world::incremental_factory_depreciation::reduce::FactoryDepreciationReduceWorkV1;
use crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryWorkV1;
use crate::world::incremental_factory_directory_records::FactoryRecipeDirectoryWorkV1;
use crate::world::incremental_factory_phase::FactoryPhaseWorkV1;
use crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1;
use crate::world::incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1;
use crate::world::incremental_material_normalization::transfer::MaterialTransferWorkV1;
use crate::world::incremental_module_execution_authorization::RootModuleAuthorizationLimitsV1;
use crate::world::incremental_module_execution_effects::RootModuleEffectWorkV1;
use crate::world::incremental_module_execution_manifest_hash::RootWorldManifestHashWorkV1;
use crate::world::incremental_module_execution_output::RootModuleOutputWorkV1;
use crate::world::incremental_module_execution_visual_decode::RootModuleVisualDecodeWorkV1;
use crate::world::incremental_module_invocation::CurrentInvocationEnumerationWorkV1;
use crate::world::incremental_module_invocation::ModuleInvocationPreparationLimitsV1;
use crate::world::incremental_module_invocation::NormalCoreExtensionWorkV1;
use crate::world::incremental_module_invocation::RootModuleExecutionWorkV1;
use crate::world::incremental_module_invocation::RootRulePhaseWorkV1;
use crate::world::incremental_normal_action_diplomacy::AllianceWorkV1;
use crate::world::incremental_normal_action_diplomacy::contracts::ContractWorkV1;
use crate::world::incremental_normal_action_factory_schedule::FactoryScheduleWorkV1;
use crate::world::incremental_normal_action_logistics::LogisticsRegistrationWorkV1;
use crate::world::incremental_normal_action_profile::ProfileGovernanceWorkV1;
use crate::world::incremental_normal_action_work::NormalActionWorkSetV1;
use crate::world::incremental_normal_action_work::NormalModuleLifecycleWorkSetV1;
use crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityWorkV1;
use crate::world::incremental_normal_business_commitment::NormalBusinessReadWorkV1;
use crate::world::incremental_normal_business_commitment::NormalTerminalMetadataReadWorkV1;
use crate::world::incremental_normal_cognition_expiry::CurrentCognitionExpiryWorkV1;
use crate::world::incremental_normal_final_sources::NormalFinalSourcesWorkV1;
use crate::world::incremental_normal_gameplay_cycles::NormalGameplayCycleWorkV1;
use crate::world::incremental_normal_host_finish::NormalHostFinishCutWorkV1;
use crate::world::incremental_normal_host_work::NormalHostBootstrapWorkV1;
use crate::world::incremental_normal_host_work::NormalHostDriverWorkV1;
use crate::world::incremental_normal_host_work::NormalHostInvocationWorkV1;
use crate::world::incremental_normal_host_work::NormalHostIoWorkV1;
use crate::world::incremental_normal_host_work::NormalHostMaterialWorkV1;
use crate::world::incremental_normal_host_work::NormalHostReadWorkV1;
use crate::world::incremental_normal_host_work::NormalHostWorkV1;
use crate::world::incremental_normal_host_work::NormalHostWriteWorkV1;
use crate::world::incremental_normal_industry_due::delivery::ProductValidationDeliveryWorkV1;
use crate::world::incremental_normal_organization_tail::NormalOrganizationTailWorkV1;
use crate::world::incremental_normal_projection_commitment::NormalProjectionWorkBankV1;
use crate::world::incremental_normal_projection_commitment_world_rules::NormalProjectionWorldRulesWorkV1;
use crate::world::incremental_normal_tick_driver::NormalTickTailWorkSetV1;
use crate::world::incremental_normal_world_rule_lifecycle::delivery::IndustrialDeliveryWorkV1;
use crate::world::incremental_scheduler::DueModuleBatchLimitsV1;
use crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use oasis7_formal_protocol::incremental_material::FactBudget;
use oasis7_formal_protocol::incremental_schedule::ScheduleScanBudget;
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_formal_protocol::incremental_value::ValueIoBudget;
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_wasm_router::bounded::RouterWorkV1;
use oasis7_world_state::incremental_agent_authority_records::AgentAuthorityWorkBudgetV1;
use oasis7_world_state::incremental_cognition_groups::CognitionGroupWriteWorkV1;
use oasis7_world_state::incremental_cognition_history::CognitionHistoryIndexWorkV1;
use oasis7_world_state::incremental_cognition_history_cbor::CognitionHistoryCborWorkV1;
use oasis7_world_state::incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1;
use oasis7_world_state::incremental_diplomacy::DiplomacyIndexWorkV1;
use oasis7_world_state::incremental_factory_lifecycle_index::FactoryLifecycleIndexWorkV1;
use oasis7_world_state::incremental_factory_records::FactoryWorkBudgetV1;
use oasis7_world_state::incremental_factory_schedule_authority::FactoryScheduleAuthorityWorkV1;
use oasis7_world_state::incremental_factory_schedule_factory::FactoryScheduleFactoryWorkV1;
use oasis7_world_state::incremental_factory_schedule_index::FactoryScheduleIndexWorkV1;
use oasis7_world_state::incremental_factory_schedule_logistics::FactoryScheduleLogisticsWorkV1;
use oasis7_world_state::incremental_factory_schedule_receipts::FactoryScheduleReceiptWorkV1;
use oasis7_world_state::incremental_field_member::MemberRecordLimitsV1;
use oasis7_world_state::incremental_industry_jobs::IndustryWorkBudgetV1;
use oasis7_world_state::incremental_inventory::AgentLedgerTransferWorkV1;
use oasis7_world_state::incremental_inventory::FactoryOutputLedgerGroupWorkV1;
use oasis7_world_state::incremental_inventory::InventoryFamilyWorkV1;
use oasis7_world_state::incremental_inventory::InventoryIdentityDirectoryWorkV1;
use oasis7_world_state::incremental_inventory::InventoryMaterialScopeWorkV1;
use oasis7_world_state::incremental_inventory::InventoryWorldGroupWorkV1;
use oasis7_world_state::incremental_json_records::JsonReadWorkV1;
use oasis7_world_state::incremental_json_records::JsonScalarChangeWorkV1;
use oasis7_world_state::incremental_json_records::JsonScalarPathReadWorkV1;
use oasis7_world_state::incremental_json_records::JsonWriteWorkV1;
use oasis7_world_state::incremental_logistics_records::LogisticsWorkBudgetV1;
use oasis7_world_state::incremental_material_records::MaterialProfileCurrentReadWorkV1;
use oasis7_world_state::incremental_material_records::MaterialWorkBudgetV1;
use oasis7_world_state::incremental_module_marketplace::ModuleMarketplaceFamilyWorkV1;
use oasis7_world_state::incremental_module_marketplace::ModuleVisualOverlayWorkV1;
use oasis7_world_state::incremental_module_state_recovery::ModuleStateContentBudgetV1;
use oasis7_world_state::incremental_normal_gameplay_cycles_directory::GameplayDirectoryWorkV1;
use oasis7_world_state::incremental_owner_closure::WorldPrimitiveFamilyWorkV1;
use oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1;
use oasis7_world_state::incremental_pending_action_family::PendingActionIndexWorkV1;
use oasis7_world_state::incremental_pending_action_original_binding::PendingActionOriginalWorkV1;
use oasis7_world_state::incremental_rule_decision_decode::RuleDecisionPayloadDecodeWorkV1;
use oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigReadWorkV1;
use oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigWriteWorkV1;
use oasis7_world_state::incremental_runtime_artifact_config::SelectedModuleManifestLimitsV1;
use oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkV1;
use oasis7_world_state::incremental_runtime_artifact_resources::RuntimeArtifactResourcesWorkV1;
use oasis7_world_state::incremental_runtime_authorization_genesis::AuthorizationWriteWorkV1;
use oasis7_world_state::incremental_runtime_business::ThreatKeyDirectoryWorkV1;
use oasis7_world_state::incremental_runtime_capability_records::RuntimeCapabilityRecordsWorkV1;
use oasis7_world_state::incremental_runtime_effect_records::RuntimeEffectRecordsWorkV1;
use oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceWorkV1;
use oasis7_world_state::incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1;
use oasis7_world_state::incremental_runtime_history_index::RuntimeHistoryIndexWorkV1;
use oasis7_world_state::incremental_runtime_module_authorization::ModuleAuthorizationReadWorkV1;
use oasis7_world_state::incremental_runtime_proposal::RuntimeProposalReadWorkV1;
use oasis7_world_state::incremental_runtime_proposal::RuntimeProposalWriteWorkV1;
use oasis7_world_state::incremental_runtime_revocation::BusinessDiffProcessingWorkV1;
use oasis7_world_state::incremental_semantic_key::SemanticKeyBudget;
use oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsWorkV1;
use oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1;
use oasis7_world_state::incremental_world_native::IndustrialClaimDirectoryWorkV1;
use oasis7_world_state::incremental_world_native::MetaProgressMutationWorkV1;
use oasis7_world_state::incremental_world_native::StarterBindingIndexWorkV1;
use oasis7_world_state::incremental_world_native::WorldNativeQualificationWorkV1 as WorldNativeFieldReadWorkV1;
use oasis7_world_state::incremental_world_rule_composite::WorldRuleCompositeReadWorkV1 as TypedRecordReadWorkV1;
use oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1;
use oasis7_world_state::incremental_world_rule_effects::WorldRuleRemainderWorkV1;
use oasis7_world_state::incremental_world_rules_family::WorldRulesFamilyWorkV1;
use oasis7_world_state::incremental_world_state_genesis::TypedRecordWriteWorkV1;

/// Independent limits for one named Host work group. No default is provided.
/// Counters are local ceilings, never separate spendable copies of Common.
/// Each configured group can preserve its own narrower original limits.
#[derive(Clone, Copy, Debug)]
pub struct NormalHostLaneLimitsV1 {
    pub steps_per_counter: u64,
    pub bytes_per_counter: u64,
    pub entries_per_counter: usize,
    pub max_collection_entries: usize,
    pub max_pending_records: usize,
    pub max_text_bytes: usize,
    pub max_key_bytes: usize,
    pub max_scalar_bytes: usize,
    pub max_payload_bytes: usize,
    pub max_record_bytes: usize,
    pub max_original_bytes: usize,
    pub max_heap_bytes: usize,
    pub max_pending_node_bytes: usize,
    pub max_depth: usize,
}
impl NormalHostLaneLimitsV1 {
    fn validate(&self) -> Result<(), BodyMutationError> {
        // Existing native/claim directories admit at most 256 pending records.
        // A collection limit cannot silently widen that separate bound.
        if self.max_pending_records == 0 || self.max_pending_records > 256 {
            return Err(BodyMutationError::Invalid(
                "normal Host pending record bound",
            ));
        }
        if self.steps_per_counter == u64::MAX
            || self.bytes_per_counter == u64::MAX
            || [
                self.entries_per_counter,
                self.max_collection_entries,
                self.max_text_bytes,
                self.max_key_bytes,
                self.max_scalar_bytes,
                self.max_payload_bytes,
                self.max_record_bytes,
                self.max_original_bytes,
                self.max_heap_bytes,
                self.max_pending_node_bytes,
                self.max_depth,
            ]
            .contains(&usize::MAX)
        {
            return Err(BodyMutationError::Invalid("normal Host finite work limits"));
        }
        Ok(())
    }
}
/// All groups are supplied explicitly once. Actions and tail phases retain
/// independent limits; no uniform fallback or implicit refill exists.
pub struct NormalHostWorkLimitsV1 {
    pub invocation_invocation: NormalHostLaneLimitsV1,
    pub invocation_manifest: NormalHostLaneLimitsV1,
    pub invocation_event_original: NormalHostLaneLimitsV1,
    pub invocation: NormalHostLaneLimitsV1,
    pub io_overlay: NormalHostLaneLimitsV1,
    pub io_current: NormalHostLaneLimitsV1,
    pub io_parent: NormalHostLaneLimitsV1,
    pub io_admission: NormalHostLaneLimitsV1,
    pub io_factory: NormalHostLaneLimitsV1,
    pub io_recipe: NormalHostLaneLimitsV1,
    pub io_industry: NormalHostLaneLimitsV1,
    pub io_industry_scan: NormalHostLaneLimitsV1,
    pub io_members: NormalHostLaneLimitsV1,
    pub io_fees: NormalHostLaneLimitsV1,
    pub io_events: NormalHostLaneLimitsV1,
    pub materials: NormalHostLaneLimitsV1,
    pub driver_bootstrap_finality: NormalHostLaneLimitsV1,
    pub driver_bootstrap_revocation_write: NormalHostLaneLimitsV1,
    pub driver_bootstrap_authorization_write: NormalHostLaneLimitsV1,
    pub driver_bootstrap_groups: NormalHostLaneLimitsV1,
    pub driver_bootstrap_journal: NormalHostLaneLimitsV1,
    pub driver_factory: NormalHostLaneLimitsV1,
    pub driver_pending: NormalHostLaneLimitsV1,
    pub driver_pending_write: NormalHostLaneLimitsV1,
    pub driver_policy_scan: NormalHostLaneLimitsV1,
    pub driver_policy: NormalHostLaneLimitsV1,
    pub driver_actions_module_lifecycle: NormalHostLaneLimitsV1,
    pub driver_actions_crisis: NormalHostLaneLimitsV1,
    pub driver_actions_contract_accept: NormalHostLaneLimitsV1,
    pub driver_actions_meta_progress: NormalHostLaneLimitsV1,
    pub driver_actions_factory_lifecycle: NormalHostLaneLimitsV1,
    pub driver_actions_route_availability: NormalHostLaneLimitsV1,
    pub driver_actions_material_transfer: NormalHostLaneLimitsV1,
    pub driver_actions_body: NormalHostLaneLimitsV1,
    pub driver_actions_data_access: NormalHostLaneLimitsV1,
    pub driver_actions_collect_data: NormalHostLaneLimitsV1,
    pub driver_actions_authenticated_data: NormalHostLaneLimitsV1,
    pub driver_actions_authenticated_work: NormalHostLaneLimitsV1,
    pub driver_actions_transfer_resource: NormalHostLaneLimitsV1,
    pub driver_actions_main_token_transfer: NormalHostLaneLimitsV1,
    pub driver_actions_factory_schedule: NormalHostLaneLimitsV1,
    pub driver_actions_contracts: NormalHostLaneLimitsV1,
    pub driver_actions_alliance: NormalHostLaneLimitsV1,
    pub driver_actions_logistics_registration: NormalHostLaneLimitsV1,
    pub driver_actions_profile: NormalHostLaneLimitsV1,
    pub driver_actions_extension: NormalHostLaneLimitsV1,
    pub driver_tail_terminal: NormalHostLaneLimitsV1,
    pub driver_tail_recipes: NormalHostLaneLimitsV1,
    pub driver_tail_orders: NormalHostLaneLimitsV1,
    pub driver_tail_world_delivery: NormalHostLaneLimitsV1,
    pub driver_tail_world: NormalHostLaneLimitsV1,
    pub driver_tail_gameplay: NormalHostLaneLimitsV1,
    pub driver_tail_organization: NormalHostLaneLimitsV1,
    pub driver_tail_threat_keys: NormalHostLaneLimitsV1,
    pub driver_tail_history_index: NormalHostLaneLimitsV1,
    pub driver_tail_cognition: NormalHostLaneLimitsV1,
    pub driver_tail_authority: NormalHostLaneLimitsV1,
    pub driver_tail: NormalHostLaneLimitsV1,
    pub driver_tail_path: NormalHostLaneLimitsV1,
    pub driver_product: NormalHostLaneLimitsV1,
    pub driver_audit: NormalHostLaneLimitsV1,
    pub driver_rules: NormalHostLaneLimitsV1,
    pub driver_enumeration: NormalHostLaneLimitsV1,
    pub driver_event_tail: NormalHostLaneLimitsV1,
    pub driver_finish_cut: NormalHostLaneLimitsV1,
    pub driver_final_sources_projection: NormalHostLaneLimitsV1,
    pub driver_final_sources_business: NormalHostLaneLimitsV1,
    pub driver_final_sources_terminal_metadata: NormalHostLaneLimitsV1,
    pub driver_final_sources_runtime_index: NormalHostLaneLimitsV1,
    pub driver_final_sources_originals: NormalHostLaneLimitsV1,
    pub driver_final_sources_cognition_hash: NormalHostLaneLimitsV1,
    pub driver_final_sources_authorization: NormalHostLaneLimitsV1,
    pub driver_final_sources: NormalHostLaneLimitsV1,
    pub bootstrap_directories: NormalHostLaneLimitsV1,
}

trait FromLimits: Sized {
    fn from_limits(limits: &NormalHostLaneLimitsV1, core: &FormalBoundedWorkV1) -> Self;
}

impl NormalHostWorkV1 {
    pub fn new_bounded(
        limits: NormalHostWorkLimitsV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        let layout = std::mem::size_of::<Self>();
        // Fixed initialization is charged before a work carrier is built.
        // No Common remaining_* value is read or copied into any lane.
        common
            .charge(1 + layout as u64, layout as u64)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        limits.invocation_invocation.validate()?;
        limits.invocation_manifest.validate()?;
        limits.invocation_event_original.validate()?;
        limits.invocation.validate()?;
        limits.io_overlay.validate()?;
        limits.io_current.validate()?;
        limits.io_parent.validate()?;
        limits.io_admission.validate()?;
        limits.io_factory.validate()?;
        limits.io_recipe.validate()?;
        limits.io_industry.validate()?;
        limits.io_industry_scan.validate()?;
        limits.io_members.validate()?;
        limits.io_fees.validate()?;
        limits.io_events.validate()?;
        limits.materials.validate()?;
        limits.driver_bootstrap_finality.validate()?;
        limits.driver_bootstrap_revocation_write.validate()?;
        limits.driver_bootstrap_authorization_write.validate()?;
        limits.driver_bootstrap_groups.validate()?;
        limits.driver_bootstrap_journal.validate()?;
        limits.driver_factory.validate()?;
        limits.driver_pending.validate()?;
        limits.driver_pending_write.validate()?;
        limits.driver_policy_scan.validate()?;
        limits.driver_policy.validate()?;
        limits.driver_actions_module_lifecycle.validate()?;
        limits.driver_actions_crisis.validate()?;
        limits.driver_actions_contract_accept.validate()?;
        limits.driver_actions_meta_progress.validate()?;
        limits.driver_actions_factory_lifecycle.validate()?;
        limits.driver_actions_route_availability.validate()?;
        limits.driver_actions_material_transfer.validate()?;
        limits.driver_actions_body.validate()?;
        limits.driver_actions_data_access.validate()?;
        limits.driver_actions_collect_data.validate()?;
        limits.driver_actions_authenticated_data.validate()?;
        limits.driver_actions_authenticated_work.validate()?;
        limits.driver_actions_transfer_resource.validate()?;
        limits.driver_actions_main_token_transfer.validate()?;
        limits.driver_actions_factory_schedule.validate()?;
        limits.driver_actions_contracts.validate()?;
        limits.driver_actions_alliance.validate()?;
        limits.driver_actions_logistics_registration.validate()?;
        limits.driver_actions_profile.validate()?;
        limits.driver_actions_extension.validate()?;
        limits.driver_tail_terminal.validate()?;
        limits.driver_tail_recipes.validate()?;
        limits.driver_tail_orders.validate()?;
        limits.driver_tail_world_delivery.validate()?;
        limits.driver_tail_world.validate()?;
        limits.driver_tail_gameplay.validate()?;
        limits.driver_tail_organization.validate()?;
        limits.driver_tail_threat_keys.validate()?;
        limits.driver_tail_history_index.validate()?;
        limits.driver_tail_cognition.validate()?;
        limits.driver_tail_authority.validate()?;
        limits.driver_tail.validate()?;
        limits.driver_tail_path.validate()?;
        limits.driver_product.validate()?;
        limits.driver_audit.validate()?;
        limits.driver_rules.validate()?;
        limits.driver_enumeration.validate()?;
        limits.driver_event_tail.validate()?;
        limits.driver_finish_cut.validate()?;
        limits.driver_final_sources_projection.validate()?;
        limits.driver_final_sources_business.validate()?;
        limits.driver_final_sources_terminal_metadata.validate()?;
        limits.driver_final_sources_runtime_index.validate()?;
        limits.driver_final_sources_originals.validate()?;
        limits.driver_final_sources_cognition_hash.validate()?;
        limits.driver_final_sources_authorization.validate()?;
        limits.driver_final_sources.validate()?;
        limits.bootstrap_directories.validate()?;
        let allocation = common
            .memory()
            .reserve(layout, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let core = common.work();
        Ok(NormalHostWorkV1 {
            invocation: NormalHostInvocationWorkV1 {
                invocation: RootModuleExecutionWorkV1::from_limits(
                    &limits.invocation_invocation,
                    core,
                ),
                manifest: RootWorldManifestHashWorkV1::from_limits(
                    &limits.invocation_manifest,
                    core,
                ),
                event_original: CanonicalCborWorkV1::from_limits(
                    &limits.invocation_event_original,
                    core,
                ),
                max_event_bytes: limits.invocation.max_original_bytes,
            },
            io: NormalHostIoWorkV1 {
                overlay: NormalHostReadWorkV1::from_limits(&limits.io_overlay, core),
                current: NormalHostReadWorkV1::from_limits(&limits.io_current, core),
                parent: NormalHostReadWorkV1::from_limits(&limits.io_parent, core),
                admission: NormalHostReadWorkV1::from_limits(&limits.io_admission, core),
                factory: NormalHostReadWorkV1::from_limits(&limits.io_factory, core),
                recipe: NormalHostReadWorkV1::from_limits(&limits.io_recipe, core),
                industry: NormalHostReadWorkV1::from_limits(&limits.io_industry, core),
                industry_scan: StateScanBudget::from_limits(&limits.io_industry_scan, core),
                members: NormalHostWriteWorkV1::from_limits(&limits.io_members, core),
                fees: NormalHostWriteWorkV1::from_limits(&limits.io_fees, core),
                events: NormalHostWriteWorkV1::from_limits(&limits.io_events, core),
            },
            materials: NormalHostMaterialWorkV1::from_limits(&limits.materials, core),
            driver: NormalHostDriverWorkV1 {
                bootstrap: NormalHostBootstrapWorkV1 {
                    finality: CurrentBootstrapFinalityWorkV1::from_limits(
                        &limits.driver_bootstrap_finality,
                        core,
                    ),
                    revocation_write: TypedRecordWriteWorkV1::from_limits(
                        &limits.driver_bootstrap_revocation_write,
                        core,
                    ),
                    authorization_write: AuthorizationWriteWorkV1::from_limits(
                        &limits.driver_bootstrap_authorization_write,
                        core,
                    ),
                    groups: CognitionGroupWriteWorkV1::from_limits(
                        &limits.driver_bootstrap_groups,
                        core,
                    ),
                    journal: ProductValidationDeliveryWorkV1::from_limits(
                        &limits.driver_bootstrap_journal,
                        core,
                    ),
                },
                factory: FactoryPhaseWorkV1::from_limits(&limits.driver_factory, core),
                pending: PendingActionFamilyWorkV1::from_limits(&limits.driver_pending, core),
                pending_write: TypedRecordWriteWorkV1::from_limits(
                    &limits.driver_pending_write,
                    core,
                ),
                policy_scan: StateScanBudget::from_limits(&limits.driver_policy_scan, core),
                policy: TypedRecordReadWorkV1::from_limits(&limits.driver_policy, core),
                actions: NormalActionWorkSetV1 {
                    module_lifecycle: NormalModuleLifecycleWorkSetV1::from_limits(
                        &limits.driver_actions_module_lifecycle,
                        core,
                    ),
                    crisis: CrisisWorkV1::from_limits(&limits.driver_actions_crisis, core),
                    contract_accept: ContractAcceptWorkV1::from_limits(
                        &limits.driver_actions_contract_accept,
                        core,
                    ),
                    meta_progress: MetaProgressWorkV1::from_limits(
                        &limits.driver_actions_meta_progress,
                        core,
                    ),
                    factory_lifecycle: FactoryLifecycleWorkV1::from_limits(
                        &limits.driver_actions_factory_lifecycle,
                        core,
                    ),
                    route_availability: RouteAvailabilityWorkV1::from_limits(
                        &limits.driver_actions_route_availability,
                        core,
                    ),
                    material_transfer: MaterialTransferWorkV1::from_limits(
                        &limits.driver_actions_material_transfer,
                        core,
                    ),
                    body: BodyActionWorkV1::from_limits(&limits.driver_actions_body, core),
                    data_access: DataAccessWorkV1::from_limits(
                        &limits.driver_actions_data_access,
                        core,
                    ),
                    collect_data: CollectDataWorkV1::from_limits(
                        &limits.driver_actions_collect_data,
                        core,
                    ),
                    authenticated_data: CollectDataWorkV1::from_limits(
                        &limits.driver_actions_authenticated_data,
                        core,
                    ),
                    authenticated_work: CollectDataAuthorizationWorkV1::from_limits(
                        &limits.driver_actions_authenticated_work,
                        core,
                    ),
                    main_token_transfer: MainTokenTransferWorkV1::from_limits(
                        &limits.driver_actions_main_token_transfer,
                        core,
                    ),
                    transfer_resource: TransferResourceWorkV1::from_limits(
                        &limits.driver_actions_transfer_resource,
                        core,
                    ),
                    factory_schedule: FactoryScheduleWorkV1::from_limits(
                        &limits.driver_actions_factory_schedule,
                        core,
                    ),
                    contracts: ContractWorkV1::from_limits(&limits.driver_actions_contracts, core),
                    alliance: AllianceWorkV1::from_limits(&limits.driver_actions_alliance, core),
                    logistics_registration: LogisticsRegistrationWorkV1::from_limits(
                        &limits.driver_actions_logistics_registration,
                        core,
                    ),
                    profile: ProfileGovernanceWorkV1::from_limits(
                        &limits.driver_actions_profile,
                        core,
                    ),
                    extension: NormalCoreExtensionWorkV1::from_limits(
                        &limits.driver_actions_extension,
                        core,
                    ),
                },
                tail: NormalTickTailWorkSetV1 {
                    terminal: IndustrialEligibilityWorkV1::from_limits(
                        &limits.driver_tail_terminal,
                        core,
                    ),
                    recipes: FactoryRecipeDirectoryWorkV1::from_limits(
                        &limits.driver_tail_recipes,
                        core,
                    ),
                    orders: FactoryOrderDirectoryWorkV1::from_limits(
                        &limits.driver_tail_orders,
                        core,
                    ),
                    world_delivery: IndustrialDeliveryWorkV1::from_limits(
                        &limits.driver_tail_world_delivery,
                        core,
                    ),
                    world: WorldRulesFamilyWorkV1::from_limits(&limits.driver_tail_world, core),
                    gameplay: NormalGameplayCycleWorkV1::from_limits(
                        &limits.driver_tail_gameplay,
                        core,
                    ),
                    organization: NormalOrganizationTailWorkV1::from_limits(
                        &limits.driver_tail_organization,
                        core,
                    ),
                    threat_keys: ThreatKeyDirectoryWorkV1::from_limits(
                        &limits.driver_tail_threat_keys,
                        core,
                    ),
                    history_index: RuntimeHistoryIndexWorkV1::from_limits(
                        &limits.driver_tail_history_index,
                        core,
                    ),
                    cognition: CurrentCognitionExpiryWorkV1::from_limits(
                        &limits.driver_tail_cognition,
                        core,
                    ),
                    authority: AgentAuthorityWorkBudgetV1::from_limits(
                        &limits.driver_tail_authority,
                        core,
                    ),
                    authority_cells: limits.driver_tail.steps_per_counter,
                    pending_entries: limits.driver_tail.entries_per_counter,
                    path: JsonScalarPathReadWorkV1::from_limits(&limits.driver_tail_path, core),
                    max_schema_depth: limits.driver_tail.max_depth,
                },
                product: ProductValidationDeliveryWorkV1::from_limits(&limits.driver_product, core),
                audit: WorldEventDecodeWorkV1::from_limits(&limits.driver_audit, core),
                rules: RootRulePhaseWorkV1::from_limits(&limits.driver_rules, core),
                enumeration: CurrentInvocationEnumerationWorkV1::from_limits(
                    &limits.driver_enumeration,
                    core,
                ),
                event_tail: CurrentWorldEventTailWorkV1::from_limits(
                    &limits.driver_event_tail,
                    core,
                ),
                finish_cut: NormalHostFinishCutWorkV1::from_limits(&limits.driver_finish_cut, core),
                final_sources: NormalFinalSourcesWorkV1 {
                    projection: NormalProjectionWorkBankV1::from_limits(
                        &limits.driver_final_sources_projection,
                        core,
                    ),
                    business: NormalBusinessReadWorkV1::from_limits(
                        &limits.driver_final_sources_business,
                        core,
                    ),
                    terminal_metadata: NormalTerminalMetadataReadWorkV1::from_limits(
                        &limits.driver_final_sources_terminal_metadata,
                        core,
                    ),
                    runtime_index: RuntimeHistoryIndexWorkV1::from_limits(
                        &limits.driver_final_sources_runtime_index,
                        core,
                    ),
                    originals: ProductValidationDeliveryWorkV1::from_limits(
                        &limits.driver_final_sources_originals,
                        core,
                    ),
                    cognition_hash: CognitionHeadHashWorkV1::from_limits(
                        &limits.driver_final_sources_cognition_hash,
                        core,
                    ),
                    authorization: AuthorizationRootSourceReadWorkV1::from_limits(
                        &limits.driver_final_sources_authorization,
                        core,
                    ),
                    authorization_pending_entries: limits.driver_final_sources.entries_per_counter,
                    final_originals_remaining: limits.driver_final_sources.steps_per_counter,
                    final_bytes_remaining: limits.driver_final_sources.bytes_per_counter,
                },
            },
            startup: StartupDirectoryWork::from_limits(&limits.bootstrap_directories, core),
            core: core.identity(),
            allocation,
        })
    }
    /// Checks the retained work owner without creating an execution permission
    /// or changing any lane allowance.
    pub fn validate_context_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.validate_context(common)
    }
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
        if self.core != common.work().identity() || !self.allocation.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "normal Host work original Core/pool",
            ));
        }
        Ok(())
    }
}

#[path = "incremental_normal_host_work_build_1.rs"]
mod fields_1;

#[path = "incremental_normal_host_work_build_2.rs"]
mod fields_2;

#[path = "incremental_normal_host_work_build_3.rs"]
mod fields_3;

#[path = "incremental_normal_host_work_build_4.rs"]
mod fields_4;

#[path = "incremental_normal_host_work_build_5.rs"]
mod fields_5;

#[path = "incremental_normal_host_work_startup.rs"]
mod startup;
pub(super) use startup::StartupDirectoryWork;

#[cfg(any(test, feature = "test_tier_required"))]
#[path = "incremental_normal_host_work_fixture.rs"]
mod fixture;
#[cfg(any(test, feature = "test_tier_required"))]
pub use fixture::two_agent_host_work_limits_v1;

#[cfg(test)]
#[path = "incremental_normal_host_work_builder_tests.rs"]
mod tests;
