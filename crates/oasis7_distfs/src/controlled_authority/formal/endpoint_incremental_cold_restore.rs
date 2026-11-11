//! Reopen recovery uses actual Qualified head, pages and original fact proofs.
use super::*;
use crate::incremental_page_store::{
    PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1,
    incremental_material::*,
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueIoBudget, ValueReadContext},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_cognition_history::*,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_index::RuntimeHistoryIndexWorkV1,
    incremental_semantic_key::SemanticKeyBudget, incremental_world_current_complete::*,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
    incremental_world_raw_complete::MailboxProvenanceScopeV1,
};
pub struct IwsColdParentWorkV1 {
    pub objects_physical: PageIoBudget,
    pub objects_values: ValueIoBudget,
    pub state_recovery: PageIoBudget,
    pub values_recovery: PageIoBudget,
    pub read_recovery: ValueIoBudget,
    pub keys_recovery: SemanticKeyBudget,
    pub state_ordinary: PageIoBudget,
    pub values_ordinary: PageIoBudget,
    pub read_ordinary: ValueIoBudget,
    pub keys_ordinary: SemanticKeyBudget,
    pub recovery_memo: Option<(usize, usize)>,
    pub core: CoreBudget,
    pub facts: FactBudget,
    pub router: oasis7_world_state::incremental_world_result_router::WorldResultRouterWorkV1,
    pub current: WorldCurrentStateWorkV1,
    pub admission: WorldCurrentStateWorkV1,
    pub runtime_index: RuntimeHistoryIndexWorkV1,
    pub cognition_index: CognitionHistoryIndexWorkV1,
    pub hashes: CognitionHeadHashWorkV1,
    pub mailbox_decode: WorldEventDecodeWorkV1,
    pub mailbox_occurrences: u64,
    pub scratch_pages: usize,
    pub semantic_bytes: u64,
    pub record_bytes: u64,
    pub local_records_remaining: u64,
    pub local_original_bytes_remaining: u64,
}
/// The two selected positions and mailbox locator callback are untrusted hints.
/// Every selected occurrence is independently authenticated by Formal readers.
pub struct IwsColdParentHintsV1 {
    pub runtime: FactLocationV1,
    pub cognition: FactLocationV1,
}
impl FileFormalEndpoint {
    pub fn restore_iws_parent<L>(
        &mut self,
        mut work: IwsColdParentWorkV1,
        hints: IwsColdParentHintsV1,
        mailbox_hint: L,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<(), FormalError>
    where
        L: FnMut(&[u8; 32]) -> Result<FactLocationV1, String>,
    {
        if cancelled() {
            return Err(invalid("IWS cold restore cancelled"));
        }
        // Hold the real monotonic endpoint guard through restoration. A held
        // old in-process parent is not fresh protected-checkpoint evidence.
        let guard = self.guard()?;
        self.reload_parent(&guard.evidence().map_err(fencing_error)?)?;
        let head = self
            .parent
            .as_ref()
            .ok_or_else(|| invalid("IWS actual Qualified head required"))?;
        // reload_parent already authenticates the durable Qualified checkpoint.
        // Compare its held exact head without deserializing another full closed
        // candidate payload merely to recover structural source tokens.
        let mut encoded = [0u8; 64];
        hex::encode_to_slice(head.verified_record_id(), &mut encoded)
            .map_err(|_| invalid("IWS cold head ID"))?;
        if self.store.body.head.record_id.as_bytes() != encoded
            || self.store.body.head.position != head.record().position
        {
            return Err(invalid("IWS cold exact durable held head"));
        }
        let capability = self
            .iws_page_store
            .as_ref()
            .ok_or_else(|| invalid("IWS cold page capability required"))?;
        let pages = &capability.pages;
        let memory = pages.memory_budget();
        let physical_cancelled = std::sync::atomic::AtomicBool::new(false);
        let stop = std::cell::RefCell::new(&mut cancelled);
        let mut poll = || {
            let stop = (*stop.borrow_mut())();
            if stop {
                physical_cancelled.store(true, std::sync::atomic::Ordering::Release);
            }
            stop
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
        let state = pages
            .state_session_with_cancelled(
                work.state_recovery,
                MemoryClass::Recovery,
                &physical_cancelled,
            )
            .map_err(page_store_formal_error)?;
        let state = if let Some((entries, bytes)) = work.recovery_memo {
            state
                .with_read_memo(entries, bytes)
                .map_err(page_store_formal_error)?
        } else {
            state
        };
        let values = pages.value_session_with_cancelled(work.values_recovery, &physical_cancelled);
        let selected = {
            let mut facts = FactReadContextV1 {
                originals: &mut reader,
                catalog_pages: &state,
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
                state: &state,
                values: ValueReadContext {
                    source: &values,
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
                let mut mailbox = cold_mailbox::HistoricalMailboxReader::new(
                    head,
                    &roots,
                    &state,
                    mailbox_hint,
                    &mut work.core,
                    &mut work.facts,
                    work.mailbox_decode,
                    work.mailbox_occurrences,
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
                catalog_pages: &state,
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
            }, &mut reader, &state, memory, MemoryClass::Recovery,
            &mut work.core, &mut work.router, &mut poll,
        ).map_err(|e|invalid(format!("IWS cold closed Result routing: {e:?}")))?;

        let ordinary_state = pages
            .state_session_with_cancelled(
                work.state_ordinary,
                MemoryClass::Ordinary,
                &physical_cancelled,
            )
            .map_err(page_store_formal_error)?;
        let ordinary_values =
            pages.value_session_with_cancelled(work.values_ordinary, &physical_cancelled);
        let mut read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Allocator),
            state: &ordinary_state,
            values: ValueReadContext {
                source: &ordinary_values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut work.read_ordinary,
            },
            key_work: &mut work.keys_ordinary,
            semantic_bytes: work.semantic_bytes,
            record_bytes: work.record_bytes,
            take_state_error: || {
                ordinary_state.take_read_error().map(|e| {
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Io(
                        std::io::Error::other(e),
                    )
                })
            },
            cancelled: &mut poll,
        };
        let mut cache = cold::readmit_recovered_parent(
            head,
            &roots,
            &current,
            &runtime,
            &cognition,
            &results,
            &mut read,
            &mut work.admission,
            &mut work.hashes,
        )?;
        let mut local_work = local_closure::LocalClosureReadWorkV1 {
            reads_remaining: work.local_records_remaining,
            original_bytes_remaining: work.local_original_bytes_remaining,
        };
        let local = local_closure::recover_authenticated_local_closure_v1(
            self,
            guard.as_ref(),
            head,
            &cache,
            pages,
            &mut local_work,
            &mut poll,
        )?;
        let recovered = closure::restore_store_read_witness_v1(local, memory, &mut poll)?;
        cache.recovered_pages = Some(recovered);
        capability.jobs.restore_cold(cache, head)?;
        Ok(())
    }
}
