//! Real physical GEN + dual history originals; no accepted GEN constructor.
use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::{
    FormalCandidateMaterialV1, IwsCurrentLimitsV1, IwsEndpointVerificationBudgetsV1,
};
use oasis7_formal_protocol::controlled_authority::replicated_protocol::{
    ArtifactObject, ArtifactRole, ClosedRecord,
};
use oasis7_formal_protocol::incremental_artifacts::*;
use oasis7_formal_protocol::incremental_history::{CatalogRootV1, HeaderPreparation};
use std::{collections::BTreeMap, path::Path};
pub(super) struct Prepared {
    pub pages: IncrementalPageStore,
    pub authority: formal::VerifiedAuthorityV1,
    pub candidate: FormalCandidateMaterialV1,
    pub current_limits: IwsCurrentLimitsV1,
}
struct EmptyPages<'a>(&'a MemoryBudget);
impl MerkleNodeReader for EmptyPages<'_> {
    type Node = Vec<u8>;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.0)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
        Err("nonempty GEN artifact tree".into())
    }
}
pub(super) fn prepare(base: &Path, pdir: &Path, rdir: &Path, resource: &str) -> Prepared {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&base.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let stop = AtomicBool::new(false);
    let (native, native_objects) = incremental::native_fixture();
    let native_bytes = native.encode().unwrap();
    let config = GenesisConfigV1 {
        world_id: "complete-genesis-endpoint".into(),
        format_profile:
            oasis7_world_state::incremental_world_format::complete_world_format_profile_v1(),
        initial_content: *blake3::hash(b"actual empty world initial content").as_bytes(),
        activation_config: *blake3::hash(b"actual endpoint activation config").as_bytes(),
        execution_environment: *blake3::hash(&native_bytes).as_bytes(),
    };
    let world = World::new_with_state(WorldState::default());
    let (roots, summary) = physical_genesis::prepare(&world, &pages, &config, &memory, &stop);
    assert_eq!(summary.typed.world_state.native_fields, 98);
    assert_eq!(summary.typed.runtime_fields, 46);
    assert_eq!(summary.typed.cognition_fields, 9);
    // Reuse the real complete GEN producer bound: each declared native
    // descriptors can traverse at most 257 SMT nodes per bootstrap leaf.
    // This sets the original caller allowance once; it never resets a scan.
    let bootstrap_nodes = roots
        .state
        .iter()
        .try_fold(0u64, |sum, root| sum.checked_add(root.leaf_count))
        .unwrap()
        .checked_mul(257)
        .and_then(|n| {
            n.checked_mul(u64::try_from(summary.typed.world_state.native_fields).unwrap())
        })
        .unwrap();
    let mut current_limits = limits();
    current_limits.current.nodes_remaining =
        bootstrap_nodes.max(current_limits.current.nodes_remaining);

    let mut originals = BTreeMap::<[u8; 32], Vec<u8>>::new();
    for bytes in native_objects {
        originals.insert(*blake3::hash(&bytes).as_bytes(), bytes);
    }
    originals.insert(*blake3::hash(&native_bytes).as_bytes(), native_bytes);
    let wasm = b"\0asm\x01\0\0\0".to_vec();
    let policy = b"original-genesis-policy".to_vec();
    let entry = |kind, bytes: &[u8]| ArtifactEntryV1 {
        kind,
        original: OriginalArtifactRefV1 {
            id: *blake3::hash(bytes).as_bytes(),
            byte_len: bytes.len() as u64,
        },
    };
    let wasm_tree = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm)
        .unwrap()
        .append(
            &[entry(ArtifactKindV1::Wasm, &wasm)],
            &EmptyPages(&memory),
            &memory,
            4 << 20,
        )
        .unwrap();
    let rules_tree = ArtifactCatalogV1::empty(ArtifactKindV1::Rules)
        .unwrap()
        .append(
            &[entry(ArtifactKindV1::Rules, &policy)],
            &EmptyPages(&memory),
            &memory,
            4 << 20,
        )
        .unwrap();
    let mut physical_io = physical();
    for node in wasm_tree.new_nodes.iter().chain(&rules_tree.new_nodes) {
        let charged = memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(node.bytes.clone());
        pages
            .put_page(
                *blake3::hash(&charged).as_bytes(),
                &charged,
                &mut physical_io,
                &stop,
            )
            .unwrap();
    }
    let manifest = IwsExecutionManifestV1 {
        scope: config.scope().unwrap(),
        format_profile: config.format_profile,
        native: config.execution_environment,
        executable: hex::decode(&native.components[0].content_digest)
            .unwrap()
            .try_into()
            .unwrap(),
        wasm: wasm_tree.after.digest().unwrap(),
        rules: rules_tree.after.digest().unwrap(),
        policy: entry(ArtifactKindV1::Rules, &policy).original,
        mode: ExecutionManifestModeV1::Genesis,
    };
    let manifest_bytes = manifest.canonical_bytes().unwrap();
    let refs = incremental::FixtureExecutionArtifactRefsV1 {
        manifest: *blake3::hash(&manifest_bytes).as_bytes(),
        wasm: manifest.wasm,
        rules: manifest.rules,
        policy: manifest.policy.id,
    };
    for bytes in [
        wasm,
        policy,
        wasm_tree.after.canonical_bytes().unwrap(),
        rules_tree.after.canonical_bytes().unwrap(),
        manifest_bytes,
    ] {
        originals.insert(*blake3::hash(&bytes).as_bytes(), bytes);
    }
    let before = incremental::setup_with_config_and_state(
        memory.clone(),
        config.clone(),
        roots.state.clone(),
        refs,
    );
    let mut writer = pages.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &memory,
        io: &mut io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8192,
            record_bytes: 8192,
        },
        cancelled: || false,
    };
    let retained = world.borrow_iws_runtime_history_v1();
    assert!(retained.snapshot_catalog.records.is_empty());
    assert!(retained.journal.events.is_empty());
    assert!(retained.ticks.is_empty());
    assert!(retained.rejection_audit.is_empty());
    let lanes = RuntimeHistoryLaneV1::ALL.map(|lane| RuntimeHistoryWindowV1 {
        root: prepare_runtime_history_index_v1(
            lane,
            0,
            std::iter::empty::<Result<RuntimeHistoryIndexRowV1, ValueTreeError>>(),
            &mut write,
            &mut work(),
        )
        .unwrap(),
        total: 0,
        start: 0,
        count: 0,
    });
    let runtime = RuntimeHistoryManifestV1 {
        roots: roots.digest().unwrap(),
        previous_view: None,
        lanes,
        max_snapshots: retained.snapshot_catalog.retention.max_snapshots as u64,
        archived_tick_count: retained.archived_tick_count,
    };
    let runtime_bytes = runtime.canonical_bytes().unwrap();
    let runtime_id = *blake3::hash(&runtime_bytes).as_bytes();
    originals.insert(runtime_id, runtime_bytes.to_vec());
    drop(write);
    drop(writer);
    let (_, _, cognition_bytes) = cognition::prepare(&roots, &pages, &memory);
    let cognition_id = *blake3::hash(&cognition_bytes).as_bytes();
    originals.insert(cognition_id, cognition_bytes);
    let mut certificate = before.certificate.clone();
    certificate
        .body
        .recovery_policy
        .authorized_fencing_resource_bindings = vec![resource.into()];
    certificate.body.authority.recovery_policy_digest =
        certificate.body.recovery_policy.digest().unwrap();
    certificate
        .body
        .authority
        .primary
        .instance
        .storage_fingerprint =
        oasis7_distfs::controlled_authority::fencing::directory_fingerprint(pdir).unwrap();
    certificate
        .body
        .authority
        .replica
        .instance
        .storage_fingerprint =
        oasis7_distfs::controlled_authority::fencing::directory_fingerprint(rdir).unwrap();
    certificate.signature_hex = signed::sign(
        formal::initial_activation_signing_bytes(&certificate.body).unwrap(),
        9,
    );
    let authority = formal::verify_initial_activation(
        &certificate,
        &formal::InitialActivationTrustV1 {
            issuer_public_key: signed::public(9),
            world: certificate.body.authority.world.clone(),
            initial_state_root: certificate.body.initial_state_root.clone(),
            governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
            recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
        },
    )
    .unwrap();
    let input = formal::initial_activation_input_bytes(&certificate).unwrap();
    let input_id = *blake3::hash(&input).as_bytes();
    originals.insert(input_id, input);
    let history = HeaderPreparation {
        config: &config,
        catalog: &CatalogRootV1::empty(config.scope().unwrap()).unwrap(),
        roots: &roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[
            ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Input,
                id: input_id,
            },
            ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: runtime_id,
            },
            ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: cognition_id,
            },
        ],
        operation: [51; 32],
        authority: formal::incremental::authority_binding(&authority, &memory).unwrap(),
        execution_environment: config.execution_environment,
        checkpoint: true,
    }
    .prepare(&memory)
    .unwrap();
    let mut record = before.proof.decision.record.clone();
    record.original_authority_id = authority.authority_id().unwrap();
    record.activation_ref = authority.activation_ref().into();
    record.payload_digest = hex::encode(input_id);
    record
        .artifact_roots
        .insert(ArtifactRole::Input, hex::encode(input_id));
    record.artifact_roots.insert(
        ArtifactRole::IncrementalCommitBody,
        hex::encode(history.body.digest().unwrap()),
    );
    record.artifact_roots.insert(
        ArtifactRole::IncrementalHistoryPrefix,
        hex::encode(history.prefix.digest().unwrap()),
    );
    let proof = signed::proof(
        record,
        &authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        None,
        4,
        5,
        6,
    );
    let candidate_signed = formal::SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    formal::verify_formal_candidate(
        &candidate_signed,
        None,
        &authority,
        None,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    // core_objects uses the freshly signed activation original and actual header.
    let mut setup = before;
    setup.certificate = certificate;
    let (blobs, catalog) = incremental::core_objects(&setup, &history);
    for blob in blobs.iter() {
        originals.insert(*blake3::hash(blob).as_bytes(), blob.to_vec());
    }
    for node in &catalog.new_nodes {
        let charged = memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(node.bytes.clone());
        pages
            .put_page(
                *blake3::hash(&charged).as_bytes(),
                &charged,
                &mut physical_io,
                &stop,
            )
            .unwrap();
    }
    let objects = IncrementalObjectStore::create(&pages, &mut physical_io, &stop).unwrap();
    let mut value_io = logical();
    let mut object_io = ObjectIo {
        physical: &mut physical_io,
        logical: &mut value_io,
        class: MemoryClass::Ordinary,
        cancelled: &stop,
    };
    for bytes in originals.values() {
        put(&objects, bytes, &mut object_io);
    }
    drop(objects);
    for endpoint_dir in [pdir, rdir] {
        let native_dir = endpoint_dir.join("formal-objects-v1");
        std::fs::create_dir_all(&native_dir).unwrap();
        for (id, bytes) in &originals {
            std::fs::write(native_dir.join(hex::encode(id)), bytes).unwrap();
        }
    }
    let body_id = history.body.digest().unwrap();
    let dependencies = originals
        .keys()
        .filter(|id| **id != body_id)
        .map(hex::encode)
        .collect::<Vec<_>>();
    let record = &candidate_signed.record;
    let material = ClosedRecord {
        payload_digest: record.payload_digest.clone(),
        before_state_root: record.before_state_root.clone(),
        after_state_root: record.after_state_root.clone(),
        roots: record.artifact_roots.clone(),
        objects: originals
            .into_iter()
            .map(|(id, bytes)| ArtifactObject {
                content_hash: hex::encode(id),
                bytes,
                references: if id == body_id {
                    dependencies.clone()
                } else {
                    vec![]
                },
            })
            .collect(),
    };
    material.validate_iws().unwrap();
    Prepared {
        current_limits,
        pages,
        authority,
        candidate: FormalCandidateMaterialV1 {
            candidate: candidate_signed,
            material,
            frozen: None,
            frozen_material: None,
            branch: formal::FormalCompletionBranchV1::Normal,
        },
    }
}

