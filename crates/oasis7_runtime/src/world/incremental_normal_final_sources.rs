//! Actual post-tick side sources captured before directory read-set closure.
//! Only this fixed producer assembles the final Runtime/Cognition/ProofLedger
//! originals. Inherited prefixes remain genuine qualified owners, not copied
//! parent manifests with replacement roots.
use super::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::*,
    incremental_normal_host_finish::{NormalHostFinishCutWorkV1, capture_normal_write_fact_cut_v1},
    incremental_normal_industry_due::delivery::{
        ProductValidationDeliveryWorkV1, read_gameplay_world_event_original_v1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tick_driver::{NormalTickCurrentDataV1, NormalTickTailSourcesV1},
};
use oasis7_consensus::normal_tick_input::PaidBoundNormalInputDataV1;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{
        ExecutionFactKindV1, ExecutionFactRefV1, ExecutionFence, ExecutionTransaction,
        PreparedGuard, RootSetV1,
    },
    incremental_material::{FactLocationV1, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StateNamespaceV1, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::{
        CognitionHistoryHeadV1, CognitionHistoryIndexRefV1, CognitionHistoryViewV1,
        PendingCurrentCognitionJournalV1,
    },
    incremental_cognition_proof_ledger::FrozenCognitionProofLedgerManifestSourceV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*,
};
use std::sync::Arc;
#[path = "incremental_normal_final_sources_capture.rs"]
mod capture;
/// Original independent caller-owned counters; all three phases borrow the
/// same remaining values, including failed attempts and final Original IO.
pub struct NormalFinalSourcesWorkV1 {
    pub projection: super::incremental_normal_projection_commitment::NormalProjectionWorkBankV1,
    pub business: super::incremental_normal_business_commitment::NormalBusinessReadWorkV1,
    pub terminal_metadata:
        super::incremental_normal_business_commitment::NormalTerminalMetadataReadWorkV1,
    pub runtime_index: RuntimeHistoryIndexWorkV1,
    pub originals: ProductValidationDeliveryWorkV1,
    pub cognition_hash: CognitionHeadHashWorkV1,
    pub authorization:
        super::incremental_authorization_root_current::AuthorizationRootSourceReadWorkV1,
    pub authorization_pending_entries: usize,
    pub final_originals_remaining: u64,
    pub final_bytes_remaining: u64,
}
pub(in crate::world) struct CapturedNormalFinalSourcesV1 {
    retention: super::incremental_normal_retention::FrozenNormalRetentionV1,
    projection: super::incremental_normal_projection_commitment::FrozenNormalProjectionCommitmentV1,
    business_delta: super::incremental_normal_business_commitment::ActualCurrentBusinessDeltaV1,
    terminal_metadata: super::incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    event_commitment: super::incremental_normal_event_commitment::NormalEventCommitmentV1,
    runtime: [ReservedValue<RuntimeHistoryIndexQualificationV1>; 4],
    previous_runtime: [u8; 32],
    max_snapshots: u64,
    archived_ticks: u64,
    cognition: CapturedCognitionViewV1,
    manifest: ReservedValue<
        super::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1,
    >,
    authorization: ReservedValue<
        super::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    >,
    delivery: ReservedValue<
        super::incremental_normal_industry_due::delivery::FrozenProductValidationDeliveryIndexV1,
    >,
    journal_frontier: Option<
        oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1,
    >,
    economy_frontier:
        Option<Arc<ReservedValue<super::normal_bootstrap_current::BootstrapEconomyFrontiersV1>>>,
    ledger: Option<ReservedValue<FrozenCognitionProofLedgerManifestSourceV1>>,
    cognition_evidence:
        Option<super::incremental_normal_cognition_service_evidence::FrozenCognitionWakeEvidenceV2>,
    before_guard: PreparedGuard,
    after_guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    world_id: String,
    _fee: MemoryReservation,
}
pub(in crate::world) struct CapturedCognitionViewV1 {
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    parent_fence: ExecutionFence,
    parent_record: [u8; 32],
    enabled: bool,
    revision: u64,
    previous: FactLocationV1,
    heads: [CognitionHistoryHeadV1; 3],
    indexes: [CognitionHistoryIndexRefV1; 4],
}
/// Borrowed only inside the genuine final-capture frame, before projection
/// freezes that same terminal cut. No public root/count/boolean constructor.
pub(in crate::world) struct NormalCognitionProjectionInputsV1<'a> {
    view: &'a CapturedCognitionViewV1,
    retention: &'a super::incremental_normal_retention::FrozenNormalRetentionV1,
    ledger: Option<&'a ReservedValue<FrozenCognitionProofLedgerManifestSourceV1>>,
    parent_ledger: Option<&'a ReservedValue<oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1>>,
    journal_frontier: Option<&'a oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1>,
    economy_frontier: Option<&'a Arc<ReservedValue<super::normal_bootstrap_current::BootstrapEconomyFrontiersV1>>>,
}
impl NormalCognitionProjectionInputsV1<'_> {
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.retention.validate_current(tx, common, live)?;
        common.consume_steps(1).map_err(formal)?;
        tx.validate_read_source_guard(&self.view.guard, live)?;
        if self.view.core != common.work().identity()
            || !self.view.memory.same_pool(common.memory())
            || !tx.uses_budget(common.memory())
            || self.view.parent_fence != tx.parent_fence()
            || self.ledger.is_some_and(|v| !v.uses_budget(common.memory()))
            || self
                .economy_frontier
                .is_some_and(|v| !v.uses_budget(common.memory()))
        {
            return Err(bad());
        }
        if let Some(parent) = self.parent_ledger {
            common.charge(4, 96).map_err(formal)?;
            if !parent.uses_budget(common.memory())
                || !parent.occurrence().uses_budget(common.memory())
                || parent.occurrence().allocation_class() != MemoryClass::Ordinary
                || parent.occurrence().head_record_id() != self.view.parent_record
                || parent.occurrence().head_body_id() != self.view.parent_fence.parent_commit
                || parent.manifest().roots != self.view.parent_fence.parent_roots
            {
                return Err(bad());
            }
        }
        if self.parent_ledger.is_some() != self.ledger.is_some() {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn retention(
        &self,
    ) -> &super::incremental_normal_retention::FrozenNormalRetentionV1 {
        self.retention
    }
    pub(in crate::world) fn parent_ledger(
        &self,
    ) -> Option<
        &ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
        >,
    > {
        self.parent_ledger
    }
    pub(in crate::world) fn enabled(&self) -> bool {
        self.view.enabled
    }
    pub(in crate::world) fn revision(&self) -> u64 {
        self.view.revision
    }
    pub(in crate::world) fn heads(&self) -> &[CognitionHistoryHeadV1; 3] {
        &self.view.heads
    }
    pub(in crate::world) fn indexes(&self) -> &[CognitionHistoryIndexRefV1; 4] {
        &self.view.indexes
    }
    pub(in crate::world) fn ledger(
        &self,
    ) -> Option<&ReservedValue<FrozenCognitionProofLedgerManifestSourceV1>> {
        self.ledger
    }
    pub(in crate::world) fn journal_frontier(&self)->Option<&oasis7_world_state::incremental_cognition_history::RetainedWrittenCurrentCognitionJournalV1>{
        self.journal_frontier
    }
    pub(in crate::world) fn economy_frontier(
        &self,
    ) -> Option<&Arc<ReservedValue<super::normal_bootstrap_current::BootstrapEconomyFrontiersV1>>>
    {
        self.economy_frontier
    }
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("normal final actual side sources")
}
fn formal(e: FormalError) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
impl CapturedNormalFinalSourcesV1 {
    pub(in crate::world) fn cognition_evidence_capture(
        &self,
    ) -> Option<&super::incremental_normal_cognition_service_evidence::FrozenCognitionWakeEvidenceV2>
    {
        self.cognition_evidence.as_ref()
    }

