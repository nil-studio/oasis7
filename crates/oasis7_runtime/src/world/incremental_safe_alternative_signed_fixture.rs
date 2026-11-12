//! Genuine signed SafeAlternative World -> actual GEN pages and all retained
//! Runtime originals -> three real Result facts -> signed acceptance/Current.
//! The paid in-memory CAS is test storage, never a custody claim.
use super::fixture_store::{Cas, Objects, Originals, Reader};
use super::genesis::{current_work, hash_work, io, json, keys, member, scan};
use super::*;
use crate::world::incremental_module_execution_manifest_hash::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::{self, test_fixtures::incremental},
    incremental_material::*,
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use oasis7_world_state::incremental_cognition_history as cognition;
use oasis7_world_state::incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1;
use oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::CandidateCognitionLifecycleBuilderV3;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*, incremental_runtime_history_original::*,
    incremental_runtime_history_source::*, incremental_world_current_complete::*,
    incremental_world_raw_complete::*,
};
use std::sync::Arc;
pub(super) struct SignedFixture<'a> {
    pub memory: &'a MemoryBudget,
    pub cas: Cas,
    pub objects: Objects,
    pub roots: RootSetV1,
    pub head: formal::VerifiedFormalCommitV1,
    pub runtime: ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub ledger: ReservedValue<
        oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1,
    >,
    pub current: ReservedValue<WorldCurrentStateQualificationV1>,
    pub normal_core: Option<ReservedValue<CheckedCommitCore>>,
    pub normal_authority: Option<formal::VerifiedAuthorityV1>,
    pub normal_storage: Option<
        ReservedValue<
            oasis7_world_state::incremental_world_storage_source::CheckedWorldStorageSourceV1,
        >,
    >,
    pub normal_cognition: Option<
        ReservedValue<
            oasis7_world_state::incremental_cognition_history::CompleteCognitionHistorySourceV3,
        >,
    >,
    pub normal_roots: Option<ReservedValue<RootSetV1>>,
    pub manifest_hash: ReservedValue<AcceptedWorldManifestHashV1>,
    pub parent: ExecutionParent,
    _workspace: MemoryReservation,
}
fn canonical() -> CanonicalCborWorkV1 {
    genesis::canonical()
}
fn index_work() -> RuntimeHistoryIndexWorkV1 {
    RuntimeHistoryIndexWorkV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 8 << 20,
    }
}
fn core_work() -> CoreBudget {
    CoreBudget {
        objects_remaining: 256,
        original_bytes_remaining: 64 << 20,
        new_pages_remaining: 16384,
        merkle_reads_remaining: 1_000_000,
    }
}
fn facts_work() -> FactBudget {
    FactBudget {
        facts_remaining: 256,
        original_bytes_remaining: 64 << 20,
        max_fact_bytes: 8 << 20,
    }
}
fn read<'a, 'b>(
    roots: &'a RootSetV1,
    reader: &'a Reader<'a>,
    io: &'b mut ValueIoBudget,
    keys: &'b mut oasis7_world_state::incremental_semantic_key::SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'b,
    Reader<'a>,
    Reader<'a>,
    impl FnMut() -> Option<ValueTreeError> + 'a,
    impl FnMut() -> bool,
>
where
    'a: 'b,
{
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Artifacts),
        state: reader,
        values: ValueReadContext {
            source: reader,
            memory: reader.memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    }
}
fn cognition_hash_work() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 128 << 20,
        hash_nodes_remaining: 1_000_000,
    }
}

