//! The fixed system-tail sequence from the original normal step. Each concrete
//! stage retains its own original batch-routing boundary. This method produces
//! no CompletedTick: Cognition ready/in-flight selection and the formal outer
//! execution boundary remain. Continuation expiry is part of this fixed sequence.
//! Threat refresh and consensus are fixed here after the last routed claim event.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_directory_records::AcceptedFactoryRecipeDirectoryV1,
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_claim_epochs::{ClaimEpochStateSourcesV1, NormalClaimEpochSourcesV1},
    incremental_normal_cognition_expiry::{
        CurrentCognitionExpiryOutcomeV1, CurrentCognitionExpiryWorkV1,
    },
    incremental_normal_gameplay_cycles::{NormalGameplayCycleSourcesV1, NormalGameplayCycleWorkV1},
    incremental_normal_industry_due::delivery::{
        CurrentProductValidationDeliveryIndexV1, ProductValidationDeliveryWorkV1,
    },
    incremental_normal_organization_tail::NormalOrganizationTailWorkV1,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tail::{
        NormalTailSourcesV1, NormalTailWorkV1, economic::NormalEconomicTailWorkV1,
    },
    incremental_normal_tick_finalize::consensus::HistoryWrittenIwsCommitmentV1,
    incremental_normal_world_rule_lifecycle::NormalWorldRuleLifecycleSourcesV1,
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    incremental_cognition_history::PendingCurrentCognitionJournalV1,
    incremental_cognition_proof_ledger::CurrentCognitionProofLedgerV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::IndustryFamilyReadContextV1,
    incremental_main_token_policy_records::RestrictedClaimGrantDirectoryV1,
    incremental_normal_gameplay_cycles_directory::GameplayCycleDirectoryV1,
    incremental_runtime_business::{ThreatKeyDirectoryV1, ThreatKeyDirectoryWorkV1},
    incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
};
/// Every directory is a mandatory genuine parent seed. The methods below
/// reopen their Current selections after earlier stage events have routed.
/// Constructing this readonly view grants no accepted/head/fence permission.
pub(in crate::world) struct NormalSystemTailSourcesV1<'a> {
    pub economic: &'a NormalWorldRuleLifecycleSourcesV1<'a>,
    pub recipes: &'a ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    pub gameplay: &'a ReservedValue<GameplayCycleDirectoryV1>,
    pub grants: &'a ReservedValue<RestrictedClaimGrantDirectoryV1>,
    pub threats: &'a ReservedValue<ThreatKeyDirectoryV1>,
    pub cognition_current: &'a ReservedValue<CompleteCognitionCurrentQualificationV3>,
    pub cognition_ledger:
        crate::world::incremental_normal_cognition_expiry::NormalCognitionExpirySourceV1<'a>,
}
/// No counters are copied or initialized. Claim epochs continue consuming the
/// same world-delivery work used by the preceding economic lifecycle stage.
pub(in crate::world) struct NormalSystemTailWorkV1<'a> {
    pub economic: NormalEconomicTailWorkV1<'a>,
    pub gameplay: &'a mut NormalGameplayCycleWorkV1,
    pub organization: &'a mut NormalOrganizationTailWorkV1,
    pub threat_keys: &'a mut ThreatKeyDirectoryWorkV1,
    pub history_index: &'a mut RuntimeHistoryIndexWorkV1,
    pub cognition: &'a mut CurrentCognitionExpiryWorkV1,
}
/// Actual stage outputs, retaining their complete source and fee owners.
/// This is provisional DATA; only the remaining fixed service phase may carry
/// it onward, and no completed-tick or accepted-cache permission is issued.
pub(in crate::world) struct NormalSystemTailOutcomeV1<'p> {
    consensus: ReservedValue<HistoryWrittenIwsCommitmentV1>,
    cognition_expiry: CurrentCognitionExpiryOutcomeV1<'p>,
    _fee: ReservedValue<()>,
}
impl<'p> NormalSystemTailOutcomeV1<'p> {
    pub(in crate::world) fn into_parts(
        self,
    ) -> (
        ReservedValue<HistoryWrittenIwsCommitmentV1>,
        CurrentCognitionExpiryOutcomeV1<'p>,
    ) {
        (self.consensus, self.cognition_expiry)
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
    pub(in crate::world) fn execute_normal_system_tail_v1<'p, P, CE, CC, PE, PC, IE, IC, ME, S>(
        &mut self,
        parent_source: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        sources: &NormalSystemTailSourcesV1<'_>,
        input: &PaidBoundNormalInputDataV1<'_>,
        continued_cognition: Option<PendingCurrentCognitionJournalV1<'p>>,
        work: &mut NormalSystemTailWorkV1<'_>,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        product_work: &mut ProductValidationDeliveryWorkV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<NormalSystemTailOutcomeV1<'p>, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
    {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let stages = self.execute_normal_system_stages_v1(
                parent_source,
                sources,
                input,
                work,
                product_delivery,
                product_work,
                industry_read,
                sink,
                materials,
                current,
                parent,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            stages
                .validate(
                    self.ctx.transaction,
                    parent_source,
                    input,
                    self.now,
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    &live_owner(),
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let result_bytes = std::mem::size_of::<NormalSystemTailOutcomeV1<'p>>();
            normal
                .charge(result_bytes)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let outcome_fee = normal
                .reserve(result_bytes, MemoryClass::Ordinary)
                .map_err(|error| RootModuleExecutionErrorV1::Body(error.into()))?
                .retain(());
            // Only after all system events and the threat CAS have finished may
            // the consensus stage take its strict Current cut. Its returned
            // owner proves this History write; it is not a reusable Current
            // guard or a completed-tick/acceptance capability.
            let consensus = self.execute_iws_tick_consensus_v1(
                parent_source,
                input,
                sources.gameplay,
                sources.economic.retention,
                work.gameplay,
                work.history_index,
                parent,
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                live_owner,
            )?;
            let common = normal
                .common_mut()
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let cognition_expiry=match &sources.cognition_ledger {
                crate::world::incremental_normal_cognition_expiry::NormalCognitionExpirySourceV1::Enabled(ledger)=>self.execute_current_cognition_expiry_v1(parent_source,&consensus,sources.cognition_current,ledger,continued_cognition,work.cognition,parent,common,live_owner),
                crate::world::incremental_normal_cognition_expiry::NormalCognitionExpirySourceV1::Disabled(source)=>self.execute_disabled_cognition_expiry_v1(parent_source,&consensus,sources.cognition_current,source,continued_cognition,work.cognition,common,live_owner),
            }?;
            Ok(NormalSystemTailOutcomeV1 {
                consensus,
                cognition_expiry,
                _fee: outcome_fee,
            })
        }));
        let result = match outcome {
            Ok(result) => result,
            Err(payload) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(payload)
            }
        };
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}

#[path = "incremental_normal_system_stages.rs"]
mod stages;
pub(in crate::world) use stages::NormalSystemStagesBeforeConsensusV1;
