//! Actual GEN producers, real signed Core/FactSet, full Current Q and accepted
//! Runtime History baseline for the module executor component fixture.
use super::*;
use crate::world::incremental_module_execution_manifest_hash::*;
use fixture_store::{Cas, Objects, Originals, Reader};
use oasis7_formal_protocol::{
    controlled_authority::formal::{self, test_fixtures::incremental},
    incremental_material::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3,
    incremental_field_read::WorldFieldReadContextV1, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*, incremental_runtime_history_source::*,
    incremental_semantic_key::SemanticKeyBudget, incremental_world_current_complete::*,
    incremental_world_raw_complete::*,
};
use std::sync::Arc;

pub(super) struct Fixture<'a> {
    pub memory: &'a MemoryBudget,
    pub cas: Cas,
    pub objects: Objects,
    pub roots: RootSetV1,
    pub head: formal::VerifiedFormalCommitV1,
    pub runtime: ReservedValue<AcceptedRuntimeHistorySourceV1>,
    pub current: ReservedValue<WorldCurrentStateQualificationV1>,
    pub manifest_hash: ReservedValue<AcceptedWorldManifestHashV1>,
    pub parent: ExecutionParent,
    _workspace: MemoryReservation,
}
pub(super) fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        write_bytes_remaining: 1 << 30,
    }
}
pub(super) fn keys() -> SemanticKeyBudget {
    SemanticKeyBudget {
        bytes_remaining: 1 << 30,
    }
}
pub(super) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
pub(super) fn canonical() -> oasis7_wasm_abi::CanonicalCborWorkV1 {
    oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 32 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 4096,
    }
}
pub(super) fn json() -> oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
    oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}
pub(super) fn config_work()
-> oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigReadWorkV1 {
    oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigReadWorkV1 {
        cells_remaining: 1_000_000,
        max_payload_bytes: 8 << 20,
        json: json(),
    }
}
pub(super) fn hash_work() -> RootWorldManifestHashWorkV1 {
    RootWorldManifestHashWorkV1 {
        body_scan: scan(),
        fields: config_work(),
        json: json(),
        hash_bytes_remaining: 32 << 20,
        max_json_heap_bytes: 8 << 20,
    }
}
fn index_work() -> RuntimeHistoryIndexWorkV1 {
    RuntimeHistoryIndexWorkV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 8 << 20,
    }
}
fn fact_work() -> FactBudget {
    FactBudget {
        facts_remaining: 256,
        original_bytes_remaining: 32 << 20,
        max_fact_bytes: 8 << 20,
    }
}
fn core_work() -> CoreBudget {
    CoreBudget {
        objects_remaining: 256,
        original_bytes_remaining: 32 << 20,
        new_pages_remaining: 8192,
        merkle_reads_remaining: 1_000_000,
    }
}
pub(super) fn member<'a, C: FnMut() -> bool>(
    cas: &'a mut Cas,
    memory: &'a MemoryBudget,
    io: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
    cancelled: C,
) -> MemberRecordWriteContext<'a, Cas, C> {
    MemberRecordWriteContext {
        sink: cas,
        memory,
        io,
        key_work: keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8 << 20,
            record_bytes: 9 << 20,
        },
        cancelled,
    }
}
fn read<'a>(
    roots: &'a RootSetV1,
    reader: &'a Reader<'a>,
    io: &'a mut ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Reader<'a>,
    Reader<'a>,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
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
struct NoMailbox {
    calls: usize,
}
impl<O: OriginalObjectStreamReader> MailboxWorldEventOriginalReaderV1<O> for NoMailbox {
    fn checked_world_event(
        &mut self,
        _: oasis7_world_state::incremental_agent_subtree::AgentMailboxReferenceV1<'_>,
        _: &MailboxProvenanceScopeV1<'_>,
        _: &mut O,
        _: &MemoryBudget,
        _: MemoryClass,
        _: &mut MailboxWorldEventOriginalWorkV1,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<
        Arc<ReservedValue<CheckedMailboxWorldEventOriginalV1>>,
        MailboxWorldEventOriginalErrorV1<O::Error>,
    > {
        self.calls += 1;
        Err(ValueTreeError::Invalid("unexpected real mailbox in fresh module fixture").into())
    }
}

pub(super) fn build<'a>(
    world: &World,
    memory: &'a MemoryBudget,
    workspace: MemoryReservation,
) -> Fixture<'a> {
    let mut cas = Cas::new(memory);
    let objects = Objects::new(memory);
    let mut rows = Vec::new();
    let mut pending_original_work = canonical();
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
                    envelope, memory, &mut pending_original_work, &mut || false, 1 << 20,
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
            &mut |_, _, _| {
                Err(crate::world::incremental_body::BodyMutationError::Invalid(
                    "unexpected mailbox in module fixture",
                ))
            },
            &mut |row| {
                assert!(rows.len() < 100000);
                rows.push(row);
                Ok(())
            },
        )
        .unwrap();
    // All five actual retained owners are empty in this fresh initialized world.
    let retained = world.borrow_iws_runtime_history_v1();
    assert!(retained.snapshot_catalog.records.is_empty());
    assert!(retained.journal.events.is_empty());
    assert!(retained.ticks.is_empty());
    assert_eq!(retained.archived_tick_count, 0);
    assert!(retained.rejection_audit.is_empty());
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
    let lanes = RuntimeHistoryLaneV1::ALL.map(|lane| {
        let root = prepare_runtime_history_index_v1(
            lane,
            0,
            std::iter::empty(),
            &mut member(&mut cas, memory, &mut io(), &mut keys(), || false),
            &mut index_work(),
        )
        .unwrap();
        RuntimeHistoryWindowV1 {
            root,
            total: 0,
            start: 0,
            count: 0,
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
        &[ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: capsule_id,
        }],
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
        verify_fact_originals(&core, &mut originals, memory, &mut fact_work(), || false).unwrap();
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
        1,
        &indexes,
        None,
        &[],
        &mut ctx,
        &mut RuntimeHistorySourceWorkV1 {
            rows_remaining: 100,
            index: index_work(),
        },
    )
    .unwrap();
    let promotion =
        prepare_runtime_history_source_promotion_v1(&checked, indexes, memory, || false).unwrap();
    let head = formal::verify_formal_qualified(&s.proof, &s.authority, None, None).unwrap();
    let runtime = finish_prepared_runtime_history_source_promotion_v1(promotion, &head).unwrap();
    let mut mailbox = NoMailbox { calls: 0 };
    let mut current_work = current_work(&roots);
    let current = qualify_world_current_state_v1(
        &roots,
        CognitionCurrentPresenceV3::Enabled,
        &MailboxProvenanceScopeV1::Candidate {
            candidate: &candidate,
            core: &core,
            originals: &facts,
        },
        &mut ctx,
        &mut current_work,
        &super::FixtureScratch::new(memory, MemoryClass::Ordinary),
        &mut originals,
        &mut mailbox,
    )
    .unwrap_or_else(|error|panic!("complete module fixture Current Q: {error:?}; runtime counters config={} effects={} scheduler={} moduleauth={} resource={} content={} revocation={} pending={}; last state={}",
        current_work.runtime.config.cells_remaining,current_work.runtime.effects.cells_remaining,
        current_work.runtime.cells_remaining,current_work.runtime.module_authorization.cells_remaining,
        current_work.runtime.resources.cells_remaining,current_work.runtime.content.objects_remaining,
        current_work.runtime.revocation.json.cells_remaining,current_work.runtime.pending.index.steps_remaining,
        reader.cas.last_state_hex()));
    assert_eq!(mailbox.calls, 0);
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
    Fixture {
        memory,
        cas,
        objects,
        roots,
        head,
        runtime,
        current,
        manifest_hash,
        parent,
        _workspace: workspace,
    }
}