fn qualify_normal_cognition<M, R, E, C>(
    request: &cognition::CognitionHistorySelectionRequestV1<'_>,
    selected: ReservedValue<cognition::CheckedCognitionHistoryViewV1>,
    view: &cognition::CognitionHistoryViewV1,
    refs: &cognition::CognitionHistoryDerivedRefsV2,
    current: &WorldCurrentStateQualificationV1,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    memory: &MemoryBudget,
) -> ReservedValue<cognition::CheckedCognitionHistorySourceV3>
where
    M: oasis7_formal_protocol::incremental_state::MerkleNodeReader,
    R: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::incremental_cognition_history::recover_cognition_head_cache_v1;
    use oasis7_world_state::incremental_cognition_history::{
        CandidateCognitionHeadSourceBuilderV3, CandidateCognitionHistoryIndexBuilderV3,
        CheckedCognitionHistoryComponentsV3, CognitionHistoryCurrentSourceV1,
        CognitionHistoryFieldV1, CognitionHistoryIndexWorkV1,
        bind_candidate_cognition_history_current_v3, qualify_cognition_history_index_v1,
    };
    let journal = qualify_cognition_history_index_v1(
        CognitionHistoryFieldV1::JournalEvents,
        &view.indexes[0],
        ctx,
        &mut CognitionHistoryIndexWorkV1 {
            cells_remaining: 64,
            max_pending_node_bytes: 1 << 20,
        },
        |_, _| panic!("signed normal fixture has an empty JournalEvents index"),
    )
    .unwrap();
    let responses = qualify_cognition_history_index_v1(
        CognitionHistoryFieldV1::Responses,
        &view.indexes[1],
        ctx,
        &mut CognitionHistoryIndexWorkV1 {
            cells_remaining: 64,
            max_pending_node_bytes: 1 << 20,
        },
        |_, _| panic!("signed normal fixture has an empty Responses index"),
    )
    .unwrap();
    let cache_id = refs.caches[0].expect("signed Cognition view retains its actual head cache");
    let physical_cache = recover_cognition_head_cache_v1(
        cache_id,
        &mut ctx.values,
        &mut cognition_hash_work(),
        || false,
    )
    .unwrap();
    let head = CandidateCognitionHeadSourceBuilderV3::genesis(
        request,
        &selected,
        &journal,
        memory,
        || false,
    )
    .unwrap()
    .finish(&physical_cache, &mut cognition_hash_work(), || false)
    .unwrap();
    let journal_source = CandidateCognitionHistoryIndexBuilderV3::new(
        request,
        &selected,
        CognitionHistoryFieldV1::JournalEvents,
        Some(&journal),
        None,
        memory,
        || false,
    )
    .unwrap()
    .finish(|| false)
    .unwrap();
    let response_source = CandidateCognitionHistoryIndexBuilderV3::new(
        request,
        &selected,
        CognitionHistoryFieldV1::Responses,
        Some(&responses),
        None,
        memory,
        || false,
    )
    .unwrap()
    .finish(|| false)
    .unwrap();
    let current_binding = bind_candidate_cognition_history_current_v3(
        request,
        &selected,
        roots,
        current.cognition().history_source(),
        ctx,
    )
    .unwrap();
    let lifecycle =
        CandidateCognitionLifecycleBuilderV3::new(request, &selected, None, memory, || false)
            .unwrap()
            .finish(|| false)
            .unwrap();
    cognition::qualify_checked_cognition_history_source_v3(
        CheckedCognitionHistoryComponentsV3 {
            selected,
            current: current_binding,
            indexes: [Some(journal_source), Some(response_source), None, None],
            heads: [Some(head), None, None],
            lifecycle: Some(lifecycle),
            disabled: None,
        },
        memory,
        || false,
    )
    .unwrap()
}
pub(super) fn build_signed<'a>(
    world: &World,
    memory: &'a MemoryBudget,
    workspace: MemoryReservation,
) -> SignedFixture<'a> {
    build_signed_inner(world, memory, workspace, false)
}

pub(super) fn build_normal_parent<'a>(
    world: &World,
    memory: &'a MemoryBudget,
    workspace: MemoryReservation,
) -> SignedFixture<'a> {
    build_signed_inner(world, memory, workspace, true)
}