    pub(in crate::world) fn owns_cognition_evidence_capture(
        &self,
        value: &super::incremental_normal_cognition_service_evidence::FrozenCognitionWakeEvidenceV2,
    ) -> bool {
        self.cognition_evidence
            .as_ref()
            .is_some_and(|actual| std::ptr::eq(actual, value))
    }

    pub(in crate::world) fn validate_cognition_evidence_current_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        if let Some(evidence) = &self.cognition_evidence {
            evidence.validate_guard(tx, common, live)?;
        }
        if self.ledger.is_some() != self.cognition_evidence.is_some() {
            return Err(bad());
        }
        Ok(())
    }

    pub(in crate::world) fn projection_capture(
        &self,
    ) -> &super::incremental_normal_projection_commitment::FrozenNormalProjectionCommitmentV1 {
        &self.projection
    }
    pub(in crate::world) fn owns_projection_capture(
        &self,
        value: &super::incremental_normal_projection_commitment::FrozenNormalProjectionCommitmentV1,
    ) -> bool {
        std::ptr::eq(value, &self.projection)
    }

    pub(in crate::world) fn business_capture(
        &self,
    ) -> &super::incremental_normal_business_commitment::ActualCurrentBusinessDeltaV1 {
        &self.business_delta
    }
    pub(in crate::world) fn terminal_metadata_capture(
        &self,
    ) -> &super::incremental_normal_business_commitment::NormalTerminalReadMetadataV1 {
        &self.terminal_metadata
    }
    pub(in crate::world) fn owns_business_capture(
        &self,
        value: &super::incremental_normal_business_commitment::ActualCurrentBusinessDeltaV1,
    ) -> bool {
        std::ptr::eq(value, &self.business_delta)
    }
    pub(in crate::world) fn owns_terminal_metadata_capture(
        &self,
        value: &super::incremental_normal_business_commitment::NormalTerminalReadMetadataV1,
    ) -> bool {
        std::ptr::eq(value, &self.terminal_metadata)
    }

    pub(in crate::world) fn validate_tick_capture<M: MerkleNodeReader>(
        &self,
        tick: &NormalTickCurrentDataV1<'_>,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.retention.validate_current(tx, common, live)?;
        self.projection.validate_current_v1(tx, common, live)?;
        self.business_delta.validate_provenance(tx, common, live)?;
        self.terminal_metadata
            .validate_provenance(tx, common, live)?;
        common.consume_steps(1).map_err(formal)?;
        if let Some(frontier) = &self.journal_frontier {
            frontier.validate_evidence_v1(tx, common.memory(), live)?;
        }
        if let Some(frontiers) = &self.economy_frontier {
            frontiers.validate(tx, common.memory(), live)?;
        }
        self.validate_cognition_evidence_current_v1(tx, common, live)?;
        self.event_commitment
            .validate_capture(tx, &self.before_guard, common, live)?;
        tx.validate_guard(&self.after_guard, live)?;
        tx.validate_transaction_identity(&self.before_guard)?;
        if !self.before_guard.same_cut(tick.final_source_guard())
            || self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || self.runtime.iter().any(|q| !q.uses_budget(common.memory()))
            || self
                .ledger
                .as_ref()
                .is_some_and(|q| !q.uses_budget(common.memory()))
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in crate::world) fn retention_capture(
        &self,
    ) -> &super::incremental_normal_retention::FrozenNormalRetentionV1 {
        &self.retention
    }
    pub(in crate::world) fn owns_retention_capture(
        &self,
        source: &super::incremental_normal_retention::FrozenNormalRetentionV1,
    ) -> bool {
        std::ptr::eq(&self.retention, source)
    }
    pub(in crate::world) fn event_capture(
        &self,
    ) -> &super::incremental_normal_event_commitment::NormalEventCommitmentV1 {
        &self.event_commitment
    }
    pub(in crate::world) fn owns_event_capture(
        &self,
        source: &super::incremental_normal_event_commitment::NormalEventCommitmentV1,
    ) -> bool {
        std::ptr::eq(&self.event_commitment, source)
    }
    pub(in crate::world) fn manifest_capture(
        &self,
    ) -> &super::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1 {
        &self.manifest
    }
    pub(in crate::world) fn authorization_capture(
        &self,
    ) -> &super::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1 {
        &self.authorization
    }
    pub(in crate::world) fn delivery_capture(
        &self,
    ) -> &super::incremental_normal_industry_due::delivery::FrozenProductValidationDeliveryIndexV1
    {
        &self.delivery
    }
    pub(in crate::world) fn owns_manifest_capture(
        &self,
        source: &super::incremental_module_execution_manifest_hash::CurrentWorldManifestHashV1,
    ) -> bool {
        std::ptr::eq(&*self.manifest, source)
    }
    pub(in crate::world) fn owns_delivery_capture(
        &self,
        source:&super::incremental_normal_industry_due::delivery::FrozenProductValidationDeliveryIndexV1,
    ) -> bool {
        std::ptr::eq(&*self.delivery, source)
    }
    pub(in crate::world) fn owns_authorization_capture(
        &self,
        source:&super::incremental_authorization_root_current::FrozenNormalAuthorizationProjectionV1,
    ) -> bool {
        std::ptr::eq(&*self.authorization, source)
    }
    pub(in crate::world) fn has_ledger(&self) -> bool {
        self.ledger.is_some()
    }
    fn check_after(
        &self,
        after: &RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FinalNormalOriginalErrorV1<std::convert::Infallible>> {
        common
            .charge(
                1,
                (self.world_id.len() + after.world_id.len() + 4096) as u64,
            )
            .map_err(FinalNormalOriginalErrorV1::Context)?;
        if self.core != common.work().identity()
            || !self._fee.uses_budget(common.memory())
            || after.simulation_tick != self.now
            || after.world_id != self.world_id
        {
            return Err(FinalNormalOriginalErrorV1::Body(bad()));
        }
        Ok(())
    }
    fn metadata(
        &self,
        after: &RootSetV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<
        (
            ReservedValue<RuntimeHistoryManifestV1>,
            ReservedValue<CognitionHistoryViewV1>,
        ),
        BodyMutationError,
    > {
        common.charge(16, 16 * 512).map_err(formal)?;
        let runtime_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<RuntimeHistoryManifestV1>() + 4 * 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let cog_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CognitionHistoryViewV1>() + 4 * 512,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let _scratch = common
            .memory()
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let roots = after.digest()?;
        let runtime = runtime_fee.retain(RuntimeHistoryManifestV1 {
            roots,
            previous_view: Some(self.previous_runtime),
            lanes: std::array::from_fn(|i| self.runtime[i].window().clone()),
            max_snapshots: self.max_snapshots,
            archived_tick_count: self.archived_ticks,
        });
        runtime.validate()?;
        let cognition = cog_fee.retain(CognitionHistoryViewV1 {
            roots,
            enabled: self.cognition.enabled,
            revision: self.cognition.revision,
            previous_view: Some(self.cognition.previous),
            heads: self.cognition.heads.clone(),
            indexes: self.cognition.indexes.clone(),
        });
        cognition.validate_shape()?;
        Ok((runtime, cognition))
    }
    pub(in crate::world) fn produce_pair<O: OriginalObjectStreamReader>(
        &self,
        after: &RootSetV1,
        originals: &mut O,
        work: &mut NormalFinalSourcesWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<[ExecutionFactRefV1; 2], FinalNormalOriginalErrorV1<O::Error>> {
        self.check_after(after, common)
            .map_err(convert_infallible)?;
        if self.ledger.is_some() {
            return Err(FinalNormalOriginalErrorV1::Body(bad()));
        }
        let (runtime, cognition) = self
            .metadata(after, common)
            .map_err(FinalNormalOriginalErrorV1::Body)?;
        common
            .charge(2, 8192)
            .map_err(FinalNormalOriginalErrorV1::Context)?;
        let _scratch = common
            .memory()
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(|e| FinalNormalOriginalErrorV1::Body(ValueTreeError::Memory(e).into()))?;
        let r = runtime
            .canonical_bytes()
            .map_err(|e| FinalNormalOriginalErrorV1::Body(e.into()))?;
        let c = encode_cognition(&cognition, common)?;
        Ok([
            persist(&r, originals, work, common)?,
            persist(&*c, originals, work, common)?,
        ])
    }
    pub(in crate::world) fn produce_three<O: OriginalObjectStreamReader>(
        &self,
        after: &RootSetV1,
        originals: &mut O,
        work: &mut NormalFinalSourcesWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<[ExecutionFactRefV1; 3], FinalNormalOriginalErrorV1<O::Error>> {
        self.check_after(after, common)
            .map_err(convert_infallible)?;
        let ledger = self
            .ledger
            .as_ref()
            .ok_or_else(|| FinalNormalOriginalErrorV1::Body(bad()))?;
        let (runtime, cognition) = self
            .metadata(after, common)
            .map_err(FinalNormalOriginalErrorV1::Body)?;
        let ledger = ledger
            .manifest(after, common)
            .map_err(|e| FinalNormalOriginalErrorV1::Body(e.into()))?;
        common
            .charge(3, 12288)
            .map_err(FinalNormalOriginalErrorV1::Context)?;
        let _scratch = common
            .memory()
            .reserve(12288, MemoryClass::Ordinary)
            .map_err(|e| FinalNormalOriginalErrorV1::Body(ValueTreeError::Memory(e).into()))?;
        let r = runtime
            .canonical_bytes()
            .map_err(|e| FinalNormalOriginalErrorV1::Body(e.into()))?;
        let c = encode_cognition(&cognition, common)?;
        let l = ledger
            .canonical_bytes()
            .map_err(|e| FinalNormalOriginalErrorV1::Body(e.into()))?;
        Ok([
            persist(&r, originals, work, common)?,
            persist(&*c, originals, work, common)?,
            persist(&l, originals, work, common)?,
        ])
    }
}
#[derive(Debug)]
pub(in crate::world) enum FinalNormalOriginalErrorV1<OE> {
    Body(BodyMutationError),
    Context(FormalError),
    Original(OE),
}
fn convert_infallible<OE>(
    e: FinalNormalOriginalErrorV1<std::convert::Infallible>,
) -> FinalNormalOriginalErrorV1<OE> {
    match e {
        FinalNormalOriginalErrorV1::Body(e) => FinalNormalOriginalErrorV1::Body(e),
        FinalNormalOriginalErrorV1::Context(e) => FinalNormalOriginalErrorV1::Context(e),
        FinalNormalOriginalErrorV1::Original(e) => match e {},
    }
}
fn persist<O: OriginalObjectStreamReader>(
    bytes: &[u8],
    originals: &mut O,
    work: &mut NormalFinalSourcesWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ExecutionFactRefV1, FinalNormalOriginalErrorV1<O::Error>> {
    if !originals.allocation_budget().same_pool(common.memory())
        || originals.allocation_class() != MemoryClass::Ordinary
    {
        return Err(FinalNormalOriginalErrorV1::Body(bad()));
    }
    work.final_originals_remaining =
        work.final_originals_remaining
            .checked_sub(1)
            .ok_or_else(|| {
                FinalNormalOriginalErrorV1::Body(
                    ValueTreeError::ResourceLimited("normal final originals").into(),
                )
            })?;
    work.final_bytes_remaining = work
        .final_bytes_remaining
        .checked_sub(bytes.len() as u64)
        .ok_or_else(|| {
            FinalNormalOriginalErrorV1::Body(
                ValueTreeError::ResourceLimited("normal final bytes").into(),
            )
        })?;
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.chunks(4096) {
        common
            .charge(1, chunk.len() as u64)
            .map_err(FinalNormalOriginalErrorV1::Context)?;
        hash.update(chunk);
    }
    let id = *hash.finalize().as_bytes();
    common
        .charge(1, bytes.len() as u64)
        .map_err(FinalNormalOriginalErrorV1::Context)?;
    let writer = originals.original_writer().ok_or_else(|| {
        FinalNormalOriginalErrorV1::Body(BodyMutationError::Invalid(
            "normal final actual Original writer required",
        ))
    })?;
    let actual = writer
        .write_original(
            &mut std::io::Cursor::new(bytes),
            bytes.len() as u64,
            id,
            common.memory(),
        )
        .map_err(FinalNormalOriginalErrorV1::Original)?;
    if actual != id {
        return Err(FinalNormalOriginalErrorV1::Body(bad()));
    }
    common
        .consume_steps(0)
        .map_err(FinalNormalOriginalErrorV1::Context)?;
    Ok(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Result,
        id,
    })
}

fn encode_cognition<OE>(
    view: &CognitionHistoryViewV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<
    ReservedValue<
        [u8; oasis7_world_state::incremental_cognition_history::COGNITION_HISTORY_VIEW_BYTES],
    >,
    FinalNormalOriginalErrorV1<OE>,
> {
    let memory = common.memory().clone();
    let mut first = None;
    let encoded = view.canonical_bytes(&memory, || match common.consume_steps(0) {
        Ok(()) => false,
        Err(error) => {
            if first.is_none() {
                first = Some(error);
            }
            true
        }
    });
    if let Some(error) = first {
        return Err(FinalNormalOriginalErrorV1::Context(error));
    }
    encoded.map_err(|e| FinalNormalOriginalErrorV1::Body(e.into()))
}

use oasis7_world_state::incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1;
