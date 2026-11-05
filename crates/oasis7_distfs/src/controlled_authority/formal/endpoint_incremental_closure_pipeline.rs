//! Private full consumer. Only this child may construct CompleteEvidence.
use super::super::super::*;
use super::*;
use oasis7_formal_protocol::incremental_material::{
    CoreBudget, CoreRequest, OriginalObjectReader, WrittenValueBudget, verify_commit_core,
    verify_state_values, verify_written_values,
};
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_formal_protocol::incremental_value::{ValueIoBudget, ValueReadContext};
pub(crate) fn consume_candidate_data_closure_v1(
    endpoint: &FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("IWS endpoint page-store capability missing"))?;
    let pages = &capability.pages;
    let memory = pages.memory_budget();
    let limits = &capability.budgets;
    let current_limits = capability
        .current_limits
        .as_ref()
        .ok_or_else(|| invalid("IWS complete Current limits required"))?;
    let mut source_work = capability
        .source_work
        .try_borrow_mut()
        .map_err(|_| invalid("IWS source work slot borrowed"))?
        .take()
        .ok_or_else(|| invalid("IWS explicit candidate source work required"))?;
    let mut original_pages = limits.original_page_io.clone();
    let physical_cancelled = std::sync::atomic::AtomicBool::new(false);
    // This API supplies cooperative polling, not an externally shared
    // AtomicBool. Latch every caller observation into the physical reader
    // signal as well. Streaming observers poll between bounded chunks;
    // core owned reads retain their explicit per-original byte bound.
    let mut poll_cancelled = || {
        if physical_cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return true;
        }
        let stop = cancelled();
        if stop {
            physical_cancelled.store(true, std::sync::atomic::Ordering::Release);
        }
        stop
    };
    let cancelled = &mut poll_cancelled;
    if cancelled() {
        return Err(invalid("IWS original capability open cancelled"));
    }
    let original_store = crate::incremental_page_store::objects::IncrementalObjectStore::open(
        pages,
        MemoryClass::Ordinary,
        &mut original_pages,
        &physical_cancelled,
    )
    .map_err(page_store_formal_error)?;
    let mut original_values = ValueIoBudget {
        nodes_remaining: limits.original_value_nodes,
        read_bytes_remaining: limits.original_value_read_bytes,
        write_bytes_remaining: 0,
    };
    let mut original_io = crate::incremental_page_store::objects::ObjectIo {
        physical: &mut original_pages,
        logical: &mut original_values,
        class: MemoryClass::Ordinary,
        cancelled: &physical_cancelled,
    };
    let mut retained = original_store.original_reader(&mut original_io);
    let mut base_objects = EndpointOriginalReader {
        candidate: ClosedRecordOriginalReader {
            record: &candidate.material,
            memory,
        },
        retained: &mut retained,
    };
    let mut original_sync_io = crate::incremental_page_store::objects::ObjectIo {
        physical: &mut source_work.original_sync_physical,
        logical: &mut source_work.original_sync_values,
        class: MemoryClass::Ordinary,
        cancelled: &physical_cancelled,
    };
    let mut original_sync_work = super::super::originals::DurableOriginalWorkV1 {
        occurrences_remaining: source_work.original_sync_occurrences,
        bytes_remaining: source_work.original_sync_bytes,
    };
    let mut objects = super::super::originals::DurableOriginalsV1 {
        original: &mut base_objects,
        candidate,
        store: &original_store,
        io: &mut original_sync_io,
        work: &mut original_sync_work,
    };
    let state = pages
        .state_session(limits.state_page_io.clone(), MemoryClass::Ordinary)
        .map_err(page_store_formal_error)?;
    let mut core_budget = CoreBudget {
        objects_remaining: limits.original_objects,
        original_bytes_remaining: limits.original_bytes,
        new_pages_remaining: limits.new_pages,
        merkle_reads_remaining: limits.core_merkle_reads,
    };
    let core_request = CoreRequest {
        candidate: context,
        original_authority: &endpoint.authority,
        parent: endpoint.parent.as_ref(),
        max_pending_node_bytes: limits.max_pending_node_bytes,
    };
    let core = verify_commit_core(
        &core_request,
        &mut objects,
        &state,
        memory,
        &mut core_budget,
        &mut *cancelled,
    )
    .map_err(|error| {
        state
            .take_read_error()
            .map(page_store_formal_error)
            .unwrap_or_else(|| invalid(format!("IWS structural core: {error:?}")))
    })?;
    if core.body().roots
        != core
            .roots()
            .digest()
            .map_err(|error| invalid(error.to_string()))?
    {
        return Err(invalid("IWS checked core root/body binding"));
    }

    let parent_cache_slot = capability.jobs.parent()?;
    let parent_cache = if core.transition().is_some() {
        let cache = parent_cache_slot
            .as_ref()
            .ok_or_else(|| invalid("IWS exact parent requires cold restoration"))?;
        cache
            .check_parent(&core, memory, &mut *cancelled)
            .map_err(|e| invalid(format!("IWS accepted parent: {e:?}")))?;
        Some(cache)
    } else {
        None
    };
    let mut bridge_work = super::super::NativeChunkReadWorkV1 {
        occurrences_remaining: source_work.native_chunk_occurrences,
        bytes_remaining: source_work.native_chunk_bytes,
        frozen_wire_bytes_remaining: source_work.frozen_wire_bytes,
    };
    let parent_material = match (
        endpoint.parent.as_ref(),
        parent_cache.and_then(|p| p.complete_data()),
    ) {
        (Some(head), Some(data)) => Some((head, data.material())),
        _ => None,
    };
    let material = super::super::verify_candidate_execution_material_v1(
        endpoint,
        candidate,
        context,
        &core,
        &mut objects,
        pages,
        &state,
        parent_material,
        &mut core_budget,
        &mut source_work.artifacts,
        &mut bridge_work,
        &mut *cancelled,
    )?;
    // Backend/native temporary admission ends before either large memo.
    let remaining_state_io = state.remaining_io().map_err(page_store_formal_error)?;
    drop(state);
    let sessions = super::super::StoreSessions::new(
        pages,
        remaining_state_io,
        limits.value_page_io.clone(),
        source_work.publish_physical,
        source_work.max_synced_pages,
        source_work.sync_memo_steps,
        &physical_cancelled,
    )?;
    let sessions = if let Some((entries, bytes)) = capability.state_memo {
        sessions.with_state_read_memo(entries, bytes)?
    } else {
        sessions
    };
    let state = &sessions.state;
    let values = &sessions.values;
    let mut value_io = ValueIoBudget {
        nodes_remaining: limits.value_nodes,
        read_bytes_remaining: limits.value_read_bytes,
        write_bytes_remaining: 0,
    };
    let mut value_context = ValueReadContext {
        source: values,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut value_io,
    };

    if core.transition().is_some() {
        let mut transition_budget = WrittenValueBudget {
            values_remaining: limits.transition_value_occurrences,
            logical_bytes_remaining: limits.transition_value_bytes,
            max_value_bytes: limits.max_transition_value_bytes,
        };
        verify_written_values(&core, &mut value_context, &mut transition_budget, || {
            cancelled()
        })
        .map_err(|error| invalid(format!("IWS final written values: {error:?}")))?;
        if capability.current_limits.is_none() {
            return Err(invalid(
                "IWS ordinary complete Current configuration required",
            ));
        }
    }

    let mut scan = StateScanBudget {
        nodes_remaining: limits.state_scan_nodes,
        read_bytes_remaining: limits.state_scan_bytes,
        leaves_remaining: limits.state_scan_leaves,
    };
    let mut full_values = WrittenValueBudget {
        values_remaining: limits.full_value_occurrences,
        logical_bytes_remaining: limits.full_value_bytes,
        max_value_bytes: limits.max_full_value_bytes,
    };
    let _checked = if core.transition().is_none() {
        Some(
            verify_state_values(
                &core,
                state,
                &mut value_context,
                &mut scan,
                &mut full_values,
                &mut *cancelled,
            )
            .map_err(|error| {
                state
                    .take_read_error()
                    .map(page_store_formal_error)
                    .unwrap_or_else(|| invalid(format!("IWS state/value closure: {error:?}")))
            })?,
        )
    } else {
        None
    };
    {
        let mut fact_budget = oasis7_formal_protocol::incremental_material::FactBudget {
            facts_remaining: core_budget.objects_remaining,
            original_bytes_remaining: core_budget.original_bytes_remaining,
            max_fact_bytes: limits.max_full_value_bytes,
        };
        let originals = oasis7_formal_protocol::incremental_material::verify_fact_originals(
            &core,
            &mut objects,
            memory,
            &mut fact_budget,
            &mut *cancelled,
        )
        .map_err(|e| invalid(format!("IWS actual fact originals: {e:?}")))?;
        // Continue the remaining generic original allowance through Result routing.
        source_work.router.facts.facts_remaining = source_work
            .router
            .facts
            .facts_remaining
            .min(fact_budget.facts_remaining);
        source_work.router.facts.original_bytes_remaining = source_work
            .router
            .facts
            .original_bytes_remaining
            .min(fact_budget.original_bytes_remaining);
        source_work.router.facts.max_fact_bytes = source_work
            .router
            .facts
            .max_fact_bytes
            .min(fact_budget.max_fact_bytes);
        let scope=oasis7_world_state::incremental_world_raw_complete::MailboxProvenanceScopeV1::Candidate {candidate:context,core:&core,originals:&originals};
        let mut mailbox = current_limits
            .candidate_mailbox(memory, MemoryClass::Ordinary)
            .map_err(|e| invalid(format!("IWS mailbox admission: {e:?}")))?;
        let mut work = current_limits
            .current
            .work()
            .map_err(|e| invalid(format!("IWS Current work: {e:?}")))?;
        let scratch =
            scratch::Scratch::new(memory, MemoryClass::Ordinary, current_limits.scratch_pages)
                .map_err(|e| invalid(format!("IWS actual original: {e:?}")))?;
        let mut key_work = oasis7_world_state::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: current_limits.current.bytes_remaining,
        };
        let root = core
            .roots()
            .state
            .first()
            .ok_or_else(|| invalid("IWS Current missing state"))?;
        let selection_request =
            oasis7_world_state::incremental_cognition_history::CognitionHistorySelectionRequestV1 {
                candidate: context,
                core: &core,
                originals: &originals,
            };
        let stop = std::cell::RefCell::new(&mut *cancelled);
        let mut owner_ctx = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
            root,
            state,
            values: ValueReadContext {
                source: values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut *value_context.io,
            },
            key_work: &mut key_work,
            semantic_bytes: u64::try_from(current_limits.current.max_semantic_bytes)
                .map_err(|_| invalid("IWS semantic bound"))?,
            record_bytes: u64::try_from(current_limits.current.max_record_bytes)
                .map_err(|_| invalid("IWS record bound"))?,
            take_state_error: || {
                state.take_read_error().map(|e| {
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Io(
                        std::io::Error::other(e),
                    )
                })
            },
            cancelled: || (*stop.borrow_mut())(),
        };
        let current_qualification=match (core.transition(),parent_cache){
                (Some(record),Some(parent))=>{
                    let execution_parent=parent_binding::transition_parent(parent,&core,memory,||(*stop.borrow_mut())()).map_err(|e|invalid(format!("IWS transition parent: {e:?}")))?;
                    let verifier=oasis7_formal_protocol::incremental_execution::TransitionVerifier {parent:&execution_parent,operation:core.body().operation,source:state,memory,class:MemoryClass::Ordinary,max_pending_node_bytes:limits.max_pending_node_bytes};
                    oasis7_world_state::incremental_world_current_complete::accept_world_current_state_transition_v1(parent.current(),&verifier,record,current_limits.cognition,&scope,&mut owner_ctx,&mut work,&scratch,&mut objects,&mut mailbox)
                },
                (None,None)=>oasis7_world_state::incremental_world_current_complete::qualify_world_current_state_v1(core.roots(),current_limits.cognition,&scope,&mut owner_ctx,&mut work,&scratch,&mut objects,&mut mailbox),
                _=>return Err(invalid("IWS Current exact parent transition")),
            }.map_err(|e|invalid(format!("IWS complete Current: {e:?}")))?;
        let routed =
            oasis7_world_state::incremental_world_result_router::route_checked_world_results_v1(
                &oasis7_world_state::incremental_world_result_router::WorldResultRouterRequestV1 {
                    candidate: context,
                    core: &core,
                    originals: &originals,
                },
                &mut objects,
                memory,
                &mut source_work.router,
                || (*stop.borrow_mut())(),
            )
            .map_err(|e| invalid(format!("IWS Result routing: {e:?}")))?;
        let runtime_original = routed.runtime_history().occurrence().location().original;
        let manifest_raw = objects
            .read_original(
                &runtime_original,
                usize::try_from(limits.max_full_value_bytes)
                    .map_err(|_| invalid("IWS manifest limit"))?,
                memory,
            )
            .map_err(|e| invalid(format!("IWS actual original: {e:?}")))?;
        let runtime_manifest=oasis7_world_state::incremental_runtime_history_manifest::RuntimeHistoryManifestV1::from_canonical_bytes(&manifest_raw).map_err(|e|invalid(format!("IWS Runtime manifest: {e:?}")))?;
        let selected=oasis7_world_state::incremental_cognition_history::check_candidate_cognition_history_view_v1(&selection_request,&mut objects,memory,&mut source_work.router.facts,||(*stop.borrow_mut())()).map_err(|e|invalid(format!("IWS selected Cognition: {e:?}")))?;
        let runtime_request=oasis7_world_state::incremental_runtime_history_source::RuntimeHistoryCandidateReadRequestV1 {candidate:context,core:&core,originals:&originals,manifest:&runtime_manifest,selected_ordinal:routed.runtime_history().occurrence().location().ordinal,parent:parent_cache.map(|p|&p.runtime),prefix:None};
        let mut writer = pages.value_session(source_work.write_physical);
        let mut write_ctx =
            oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
                sink: &mut writer,
                memory,
                io: &mut source_work.write_values,
                key_work: &mut source_work.write_keys,
                limits: source_work.write_limits,
                cancelled: || (*stop.borrow_mut())(),
            };
        let mut history_io = crate::incremental_page_store::objects::ObjectIo {
            physical: &mut source_work.history_physical,
            logical: &mut source_work.history_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical_cancelled,
        };
        let mut history_cancel = || (*stop.borrow_mut())();
        let mut history_reader = history_originals::HistoryOriginalReader {
            candidate: &candidate.material,
            retained: &original_store,
            io: &mut history_io,
            memory,
            occurrences: &mut source_work.history_occurrences,
            hash_bytes: &mut source_work.history_hash_bytes,
            cancelled: &mut history_cancel,
        };
        let mut cognition_work=oasis7_world_state::incremental_cognition_history::CognitionHistoryCandidateReaderWorkV3 {indexes:&mut source_work.cognition_index,cbor:&mut source_work.cognition_cbor,hashes:&mut source_work.cognition_hash,lifecycle:&mut source_work.cognition_lifecycle};
        // Runtime consumes the generic original stream first. Drop that adapter
        // before Cognition borrows the SAME durable-original account; neither
        // lane receives a reset allowance. Every failure returns immediately.
        let runtime_checked = oasis7_world_state::incremental_runtime_history_source::read_candidate_runtime_history_source_v1(
            &runtime_request, &mut objects, &mut owner_ctx, &mut write_ctx,
            &mut source_work.runtime,
        ).map_err(|e| invalid(format!("IWS Runtime History readers: {e:?}")))?;
        drop(objects);
        let mut cognition_sync_io = crate::incremental_page_store::objects::ObjectIo {
            physical: &mut source_work.cognition_sync_physical,
            logical: &mut source_work.cognition_sync_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical_cancelled,
        };
        let mut durable_history = super::super::originals::DurableCognitionOriginalsV1 {
            original: &mut history_reader,
            candidate,
            store: &original_store,
            memory,
            io: &mut cognition_sync_io,
            work: &mut original_sync_work,
        };
        let cognition_checked = oasis7_world_state::incremental_cognition_history::check_candidate_complete_cognition_history_from_readers_v3(
            &selection_request, selected,
            current_qualification.cognition().history_source(),
            parent_cache.map(|p| &p.cognition), &mut owner_ctx, &mut write_ctx,
            &mut durable_history, &mut cognition_work,
        ).map_err(|e| invalid(format!("IWS Cognition History readers: {e:?}")))?;
        let checked_sources = sources::CheckedSources {
            runtime: runtime_checked.source,
            indexes: runtime_checked.indexes,
            cognition: cognition_checked,
        };
        drop(owner_ctx);
        let provisional = provisional::ProvisionalCandidate::checked(
            core,
            current::CheckedCurrent {
                qualification: current_qualification,
            },
            checked_sources.cognition,
            checked_sources.runtime,
            routed,
            memory,
            || (*stop.borrow_mut())(),
        )
        .map_err(|e| invalid(format!("IWS source conjunction: {e:?}")))?;
        let mut persist_io = crate::incremental_page_store::objects::ObjectIo {
            physical: &mut source_work.persist_physical,
            logical: &mut source_work.persist_values,
            class: MemoryClass::Ordinary,
            cancelled: &physical_cancelled,
        };
        persist::persist_checked_originals(
            candidate,
            &provisional,
            &original_store,
            &mut persist_io,
            &mut source_work.persist_occurrences,
            &mut || (*stop.borrow_mut())(),
        )?;
        super::super::catalog::sync_checked_catalog_delta_v1(
            &provisional.core,
            pages,
            &mut source_work.catalog_read_physical,
            &mut source_work.catalog_publish_physical,
            &mut source_work.catalog_occurrences,
            &physical_cancelled,
            &mut || (*stop.borrow_mut())(),
        )?;
        super::super::catalog::sync_checked_transition_paths_v1(
            &provisional.core,
            &sessions.state,
            memory,
            &mut source_work.transition_scan,
            &mut source_work.transition_occurrences,
            &mut || (*stop.borrow_mut())(),
        )?;
        let page_witness = sessions.seal(
            &provisional.core,
            &provisional.source,
            parent_cache
                .and_then(|p| p.complete_data())
                .map(|p| p.pages()),
            &mut || (*stop.borrow_mut())(),
        )?;
        let evidence_charge = memory
            .reserve(
                std::mem::size_of::<CompleteDataClosureEvidenceV1>() + 512,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("IWS complete evidence admission"))?;
        if !material.uses_budget(memory)
            || !page_witness.uses_budget(memory)
            || material.reserved_bytes()
                < std::mem::size_of::<super::super::CandidateExecutionMaterialV1>() + 512
            || page_witness.reserved_bytes()
                < std::mem::size_of::<super::super::StoreReadWitness>() + 128
            || material.candidate_record_id() != page_witness.candidate_record_id()
            || material.body_id() != page_witness.body_id()
            || material.roots_id() != page_witness.roots_id()
            || material.page_namespace_identity() != page_witness.page_namespace_identity()
        {
            return Err(invalid("IWS complete data exact Core/store binding"));
        }
        let evidence = evidence_charge.retain(CompleteDataClosureEvidenceV1 {
            cognition: provisional
                .source
                .cognition()
                .selected()
                .occurrence()
                .location(),
            runtime: provisional.source.runtime().occurrence().location(),
            material,
            pages: page_witness,
        });
        // The sealed witness retains only its real lease, not either large
        // memo. Release those before uncached native metadata publication.
        drop(sessions);
        let local = super::super::super::local_closure::prepare_local_data_closure_v1(
            &evidence,
            &endpoint.store,
            memory,
            &mut || (*stop.borrow_mut())(),
        )?;
        let prepared = provisional::PreparedCandidateInstall::prepare(
            provisional,
            checked_sources.indexes,
            memory,
            || (*stop.borrow_mut())(),
        )?;
        let prepared = prepared.with_complete_data(evidence, local)?;
        capability.jobs.stage(
            candidate,
            memory,
            prepared,
            &mut source_work.history_hash_bytes,
            &mut || (*stop.borrow_mut())(),
        )?;
        Ok(())
    }
}