fn build_signed_inner<'a>(
    world: &World,
    memory: &'a MemoryBudget,
    workspace: MemoryReservation,
    retain_normal_parent: bool,
) -> SignedFixture<'a> {
    let mut cas = Cas::new(memory);
    let objects = Objects::new(memory);
    let mut rows = Vec::new();
    let pending_original_work = std::cell::RefCell::new(canonical());
    world
        .prepare_iws_genesis_v1(
            &mut member(&mut cas, memory, &mut io(), &mut keys(), || false),
            &mut super::runtime_genesis_work(),
            &mut |reference, bytes| {
                assert_eq!(reference.id, objects.put(bytes));
                Ok(())
            },
            &mut |ordinal, envelope| {
                assert!(std::ptr::eq(envelope, &world.pending_actions[ordinal as usize]));
                let receipt = crate::world::incremental_action_original::persist_action_original_v1(
                    envelope, memory, &mut pending_original_work.borrow_mut(), &mut || false, 1 << 20,
                    &mut |stream: &mut dyn crate::world::incremental_action_original::ActionOriginalStreamV1,
                          cancelled: &mut dyn FnMut() -> bool| {
                        let length = usize::try_from(stream.original_bytes()).unwrap();
                        assert!(length <= 1 << 20);
                        let charge = memory.reserve(length, MemoryClass::Ordinary)
                            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
                        let mut bytes = Vec::with_capacity(length);
                        stream.write_to(&mut bytes)?;
                        assert_eq!(bytes.len(), length);
                        if cancelled() { return Err(ValueTreeError::Cancelled.into()); }
                        assert_eq!(stream.original_id(), objects.put(&bytes));
                        drop(charge);
                        Ok(())
                    }).map_err(|_| ValueTreeError::Invalid("fixture pending canonical original"))?;
                Ok(oasis7_formal_protocol::incremental_artifacts::OriginalArtifactRefV1 {
                    id: receipt.original_id(), byte_len: receipt.original_bytes(),
                })
            },
            &mut |_,_,event| {
                let mut matches=world.journal().events.iter().filter(|original|
                    matches!(&original.body,crate::WorldEventBody::Domain(domain) if domain==event));
                let original=matches.next().expect("actual mailbox journal original");
                assert!(matches.next().is_none(),"fixture has unambiguous original envelope");
                let receipt=crate::world::incremental_action_original::persist_world_event_original_v1(
                    original,memory,&mut pending_original_work.borrow_mut(),&mut ||false,1<<20,
                    &mut |stream:&mut dyn crate::world::incremental_action_original::ActionOriginalStreamV1,_:&mut dyn FnMut()->bool| {
                        let charge=memory.reserve(stream.original_bytes() as usize+128,MemoryClass::Ordinary)
                            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
                        let mut bytes=Vec::with_capacity(stream.original_bytes() as usize);
                        stream.write_to(&mut bytes)?;assert_eq!(objects.put(&bytes),stream.original_id());drop(charge);Ok(())
                    })?;
                Ok(receipt.original_id())
            },
            &mut |row| {
                assert!(rows.len() < 100000);
                rows.push(row);
                Ok(())
            },
        )
        .unwrap();
    let retained = world.borrow_iws_runtime_history_v1();
    assert!(
        !retained.journal.events.is_empty(),
        "signed flow must preserve real history"
    );
    assert_eq!(
        retained.archived_tick_count, 0,
        "small fixture has no archived prefix"
    );
    let config = incremental::genesis_config([4; 32]);
    let scope = config.scope().unwrap();
    let mut roots = RootSetV1::empty(scope.world_id, scope.genesis_config).unwrap();
    let reader = Reader {
        cas: cas.clone(),
        memory,
    };
    for root in &mut roots.state {
        let mut changes: Vec<_> = rows
            .iter()
            .filter(|row| row.namespace == root.namespace)
            .map(|row| row.mutation.clone())
            .collect();
        changes.sort_by(|a, b| a.key.cmp(&b.key));
        if changes.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(root.clone(), changes, &reader, 16 << 20).unwrap();
        for node in delta.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            cas.put_node(
                id,
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes),
            )
            .unwrap();
        }
        *root = delta.after;
    }
    drop(rows);
    let refs = incremental::FixtureExecutionArtifactRefsV1 {
        manifest: [5; 32],
        wasm: [6; 32],
        rules: [7; 32],
        policy: [8; 32],
    };
    let before = incremental::setup_with_config_and_state(
        memory.clone(),
        config.clone(),
        roots.state.clone(),
        refs,
    );
    let first = before.history.facts.facts.len() as u64;
    let mut extras = Vec::new();
    let mut bindings = Vec::new();
    let mut lane_rows: [Vec<RuntimeHistoryIndexRowV1>; 4] = std::array::from_fn(|_| Vec::new());
    for record in &retained.snapshot_catalog.records {
        push_history(
            RuntimeHistoryOriginalRefV1::Snapshot(record),
            memory,
            &objects,
            first,
            &mut extras,
            &mut bindings,
            &mut lane_rows,
        );
    }
    for event in &retained.journal.events {
        push_history(
            RuntimeHistoryOriginalRefV1::Event(event),
            memory,
            &objects,
            first,
            &mut extras,
            &mut bindings,
            &mut lane_rows,
        );
    }
    for tick in retained.ticks {
        push_history(
            RuntimeHistoryOriginalRefV1::Tick(tick),
            memory,
            &objects,
            first,
            &mut extras,
            &mut bindings,
            &mut lane_rows,
        );
    }
    for audit in retained.rejection_audit {
        push_history(
            RuntimeHistoryOriginalRefV1::RejectionAudit(audit),
            memory,
            &objects,
            first,
            &mut extras,
            &mut bindings,
            &mut lane_rows,
        );
    }
    let lanes = RuntimeHistoryLaneV1::ALL.map(|lane| {
        let rows = &lane_rows[lane.index()];
        let total = rows.len() as u64;
        let root = prepare_runtime_history_index_v1(
            lane,
            total,
            rows.iter().copied().map(Ok),
            &mut member(&mut cas, memory, &mut io(), &mut keys(), || false),
            &mut index_work(),
        )
        .unwrap();
        RuntimeHistoryWindowV1 {
            root,
            total,
            start: 0,
            count: total,
        }
    });
    let manifest = RuntimeHistoryManifestV1 {
        roots: roots.digest().unwrap(),
        previous_view: None,
        lanes,
        max_snapshots: retained.snapshot_catalog.retention.max_snapshots as u64,
        archived_tick_count: 0,
    };
    let capsule = manifest.canonical_bytes().unwrap();
    let capsule_id = objects.put(&capsule);
    let runtime_ordinal = first + extras.len() as u64;
    extras.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Result,
        id: capsule_id,
    });
    let (cog_view, cog_refs, cog_bytes) = cognition_bytes(world, &roots, &mut cas, memory);
    let cog_id = objects.put(&cog_bytes);
    extras.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Result,
        id: cog_id,
    });
    let ledger_manifest = world
        .prepare_iws_cognition_proof_ledger_genesis_v1(
            &roots,
            &mut member(&mut cas, memory, &mut io(), &mut keys(), || false),
            &mut oasis7_world_state::incremental_cognition_proof_ledger::ProofLedgerWriteWorkV1 {
                cells_remaining: 1_000_000,
                max_pending_node_bytes: 8 << 20,
                json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                    cells_remaining: 1_000_000,
                    json_nodes_remaining: 1_000_000,
                    max_json_depth: 64,
                    max_pending_node_bytes: 8 << 20,
                },
            },
        )
        .unwrap()
        .expect("real enabled Cognition ledger");
    assert!(
        ledger_manifest.fields[4].is_some(),
        "actual runtime_binding is nonempty"
    );
    let ledger_bytes = ledger_manifest.canonical_bytes().unwrap();
    let ledger_id = objects.put(&ledger_bytes);
    let ledger_ordinal = first + extras.len() as u64;
    extras.push(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Result,
        id: ledger_id,
    });
    let s = incremental::setup_with_config_state_and_facts(
        memory.clone(),
        config,
        roots.state.clone(),
        incremental::FixtureExecutionArtifactRefsV1 {
            manifest: [5; 32],
            wasm: [6; 32],
            rules: [7; 32],
            policy: [8; 32],
        },
        &extras,
    );
    assert_eq!(roots, s.roots);
    let (blobs, catalog) = incremental::core_objects(&s, &s.history);
    for blob in blobs.iter() {
        objects.put(blob.as_slice());
    }
    for node in &catalog.new_nodes {
        let id = *blake3::hash(&node.bytes).as_bytes();
        cas.put_node(
            id,
            memory
                .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                .unwrap()
                .retain(node.bytes.clone()),
        )
        .unwrap();
    }
    let candidate = formal::verify_formal_candidate(
        &formal::SignedCanonicalCandidateV1 {
            record: s.proof.decision.record.clone(),
            writer_signature_hex: s.proof.decision.writer_signature_hex.clone(),
        },
        None,
        &s.authority,
        None,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let mut originals = Originals {
        objects: objects.clone(),
        memory,
    };
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: &s.authority,
            parent: None,
            max_pending_node_bytes: 8 << 20,
        },
        &mut originals,
        &reader,
        memory,
        &mut core_work(),
        || false,
    )
    .unwrap();
    let facts =
        verify_fact_originals(&core, &mut originals, memory, &mut facts_work(), || false).unwrap();
    let mut quota = io();
    let mut key_work = keys();
    let mut ctx = read(&roots, &reader, &mut quota, &mut key_work);
    let indexes = RuntimeHistoryLaneV1::ALL.map(|lane| {
        qualify_runtime_history_index_v1(
            lane,
            &manifest.lanes[lane.index()],
            &mut ctx,
            &mut index_work(),
        )
        .unwrap()
    });
    let checked = check_candidate_runtime_history_source_v1(
        &candidate,
        &core,
        &facts,
        &manifest,
        runtime_ordinal,
        &indexes,
        None,
        &bindings
            .iter()
            .map(
                |(binding, fact_ordinal, index_ordinal)| RuntimeHistoryCandidateRowV1 {
                    binding,
                    fact_ordinal: *fact_ordinal,
                    index_ordinal: *index_ordinal,
                },
            )
            .collect::<Vec<_>>(),
        &mut ctx,
        &mut RuntimeHistorySourceWorkV1 {
            rows_remaining: 100,
            index: index_work(),
        },
    )
    .unwrap();
    let routing =
        oasis7_world_state::incremental_world_result_router::route_checked_world_results_v1(
            &oasis7_world_state::incremental_world_result_router::WorldResultRouterRequestV1 {
                candidate: &candidate,
                core: &core,
                originals: &facts,
            },
            &mut originals,
            memory,
            &mut router_work(),
            || false,
        )
        .unwrap();
    assert!(routing.proof_ledger().is_some());
    let checked_ledger=oasis7_world_state::incremental_cognition_proof_ledger::check_candidate_cognition_proof_ledger_v1(
        &CandidateFactRequestV1 {candidate:&candidate,core:&core,originals:&facts,kind:ExecutionFactKindV1::Result,ordinal:ledger_ordinal},
        &routing,&ledger_bytes,None,&mut ctx,&mut json(),
        &mut oasis7_world_state::incremental_json_records::JsonChangeWorkV1 {json:json(),paths_remaining:1_000_000,max_pending_pairs:4096}).unwrap();
    let ledger_promotion=oasis7_world_state::incremental_cognition_proof_ledger::prepare_cognition_proof_ledger_promotion_v1(&checked_ledger,memory,||false).unwrap();
    let promotion =
        prepare_runtime_history_source_promotion_v1(&checked, indexes, memory, || false).unwrap();
    let head = formal::verify_formal_qualified(&s.proof, &s.authority, None, None).unwrap();
    let runtime = finish_prepared_runtime_history_source_promotion_v1(promotion, &head).unwrap();
    let ledger=oasis7_world_state::incremental_cognition_proof_ledger::finish_prepared_cognition_proof_ledger_promotion_v1(ledger_promotion,&head).unwrap();
    let mut mailbox = ActualMailbox {
        decode: decode_work(),
        ordinals: bindings
            .iter()
            .filter(|(b, _, _)| b.lane() == RuntimeHistoryLaneV1::Journal)
            .map(|(b, f, _)| (b.original_id(), *f))
            .collect(),
        calls: 0,
    };
    let mut safe_current_work = current_work(&roots);
    let current = qualify_world_current_state_v1(
        &roots,
        CognitionCurrentPresenceV3::Enabled,
        &MailboxProvenanceScopeV1::Candidate {
            candidate: &candidate,
            core: &core,
            originals: &facts,
        },
        &mut ctx,
        &mut safe_current_work,
        &super::FixtureScratch::new(memory, MemoryClass::Ordinary),
        &mut originals,
        &mut mailbox,
    )
    .unwrap_or_else(|error|panic!("complete module fixture Current Q: {error:?}; runtime counters config={} effects={} scheduler={} moduleauth={} resource={} content={} revocation={} pending={}; last state={}",
        safe_current_work.runtime.config.cells_remaining,safe_current_work.runtime.effects.cells_remaining,
        safe_current_work.runtime.cells_remaining,safe_current_work.runtime.module_authorization.cells_remaining,
        safe_current_work.runtime.resources.cells_remaining,safe_current_work.runtime.content.objects_remaining,
        safe_current_work.runtime.revocation.json.cells_remaining,safe_current_work.runtime.pending.index.steps_remaining,
        reader.cas.last_state_hex()));
    assert!(
        mailbox.calls > 0,
        "actual signed world mailboxes were authenticated"
    );
    let (normal_core, normal_authority, normal_storage, normal_cognition, normal_roots) =
        if retain_normal_parent {
            // The due-transit fixture retains the full parent conjunction. Re-run
            // Current qualification with fresh work rather than cloning a proof or
            // borrowing the SafeAlternative fixture's independent Current owner.
            let mut normal_mailbox = ActualMailbox {
                decode: decode_work(),
                ordinals: bindings
                    .iter()
                    .filter(|(binding, _, _)| binding.lane() == RuntimeHistoryLaneV1::Journal)
                    .map(|(binding, fact, _)| (binding.original_id(), *fact))
                    .collect(),
                calls: 0,
            };
            let mut normal_current_budget = current_work(&roots);
            let normal_current = qualify_world_current_state_v1(
                &roots,
                CognitionCurrentPresenceV3::Enabled,
                &MailboxProvenanceScopeV1::Candidate {
                    candidate: &candidate,
                    core: &core,
                    originals: &facts,
                },
                &mut ctx,
                &mut normal_current_budget,
                &super::FixtureScratch::new(memory, MemoryClass::Ordinary),
                &mut originals,
                &mut normal_mailbox,
            )
            .unwrap_or_else(|error| {
                panic!(
                    "complete normal-parent Current Q: {error:?}; last state={}",
                    reader.cas.last_state_hex()
                )
            });
            assert!(
                normal_mailbox.calls > 0,
                "normal-parent Current reauthenticated original mailboxes"
            );
            let request = cognition::CognitionHistorySelectionRequestV1 {
                candidate: &candidate,
                core: &core,
                originals: &facts,
            };
            let selected = cognition::check_candidate_cognition_history_view_v1(
                &request,
                &mut originals,
                memory,
                &mut facts_work(),
                || false,
            )
            .unwrap();
            let checked_cognition = qualify_normal_cognition(
                &request,
                selected,
                &cog_view,
                &cog_refs,
                &normal_current,
                &roots,
                &mut ctx,
                memory,
            );
            let complete_cognition = cognition::promote_complete_cognition_history_source_v3(
                &checked_cognition,
                &head,
                memory,
                || false,
            )
            .unwrap();
            let storage=oasis7_world_state::incremental_world_storage_source::qualify_checked_world_storage_source_v1(
            &core,normal_current,checked_cognition,checked,routing,memory,||false,
        ).unwrap();
            let roots_heap = roots.state.iter().fold(
                std::mem::size_of::<RootSetV1>()
                    .checked_add(roots.world_id.capacity())
                    .expect("normal-parent RootSet heap bound overflow"),
                |total, state| {
                    total
                        .checked_add(state.node_root.capacity())
                        .expect("normal-parent RootSet heap bound overflow")
                },
            );
            let retained_roots = memory
                .reserve(roots_heap, MemoryClass::Ordinary)
                .expect("normal-parent RootSet retention is charged")
                .retain(roots.clone());
            (
                Some(core),
                Some(s.authority),
                Some(storage),
                Some(complete_cognition),
                Some(retained_roots),
            )
        } else {
            (None, None, None, None, None)
        };
    let manifest_hash = recover_world_manifest_hash_v1(
        &runtime,
        &head,
        &roots,
        current.runtime().config(),
        &mut ctx,
        &mut hash_work(),
    )
    .unwrap();
    assert_eq!(
        manifest_hash.hash(),
        crate::util::hash_json(&world.manifest).unwrap()
    );
    drop(ctx);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: s.history.body.digest().unwrap(),
        authority: s.history.body.authority,
    };
    SignedFixture {
        memory,
        cas,
        objects,
        roots,
        head,
        runtime,
        ledger,
        current,
        normal_core,
        normal_authority,
        normal_storage,
        normal_cognition,
        normal_roots,
        manifest_hash,
        parent,
        _workspace: workspace,
    }
}

