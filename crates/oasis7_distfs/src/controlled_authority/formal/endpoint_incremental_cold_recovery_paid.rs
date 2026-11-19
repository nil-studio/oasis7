//! Actual GEN/cold-only full Recovery source production. Each original
//! qualifier/codec remains unchanged and all original scalar/physical lanes
//! debit the one caller Common child. No ordinary-tick cold scan is exposed.
use super::*;
use crate::incremental_page_store::objects::{IncrementalObjectStore, ObjectIo};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1,
    incremental_material::*,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_cognition_history::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_world_current_complete::*,
    incremental_world_raw_complete::MailboxProvenanceScopeV1,
};

use oasis7_proto::memory_budget::ReservedValue;
pub(super) struct RecoveredColdSourcesV1 {
    pub(super) roots: ReservedValue<oasis7_formal_protocol::incremental_execution::RootSetV1>,
    pub(super) current: ReservedValue<WorldCurrentStateQualificationV1>,
    pub(super) runtime: ReservedValue<
        oasis7_world_state::incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
    >,
    pub(super) cognition: ReservedValue<CompleteCognitionHistorySourceV3>,
    pub(super) results: ReservedValue<
        oasis7_world_state::incremental_world_result_router_accepted::AcceptedWorldResultRoutingV1,
    >,
    pub(super) ledger: Option<
        ReservedValue<
            oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
        >,
    >,
    pub(super) retention:Option<ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>>,
    _outer: ReservedValue<()>,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn recover_complete_sources_bounded_v1<L>(
    pages: &crate::incremental_page_store::IncrementalPageStore,
    checkpoint: &super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    core: &ReservedValue<CheckedCommitCore>,
    work: &mut IwsColdParentWorkV1,
    hints: IwsColdParentHintsV1,
    mailbox_hint: L,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<RecoveredColdSourcesV1, FormalError>
where
    L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
{
    checkpoint.validate_context(common)?;
    if !pages.memory_budget().same_pool(common.memory()) {
        return Err(invalid("cold full source actual pool"));
    }
    let head = checkpoint.head();
    let memory = pages.memory_budget();
    super::cold_io::run(work, common, |work, cancelled| {
        let physical_cancelled = std::sync::atomic::AtomicBool::new(false);
        let stop = std::cell::RefCell::new(cancelled);
        let mut poll = || {
            let cancelled = (*stop.borrow_mut())();
            if cancelled {
                physical_cancelled.store(true, std::sync::atomic::Ordering::Release);
            }
            cancelled
        };
        let objects = IncrementalObjectStore::open(
            pages,
            MemoryClass::Recovery,
            &mut work.objects_physical,
            &physical_cancelled,
        )
        .map_err(page_store_formal_error)?;
        let mut object_io = ObjectIo {
            physical: &mut work.objects_physical,
            logical: &mut work.objects_values,
            class: MemoryClass::Recovery,
            cancelled: &physical_cancelled,
        };
        let mut reader = objects
            .recovery_original_reader(&mut object_io)
            .map_err(page_store_formal_error)?;
        let roots =
            cold_roots::recover_roots(head, &mut reader, memory, &mut work.core, &mut poll)?;
        let state_loan = super::cold_page_loans::StateLoan::new(
            pages,
            &mut work.state_recovery,
            MemoryClass::Recovery,
            &physical_cancelled,
            work.recovery_memo,
        )
        .map_err(page_store_formal_error)?;
        let state = state_loan.source();
        let values_loan = super::cold_page_loans::ValueLoan::new(
            pages,
            &mut work.values_recovery,
            &physical_cancelled,
        );
        let values = values_loan.source();
        let selected = {
            let mut facts = FactReadContextV1 {
                originals: &mut reader,
                catalog_pages: state,
                memory,
                class: MemoryClass::Recovery,
                core_budget: &mut work.core,
                fact_budget: &mut work.facts,
                cancelled: &mut poll,
            };
            recover_selected_cognition_history_view_v1(
                &FactRequestV1 {
                    kind: ExecutionFactKindV1::Result,
                    head,
                    roots: &roots,
                    location: hints.cognition,
                },
                &mut facts,
            )
            .map_err(|e| invalid(format!("IWS cold selected view: {e:?}")))?
        };
        let presence = if selected.manifest().enabled {
            CognitionCurrentPresenceV3::Enabled
        } else {
            CognitionCurrentPresenceV3::Disabled
        };
        drop(selected);
        let scope = MailboxProvenanceScopeV1::Accepted {
            head,
            roots: &roots,
        };
        // Keep the charged scratch owner live across the full recovery path;
        // only the borrowing read/fact adapters end in the following scope.
        let scratch = scratch::Scratch::new(memory, MemoryClass::Recovery, work.scratch_pages)
            .map_err(invalid)?;
        let (current, runtime, cognition) = {
            let mut read = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Allocator),
                state,
                values: ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Recovery,
                    io: &mut work.read_recovery,
                },
                key_work: &mut work.keys_recovery,
                semantic_bytes: work.semantic_bytes,
                record_bytes: work.record_bytes,
                take_state_error: || {
                    state.take_read_error().map(|e| {
                        oasis7_formal_protocol::incremental_value::ValueTreeError::Io(
                            std::io::Error::other(e),
                        )
                    })
                },
                cancelled: || (*stop.borrow_mut())(),
            };
            let current = {
                let mut mailbox = cold_mailbox_borrowed::BorrowedHistoricalMailboxReader::new(
                    head,
                    &roots,
                    state,
                    mailbox_hint,
                    &mut work.core,
                    &mut work.facts,
                    &mut work.mailbox_decode,
                    &mut work.mailbox_occurrences,
                    memory,
                    MemoryClass::Recovery,
                )
                .map_err(|e| invalid(format!("IWS cold mailbox admission: {e}")))?;
                qualify_world_current_state_v1(
                    &roots,
                    presence,
                    &scope,
                    &mut read,
                    &mut work.current,
                    &scratch,
                    &mut reader,
                    &mut mailbox,
                )
                .map_err(|e| invalid(format!("IWS actual cold Current: {e:?}")))?
            };
            let mut facts = FactReadContextV1 {
                originals: &mut reader,
                catalog_pages: state,
                memory,
                class: MemoryClass::Recovery,
                core_budget: &mut work.core,
                fact_budget: &mut work.facts,
                cancelled: &mut poll,
            };
            let runtime = cold_runtime::recover_runtime(
                head,
                &roots,
                hints.runtime,
                &mut facts,
                &mut read,
                &mut work.runtime_index,
            )?;
            let cognition = cold_cognition::recover_cognition(
                head,
                &roots,
                hints.cognition,
                &current,
                &mut facts,
                &mut read,
                &mut work.cognition_index,
                &mut work.hashes,
            )?;
            (current, runtime, cognition)
        };
        // Earlier selected/Core reads and this closed dispatch consume one
        // continuous fact allowance; the separate configured cap can tighten it.
        work.router.facts.facts_remaining = work
            .router
            .facts
            .facts_remaining
            .min(work.facts.facts_remaining);
        work.router.facts.original_bytes_remaining = work
            .router
            .facts
            .original_bytes_remaining
            .min(work.facts.original_bytes_remaining);
        work.router.facts.max_fact_bytes = work
            .router
            .facts
            .max_fact_bytes
            .min(work.facts.max_fact_bytes);
        let results = oasis7_world_state::incremental_world_result_router_accepted::route_accepted_world_results_v1(
            &oasis7_world_state::incremental_world_result_router_accepted::AcceptedWorldResultRouterRequestV1 {
                head, roots:&roots, anchor:runtime.occurrence(),
            }, &mut reader, state, memory, MemoryClass::Recovery,
            &mut work.core, &mut work.router, &mut poll,
        ).map_err(|e|invalid(format!("IWS cold closed Result routing: {e:?}")))?;

        let ledger = if let Some(row) = results.proof_ledger() {
            let mut ledger_read = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Allocator),
                state,
                values: ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Recovery,
                    io: &mut work.read_recovery,
                },
                key_work: &mut work.keys_recovery,
                semantic_bytes: work.semantic_bytes,
                record_bytes: work.record_bytes,
                take_state_error: || {
                    state.take_read_error().map(|e| {
                        oasis7_formal_protocol::incremental_value::ValueTreeError::Io(
                            std::io::Error::other(e),
                        )
                    })
                },
                cancelled: &mut poll,
            };
            Some(oasis7_world_state::incremental_cognition_proof_ledger::recover_cognition_proof_ledger_v1(
                &FactRequestV1{kind:ExecutionFactKindV1::Result,head,roots:&roots,location:row.occurrence().location()},&results,
                &mut FactReadContextV1{originals:&mut reader,catalog_pages:state,memory,class:MemoryClass::Recovery,core_budget:&mut work.core,fact_budget:&mut work.facts,cancelled:||(*stop.borrow_mut())()},
                &mut ledger_read,&mut work.proof_ledger_read,
            ).map_err(|e|invalid(format!("IWS cold ProofLedger source: {e:?}")))?)
        } else {
            None
        };

        let fee = memory
            .reserve(
                std::mem::size_of::<RecoveredColdSourcesV1>(),
                MemoryClass::Recovery,
            )
            .map_err(|_| invalid("cold recovered source owner admission"))?;
        let retention = if let Some(id) = core.retention_ledger_artifact_id() {
            let bound=oasis7_formal_protocol::incremental_retention::RETENTION_LEDGER_MANIFEST_MAX_BYTES_V1;
            work.facts.original_bytes_remaining = work
                .facts
                .original_bytes_remaining
                .checked_sub(bound as u64)
                .ok_or_else(|| invalid("cold retention original allowance"))?;
            let raw = reader
                .read_original(&id, bound, memory)
                .map_err(|e| invalid(format!("cold retention original: {e:?}")))?;
            let mut retention_read = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Artifacts),
                state,
                values: ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Recovery,
                    io: &mut work.read_recovery,
                },
                key_work: &mut work.keys_recovery,
                semantic_bytes: work.semantic_bytes,
                record_bytes: work.record_bytes,
                take_state_error: || {
                    state
                        .take_read_error()
                        .map(|e| ValueTreeError::Io(std::io::Error::other(e)))
                },
                cancelled: &mut poll,
            };
            Some(oasis7_world_state::incremental_cognition_retention_ledger::recover_qualified_cognition_retention_ledger_v1(head,core,&raw,&mut retention_read,&mut work.proof_ledger_read).map_err(|e|invalid(format!("cold retention qualification: {e:?}")))?)
        } else {
            None
        };
        Ok(RecoveredColdSourcesV1 {
            roots,
            current,
            runtime,
            cognition,
            results,
            ledger,
            retention,
            _outer: fee.retain(()),
        })
    })
}