use oasis7_world_state::incremental_world_current_limits::WorldCurrentStateLimitsV1;
pub(super) fn current_work(roots: &RootSetV1) -> WorldCurrentStateWorkV1 {
    let mut work =
    // Exercise the actual endpoint limits adapter, with explicit finite test
    // allowances. Each operation receives its own caller-owned work value.
    WorldCurrentStateLimitsV1 {
        buckets_remaining: 1_000_000,
        bytes_remaining: 256 << 20,
        cells_remaining: 1_000_000,
        changed_cells_remaining: 1_000_000,
        comparison_bytes_remaining: 256 << 20,
        index_steps_remaining: 1_000_000,
        instance_relations_remaining: 1_000_000,
        instances_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        key_changes_remaining: 1_000_000,
        leaves_remaining: 1_000_000,
        legacy_cells_remaining: 1_000_000,
        logical_bytes_remaining: 256 << 20,
        mailbox_occurrences_remaining: 1_000_000,
        max_agents: 100_000,
        max_depth: 64,
        max_instance_scalar_bytes: 8 << 20,
        max_key_bytes: 8 << 20,
        max_key_heap: 8 << 20,
        max_map_keys: 1024,
        max_object_bytes: 8 << 20,
        max_original_bytes: 8 << 20,
        max_payload_bytes: 8 << 20,
        max_pending_node_bytes: 8 << 20,
        max_pending_pairs: 1024,
        max_pending_records: 8,
        max_record_bytes: 8 << 20,
        max_retained_trees: 100_000,
        max_scalar_bytes: 8 << 20,
        max_selected_heap: 8 << 20,
        max_semantic_bytes: 8 << 20,
        max_state_bytes: 8 << 20,
        max_text_bytes: 8 << 20,
        max_tree_heap: 8 << 20,
        membership_cells_remaining: 1_000_000,
        nodes_remaining: 1_000_000,
        objects_remaining: 1_000_000,
        original_bytes_remaining: 256 << 20,
        outer_cells_remaining: 1_000_000,
        outer_records_remaining: 1_000_000,
        outer_rows_remaining: 1_000_000,
        paths_remaining: 1_000_000,
        read_bytes: 256 << 20,
        read_bytes_remaining: 256 << 20,
        reads: 1_000_000,
        records_remaining: 1_000_000,
        states_remaining: 1_000_000,
        steps_remaining: 1_000_000,
        trees_remaining: 1_000_000,
        visits_remaining: 1_000_000,
        write_bytes: 256 << 20,
        writes: 1_000_000,
    }
    .work()
    .unwrap();
    // Canonical ActionEnvelope map keys are short schema field names, not
    // multi-MiB typed record payloads. Key buffers still reserve max_key_bytes
    // before encoding, so keep this limit independent from the larger
    // logical state/record limits used by the complete-Q fixture.
    work.runtime.pending.original.canonical.max_key_bytes = 4096;
    super::super::set_current_fixture_outer_scan_work(roots, &mut work);
    work
}