fn router_work() -> oasis7_world_state::incremental_world_result_router::WorldResultRouterWorkV1 {
    use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
    let structure = || CborPreflightWorkV1 {
        bytes_remaining: 64 << 20,
        values_remaining: 1_000_000,
        max_depth: 64,
    };
    oasis7_world_state::incremental_world_result_router::WorldResultRouterWorkV1 {
        facts: facts_work(),
        envelope: structure(),
        runtime:
            oasis7_world_state::incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1 {
                structure: structure(),
                canonical: canonical(),
                max_original_bytes: 8 << 20,
                max_heap_bytes: 32 << 20,
            },
        cognition:
            oasis7_world_state::incremental_cognition_history_cbor::CognitionHistoryCborWorkV1 {
                bytes_remaining: 64 << 20,
                nodes_remaining: 1_000_000,
                source_reads_remaining: 1_000_000,
                key_bytes_remaining: 16 << 20,
                max_depth: 64,
                max_key_bytes: 4096,
            },
    }
}
fn borrow_original<'a>(v: &RuntimeHistoryOriginalRefV1<'a>) -> RuntimeHistoryOriginalRefV1<'a> {
    match v {
        RuntimeHistoryOriginalRefV1::Snapshot(v) => RuntimeHistoryOriginalRefV1::Snapshot(v),
        RuntimeHistoryOriginalRefV1::Event(v) => RuntimeHistoryOriginalRefV1::Event(v),
        RuntimeHistoryOriginalRefV1::Tick(v) => RuntimeHistoryOriginalRefV1::Tick(v),
        RuntimeHistoryOriginalRefV1::RejectionAudit(v) => {
            RuntimeHistoryOriginalRefV1::RejectionAudit(v)
        }
    }
}
fn push_history(
    value: RuntimeHistoryOriginalRefV1<'_>,
    memory: &MemoryBudget,
    objects: &Objects,
    first: u64,
    extras: &mut Vec<ExecutionFactRefV1>,
    bindings: &mut Vec<(ReservedValue<RuntimeHistoryOriginalBindingV1>, u64, u64)>,
    rows: &mut [Vec<RuntimeHistoryIndexRowV1>; 4],
) {
    let binding = measure_runtime_history_original_v1(
        borrow_original(&value),
        memory,
        &mut canonical(),
        &mut || false,
        8 << 20,
    )
    .unwrap();
    with_runtime_history_original_v1(borrow_original(&value),memory,&mut canonical(),&mut ||false,8<<20,|producer| {
        let fee=memory.reserve(producer.original_bytes() as usize+128,MemoryClass::Ordinary)
            .map_err(oasis7_formal_protocol::incremental_execution::ExecutionError::Memory)?;
        let mut raw=Vec::with_capacity(producer.original_bytes() as usize);
        producer.persist_to(&mut |stream:&mut dyn oasis7_world_state::incremental_original_producer::AgentEventOriginalStreamV1,_:&mut dyn FnMut()->bool| {
            stream.write_to(&mut raw)
        })?;
        assert_eq!(objects.put(&raw),binding.original_id());assert_eq!(raw.len() as u64,binding.original_bytes());drop(fee);Ok(())
    }).unwrap();
    let lane = binding.lane();
    let ordinal = first + extras.len() as u64;
    let index = rows[lane.index()].len() as u64;
    rows[lane.index()].push(RuntimeHistoryIndexRowV1 {
        location: FactLocationV1 {
            height: 0,
            ordinal,
            original: binding.original_id(),
        },
        encoded_bytes: binding.original_bytes(),
    });
    extras.push(ExecutionFactRefV1 {
        kind: if lane == RuntimeHistoryLaneV1::Journal {
            ExecutionFactKindV1::Event
        } else {
            ExecutionFactKindV1::Result
        },
        id: binding.original_id(),
    });
    bindings.push((binding, ordinal, index));
}
fn cognition_bytes(
    world: &World,
    roots: &RootSetV1,
    cas: &mut Cas,
    memory: &MemoryBudget,
) -> (
    oasis7_world_state::incremental_cognition_history::CognitionHistoryViewV1,
    oasis7_world_state::incremental_cognition_history::CognitionHistoryDerivedRefsV2,
    Vec<u8>,
) {
    use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
    use oasis7_world_state::{
        incremental_cognition_history::*, incremental_cognition_history_hash_outboard::*,
    };
    let source = world.borrow_iws_cognition_history_source_v1().unwrap();
    assert!(source.enabled());
    let envelope = world.borrow_iws_cognition_envelope_v1().unwrap();
    assert!(
        envelope
            .field("runtime_binding")
            .is_some_and(serde_json::Value::is_object)
    );
    let journal_head_seq = envelope
        .journal_field("head_seq")
        .and_then(serde_json::Value::as_u64)
        .expect("the original Cognition journal retains its sequence");
    let journal_events = envelope
        .journal_field("events")
        .and_then(serde_json::Value::as_array)
        .expect("the original Cognition journal retains its event collection");
    let journal_head_digest = envelope
        .journal_field("head_digest")
        .and_then(serde_json::Value::as_str)
        .expect("the original Cognition journal retains its digest string");
    let highest_journal_seq = journal_events
        .iter()
        .filter_map(|event| event.get("journal_seq").and_then(serde_json::Value::as_u64))
        .max()
        .unwrap_or_default();
    assert_eq!(
        journal_head_seq, highest_journal_seq,
        "the real Cognition header matches its event prefix"
    );
    if journal_head_digest.is_empty() {
        // The legacy empty Cognition journal uses the explicit empty-string
        // sentinel. Runtime WorldEvent originals are retained independently.
        assert_eq!(journal_head_seq, 0);
        assert!(
            journal_events.is_empty(),
            "a nonempty Cognition journal cannot use the legacy empty digest"
        );
    } else {
        let expected_journal_head = crate::cognition_recovery::cognition_digest_v1(
            "oasis7.cognition.journal-head.v1",
            &serde_json::json!({"head_seq":journal_head_seq,"events":journal_events}),
        );
        assert_eq!(
            journal_head_digest,
            expected_journal_head.as_str(),
            "the real Cognition header matches its canonical digest"
        );
    }
    let indexes = std::array::from_fn(|i| {
        let field = [
            CognitionHistoryFieldV1::JournalEvents,
            CognitionHistoryFieldV1::Responses,
            CognitionHistoryFieldV1::EconomyJournal,
            CognitionHistoryFieldV1::EconomyProvisionJournal,
        ][i];
        let actual = CognitionHistoryCollectionRefV1::new(field, source).unwrap();
        assert!(actual.is_empty());
        assert_eq!(
            actual.present(),
            i < 2,
            "only actual present fields may enter this lightweight history fixture"
        );
        CognitionHistoryIndexRefV1 {
            present: actual.present(),
            records: 0,
            root: if actual.present() {
                prepare_cognition_history_index_v1(
                    field,
                    0,
                    std::iter::empty::<Result<CognitionHistoryIndexRowV1, ValueTreeError>>(),
                    &mut member(cas, memory, &mut io(), &mut keys(), || false),
                    &mut CognitionHistoryIndexWorkV1 {
                        cells_remaining: 64,
                        max_pending_node_bytes: 1 << 20,
                    },
                )
                .unwrap()
            } else {
                StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap()
            },
        }
    });
    let cache = CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || false)
        .unwrap()
        .finish(|| false)
        .unwrap();
    let hash_work = || CognitionHeadHashWorkV1 {
        input_bytes_remaining: 128 << 20,
        hash_nodes_remaining: 1_000_000,
    };
    let cache_id =
        persist_cognition_head_cache_v1(&cache, cas, memory, &mut io(), &mut hash_work(), || false)
            .unwrap();
    struct Hasher(blake3::Hasher);
    impl std::io::Write for Hasher {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hash = Hasher(blake3::Hasher::new());
    cache
        .write_canonical_cache_v1(&mut hash, &mut hash_work(), || false)
        .unwrap();
    let view = CognitionHistoryViewV1 {
        roots: roots.digest().unwrap(),
        enabled: true,
        revision: 0,
        previous_view: None,
        heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
            sequence: 0,
            digest: None,
        }),
        indexes,
    };
    let refs = CognitionHistoryDerivedRefsV2 {
        caches: [Some(cache_id), None, None],
        lifecycle: Some(
            RecordRootRefV1::new(
                lifecycle_schema(),
                lifecycle_owner(),
                StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            )
            .unwrap(),
        ),
    };
    let summary = CognitionHistorySourceSummaryV3 {
        mode: CognitionHistorySourceModeV3::Bootstrap,
        cache_state_digests: [Some(*hash.0.finalize().as_bytes()), None, None],
        last: std::array::from_fn(|_| None),
        lifecycle_keys: 0,
        lifecycle_journal_records: 0,
    };
    let bytes = cognition_history_view_v3_bytes(&view, &refs, &summary, memory, || false)
        .unwrap()
        .to_vec();
    (view, refs, bytes)
}

