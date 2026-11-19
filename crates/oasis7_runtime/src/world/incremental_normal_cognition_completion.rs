//! Fixed tail-to-scheduler handoff. The actual consensus and expiry outputs
//! are consumed together; callers cannot substitute a historical written cut
//! or lose the Journal frontier between the two concrete phases.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::{RootModuleExecutionContextV1, RootModuleExecutionErrorV1},
    incremental_normal_cognition_service::{
        CurrentCognitionServiceOutcomeV2, CurrentCognitionServiceSourcesV2,
        CurrentCognitionServiceWorkV2,
    },
    incremental_normal_cognition_service_evidence::AcceptedCognitionWakeEvidenceV2,
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_system_tail::NormalSystemTailOutcomeV1,
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CompleteCognitionCurrentQualificationV3,
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, CurrentCognitionProofLedgerV1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
/// Borrow only real paired parent/Current sources. No written consensus or
/// expiry output is accepted here: both come from the consumed fixed tail.
pub(in crate::world) struct NormalCognitionCompletionSourcesV2<'a, 'p> {
    pub parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    pub input: &'a PaidBoundNormalInputDataV1<'a>,
    pub current: &'a ReservedValue<CompleteCognitionCurrentQualificationV3>,
    pub ledger: &'a CurrentCognitionProofLedgerV1,
    pub accepted_ledger: &'a ReservedValue<AcceptedCognitionProofLedgerV1>,
    pub evidence: &'a ReservedValue<AcceptedCognitionWakeEvidenceV2>,
}
/// The Disabled case carries a genuinely minted closed-routing absence owner;
/// it is not selected by a bare optional ledger or config.enabled bit.
pub(in crate::world) enum NormalCognitionCompletionSourceV2<'a, 'p> {
    Enabled(&'a NormalCognitionCompletionSourcesV2<'a, 'p>),
    Disabled {
        parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
        input: &'a PaidBoundNormalInputDataV1<'a>,
        current: &'a ReservedValue<CompleteCognitionCurrentQualificationV3>,
        source: &'a ReservedValue<
            crate::world::incremental_normal_cognition_expiry::PaidDisabledNormalCognitionSourceV1,
        >,
    },
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_cognition_completion_v2<'p, P, PE, PC>(
        &mut self,
        tail: NormalSystemTailOutcomeV1<'p>,
        sources: &NormalCognitionCompletionSourceV2<'_, 'p>,
        work: &mut CurrentCognitionServiceWorkV2<'_>,
        parent_read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (CurrentCognitionServiceOutcomeV2<'p>, ReservedValue<crate::world::incremental_normal_tick_finalize::consensus::HistoryWrittenIwsCommitmentV1>),
        RootModuleExecutionErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let (written, expiry) = tail.into_parts();
        let result = match sources {
            NormalCognitionCompletionSourceV2::Enabled(sources) => {
                let service = CurrentCognitionServiceSourcesV2 {
                    parent: sources.parent,
                    written: &written,
                    input: sources.input,
                    current: sources.current,
                    ledger: sources.ledger,
                    accepted_ledger: sources.accepted_ledger,
                    evidence: sources.evidence,
                };
                self.execute_current_cognition_scheduler_v2(
                    &service,
                    expiry,
                    work,
                    parent_read,
                    common,
                    live_owner,
                )
            }
            NormalCognitionCompletionSourceV2::Disabled {
                parent,
                input,
                current,
                source,
            } => self.execute_disabled_current_cognition_scheduler_v2(
                parent,
                input,
                current,
                source,
                &written,
                expiry,
                work.expiry,
                common,
                live_owner,
            ),
        };
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        // Retain the actual single system-tail History write across the
        // scheduler and final tick validation, without cloning its owner or
        // reusing its historical strict guard as a current permission.
        result.map(|outcome| (outcome, written))
    }
}