pub(super) fn limits() -> IwsCurrentLimitsV1 {
    use oasis7_world_state::incremental_world_current_limits::WorldCurrentStateLimitsV1;
    let current = WorldCurrentStateLimitsV1 {
        buckets_remaining: 1000000,
        bytes_remaining: 1073741824,
        cells_remaining: 1000000,
        changed_cells_remaining: 1000000,
        comparison_bytes_remaining: 1073741824,
        index_steps_remaining: 1000000,
        instance_relations_remaining: 1000000,
        instances_remaining: 1000000,
        json_nodes_remaining: 1000000,
        key_changes_remaining: 1000000,
        leaves_remaining: 1000000,
        legacy_cells_remaining: 1000000,
        logical_bytes_remaining: 1073741824,
        // CandidateMailboxReader pre-admits 256 bytes per occurrence. This
        // fresh GEN has no mailbox history; keep a finite 1 MiB allowance,
        // rather than a 256 MB reservation competing with its page memo.
        mailbox_occurrences_remaining: 4096,
        max_agents: 100,
        max_depth: 64,
        max_instance_scalar_bytes: 16777216,
        max_key_bytes: 4096,
        max_key_heap: 16777216,
        max_map_keys: 1024,
        max_object_bytes: 16777216,
        max_original_bytes: 16777216,
        max_payload_bytes: 16777216,
        max_pending_node_bytes: 16777216,
        max_pending_pairs: 65536,
        // The native field scanner has a hard 256-record page contract.
        max_pending_records: 256,
        max_record_bytes: 16777216,
        max_retained_trees: 100000,
        max_scalar_bytes: 16777216,
        max_selected_heap: 16777216,
        max_semantic_bytes: 1048576,
        max_state_bytes: 16777216,
        max_text_bytes: 16777216,
        max_tree_heap: 16777216,
        membership_cells_remaining: 1000000,
        nodes_remaining: 1000000,
        objects_remaining: 1000000,
        original_bytes_remaining: 1073741824,
        outer_cells_remaining: 1000000,
        outer_records_remaining: 1000000,
        outer_rows_remaining: 1000000,
        paths_remaining: 1000000,
        read_bytes: 1073741824,
        read_bytes_remaining: 1073741824,
        reads: 1000000,
        records_remaining: 1000000,
        states_remaining: 1000000,
        steps_remaining: 1000000,
        trees_remaining: 1000000,
        visits_remaining: 1000000,
        write_bytes: 1073741824,
        writes: 1000000,
    };
    IwsCurrentLimitsV1 { current,scratch_pages:4096,cognition:oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3::Enabled,
        decode_bytes:1<<30,decode_values:1_000_000,decode_depth:64,decode_max_original_bytes:16<<20,decode_max_heap_bytes:32<<20,
        canonical_visits:1_000_000,canonical_bytes:1<<30,canonical_map_keys:4096,canonical_key_bytes:4096 }
}
pub(super) fn budgets() -> IwsEndpointVerificationBudgetsV1 {
    IwsEndpointVerificationBudgetsV1 {
        original_objects: 10000,
        original_bytes: 512 << 20,
        new_pages: 65_536,
        core_merkle_reads: 4_000_000,
        state_scan_nodes: 4_000_000,
        state_scan_bytes: 1 << 30,
        state_scan_leaves: 1_000_000,
        full_value_occurrences: 1_000_000,
        full_value_bytes: 1 << 30,
        max_full_value_bytes: 32 << 20,
        transition_value_occurrences: 1_000_000,
        transition_value_bytes: 1 << 30,
        max_transition_value_bytes: 32 << 20,
        value_nodes: 4_000_000,
        value_read_bytes: 1 << 30,
        max_pending_node_bytes: 16 << 20,
        state_page_io: physical(),
        value_page_io: physical(),
        original_page_io: physical(),
        original_value_nodes: 1_000_000,
        original_value_read_bytes: 512 << 20,
    }
}

