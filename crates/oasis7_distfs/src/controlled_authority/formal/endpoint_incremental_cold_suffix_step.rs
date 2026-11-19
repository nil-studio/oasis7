//! One real accepted Transition applied to the preceding fully qualified
//! suffix owner. Current and both Histories take their incremental receivers.
use super::*;
use crate::incremental_page_store::objects::{IncrementalObjectStore, ObjectIo};
use oasis7_formal_protocol::{
    controlled_authority::formal::PaidVerifiedNormalCandidateContextV1,
    incremental_execution::{ExecutionParent, RootSetV1, TransitionVerifier},
    incremental_history::HISTORY_MAX_HEADER_BYTES,
    incremental_material::{
        CheckedCommitCore, CoreBudget, OriginalObjectReader, WrittenValueBudget,
        verify_fact_originals, verify_written_values,
    },
    incremental_state::StateNamespaceV1,
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_cognition_history::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_manifest::{
        RUNTIME_HISTORY_MANIFEST_BYTES_V1, RuntimeHistoryManifestV1,
    },
    incremental_runtime_history_source::*,
    incremental_world_current_complete::accept_world_current_state_transition_v1,
    incremental_world_raw_complete::MailboxProvenanceScopeV1,
    incremental_world_result_router::{WorldResultRouterRequestV1, route_checked_world_results_v1},
    incremental_world_result_router_accepted::{
        AcceptedWorldResultRouterRequestV1, route_accepted_world_results_v1,
    },
};
use std::cell::RefCell;

pub(super) struct SuffixStepRefsV1<'a> {
    pub(super) endpoint: &'a FileFormalEndpoint,
    pub(super) parent: &'a super::cold_install::ColdParentSourcesV1,
    pub(super) checkpoint: &'a super::cold_checkpoint::AuthenticatedColdCheckpointV1,
    pub(super) core: &'a ReservedValue<CheckedCommitCore>,
    pub(super) context: &'a PaidVerifiedNormalCandidateContextV1,
}

fn root_layout(roots: &RootSetV1) -> Result<usize, FormalError> {
    roots.state.iter().try_fold(
        std::mem::size_of::<ExecutionParent>()
            .checked_add(512)
            .and_then(|n| n.checked_add(roots.world_id.len()))
            .ok_or_else(|| invalid("suffix root layout"))?,
        |n, root| {
            n.checked_add(root.node_root.len())
                .and_then(|n| n.checked_add(128))
                .ok_or_else(|| invalid("suffix root layout"))
        },
    )
}

/// Written values share the remaining structural object/byte accounts. The
/// temporary typed view never refills either allowance, including on failure.
struct WrittenLoan<'a> {
    target: &'a mut CoreBudget,
    work: WrittenValueBudget,
}
impl<'a> WrittenLoan<'a> {
    fn new(target: &'a mut CoreBudget, max_value_bytes: u64) -> Self {
        let work = WrittenValueBudget {
            values_remaining: std::mem::take(&mut target.objects_remaining),
            logical_bytes_remaining: std::mem::take(&mut target.original_bytes_remaining),
            max_value_bytes,
        };
        Self { target, work }
    }
}
impl Drop for WrittenLoan<'_> {
    fn drop(&mut self) {
        self.target.objects_remaining = self.work.values_remaining;
        self.target.original_bytes_remaining = self.work.logical_bytes_remaining;
    }
}

