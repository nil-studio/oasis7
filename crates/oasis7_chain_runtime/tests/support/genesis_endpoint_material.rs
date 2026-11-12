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
    pub activation: formal::SignedInitialActivationV1,
    pub trust: formal::InitialActivationTrustV1,
    pub candidate: FormalCandidateMaterialV1,
    pub current_limits: IwsCurrentLimitsV1,
}
#[path = "genesis_native_archive.rs"]
mod native_archive;
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
    prepare_inputs(
        base,
        pdir,
        rdir,
        resource,
        incremental::native_fixture(),
        None,
    )
}
/// Original byte inputs only. The target endpoint still verifies the signed
/// authority against its own actual directories before any acceptance.
pub(super) fn prepare_with_native_archive(
    base: &Path,
    pdir: &Path,
    rdir: &Path,
    resource: &str,
    archive: &Path,
    target_fingerprints: Option<[String; 2]>,
) -> Prepared {
    prepare_inputs(
        base,
        pdir,
        rdir,
        resource,
        native_archive::load(archive),
        target_fingerprints,
    )
}
fn prepare_inputs(
    base: &Path,
    pdir: &Path,
    rdir: &Path,
    resource: &str,
    native_input: (
        oasis7_formal_protocol::native_environment::NativeExecutionEnvironmentV1,
        Vec<Vec<u8>>,
    ),
    target_fingerprints: Option<[String; 2]>,
) -> Prepared {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let pages =
        IncrementalPageStore::create(&base.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let stop = AtomicBool::new(false);
    let (native, native_objects) = native_input;
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
    // Native chunks are external descriptor dependencies. In particular a
    // real executable must not enter the 8 MiB ClosedRecord object envelope.
    let native_originals = native_objects
        .into_iter()
        .map(|bytes| (*blake3::hash(&bytes).as_bytes(), bytes))
        .collect::<BTreeMap<_, _>>();
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
    let [primary_fingerprint, replica_fingerprint] = target_fingerprints.unwrap_or_else(|| {
        [
            oasis7_distfs::controlled_authority::fencing::directory_fingerprint(pdir).unwrap(),
            oasis7_distfs::controlled_authority::fencing::directory_fingerprint(rdir).unwrap(),
        ]
    });
    certificate
        .body
        .authority
        .primary
        .instance
        .storage_fingerprint = primary_fingerprint;
    certificate
        .body
        .authority
        .replica
        .instance
        .storage_fingerprint = replica_fingerprint;
    certificate.signature_hex = signed::sign(
        formal::initial_activation_signing_bytes(&certificate.body).unwrap(),
        9,
    );
    let trust = formal::InitialActivationTrustV1 {
        issuer_public_key: signed::public(9),
        world: certificate.body.authority.world.clone(),
        initial_state_root: certificate.body.initial_state_root.clone(),
        governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
        recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
    };
    let authority = formal::verify_initial_activation(&certificate, &trust).unwrap();
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
    setup.certificate = certificate.clone();
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
        for (id, bytes) in originals.iter().chain(native_originals.iter()) {
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
        activation: certificate,
        trust,
        candidate: FormalCandidateMaterialV1 {
            candidate: candidate_signed,
            material,
            frozen: None,
            frozen_material: None,
            branch: formal::FormalCompletionBranchV1::Normal,
        },
    }
}

pub(super) use super::endpoint_work::{budgets, limits, source_work};