pub(super) fn source_work()
-> oasis7_distfs::controlled_authority::formal::endpoint::IwsCandidateSourceWorkV1 {
    use oasis7_distfs::controlled_authority::formal::endpoint::IwsCandidateSourceWorkV1;
    use oasis7_world_state::{
        incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
        incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
        incremental_runtime_history_source::RuntimeHistoryCandidateReadWorkV1,
    };
    let structure = || CborPreflightWorkV1 {
        bytes_remaining: 512 << 20,
        values_remaining: 1_000_000,
        max_depth: 64,
    };
    let decode = || RuntimeHistoryDecodeWorkV1 {
        structure: structure(),
        canonical: canonical_work(),
        max_original_bytes: 16 << 20,
        max_heap_bytes: 32 << 20,
    };
    let cbor = || CognitionHistoryCborWorkV1 {
        bytes_remaining: 512 << 20,
        nodes_remaining: 1_000_000,
        source_reads_remaining: 1_000_000,
        key_bytes_remaining: 64 << 20,
        max_depth: 64,
        max_key_bytes: 4096,
    };
    IwsCandidateSourceWorkV1 {
        router: WorldResultRouterWorkV1 {
            facts: facts(),
            envelope: structure(),
            runtime: decode(),
            cognition: cbor(),
        },
        runtime: RuntimeHistoryCandidateReadWorkV1 {
            rows_remaining: 1_000_000,
            max_new_rows: 100000,
            index: work(),
            originals: facts(),
            decode: decode(),
            max_world_event_heap_bytes: 32 << 20,
        },
        cognition_index: CognitionHistoryIndexWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
        },
        cognition_cbor: cbor(),
        cognition_hash: CognitionHeadHashWorkV1 {
            input_bytes_remaining: 512 << 20,
            hash_nodes_remaining: 1_000_000,
        },
        cognition_lifecycle: CognitionLifecycleWorkV2 {
            key_bytes_remaining: 64 << 20,
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
        },
        history_physical: physical(),
        history_values: logical(),
        persist_physical: physical(),
        persist_values: logical(),
        write_physical: physical(),
        write_values: logical(),
        write_keys: SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        },
        write_limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 16 << 20,
            record_bytes: 16 << 20,
        },
        history_occurrences: 1_000_000,
        history_hash_bytes: 512 << 20,
        persist_occurrences: 1_000_000,
        artifacts: ExecutionArtifactBudget {
            structural: CoreBudget {
                objects_remaining: 10000,
                original_bytes_remaining: 512 << 20,
                new_pages_remaining: 1_000_000,
                merkle_reads_remaining: 4_000_000,
            },
            originals: OriginalArtifactBudget {
                objects_remaining: 10000,
                bytes_remaining: 512 << 20,
                max_object_bytes: 16 << 20,
            },
        },
        native_chunk_occurrences: 10000,
        native_chunk_bytes: 512 << 20,
        frozen_wire_bytes: 64 << 20,
        frozen_material: formal::IncrementalFrozenMaterialWorkV1 {
            metadata_entries_remaining: 10000,
            comparison_bytes_remaining: 512 << 20,
            hash_bytes_remaining: 512 << 20,
            frozen_wire_bytes_remaining: 64 << 20,
            original_calls_remaining: 10000,
            original_bytes_remaining: 512 << 20,
            max_original_bytes: 16 << 20,
        },
        original_sync_occurrences: 1_000_000,
        original_sync_bytes: 512 << 20,
        original_sync_physical: physical(),
        original_sync_values: logical(),
        cognition_sync_physical: physical(),
        cognition_sync_values: logical(),
        publish_physical: physical(),
        // StoreSessions receives this field, not Core.new_pages. Publisher
        // pre-admits 2048 bytes per memo slot plus 4 KiB: about 256 MiB,
        // within the fixture's real 512 MiB pool. The genuine fixture writes
        // over 100,000 recipes; 65,536 distinct synchronized pages is too low.
        max_synced_pages: 131_072,
        // Publisher charges 304 per lookup and up to 912 per insertion at
        // this bound. Cover state/value logical revisits as well as unique
        // pages; four million work units cannot cover this full GEN graph.
        sync_memo_steps: 4_000_000_000,
        catalog_read_physical: physical(),
        catalog_publish_physical: physical(),
        catalog_occurrences: 1_000_000,
        transition_scan: StateScanBudget {
            nodes_remaining: 4_000_000,
            read_bytes_remaining: 1 << 30,
            leaves_remaining: 1_000_000,
        },
        transition_occurrences: 1_000_000,
    }
}
