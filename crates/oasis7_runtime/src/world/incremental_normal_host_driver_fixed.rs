//! The fixed external facade. Persistence adapters are IO, not execution
//! callbacks; only the original normal dispatcher can run inside this scope.
use super::*;
use crate::world::{
    incremental_normal_final_sources::FinalNormalOriginalErrorV1,
    incremental_normal_host_resident::NormalHostResidentSourcesV1,
};
use oasis7_formal_protocol::incremental_execution::{ExecutionParent, TransactionLimits};
pub enum NormalHostCognitionParentSourcesV1<'a> {
    Absent,
    Present { accepted:&'a ReservedValue<oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1>, evidence:&'a ReservedValue<crate::world::incremental_normal_cognition_service_evidence::AcceptedCognitionWakeEvidenceV2> },
}
pub struct PreparedNormalHostRuntimeV1 {
    inner: crate::world::incremental_commit::PreparedNormalCommitV1<'static>,
}
impl PreparedNormalHostRuntimeV1 {
    pub fn retention_manifest_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        ReservedValue<oasis7_formal_protocol::incremental_retention::RetentionLedgerManifestV1>,
        BodyMutationError,
    > {
        self.inner.retention_manifest_v1(common)
    }
    pub fn candidate(&self) -> &crate::world::incremental_commit::PreparedRuntimeCommit {
        self.inner.candidate()
    }
    /// Validates the held terminal seal and every paid preparation owner.
    /// This is DATA lineage validation, not a publication/acceptance permit.
    pub fn validate_context_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.inner.validate_context_v1(common)
    }
    pub fn formal_projection_root_bounded_v1<'a>(
        &'a self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<&'a str, BodyMutationError> {
        self.inner.formal_projection_root_v1(common)
    }
    /// Original typed side value from the exact sealed projection capture.
    /// This is DATA; cold admission must recompute the signed projection root.
    pub fn retained_history_root_json_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        self.inner.retained_history_root_json_v1(common)
    }
    pub fn business_source_bounded_v1<'a>(
        &'a self,
        parent: &'a BoundNormalTickParentV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        crate::world::incremental_normal_business_commitment::PaidBorrowedNormalBusinessSourceV1<
            'a,
        >,
        BodyMutationError,
    > {
        self.inner.business_source_v1(parent, common)
    }
    /// The Frozen material must be the exact input that this sealed tick used.
    pub fn validate_input_source_bounded_v1(
        &self,
        input: &PaidBoundNormalInputDataV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.inner.validate_input_source_v1(input, common)
    }
    pub fn validate_business_source_bounded_v1(
        &self,
        source: &crate::world::incremental_normal_business_commitment::ActualCurrentBusinessDeltaV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        self.inner.validate_business_source_v1(source, common)
    }
    /// Read-only formal output DATA from this actual terminal seal/capture.
    pub fn formal_event_commitment_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<([u8; 32], usize), BodyMutationError> {
        self.inner.formal_event_commitment_v1(common)
    }
    pub fn formal_governing_manifest_digest_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<ReservedValue<String>, BodyMutationError> {
        self.inner
            .formal_governing_manifest_digest_v1(common, canonical)
    }
    pub(in crate::world) fn into_inner(
        self,
    ) -> crate::world::incremental_commit::PreparedNormalCommitV1<'static> {
        self.inner
    }
}
pub struct NormalHostFixedFailureV1<OE> {
    kind: FixedFailure<OE>,
}
enum FixedFailure<OE> {
    Body(BodyMutationError),
    Preparation(NormalHostPreparationErrorV1<OE, FinalNormalOriginalErrorV1<OE>>),
}
impl<OE: std::fmt::Debug> std::fmt::Debug for NormalHostFixedFailureV1<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            FixedFailure::Body(e) => f.debug_tuple("Body").field(e).finish(),
            FixedFailure::Preparation(e) => f.debug_tuple("Preparation").field(e).finish(),
        }
    }
}
impl<OE> NormalHostFixedFailureV1<OE> {
    fn body(e: BodyMutationError) -> Self {
        Self {
            kind: FixedFailure::Body(e),
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_normal_host_from_resident_v1<'p, 'common, M, R, E, C, W, F, O, S>(
    parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    input: &PaidBoundNormalInputDataV1<'_>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    fee_sink: &mut W,
    event_sink: &mut W,
    originals: &mut O,
    persist_original: F,
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    resident: &'p NormalHostResidentSourcesV1,
    compiler: &'p crate::module_source_compiler::RuntimeSourceCompiler,
    directories: NormalTailDirectoryInputsV1<'p>,
    cognition: NormalHostCognitionParentSourcesV1<'p>,
    work: &mut NormalHostWorkV1<'common>,
    take_state_error: E,
    cancelled: C,
    input_limits: &CborDataLimitsV1,
    max_action_bytes: usize,
    max_grant_payload_bytes: u64,
    sink: &mut S,
    policy: crate::world::incremental_commit::NormalCommitHeaderLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<PreparedNormalHostRuntimeV1, NormalHostFixedFailureV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader
        + crate::world::PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
    O::Error: std::error::Error + Send + Sync + 'static,
    S: ObservationJsonOriginalSinkV2,
    S::Error: std::fmt::Debug + Send + Sync + 'static,
{
    parent
        .validate_context(common)
        .map_err(|e| NormalHostFixedFailureV1::body(BodyMutationError::Original(Box::new(e))))?;
    input
        .validate_context(common)
        .map_err(|e| NormalHostFixedFailureV1::body(BodyMutationError::Original(Box::new(e))))?;
    // Pay the sole bounded RootSet clone, then derive all identity from genuine
    // parent/input owners. Limits are caller resources, never qualifications.
    common
        .charge(1, 8192)
        .map_err(|e| NormalHostFixedFailureV1::body(BodyMutationError::Original(Box::new(e))))?;
    let _scope = common
        .memory()
        .reserve(8192, oasis7_proto::memory_budget::MemoryClass::Ordinary)
        .map_err(|e| NormalHostFixedFailureV1::body(ValueTreeError::Memory(e).into()))?;
    let memory = common.memory().clone();
    let mut operation = [0u8; 32];
    let digest = &input.checked().reservation().body().request_digest;
    common
        .charge(1, digest.len() as u64)
        .map_err(|e| NormalHostFixedFailureV1::body(BodyMutationError::Original(Box::new(e))))?;
    hex::decode_to_slice(digest, &mut operation).map_err(|_| {
        NormalHostFixedFailureV1::body(BodyMutationError::Invalid("normal frozen actual operation"))
    })?;
    let mut tx = ExecutionTransaction::begin(
        ExecutionParent {
            commit: parent.fence().parent_commit,
            roots: (**parent.refs().roots).clone(),
            authority: parent.fence().authority,
        },
        operation,
        state,
        &memory,
        TransactionLimits {
            max_reads: policy.transaction.max_reads,
            max_writes: policy.transaction.max_writes,
            max_facts: policy.transaction.max_facts,
            max_pending_node_bytes: policy.transaction.max_pending_node_bytes,
        },
    )
    .map_err(|e| NormalHostFixedFailureV1::body(e.into()))?;
    let current_ledger=match &cognition { NormalHostCognitionParentSourcesV1::Absent=>None, NormalHostCognitionParentSourcesV1::Present{accepted,..}=>Some(oasis7_world_state::incremental_cognition_proof_ledger::begin_cognition_proof_ledger_current_v1(accepted,parent.refs().head,&mut tx,common,&live_owner()).map_err(|e|NormalHostFixedFailureV1::body(e.into()))?) };
    let cognition = match (cognition, current_ledger.as_ref()) {
        (NormalHostCognitionParentSourcesV1::Absent, None) => {
            NormalTailCognitionInputsV1::NoSources
        }
        (NormalHostCognitionParentSourcesV1::Present { accepted, evidence }, Some(ledger)) => {
            NormalTailCognitionInputsV1::Present {
                ledger,
                accepted,
                evidence,
            }
        }
        _ => {
            return Err(NormalHostFixedFailureV1::body(BodyMutationError::Invalid(
                "normal actual ledger source",
            )));
        }
    };
    let mut authorization = resident
        .begin_authorization(parent, &tx, common, &live_owner())
        .map_err(NormalHostFixedFailureV1::body)?;
    let sources = resident.sources(compiler);
    let prepared = prepare_normal_host_runtime_v1(
        parent,
        input,
        tx,
        state,
        values,
        member_sink,
        fee_sink,
        event_sink,
        originals,
        persist_original,
        worker,
        &sources,
        &mut authorization,
        directories,
        cognition,
        work,
        take_state_error,
        cancelled,
        input_limits,
        max_action_bytes,
        max_grant_payload_bytes,
        sink,
        None,
        policy,
        common,
        live_owner,
    )
    .map_err(|e| NormalHostFixedFailureV1 {
        kind: FixedFailure::Preparation(e),
    })?;
    Ok(PreparedNormalHostRuntimeV1 {
        inner: prepared
            .detach_after_actual_capture()
            .map_err(NormalHostFixedFailureV1::body)?,
    })
}
