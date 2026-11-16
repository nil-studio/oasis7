//! Recovery-to-Ordinary source admission. Ledger's fixed Common kernel runs
//! outside the scalar child; all other logical/physical lanes share that child.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_history::*,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_history_source::*,
    incremental_world_current_complete::*,
    incremental_world_raw_complete::MailboxProvenanceScopeV1,
};

pub(super) struct OrdinaryColdSourcesV1 {
    pub(super) current: ReservedValue<WorldCurrentStateQualificationV1>,
    pub(super) results: ReservedValue<
        oasis7_world_state::incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1,
    >,
    pub(super) roots: ReservedValue<RootSetV1>,
    pub(super) runtime: ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub(super) cognition: ReservedValue<CompleteCognitionHistorySourceV3>,
    pub(super) ledger: Option<
        ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
        >,
    >,
    pub(super) retention:Option<ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>>,
    _outer: ReservedValue<()>,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn readmit_complete_sources_bounded_v1(
    pages: &crate::incremental_page_store::IncrementalPageStore,
    checkpoint: &super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    recovered: &super::cold_recovery_paid::RecoveredColdSourcesV1,
    core: &ReservedValue<oasis7_formal_protocol::incremental_material::CheckedCommitCore>,
    work: &mut IwsColdParentWorkV1,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<OrdinaryColdSourcesV1, FormalError> {
    checkpoint.validate_context(common)?;
    common.charge(32, 1024 * 16)?;
    let head = checkpoint.head();
    let memory = pages.memory_budget();
    let roots = &*recovered.roots;
    let current = &recovered.current;
    let runtime = &recovered.runtime;
    let cognition = &recovered.cognition;
    let results = &recovered.results;
    if !memory.same_pool(common.memory())
        || current.allocation_class() != MemoryClass::Recovery
        || results.allocation_class() != MemoryClass::Recovery
        || runtime.allocation_class() != MemoryClass::Recovery
        || cognition.selected().occurrence().allocation_class() != MemoryClass::Recovery
    {
        return Err(invalid(
            "IWS explicit Recovery to Ordinary parent admission",
        ));
    }
    if results.proof_ledger().is_some() != recovered.ledger.is_some() {
        return Err(invalid("IWS cold complete ProofLedger role selection"));
    }
    let ledger = recovered.ledger.as_ref().map(|ledger|
        oasis7_world_state::incremental_cognition_proof_ledger::readmit_cognition_proof_ledger_ordinary_v1(
            ledger, head, roots, common).map_err(|e| invalid(format!("IWS cold ProofLedger ordinary admission: {e:?}"))))
        .transpose()?;
    if core.retention_ledger_artifact_id().is_some() != recovered.retention.is_some() {
        return Err(invalid("cold retention exact signed role selection"));
    }
    let retention=recovered.retention.as_ref().map(|source|oasis7_world_state::incremental_cognition_retention_ledger::readmit_cognition_retention_ordinary_v1(source,head,core,common).map_err(|e|invalid(format!("cold retention ordinary admission: {e:?}")))).transpose()?;
    super::cold_io::run(work, common, |work, cancelled| {
        let physical = std::sync::atomic::AtomicBool::new(false);
        let stop = std::cell::RefCell::new(cancelled);
        let state_loan = super::cold_page_loans::StateLoan::new(
            pages,
            &mut work.state_ordinary,
            MemoryClass::Ordinary,
            &physical,
            None,
        )
        .map_err(page_store_formal_error)?;
        let state = state_loan.source();
        let values_loan =
            super::cold_page_loans::ValueLoan::new(pages, &mut work.values_ordinary, &physical);
        let values = values_loan.source();
        let mut ctx = WorldFieldReadContextV1 {
            root: roots
                .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Allocator),
            state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut work.read_ordinary,
            },
            key_work: &mut work.keys_ordinary,
            semantic_bytes: work.semantic_bytes,
            record_bytes: work.record_bytes,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e)))
            },
            cancelled: || {
                let cancelled = (*stop.borrow_mut())();
                if cancelled {
                    physical.store(true, std::sync::atomic::Ordering::Release);
                }
                cancelled
            },
        };
        let hash_work = &mut work.hashes;
        let work = &mut work.admission;
        let roots_id = roots.digest().map_err(|_| invalid("IWS cold roots"))?;
        let id = head.verified_record_id();
        if !results.uses_budget(memory)
            || results.record_id() != id
            || results.roots_id() != roots_id
            || results.body_id() != runtime.occurrence().head_body_id()
            || results.cognition().occurrence().location()
                != cognition.selected().occurrence().location()
            || results.runtime_history().occurrence().location() != runtime.occurrence().location()
            || runtime.occurrence().head_record_id() != id
            || cognition.selected().occurrence().head_record_id() != id
            || runtime.manifest().roots != roots_id
            || cognition.selected().manifest().roots != roots_id
            || runtime.occurrence().head_body_id()
                != cognition.selected().occurrence().head_body_id()
        {
            return Err(invalid("IWS cold exact accepted provenance"));
        }
        let results=oasis7_world_state::incremental_world_result_router_accepted::readmit_accepted_world_result_routing_ordinary_v1(
        results,head,roots,memory,&mut ctx.cancelled,
    ).map_err(|e|invalid(format!("IWS cold Result admission: {e:?}")))?;
        current
            .validate_binding(roots)
            .map_err(|_| invalid("IWS cold Current root binding"))?;
        let scope = MailboxProvenanceScopeV1::Accepted { head, roots };
        let current =
            readmit_world_current_state_ordinary_v1(current, roots, &scope, &mut ctx, work)
                .map_err(|e| invalid(format!("IWS cold Current admission: {e:?}")))?;
        let selected = readmit_cognition_history_view_ordinary_v1(
            cognition.selected(),
            head,
            roots,
            memory,
            &mut ctx.cancelled,
        )
        .map_err(|e| invalid(format!("IWS cold Cognition view admission: {e:?}")))?;
        let binding = bind_cognition_history_current_v1(
            &selected,
            roots,
            current.cognition().history_source(),
            &mut ctx,
        )
        .map_err(|e| invalid(format!("IWS cold actual Current binding: {e}")))?;
        let cognition = readmit_complete_cognition_history_source_ordinary_v3(
            cognition,
            selected,
            binding,
            memory,
            hash_work,
            &mut ctx.cancelled,
        )
        .map_err(|e| invalid(format!("IWS cold complete Cognition admission: {e}")))?;
        let runtime = readmit_runtime_history_source_ordinary_v1(
            runtime,
            head,
            roots,
            memory,
            &mut ctx.cancelled,
        )
        .map_err(|e| invalid(format!("IWS cold Runtime admission: {e:?}")))?;
        let bytes = roots.state.iter().try_fold(
            std::mem::size_of::<RootSetV1>()
                .checked_add(512)
                .and_then(|n| n.checked_add(roots.world_id.len()))
                .ok_or_else(|| invalid("IWS cold root accounting"))?,
            |n, r| {
                n.checked_add(r.node_root.len())
                    .and_then(|n| n.checked_add(128))
                    .ok_or_else(|| invalid("IWS cold root accounting"))
            },
        )?;
        let roots = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid("IWS cold roots admission"))?
            .retain(roots.clone());
        ctx.check()
            .map_err(|e| invalid(format!("IWS cold final context: {e}")))?;

        let outer = memory
            .reserve(
                std::mem::size_of::<OrdinaryColdSourcesV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("cold Ordinary source owner admission"))?;
        Ok(OrdinaryColdSourcesV1 {
            roots,
            current,
            results,
            runtime,
            cognition,
            ledger,
            retention,
            _outer: outer.retain(()),
        })
    })
}