pub(super) fn advance_suffix_sources_bounded_v1<L>(
    refs: SuffixStepRefsV1<'_>,
    work: &mut IwsColdParentWorkV1,
    source: &mut super::work::IwsCandidateSourceWorkV1,
    mailbox_hint: &mut L,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<super::cold_readmission_paid::OrdinaryColdSourcesV1, FormalError>
where
    L: FnMut(
        &[u8; 32],
    ) -> Result<oasis7_formal_protocol::incremental_material::FactLocationV1, String>,
{
    refs.parent.checkpoint.validate_context(common)?;
    refs.checkpoint.validate_context(common)?;
    refs.context.validate_context(common)?;
    common.charge(16, 1024)?;
    let capability = refs
        .endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("suffix actual page capability"))?;
    let pages = &capability.pages;
    let memory = pages.memory_budget();
    let parent = &refs.parent.ordinary;
    let core = refs.core;
    let head = refs.checkpoint.head();
    if !memory.same_pool(common.memory())
        || !core.uses_budget(memory)
        || core.parent_record_id() != Some(refs.parent.checkpoint.head().verified_record_id())
        || refs
            .checkpoint
            .predecessor()
            .map(|p| p.verified_record_id())
            != Some(refs.parent.checkpoint.head().verified_record_id())
        || core.parent_roots() != Some(&*parent.roots)
        || !parent.current.uses_budget(memory)
        || !parent.roots.uses_budget(memory)
        || parent.current.allocation_class() != MemoryClass::Ordinary
    {
        return Err(invalid("suffix exact preceding paid owner"));
    }
    let FormalRequestStateV1::Qualified { candidate, .. } = refs.checkpoint.state() else {
        return Err(invalid("suffix original Qualified candidate"));
    };
    super::cold_io::run_suffix(work, source, common, |work, source, metadata, cancelled| {
        // All fixed root/header codecs and fixed promotion copies are paid
        // before they run. Their own retained carriers keep separate charges.
        metadata.charge(128 * 1024, 128 * 1024)?;
        let _phase = memory
            .reserve(8 * HISTORY_MAX_HEADER_BYTES, MemoryClass::Ordinary)
            .map_err(|_| invalid("suffix header phase admission"))?;
        let outer = memory
            .reserve(
                std::mem::size_of::<super::cold_readmission_paid::OrdinaryColdSourcesV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("suffix Ordinary source owner admission"))?;
        let parent_fee = memory
            .reserve(root_layout(&parent.roots)?, MemoryClass::Ordinary)
            .map_err(|_| invalid("suffix execution parent admission"))?;
        let execution_parent = parent_fee.retain(ExecutionParent {
            commit: refs
                .parent
                .core
                .body()
                .digest()
                .map_err(|e| invalid(e.to_string()))?,
            roots: (*parent.roots).clone(),
            authority: core.body().authority,
        });
        let record = core
            .transition()
            .ok_or_else(|| invalid("suffix requires original Transition"))?;
        if record.parent_commit != execution_parent.commit
            || record.authority != execution_parent.authority
            || record.before != execution_parent.roots
            || record.after != *core.roots()
        {
            return Err(invalid("suffix original Transition continuity"));
        }
        parent
            .current
            .validate_binding(&parent.roots)
            .map_err(|e| invalid(format!("suffix parent Current binding: {e:?}")))?;
        let physical = std::sync::atomic::AtomicBool::new(false);
        let stop = RefCell::new(cancelled);
        let mut poll = || {
            let stopped = (*stop.borrow_mut())();
            if stopped {
                physical.store(true, std::sync::atomic::Ordering::Release);
            }
            stopped
        };
        let store = IncrementalObjectStore::open(
            pages,
            MemoryClass::Ordinary,
            &mut work.objects_physical,
            &physical,
        )
        .map_err(page_store_formal_error)?;
        let mut object_io = ObjectIo {
            physical: &mut work.objects_physical,
            logical: &mut work.objects_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical,
        };
        let mut objects = store.original_reader(&mut object_io);
        let state_loan = super::cold_page_loans::StateLoan::new(
            pages,
            &mut work.state_ordinary,
            MemoryClass::Ordinary,
            &physical,
            None,
        )
        .map_err(page_store_formal_error)?;
        let state = state_loan.source();
        let value_loan =
            super::cold_page_loans::ValueLoan::new(pages, &mut work.values_ordinary, &physical);
        let values = value_loan.source();
        let result = (|| {
            {
                let mut written = WrittenLoan::new(
                    &mut work.core,
                    capability.budgets.max_transition_value_bytes,
                );
                let mut read = ValueReadContext {
                    source: values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut work.read_ordinary,
                };
                verify_written_values(core, &mut read, &mut written.work, &mut poll)
                    .map_err(|e| invalid(format!("suffix written value closure: {e:?}")))?;
            }
            let originals =
                verify_fact_originals(core, &mut objects, memory, &mut work.facts, &mut poll)
                    .map_err(|e| invalid(format!("suffix original input/results: {e:?}")))?;
            source.router.facts.facts_remaining = source
                .router
                .facts
                .facts_remaining
                .min(work.facts.facts_remaining);
            source.router.facts.original_bytes_remaining = source
                .router
                .facts
                .original_bytes_remaining
                .min(work.facts.original_bytes_remaining);
            source.router.facts.max_fact_bytes = source
                .router
                .facts
                .max_fact_bytes
                .min(work.facts.max_fact_bytes);
            let routed = route_checked_world_results_v1(
                &WorldResultRouterRequestV1 {
                    candidate: refs.context.value(),
                    core,
                    originals: &originals,
                },
                &mut objects,
                memory,
                &mut source.router,
                &mut poll,
            )
            .map_err(|e| invalid(format!("suffix checked Result routing: {e:?}")))?;
            let selection = CognitionHistorySelectionRequestV1 {
                candidate: refs.context.value(),
                core,
                originals: &originals,
            };
            let selected = check_candidate_cognition_history_view_v1(
                &selection,
                &mut objects,
                memory,
                &mut source.router.facts,
                &mut poll,
            )
            .map_err(|e| invalid(format!("suffix selected Cognition: {e:?}")))?;
            let presence = if selected.manifest().enabled {
                CognitionCurrentPresenceV3::Enabled
            } else {
                CognitionCurrentPresenceV3::Disabled
            };
            let scope = MailboxProvenanceScopeV1::Accepted {
                head,
                roots: core.roots(),
            };
            let scratch = scratch::Scratch::new(memory, MemoryClass::Ordinary, work.scratch_pages)
                .map_err(invalid)?;
            // The original paid intermediates outlive the read context. In
            // particular, runtime_checked retains its private charge after
            // its indexes move into the accepted Runtime source.
            let current;
            let ledgers;
            let raw;
            let manifest;
            let runtime_checked;
            let cognition_checked;
            let runtime;
            let cognition;
            {
                let mut read = WorldFieldReadContextV1 {
                    root: core.roots().state_root(StateNamespaceV1::Allocator),
                    state,
                    values: ValueReadContext {
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
                    cancelled: || (*stop.borrow_mut())(),
                };
                current = {
                    let mut mailbox =
                        super::cold_mailbox_borrowed::BorrowedHistoricalMailboxReader::new(
                            head,
                            core.roots(),
                            state,
                            &mut *mailbox_hint,
                            &mut work.core,
                            &mut work.facts,
                            &mut work.mailbox_decode,
                            &mut work.mailbox_occurrences,
                            memory,
                            MemoryClass::Ordinary,
                        )
                        .map_err(|e| invalid(format!("suffix mailbox admission: {e}")))?;
                    let verifier = TransitionVerifier {
                        parent: &execution_parent,
                        operation: core.body().operation,
                        source: state,
                        memory,
                        class: MemoryClass::Ordinary,
                        max_pending_node_bytes: capability.budgets.max_pending_node_bytes,
                    };
                    accept_world_current_state_transition_v1(
                        &parent.current,
                        &verifier,
                        record,
                        presence,
                        &scope,
                        &mut read,
                        &mut work.current,
                        &scratch,
                        &mut objects,
                        &mut mailbox,
                    )
                    .map_err(|e| invalid(format!("suffix incremental Current: {e:?}")))?
                };
                ledgers = super::cold_suffix_ledger::read_ledgers(
                    super::cold_suffix_ledger::LedgerRefs {
                        candidate: refs.context.value(),
                        head,
                        core,
                        originals: &originals,
                        routed: &routed,
                        parent,
                    },
                    &mut objects,
                    &mut read,
                    source,
                )?;
                let runtime_original = routed.runtime_history().occurrence().location().original;
                raw = objects
                    .read_original(&runtime_original, RUNTIME_HISTORY_MANIFEST_BYTES_V1, memory)
                    .map_err(|e| invalid(format!("suffix Runtime manifest original: {e:?}")))?;
                if !raw.uses_budget(memory) || raw.len() != RUNTIME_HISTORY_MANIFEST_BYTES_V1 {
                    return Err(invalid("suffix Runtime manifest binding"));
                }
                manifest = RuntimeHistoryManifestV1::from_canonical_bytes(&raw)
                    .map_err(|e| invalid(format!("suffix Runtime manifest: {e:?}")))?;
                let runtime_request = RuntimeHistoryCandidateReadRequestV1 {
                    candidate: refs.context.value(),
                    core,
                    originals: &originals,
                    manifest: &manifest,
                    selected_ordinal: routed.runtime_history().occurrence().location().ordinal,
                    parent: Some(&parent.runtime),
                    prefix: None,
                };
                let mut writer = super::cold_page_loans::ValueLoan::new(
                    pages,
                    &mut source.write_physical,
                    &physical,
                );
                (runtime_checked, cognition_checked) = {
                    let mut write = MemberRecordWriteContext {
                        sink: writer.sink(),
                        memory,
                        io: &mut source.write_values,
                        key_work: &mut source.write_keys,
                        limits: source.write_limits,
                        cancelled: || (*stop.borrow_mut())(),
                    };
                    let runtime_checked = read_candidate_runtime_history_source_v1(
                        &runtime_request,
                        &mut objects,
                        &mut read,
                        &mut write,
                        &mut source.runtime,
                    )
                    .map_err(|e| invalid(format!("suffix incremental Runtime History: {e:?}")))?;
                    let cognition_checked = {
                        let mut history_io = ObjectIo {
                            physical: &mut source.history_physical,
                            logical: &mut source.history_values,
                            class: MemoryClass::Ordinary,
                            cancelled: &physical,
                        };
                        let mut history_poll = || (*stop.borrow_mut())();
                        let mut history = super::history_originals::HistoryOriginalReader {
                            candidate: &candidate.material,
                            retained: &store,
                            io: &mut history_io,
                            memory,
                            occurrences: &mut source.history_occurrences,
                            hash_bytes: &mut source.history_hash_bytes,
                            cancelled: &mut history_poll,
                        };
                        let mut cognition_work = CognitionHistoryCandidateReaderWorkV3 {
                            indexes: &mut source.cognition_index,
                            cbor: &mut source.cognition_cbor,
                            hashes: &mut source.cognition_hash,
                            lifecycle: &mut source.cognition_lifecycle,
                        };
                        check_candidate_complete_cognition_history_from_readers_v3(
                            &selection,
                            selected,
                            current.cognition().history_source(),
                            Some(&parent.cognition),
                            &mut read,
                            &mut write,
                            &mut history,
                            &mut cognition_work,
                        )
                        .map_err(|e| {
                            invalid(format!("suffix incremental Cognition History: {e:?}"))
                        })?
                    };
                    (runtime_checked, cognition_checked)
                };
                drop(writer);
                runtime = finish_prepared_runtime_history_source_promotion_v1(
                    prepare_runtime_history_source_promotion_v1(
                        &runtime_checked.source,
                        runtime_checked.indexes,
                        memory,
                        &mut read.cancelled,
                    )
                    .map_err(|e| invalid(format!("suffix Runtime admission: {e:?}")))?,
                    head,
                )
                .map_err(|e| invalid(format!("suffix Runtime accepted binding: {e:?}")))?;
                cognition = finish_prepared_complete_cognition_history_promotion_v3(
                    prepare_complete_cognition_history_promotion_v3(
                        &cognition_checked,
                        memory,
                        &mut read.cancelled,
                    )
                    .map_err(|e| invalid(format!("suffix Cognition admission: {e:?}")))?,
                    head,
                )
                .map_err(|e| invalid(format!("suffix Cognition accepted binding: {e:?}")))?;
                read.check()
                    .map_err(|e| invalid(format!("suffix final Current context: {e}")))?;
            }
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
            let results = route_accepted_world_results_v1(
                &AcceptedWorldResultRouterRequestV1 {
                    head,
                    roots: core.roots(),
                    anchor: runtime.occurrence(),
                },
                &mut objects,
                state,
                memory,
                MemoryClass::Ordinary,
                &mut work.core,
                &mut work.router,
                &mut poll,
            )
            .map_err(|e| invalid(format!("suffix accepted Result routing: {e:?}")))?;
            let roots = memory
                .reserve(root_layout(core.roots())?, MemoryClass::Ordinary)
                .map_err(|_| invalid("suffix retained roots admission"))?
                .retain(core.roots().clone());
            if poll() {
                return Err(invalid("suffix final source cancellation"));
            }
            Ok(super::cold_readmission_paid::OrdinaryColdSourcesV1 {
                current,
                results,
                roots,
                runtime,
                cognition,
                ledger: ledgers.proof,
                retention: ledgers.retention,
                _outer: outer.retain(()),
            })
        })();
        if let Some(first) = state.take_read_error() {
            return Err(page_store_formal_error(first));
        }
        result
    })
}
