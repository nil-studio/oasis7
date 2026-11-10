//! Fixed 23-field native coverage; never an arbitrary Codec/F mint or a complete
//! World capability. The independent global router and physical leases remain required.
use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
pub use family::WorldNativeFieldReadWorkV1 as WorldNativeQualificationWorkV1;
use family::{WorldNativeFieldQualificationV1, WorldNativeVerifiedTransitionV1};
use oasis7_formal_protocol::incremental_execution::{
    RootSetV1, TransitionRecordV1, TransitionVerifier,
};
pub type WorldNativeRecoveredRecordV1<T> = RecoveredTypedRecordV1<WorldNativeRecordV1<T>>;
pub use crate::incremental_typed_record_codec::TypedRecordReadWorkV1 as WorldNativeSelectedReadWorkV1;
#[path = "incremental_world_native_starter_binding_index.rs"]
mod starter_binding_index;
pub use starter_binding_index::{
    StarterBindingIndexV1, StarterBindingIndexWorkV1, StarterBindingObservationV1,
    observe_current_starter_binding_v1, readmit_starter_binding_index_ordinary_v1,
};
#[path = "incremental_world_native_starter_binding_budget.rs"]
mod starter_binding_budget;
fn qualify_at<F: WorldNativeFieldSpecV1>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeQualificationWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, ValueTreeError> {
    read::check(ctx)?;
    let FieldDestination::State(ns) = F::descriptor()?.destination else {
        return Err(invalid());
    };
    let mut local = WorldFieldReadContextV1 {
        root: roots.state_root(ns),
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let result = family::qualify_world_native_field_v1::<F>(roots, &mut local, scan, work);
    read::check(&mut local)?;
    read::check(ctx)?;
    result
}
fn accept_at<F: WorldNativeFieldSpecV1>(
    parent: &ReservedValue<WorldNativeFieldQualificationV1<F>>,
    verified: &WorldNativeVerifiedTransitionV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut WorldNativeQualificationWorkV1,
) -> Result<ReservedValue<WorldNativeFieldQualificationV1<F>>, WorldFieldOverlayErrorV1> {
    read::check(ctx)?;
    let FieldDestination::State(ns) = F::descriptor()?.destination else {
        return Err(invalid().into());
    };
    let mut local = WorldFieldReadContextV1 {
        root: verified.record().after.state_root(ns),
        state: ctx.state,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let result = family::accept_world_native_field_verified_v1::<F>(
        parent, verified, &mut local, scan, work,
    );
    read::check(&mut local)?;
    read::check(ctx)?;
    result
}
macro_rules! fields {
($($slot:ident:$spec:ty => ($key:ty,$value:ty,$recover:ident,$name:literal);)+)=>{
    struct WorldNativeFieldPartsV1 { $($slot:ReservedValue<WorldNativeFieldQualificationV1<$spec>>,)+ }
    pub struct WorldNativeFieldsQualificationV1 {
        starter_binding_index:ReservedValue<StarterBindingIndexV1>,
        roots:[u8;32],class:MemoryClass,
        $($slot:ReservedValue<WorldNativeFieldQualificationV1<$spec>>,)+
    }
    impl WorldNativeFieldsQualificationV1 {
        pub fn starter_binding_index(&self)->&ReservedValue<StarterBindingIndexV1>{&self.starter_binding_index}
        pub fn validate_binding(&self,roots:&RootSetV1)->Result<(),ValueTreeError>{
            roots.validate().map_err(|_|invalid())?;
            if self.roots!=roots.digest().map_err(|_|invalid())? {return Err(invalid());}
            $(self.$slot.validate_binding(roots)?; if self.$slot.allocation_class()!=self.class{return Err(invalid());})+
            self.starter_binding_index.validate_aggregate_binding_v1(roots,self.class,self.starter_oc_claims.records())?;
            // Each slot is a different fixed descriptor. Its count cannot be
            // used to claim more leaves than the authenticated RootSet owns.
            let records=0u128 $(+u128::from(self.$slot.records()))+;
            let leaves=roots.state.iter().map(|r|u128::from(r.leaf_count)).sum::<u128>();
            if records+23>leaves{return Err(invalid());}Ok(())
        }
    }
    /// Exact catalogue membership only; callers still need this aggregate's
    /// private complete qualification and the independent global key router.
    pub fn world_native_field_descriptors_v1()->Result<[FieldDescriptor;23],ValueTreeError>{
        let fields=[$(<$spec as WorldNativeFieldSpecV1>::descriptor()?,)+];
        let names=[$($name,)+];
        for (i,(field,name)) in fields.iter().zip(names).enumerate() {
            if field.owner!=FieldOwner::WorldState||field.name!=name||!matches!(field.destination,FieldDestination::State(_))||fields[..i].iter().any(|old|old.name==field.name){return Err(invalid());}
        }
        Ok(fields)
    }
    pub fn qualify_world_native_fields_v1(roots:&RootSetV1,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>,scan:&mut StateScanBudget,work:&mut WorldNativeQualificationWorkV1)->Result<ReservedValue<WorldNativeFieldsQualificationV1>,ValueTreeError>{
        let result=(||{
            read::check(ctx)?;roots.validate().map_err(|_|invalid())?;world_native_field_descriptors_v1()?;
            if !roots.state.iter().any(|root|root==ctx.root){return Err(invalid());}
            let charge=ctx.values.memory.reserve(std::mem::size_of::<WorldNativeFieldsQualificationV1>()+512,ctx.values.class)?;
            let fields=WorldNativeFieldPartsV1 {$($slot:qualify_at::<$spec>(roots,ctx,scan,work)?,)+};
            let starter_binding_index=starter_binding_budget::build(&fields.starter_oc_claims,roots,ctx,scan,work)?;
            let value=WorldNativeFieldsQualificationV1 {roots:roots.digest().map_err(|_|invalid())?,class:ctx.values.class,starter_binding_index,$($slot:fields.$slot,)+};
            value.validate_binding(roots)?;Ok(charge.retain(value))
        })();read::check(ctx)?;result
    }
    fn world_native_fields_preflight_v1(parent:&ReservedValue<WorldNativeFieldsQualificationV1>,record:&TransitionRecordV1,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>)->Result<oasis7_proto::memory_budget::MemoryReservation,ValueTreeError>{
        read::check(ctx)?;
        if parent.class!=MemoryClass::Ordinary||ctx.values.class!=MemoryClass::Ordinary||!parent.uses_budget(ctx.values.memory)||parent.reserved_bytes()<std::mem::size_of::<WorldNativeFieldsQualificationV1>()+512||!record.after.state.iter().any(|root|root==ctx.root){return Err(invalid());}
        let phase=ctx.values.memory.reserve(128<<10,MemoryClass::Ordinary)?;
        parent.validate_binding(&record.before)?;world_native_field_descriptors_v1()?;read::check(ctx)?;Ok(phase)
    }
    pub fn accept_world_native_fields_transition_v1(parent:&ReservedValue<WorldNativeFieldsQualificationV1>,verifier:&TransitionVerifier<'_,impl MerkleNodeReader>,record:&TransitionRecordV1,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>,scan:&mut StateScanBudget,work:&mut WorldNativeQualificationWorkV1)->Result<ReservedValue<WorldNativeFieldsQualificationV1>,WorldFieldOverlayErrorV1>{
        let result=(||{
            read::check(ctx)?;
            if verifier.class!=MemoryClass::Ordinary||!verifier.memory.same_pool(ctx.values.memory){return Err(invalid().into());}
            {let _phase=world_native_fields_preflight_v1(parent,record,ctx)?;}
            let verified=family::verify_world_native_transition_v1(verifier,record,ctx)?;
            accept_world_native_fields_verified_v1(parent,&verified,ctx,scan,work)
        })();read::check(ctx)?;result
    }
    pub(crate) fn accept_world_native_fields_verified_v1(parent:&ReservedValue<WorldNativeFieldsQualificationV1>,verified:&crate::incremental_verified_world_transition::VerifiedWorldTransitionV1,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>,scan:&mut StateScanBudget,work:&mut WorldNativeQualificationWorkV1)->Result<ReservedValue<WorldNativeFieldsQualificationV1>,WorldFieldOverlayErrorV1>{
        let result=(||{
            read::check(ctx)?;verified.check_context(ctx.values.memory,ctx.values.class,ctx.root)?;
            let record=verified.record();
            let _phase=world_native_fields_preflight_v1(parent,record,ctx)?;
            let charge=ctx.values.memory.reserve(std::mem::size_of::<WorldNativeFieldsQualificationV1>()+512,MemoryClass::Ordinary).map_err(ValueTreeError::Memory)?;
            let fields=WorldNativeFieldPartsV1 {$($slot:accept_at::<$spec>(&parent.$slot,verified,ctx,scan,work)?,)+};
            let starter_binding_index=starter_binding_budget::accept(&parent.starter_binding_index,&fields.starter_oc_claims,verified,ctx,scan,work)?;
            let value=WorldNativeFieldsQualificationV1 {roots:record.after.digest().map_err(|_|invalid())?,class:MemoryClass::Ordinary,starter_binding_index,$($slot:fields.$slot,)+};
            value.validate_binding(&record.after)?;Ok(charge.retain(value))
        })();read::check(ctx)?;result
    }
    /// Resource-only same-pool promotion; no physical lease or domain authority
    /// is manufactured, and no field may be omitted from the promoted carrier.
    pub fn readmit_world_native_fields_ordinary_v1(source:&ReservedValue<WorldNativeFieldsQualificationV1>,roots:&RootSetV1,memory:&MemoryBudget,mut cancelled:impl FnMut()->bool)->Result<ReservedValue<WorldNativeFieldsQualificationV1>,ValueTreeError>{
        let result=(||{
            if cancelled(){return Err(ValueTreeError::Cancelled);}
            if !source.uses_budget(memory)||source.class!=MemoryClass::Recovery||source.reserved_bytes()<std::mem::size_of::<WorldNativeFieldsQualificationV1>()+512{return Err(invalid());}
            source.validate_binding(roots)?;
            let charge=memory.reserve(std::mem::size_of::<WorldNativeFieldsQualificationV1>()+512,MemoryClass::Ordinary)?;
            let fields=WorldNativeFieldPartsV1 {$($slot:family::readmit_world_native_field_ordinary_v1::<$spec>(&source.$slot,roots,memory,&mut cancelled)?,)+};
            let starter_binding_index=starter_binding_index::readmit_starter_binding_index_ordinary_bounded_v1(&source.starter_binding_index,roots,memory,&mut cancelled)?;
            let value=WorldNativeFieldsQualificationV1 {roots:source.roots,class:MemoryClass::Ordinary,starter_binding_index,$($slot:fields.$slot,)+};
            value.validate_binding(roots)?;Ok(charge.retain(value))
        })();if cancelled(){return Err(ValueTreeError::Cancelled);}result
    }
    $(
        /// Selected original DTO data, with its complete admission retained.
        /// The caller binds the context to its qualified formal snapshot.
        pub fn $recover(key:&$key,ctx:&mut WorldFieldReadContextV1<'_,impl MerkleNodeReader,impl ValueNodeReader,impl FnMut()->Option<ValueTreeError>,impl FnMut()->bool>,scan:&mut StateScanBudget,work:&mut WorldNativeSelectedReadWorkV1)->Result<Option<WorldNativeRecoveredRecordV1<$value>>,ValueTreeError>{
            let result=read::recover_world_native_record_v1::<$spec>(key,ctx,scan,work);read::check(ctx)?;result
        }
    )+
};}

fields! {
    product_validation_receipts:crate::incremental_world_native_industry::ProductValidationReceiptsFieldV1=>(u64,Vec<crate::ProductValidationReceiptV1>,recover_product_validation_receipts_v1,"product_validation_receipts");
    product_profiles:crate::incremental_world_native_industry::ProductProfilesFieldV1=>(String,oasis7_wasm_abi::ProductProfileV1,recover_product_profiles_v1,"product_profiles");
    latest_product_validation:crate::incremental_world_native_industry::LatestProductValidationFieldV1=>((),Option<crate::LastProductValidationState>,recover_latest_product_validation_v1,"latest_product_validation");
    recipe_profiles:crate::incremental_world_native_industry::RecipeProfilesFieldV1=>(String,oasis7_wasm_abi::RecipeProfileV1,recover_recipe_profiles_v1,"recipe_profiles");
    factory_profiles:crate::incremental_world_native_industry::FactoryProfilesFieldV1=>(String,oasis7_wasm_abi::FactoryProfileV1,recover_factory_profiles_v1,"factory_profiles");
    factory_construction_receipts:crate::incremental_world_native_industry::FactoryConstructionReceiptsFieldV1=>(String,oasis7_agent_api::world_quote::FactoryBuildPowerObligationV1,recover_factory_construction_receipts_v1,"factory_construction_receipts");
    product_validation_attempts:crate::incremental_world_native_industry::ProductValidationAttemptsFieldV1=>(u64,Vec<crate::ProductValidationAttemptV1>,recover_product_validation_attempts_v1,"product_validation_attempts");
    factory_recycle_receipts:crate::incremental_world_native_industry::FactoryRecycleReceiptsFieldV1=>(String,crate::FactoryRecycleReceiptV1,recover_factory_recycle_receipts_v1,"factory_recycle_receipts");
    industry_progress:crate::incremental_world_native_industry::IndustryProgressFieldV1=>((),crate::IndustryProgressState,recover_industry_progress_v1,"industry_progress");
    gameplay_policy:crate::incremental_world_native_gameplay::GameplayPolicyFieldV1=>((),crate::GameplayPolicyState,recover_gameplay_policy_v1,"gameplay_policy");
    agent_claims:crate::incremental_world_native_gameplay::AgentClaimsFieldV1=>(String,crate::AgentClaimState,recover_agent_claims_v1,"agent_claims");
    starter_oc_claims:crate::incremental_world_native_gameplay::StarterOcClaimsFieldV1=>(String,crate::StarterOcClaimState,recover_starter_oc_claims_v1,"starter_oc_claims");
    meta_progress:crate::incremental_world_native_gameplay::MetaProgressFieldV1=>(String,crate::MetaProgressState,recover_meta_progress_v1,"meta_progress");
    module_admission_freezes:crate::incremental_world_native_gameplay::ModuleAdmissionFreezesFieldV1=>(String,crate::state::ModuleAdmissionFreeze,recover_module_admission_freezes_v1,"module_admission_freezes");
    module_release_requests:crate::incremental_world_native_gameplay::ModuleReleaseRequestsFieldV1=>(u64,crate::state::ModuleReleaseRequestState,recover_module_release_requests_v1,"module_release_requests");
    module_release_manifest_mappings:crate::incremental_world_native_gameplay::ModuleReleaseManifestMappingsFieldV1=>(u64,crate::state::ModuleReleaseManifestMappingState,recover_module_release_manifest_mappings_v1,"module_release_manifest_mappings");
    governance_finality_signer_registry:crate::incremental_world_native_governance::GovernanceFinalitySignerRegistryFieldV1=>((),Option<crate::governance::GovernanceFinalitySignerRegistry>,recover_governance_finality_signer_registry_v1,"governance_finality_signer_registry");
    governance_validator_admissions:crate::incremental_world_native_governance::GovernanceValidatorAdmissionsFieldV1=>(String,crate::governance::GovernanceValidatorAdmissionRecord,recover_governance_validator_admissions_v1,"governance_validator_admissions");
    governance_finality_key_rotations:crate::incremental_world_native_governance::GovernanceFinalityKeyRotationsFieldV1=>(String,crate::governance::FinalityKeyRotationRecordV1,recover_governance_finality_key_rotations_v1,"governance_finality_key_rotations");
    governance_main_token_controller_registry:crate::incremental_world_native_governance::GovernanceMainTokenControllerRegistryFieldV1=>((),Option<crate::governance::GovernanceMainTokenControllerRegistry>,recover_governance_main_token_controller_registry_v1,"governance_main_token_controller_registry");
    main_token_node_points_bridge_records:crate::incremental_world_native_governance::MainTokenNodePointsBridgeRecordsFieldV1=>(u64,crate::main_token::MainTokenNodePointsBridgeEpochRecord,recover_main_token_node_points_bridge_records_v1,"main_token_node_points_bridge_records");
    main_token_treasury_distribution_records:crate::incremental_world_native_governance::MainTokenTreasuryDistributionRecordsFieldV1=>(String,crate::main_token::MainTokenTreasuryDistributionRecord,recover_main_token_treasury_distribution_records_v1,"main_token_treasury_distribution_records");
    restricted_starter_claim_liveops_pool_top_up_records:crate::incremental_world_native_governance::RestrictedStarterClaimLiveopsPoolTopUpRecordsFieldV1=>(String,crate::main_token::RestrictedStarterClaimLiveopsPoolTopUpRecord,recover_restricted_starter_claim_liveops_pool_top_up_records_v1,"restricted_starter_claim_liveops_pool_top_up_records");
}

#[cfg(test)]
#[path = "incremental_world_native_qualification_tests.rs"]
mod tests;

#[path = "incremental_world_native_current_claims.rs"]
mod current_claims;
pub use current_claims::{recover_current_agent_claim_v1, recover_current_starter_oc_claim_v1};

#[path = "incremental_factory_lifecycle_native_current.rs"]
mod factory_lifecycle_current;
pub use factory_lifecycle_current::{
    CurrentFactoryNativeRecordV1, change_current_factory_industry_stage_v1,
    recover_current_factory_industry_progress_v1, recover_current_factory_recycle_receipt_v1,
    write_current_factory_recycle_receipt_v1,
};

#[path = "incremental_meta_progress_current.rs"]
mod meta_progress_current;
pub use meta_progress_current::{
    CurrentMetaProgressRecordV1, MetaProgressMutationWorkV1, PreparedCurrentMetaProgressV1,
    prepare_current_meta_progress_delta_v1, recover_current_meta_progress_v1,
    write_prepared_current_meta_progress_v1,
};
