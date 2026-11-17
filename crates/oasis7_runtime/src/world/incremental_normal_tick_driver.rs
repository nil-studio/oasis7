//! Fixed Runtime normal sequence after Chain's committed-input application.
//! This executes Current phases, not a World mirror, and returns provisional
//! DATA only. Node input admission, formal acceptance and cache promotion are
//! separate mandatory consumers; no CompletedTick or resume permit is minted.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_phase::{
        FactoryPhaseAcceptedCachesV1, FactoryPhaseErrorV1, FactoryPhaseFamiliesV1,
        FactoryPhaseOutcomeV1, FactoryPhaseWorkV1,
    },
    incremental_module_invocation::*,
    incremental_normal_action_work::{NormalActionSourcesV1, NormalActionWorkSetV1},
    incremental_normal_cognition_completion::{
        NormalCognitionCompletionSourceV2, NormalCognitionCompletionSourcesV2,
    },
    incremental_normal_cognition_expiry::{
        CurrentCognitionExpiryWorkV1, NormalCognitionExpirySourceV1,
    },
    incremental_normal_cognition_service::{
        CurrentCognitionServiceOutcomeV2, CurrentCognitionServiceWorkV2,
    },
    incremental_normal_cognition_service_evidence::AcceptedCognitionWakeEvidenceV2,
    incremental_normal_industry_due::delivery::{
        CurrentProductValidationDeliveryIndexV1, ProductValidationDeliveryWorkV1,
    },
    incremental_normal_queue::{DrainedNormalQueueV1, NormalQueueErrorV1},
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_system_tail::{NormalSystemTailSourcesV1, NormalSystemTailWorkV1},
    incremental_normal_tail::{NormalTailWorkV1, economic::NormalEconomicTailWorkV1},
    incremental_normal_world_rule_lifecycle::{
        NormalWorldRuleLifecycleSourcesV1, delivery::IndustrialDeliveryWorkV1,
    },
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::PendingCurrentCognitionJournalV1,
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, CurrentCognitionProofLedgerV1,
    },
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{IndustryFamilyReadContextV1, IndustryTransitionReadContextV1},
    incremental_pending_action_family::PendingActionFamilyWorkV1,
    incremental_pending_action_overlay::PendingActionCurrentV1,
    incremental_runtime_revocation::{
        RevocationReadWorkV1, RuntimeRevocationCurrentV1, RuntimeRevocationQualificationV1,
    },
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
#[derive(Debug)]
pub(in crate::world) enum NormalTickDriverErrorV1<OE> {
    Module(RootModuleExecutionErrorV1<OE>),
    Factory(FactoryPhaseErrorV1<OE>),
    Queue(NormalQueueErrorV1<OE>),
}
impl<OE> From<RootModuleExecutionErrorV1<OE>> for NormalTickDriverErrorV1<OE> {
    fn from(e: RootModuleExecutionErrorV1<OE>) -> Self {
        Self::Module(e)
    }
}
/// Sources are required when the real complete parent router contains the
/// ProofLedger role. None never asserts absence: the router chooses the mode.
pub(in crate::world) struct NormalTickEnabledCognitionV2<'a> {
    pub ledger: &'a CurrentCognitionProofLedgerV1,
    pub accepted_ledger: &'a ReservedValue<AcceptedCognitionProofLedgerV1>,
    pub evidence: &'a ReservedValue<AcceptedCognitionWakeEvidenceV2>,
}
pub(in crate::world) struct NormalTickTailSourcesV1<'a> {
    pub economic: &'a NormalWorldRuleLifecycleSourcesV1<'a>,
    pub recipes: &'a ReservedValue<
        crate::world::incremental_factory_directory_records::AcceptedFactoryRecipeDirectoryV1,
    >,
    pub gameplay: &'a ReservedValue<
        oasis7_world_state::incremental_normal_gameplay_cycles_directory::GameplayCycleDirectoryV1,
    >,
    pub grants: &'a ReservedValue<
        oasis7_world_state::incremental_main_token_policy_records::RestrictedClaimGrantDirectoryV1,
    >,
    pub threats:
        &'a ReservedValue<oasis7_world_state::incremental_runtime_business::ThreatKeyDirectoryV1>,
    pub cognition: Option<NormalTickEnabledCognitionV2<'a>>,
}
/// Explicit outer work values, never constructed or reset by the driver.
/// Factory actions and the economic tail borrow actions.factory_schedule in
/// sequence, while expiry and service borrow this same cognition value.
pub(in crate::world) struct NormalTickTailWorkSetV1 {
    pub terminal: crate::world::incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    pub recipes: crate::world::incremental_factory_directory_records::FactoryRecipeDirectoryWorkV1,
    pub orders: crate::world::incremental_factory_directory_orders::FactoryOrderDirectoryWorkV1,
    pub world_delivery: IndustrialDeliveryWorkV1,
    pub world: oasis7_world_state::incremental_world_rules_family::WorldRulesFamilyWorkV1,
    pub gameplay: crate::world::incremental_normal_gameplay_cycles::NormalGameplayCycleWorkV1,
    pub organization:
        crate::world::incremental_normal_organization_tail::NormalOrganizationTailWorkV1,
    pub threat_keys: oasis7_world_state::incremental_runtime_business::ThreatKeyDirectoryWorkV1,
    pub history_index:
        oasis7_world_state::incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
    pub cognition: CurrentCognitionExpiryWorkV1,
    pub authority:
        oasis7_world_state::incremental_agent_authority_records::AgentAuthorityWorkBudgetV1,
    pub authority_cells: u64,
    pub pending_entries: usize,
    pub path: oasis7_world_state::incremental_json_records::JsonScalarPathReadWorkV1,
    pub max_schema_depth: usize,
}
pub(in crate::world) struct NormalTickCurrentDataV1<'p> {
    consensus: ReservedValue<
        crate::world::incremental_normal_tick_finalize::consensus::HistoryWrittenIwsCommitmentV1,
    >,
    prologue: FactoryPhaseOutcomeV1,
    queue: DrainedNormalQueueV1,
    cognition: CurrentCognitionServiceOutcomeV2<'p>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    parent_record: [u8; 32],
    input_material: [u8; 32],
    now: u64,
    fee: ReservedValue<()>,
}
impl<'p> NormalTickCurrentDataV1<'p> {
    pub(in crate::world) fn consensus_for_final_source(
        &self,
    ) -> &ReservedValue<
        crate::world::incremental_normal_tick_finalize::consensus::HistoryWrittenIwsCommitmentV1,
    > {
        &self.consensus
    }
    pub(in crate::world) fn cognition_for_final_source(
        &mut self,
    ) -> &mut Option<
        oasis7_world_state::incremental_cognition_history::PendingCurrentCognitionJournalV1<'p>,
    > {
        self.cognition.pending_for_final_source()
    }

