//! Fixed Current ProviderBacked bootstrap transaction. Source templates and
//! work loans are DATA; authority is checked by the original fixed kernels.
//! No constructor here fabricates an installed bootstrap, completed tick, or
//! accepted history. Actual publication owners have private constructors.
use crate::world::{
    ModuleCapabilityAdmissionSourcesV1,
    incremental_body::BodyMutationError,
    incremental_module_invocation::RootModuleExecutionFamiliesV1,
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_normal_action_module_lifecycle::{ModuleLifecycleSourcesV1, ModuleLifecycleWorkV1},
};
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1;

#[path = "normal_bootstrap_current_publication.rs"]
mod publication;
pub(in crate::world) use publication::CommittedCurrentBootstrapAuthorizationV1;

fn overlay_error(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
/// The stage borrow is deliberately absent. Each concrete operation constructs
/// a short read-only lifecycle view, drops it, then mutates the sole real stage.
/// Keeping the old wide view while borrowing that stage mutably is forbidden.
pub(in crate::world) struct BootstrapLifecycleSourcesV1<'s>{
    pub(in crate::world) controls:&'s ReservedValue<oasis7_world_state::incremental_world_collection_controls::WorldCollectionControlsQualificationV1>,
    pub(in crate::world) proposals:&'s ReservedValue<oasis7_world_state::incremental_runtime_proposal::RuntimeProposalsQualificationV1>,
    pub(in crate::world) families:RootModuleExecutionFamiliesV1<'s>,pub(in crate::world) factory:&'s FactoryScheduleSourcesV1<'s>,
    pub(in crate::world) governance:&'s ReservedValue<oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceQualificationV1>,
    pub(in crate::world) compiler:&'s crate::module_source_compiler::RuntimeSourceCompiler,
    pub(in crate::world) typed:&'s ReservedValue<oasis7_world_state::incremental_runtime_typed_complete::RuntimeTypedCompleteQualificationV1>,
    pub(in crate::world) native:&'s ReservedValue<oasis7_world_state::incremental_world_native::WorldNativeFieldsQualificationV1>,
    pub(in crate::world) primitive:&'s ReservedValue<oasis7_world_state::incremental_owner_closure::WorldPrimitiveQualificationV1>,
}
impl<'s> BootstrapLifecycleSourcesV1<'s> {
    pub(in crate::world) fn borrow_from_lifecycle_v1(
        source: &ModuleLifecycleSourcesV1<'s>,
    ) -> Self {
        Self {
            controls: source.controls,
            proposals: source.proposals,
            families: source.families,
            factory: source.factory,
            governance: source.governance,
            compiler: source.compiler,
            typed: source.capability.typed,
            native: source.capability.native,
            primitive: source.capability.primitive,
        }
    }
    // Private reborrow plumbing. The callback is not an external executor or
    // admission hook; only fixed bootstrap readers/verifiers in this module use
    // it, and they cannot replace business predicates or publication bodies.
    fn with_actual_sources<T>(
        &self,
        current:&crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        read: impl FnOnce(&ModuleLifecycleSourcesV1<'_>) -> T,
    ) -> T {
        let capability = ModuleCapabilityAdmissionSourcesV1 {
            current,
            typed: self.typed,
            governance: self.governance,
            proposals: self.proposals,
            native: self.native,
            roots: self.families.roots,
            legacy: self.families.authorization,
            primitive: self.primitive,
        };
        let source = ModuleLifecycleSourcesV1 {
            controls: self.controls,
            capability: &capability,
            proposals: self.proposals,
            families: self.families,
            factory: self.factory,
            governance: self.governance,
            compiler: self.compiler,
        };
        read(&source)
    }
}
/// Borrow the caller's one continuous bank. There is no new/default budget,
/// cloned token, reset counter, or second mutable Factory loan in this view.
pub(in crate::world) struct CurrentBootstrapWorkV1<'a,'w>{
    pub module:ModuleLifecycleWorkV1<'w>,
    pub factory:&'a mut FactoryScheduleWorkV1,
    pub cognition:&'a mut crate::world::incremental_normal_cognition_expiry::CurrentCognitionExpiryWorkV1,
    pub revocation_write:&'a mut oasis7_world_state::incremental_runtime_revocation::RevocationWriteWorkV1,
    pub authorization_write:&'a mut oasis7_world_state::incremental_runtime_authorization_genesis::AuthorizationWriteWorkV1,
    pub eligibility:&'a mut crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    pub orders:&'a mut crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryWorkV1,
    pub groups:&'a mut oasis7_world_state::incremental_cognition_groups::CognitionGroupWriteWorkV1,
    pub journal:&'a mut crate::world::incremental_normal_industry_due::delivery::ProductValidationDeliveryWorkV1,
    pub data_limits:&'a oasis7_formal_protocol::controlled_authority::formal::bounded_cbor_data::CborDataLimitsV1,
}

#[path = "normal_bootstrap_current_write.rs"]
mod write;
pub(in crate::world) use write::WrittenCurrentBootstrapAuthorizationV1;

/// Actual accepted/current carriers required by the fixed bootstrap consumer.
/// None of these readonly sources is a Node or Store permission.
pub(in crate::world) struct CurrentBootstrapSourcesV1<'a,'s>{
    pub lifecycle:&'a BootstrapLifecycleSourcesV1<'s>,
    pub accepted_orders:&'a ReservedValue<crate::world::incremental_factory_directory_orders::AcceptedFactoryOrderDirectoryV1>,
    pub cognition:&'a ReservedValue<oasis7_world_state::incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3>,
    pub artifacts:&'a ReservedValue<oasis7_world_state::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1>,
    pub ledger:&'a oasis7_world_state::incremental_cognition_proof_ledger::CurrentCognitionProofLedgerV1,
    pub process:&'a ReservedValue<crate::world::incremental_normal_process_source::NormalProcessSourceV1>,
    pub finality:&'a crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapLiveFinalityV1,
}

#[path = "normal_bootstrap_current_catalog.rs"]
mod catalog;
#[path = "normal_bootstrap_current_economy.rs"]
mod economy;
#[path = "normal_bootstrap_current_execute.rs"]
mod execute;
pub(in crate::world) use execute::{BootstrapEconomyFrontiersV1, CommittedCurrentBootstrapBatchV1};
#[path = "normal_bootstrap_current_hash.rs"]
pub(in crate::world) mod hash;