pub(super) fn decode_work()
-> oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
    oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
        structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
            bytes_remaining: 64 << 20,
            values_remaining: 1_000_000,
            max_depth: 64,
        },
        canonical: canonical(),
        max_original_bytes: 8 << 20,
        max_heap_bytes: 32 << 20,
    }
}
struct ActualMailbox {
    ordinals: std::collections::BTreeMap<[u8; 32], u64>,
    calls: usize,
    decode: oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
}
impl<O: OriginalObjectStreamReader> MailboxWorldEventOriginalReaderV1<O> for ActualMailbox {
    fn checked_world_event(
        &mut self,
        reference: oasis7_world_state::incremental_agent_subtree::AgentMailboxReferenceV1<'_>,
        scope: &MailboxProvenanceScopeV1<'_>,
        originals: &mut O,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut MailboxWorldEventOriginalWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<
        Arc<ReservedValue<CheckedMailboxWorldEventOriginalV1>>,
        MailboxWorldEventOriginalErrorV1<O::Error>,
    > {
        use oasis7_world_state::incremental_world_event_decode::decode_world_event_original_v1;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let MailboxProvenanceScopeV1::Candidate {
            candidate,
            core,
            originals: checked,
        } = scope
        else {
            return Err(MailboxWorldEventOriginalErrorV1::Invalid(
                "signed GEN mailbox candidate scope",
            ));
        };
        let ordinal = *self.ordinals.get(&reference.original).ok_or(
            MailboxWorldEventOriginalErrorV1::Invalid("actual journal occurrence missing"),
        )?;
        let occurrence = check_candidate_event_fact_v1(
            &CandidateEventFactRequestV1 {
                candidate,
                core,
                originals: checked,
                ordinal,
            },
            memory,
            &mut *cancelled,
        )
        .expect("genuine checked signed mailbox occurrence");
        struct Output {
            bytes: Vec<u8>,
            max: usize,
        }
        impl std::io::Write for Output {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                if b.len() > self.max - self.bytes.len() {
                    return Err(std::io::Error::other("signed mailbox original bound"));
                }
                self.bytes.extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let max = usize::try_from(work.max_original_bytes)
            .map_err(|_| ValueTreeError::ResourceLimited("signed mailbox maximum"))?;
        let _fee = memory
            .reserve(max + std::mem::size_of::<Vec<u8>>(), class)
            .map_err(ValueTreeError::Memory)?;
        let mut output = Output {
            bytes: Vec::with_capacity(max),
            max,
        };
        if output.bytes.capacity() > max {
            return Err(ValueTreeError::ResourceLimited("signed mailbox capacity").into());
        }
        let length = originals
            .stream_original(
                &reference.original,
                work.max_original_bytes,
                memory,
                &mut output,
            )
            .map_err(MailboxWorldEventOriginalErrorV1::Source)?;
        if length != output.bytes.len() as u64
            || *blake3::hash(&output.bytes).as_bytes() != reference.original
        {
            return Err(MailboxWorldEventOriginalErrorV1::Invalid(
                "signed mailbox actual bytes",
            ));
        }
        let event = Arc::new(decode_world_event_original_v1(
            &output.bytes,
            memory,
            class,
            &mut self.decode,
            &mut *cancelled,
        )?);
        let qualified = verify_mailbox_world_event_original_v1(
            event,
            MailboxWorldEventOccurrenceV1::Candidate(occurrence),
            scope,
            memory,
            class,
            work,
            &mut *cancelled,
        )?;
        self.calls += 1;
        Ok(Arc::new(qualified))
    }
}