    pub(in crate::world) fn parent_record_for_resident_v1(&self) -> [u8; 32] {
        self.parent_record
    }
    pub(in crate::world) fn into_detached_after_capture(
        self,
    ) -> Result<NormalTickCurrentDataV1<'static>, BodyMutationError> {
        let cognition = self.cognition.into_detached_after_capture().map_err(|_| {
            BodyMutationError::Invalid("normal pending Cognition frontier not captured")
        })?;
        Ok(NormalTickCurrentDataV1 {
            consensus: self.consensus,
            prologue: self.prologue,
            queue: self.queue,
            cognition,
            guard: self.guard,
            core: self.core,
            parent_record: self.parent_record,
            input_material: self.input_material,
            now: self.now,
            fee: self.fee,
        })
    }

    pub(in crate::world) fn processed_actions(&self) -> usize {
        self.queue.processed()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        input
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&self.guard, live)?;
        self.validate_historical_parts(tx, parent, input, common, live)
    }
    pub(in crate::world) fn final_source_guard(&self) -> &PreparedGuard {
        &self.guard
    }
    pub(in crate::world) fn validate_with_final_sources<M: MerkleNodeReader>(
        &self,
        source: &crate::world::incremental_normal_final_sources::CapturedNormalFinalSourcesV1,
        tx: &ExecutionTransaction<'_, M>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        parent
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        input
            .validate_context(common)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate_tick_capture(self, tx, common, live)?;
        self.validate_historical_parts(tx, parent, input, common, live)
    }
    fn validate_historical_parts<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        parent: &ReservedValue<BoundNormalTickParentV1<'_>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        self.consensus.validate_evidence(tx, common, live)?;
        self.queue.validate_evidence(tx, common)?;
        self.cognition.validate_evidence(tx, common, live)?;
        if self.core != common.work().identity()
            || !self.consensus.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
            || self.parent_record != parent.refs().core.candidate_record_id()
            || self.input_material != input.checked().material_id()
            || tx.parent_fence() != parent.fence()
            || self.prologue.resumed
        {
            return Err(BodyMutationError::Invalid(
                "normal Current fixed sequence evidence",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn into_pending(self) -> Option<PendingCurrentCognitionJournalV1<'p>> {
        self.cognition.into_pending()
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_current_tick_with_cached_parent_v1<
        'p,
        P,
        CE,
        CC,
        PE,
        PC,
        AP,
        AE,
        AC,
        FR,
        FC,
        RR,
        RC,
        IE,
        IC,
        ME,
        S,
    >(
        &mut self,
        parent_source: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        factory_caches: &FactoryPhaseAcceptedCachesV1<'_>,
        factory_families: &FactoryPhaseFamiliesV1<'_>,
        factory_read: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe_read: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        factory_work: &mut FactoryPhaseWorkV1,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        pending_work: &mut PendingActionFamilyWorkV1,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        revocation: &RuntimeRevocationCurrentV1,
        admission_read: &mut WorldFieldReadContextV1<'_, AP, R, AE, AC>,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut RevocationReadWorkV1,
        caches: &ReservedValue<
            crate::world::incremental_normal_parent_caches::NormalParentCachesV1,
        >,
        authorization: &crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        compiler: &crate::module_source_compiler::RuntimeSourceCompiler,
        actions: &mut NormalActionWorkSetV1,
        tail_sources: &NormalTickTailSourcesV1<'_>,
        tail_work: &mut NormalTickTailWorkSetV1,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        product_work: &mut ProductValidationDeliveryWorkV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        audit: &mut WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rules: &mut RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        event_tail: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<NormalTickCurrentDataV1<'p>, NormalTickDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        // The binder below validates all five sources once. Check the owning
        // header here without repeating every directory walk and IO allowance.
        if !caches.uses_budget(common.memory()) {
            return Err(NormalTickDriverErrorV1::Module(
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "normal parent cache owner pool",
                )),
            ));
        }
        let source_parents=crate::world::incremental_normal_source_action_views::bind_normal_action_source_parents_v1(parent_source,caches.borrow(),common).map_err(|error|NormalTickDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(error)))?;
        self.execute_normal_current_tick_from_parent_sources_v1(
            parent_source,
            input,
            factory_caches,
            factory_families,
            factory_read,
            recipe_read,
            factory_work,
            pending,
            pending_work,
            qualification,
            revocation,
            admission_read,
            policy_scan,
            policy_work,
            &source_parents,
            authorization,
            compiler,
            actions,
            tail_sources,
            tail_work,
            product_delivery,
            product_work,
            industry_read,
            sink,
            continued_cognition,
            materials,
            current,
            parent,
            audit,
            directory,
            rules,
            enumeration,
            event_tail,
            common,
            live_owner,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_current_tick_from_parent_sources_v1<
        'p,
        P,
        CE,
        CC,
        PE,
        PC,
        AP,
        AE,
        AC,
        FR,
        FC,
        RR,
        RC,
        IE,
        IC,
        ME,
        S,
    >(
        &mut self,
        parent_source: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        factory_caches: &FactoryPhaseAcceptedCachesV1<'_>,
        factory_families: &FactoryPhaseFamiliesV1<'_>,
        factory_read: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe_read: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        factory_work: &mut FactoryPhaseWorkV1,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        pending_work: &mut PendingActionFamilyWorkV1,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        revocation: &RuntimeRevocationCurrentV1,
        admission_read: &mut WorldFieldReadContextV1<'_, AP, R, AE, AC>,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut RevocationReadWorkV1,
        source_parents: &crate::world::incremental_normal_source_action_views::BoundNormalActionSourceParentsV1<'_, '_>,
        authorization: &crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        compiler: &crate::module_source_compiler::RuntimeSourceCompiler,
        actions: &mut NormalActionWorkSetV1,
        tail_sources: &NormalTickTailSourcesV1<'_>,
        tail_work: &mut NormalTickTailWorkSetV1,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        product_work: &mut ProductValidationDeliveryWorkV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        audit: &mut WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rules: &mut RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        event_tail: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<NormalTickCurrentDataV1<'p>, NormalTickDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        source_parents
            .validate_tick_parent_v1(parent_source, common)
            .map_err(|error| {
                NormalTickDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(error))
            })?;
        source_parents
            .with_sources_v1(authorization, compiler, common, |action_sources, common| {
                self.execute_normal_current_tick_v1(
                    parent_source,
                    input,
                    factory_caches,
                    factory_families,
                    factory_read,
                    recipe_read,
                    factory_work,
                    pending,
                    pending_work,
                    qualification,
                    revocation,
                    admission_read,
                    policy_scan,
                    policy_work,
                    action_sources,
                    actions,
                    tail_sources,
                    tail_work,
                    product_delivery,
                    product_work,
                    industry_read,
                    sink,
                    continued_cognition,
                    materials,
                    current,
                    parent,
                    audit,
                    directory,
                    rules,
                    enumeration,
                    event_tail,
                    common,
                    live_owner,
                )
            })
            .map_err(|error| {
                NormalTickDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(error))
            })?
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_current_tick_v1<
        'p,
        P,
        CE,
        CC,
        PE,
        PC,
        AP,
        AE,
        AC,
        FR,
        FC,
        RR,
        RC,
        IE,
        IC,
        ME,
        S,
    >(
        &mut self,
        parent_source: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &PaidBoundNormalInputDataV1<'_>,
        factory_caches: &FactoryPhaseAcceptedCachesV1<'_>,
        factory_families: &FactoryPhaseFamiliesV1<'_>,
        factory_read: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe_read: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        factory_work: &mut FactoryPhaseWorkV1,
        pending: &mut ReservedValue<PendingActionCurrentV1>,
        pending_work: &mut PendingActionFamilyWorkV1,
        qualification: &ReservedValue<RuntimeRevocationQualificationV1>,
        revocation: &RuntimeRevocationCurrentV1,
        admission_read: &mut WorldFieldReadContextV1<'_, AP, R, AE, AC>,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut RevocationReadWorkV1,
        action_sources: &NormalActionSourcesV1<'_>,
        actions: &mut NormalActionWorkSetV1,
        tail_sources: &NormalTickTailSourcesV1<'_>,
        tail_work: &mut NormalTickTailWorkSetV1,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        product_work: &mut ProductValidationDeliveryWorkV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        audit: &mut WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rules: &mut RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        event_tail: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<NormalTickCurrentDataV1<'p>, NormalTickDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent_source.validate_context(common).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            parent_source
                .validate_module_execution_families_v1(&self.families, common)
                .map_err(|error| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(error)))
                })?;
            input.validate_context(common).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            let compared = crate::world::incremental_normal_sources::root_bytes(
                self.families.roots,
                parent_source.refs().roots,
            )
            .map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            common.charge(8, compared + 512).map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            if self.ctx.transaction.parent_fence() != parent_source.fence()
                || self.families.roots != &**parent_source.refs().roots
                || self.families.head.verified_record_id()
                    != parent_source.refs().head.verified_record_id()
                || input.checked().parent_record_id()
                    != parent_source.refs().core.candidate_record_id()
                || input.context().world_id != parent_source.refs().roots.world_id
                || actions.common_identity() != common.work().identity()
            {
                return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "normal fixed driver actual parent/input/work",
                ))
                .into());
            }
            common
                .charge(1, std::mem::size_of::<NormalTickCurrentDataV1>() as u64)
                .map_err(|e| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
                })?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<NormalTickCurrentDataV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Value(ValueTreeError::Memory(e)))?
                .retain(());
            let prologue = self
                .begin_current_factory_prologue_bounded_v1(
                    factory_caches,
                    factory_families,
                    // A preceding Cognition full step may already have
                    // advanced Current Time within this same parent lease.
                    // The prologue checks this expected value against its
                    // actual Current Time CAS; parent simulation_tick is not
                    // the clock of a later step in the transaction.
                    self.now.checked_sub(1).ok_or_else(|| {
                        NormalTickDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                            BodyMutationError::Invalid("normal full-step Current clock"),
                        ))
                    })?,
                    current,
                    factory_read,
                    recipe_read,
                    factory_work,
                    common,
                    live_owner,
                )
                .map_err(NormalTickDriverErrorV1::Factory)?;
            let queue = self
                .drain_normal_current_queue_v1(
                    parent_source,
                    &prologue,
                    pending,
                    pending_work,
                    qualification,
                    revocation,
                    admission_read,
                    policy_scan,
                    policy_work,
                    action_sources,
                    actions,
                    materials,
                    current,
                    parent,
                    audit,
                    directory,
                    rules,
                    enumeration,
                    event_tail,
                    common,
                    live_owner,
                )
                .map_err(NormalTickDriverErrorV1::Queue)?;
            queue
                .validate_before_tail(
                    self.ctx.transaction,
                    parent_source,
                    common,
                    self.now,
                    &live_owner(),
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let cognition_current = parent_source.refs().storage.current().cognition();
            let disabled = if parent_source
                .refs()
                .storage
                .results()
                .proof_ledger()
                .is_none()
            {
                if tail_sources.cognition.is_some() {
                    return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "absent ledger with supplied enabled service",
                    ))
                    .into());
                }
                Some(self.prepare_disabled_normal_cognition_source_v1(
                    parent_source,
                    cognition_current,
                    &mut tail_work.cognition,
                    common,
                    live_owner,
                )?)
            } else {
                if tail_sources.cognition.is_none() {
                    return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "actual ledger requires its enabled service sources",
                    ))
                    .into());
                }
                None
            };
            let expiry_source = match (&disabled, &tail_sources.cognition) {
                (Some(source), None) => NormalCognitionExpirySourceV1::Disabled(source),
                (None, Some(source)) => NormalCognitionExpirySourceV1::Enabled(source.ledger),
                _ => {
                    return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "normal Cognition full-router mode",
                    ))
                    .into());
                }
            };
            let system_sources = NormalSystemTailSourcesV1 {
                economic: tail_sources.economic,
                recipes: tail_sources.recipes,
                gameplay: tail_sources.gameplay,
                grants: tail_sources.grants,
                threats: tail_sources.threats,
                cognition_current,
                cognition_ledger: expiry_source,
            };
            let system = {
                let economic = NormalEconomicTailWorkV1 {
                    factory: &mut actions.factory_schedule,
                    terminal: &mut tail_work.terminal,
                    recipes: &mut tail_work.recipes,
                    orders: &mut tail_work.orders,
                    world_delivery: &mut tail_work.world_delivery,
                    world: &mut tail_work.world,
                };
                let mut work = NormalSystemTailWorkV1 {
                    economic,
                    gameplay: &mut tail_work.gameplay,
                    organization: &mut tail_work.organization,
                    threat_keys: &mut tail_work.threat_keys,
                    history_index: &mut tail_work.history_index,
                    cognition: &mut tail_work.cognition,
                };
                let mut normal = NormalTailWorkV1::new(common);
                self.execute_normal_system_tail_v1(
                    parent_source,
                    &system_sources,
                    input,
                    continued_cognition,
                    &mut work,
                    product_delivery,
                    product_work,
                    industry_read,
                    sink,
                    materials,
                    current,
                    parent,
                    &mut normal,
                    directory,
                    enumeration,
                    live_owner,
                )?
            };
            // The fixed system tail already performed threat refresh and the
            // one consensus History write. Move that exact owner through the
            // scheduler rather than executing either stage a second time.
            let mut service_work = CurrentCognitionServiceWorkV2 {
                expiry: &mut tail_work.cognition,
                authority: &mut tail_work.authority,
                authority_cells: &mut tail_work.authority_cells,
                pending_entries: &mut tail_work.pending_entries,
                path: &mut tail_work.path,
                max_schema_depth: tail_work.max_schema_depth,
            };
            let (cognition, consensus) = match (&disabled, &tail_sources.cognition) {
                (Some(source), None) => self.execute_normal_cognition_completion_v2(
                    system,
                    &NormalCognitionCompletionSourceV2::Disabled {
                        parent: parent_source,
                        input,
                        current: cognition_current,
                        source,
                    },
                    &mut service_work,
                    parent,
                    common,
                    live_owner,
                )?,
                (None, Some(source)) => {
                    let sources = NormalCognitionCompletionSourcesV2 {
                        parent: parent_source,
                        input,
                        current: cognition_current,
                        ledger: source.ledger,
                        accepted_ledger: source.accepted_ledger,
                        evidence: source.evidence,
                    };
                    self.execute_normal_cognition_completion_v2(
                        system,
                        &NormalCognitionCompletionSourceV2::Enabled(&sources),
                        &mut service_work,
                        parent,
                        common,
                        live_owner,
                    )?
                }
                _ => {
                    return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "normal service mode changed",
                    ))
                    .into());
                }
            };
            let output = NormalTickCurrentDataV1 {
                consensus,
                prologue,
                queue,
                cognition,
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                parent_record: parent_source.refs().core.candidate_record_id(),
                input_material: input.checked().material_id(),
                now: self.now,
                fee,
            };
            output
                .validate(
                    self.ctx.transaction,
                    parent_source,
                    input,
                    common,
                    &live_owner(),
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(output)
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}
