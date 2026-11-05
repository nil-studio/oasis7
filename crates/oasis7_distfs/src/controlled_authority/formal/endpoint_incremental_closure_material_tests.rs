//! Real signed candidate/core and native/artifact originals; executable fixture
//! bytes are inspected, never executed. These are not business-driver proofs.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            self,
            test_fixtures::{self as fixture, incremental as f},
        },
        replicated_protocol::ClosedRecord,
    },
    incremental_artifacts::*,
    incremental_material::{CoreRequest, verify_commit_core},
};
use std::{collections::BTreeMap, io::Write};
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
        Err("unexpected nonempty artifact fixture page".into())
    }
}
struct Objects {
    memory: MemoryBudget,
    values: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    forbidden: Vec<[u8; 32]>,
}
impl OriginalObjectReader for Objects {
    type Error = std::io::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        &self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> std::io::Result<Arc<ReservedValue<Vec<u8>>>> {
        assert!(
            !self.forbidden.contains(id),
            "unchanged static inventory read"
        );
        let b = self.values.get(id).ok_or(std::io::ErrorKind::NotFound)?;
        if b.len() > max || !b.uses_budget(memory) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        Ok(b.clone())
    }
}
impl OriginalObjectStreamReader for Objects {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        out: &mut dyn Write,
    ) -> std::io::Result<u64> {
        let b = self.read_original(id, usize::try_from(max).unwrap(), memory)?;
        out.write_all(&b)?;
        Ok(b.len() as u64)
    }
}
impl Objects {
    fn put(&mut self, b: Vec<u8>) -> [u8; 32] {
        let id = *blake3::hash(&b).as_bytes();
        let fee = self
            .memory
            .reserve(b.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        self.values.insert(id, Arc::new(fee.retain(b)));
        id
    }
}
fn work() -> CoreBudget {
    CoreBudget {
        objects_remaining: 128,
        original_bytes_remaining: 8 << 20,
        new_pages_remaining: 8192,
        merkle_reads_remaining: 20000,
    }
}
fn artifacts() -> ExecutionArtifactBudget {
    ExecutionArtifactBudget {
        structural: work(),
        originals: oasis7_formal_protocol::incremental_material::OriginalArtifactBudget {
            objects_remaining: 128,
            bytes_remaining: 8 << 20,
            max_object_bytes: 1 << 20,
        },
    }
}
fn bridge() -> NativeChunkReadWorkV1 {
    NativeChunkReadWorkV1 {
        occurrences_remaining: 128,
        bytes_remaining: 8 << 20,
        frozen_wire_bytes_remaining: 64 << 20,
    }
}
struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Material {
    s: f::Setup,
    candidate: FormalCandidateMaterialV1,
    context: formal::VerifiedCandidateContextV1,
    core: ReservedValue<CheckedCommitCore>,
    objects: Objects,
    pages: IncrementalPageStore,
    directory: Directory,
    manifest: IwsExecutionManifestV1,
    _fixture: oasis7_proto::memory_budget::MemoryReservation,
}
fn io() -> crate::incremental_page_store::PageIoBudget {
    crate::incremental_page_store::PageIoBudget {
        reads_remaining: 100000,
        read_bytes_remaining: 128 << 20,
        writes_remaining: 100000,
        write_bytes_remaining: 128 << 20,
    }
}
fn genesis() -> Material {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory = Directory(std::env::temp_dir().join(format!(
        "oasis7-material-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )));
    std::fs::create_dir(&directory.0).unwrap();
    let native_dir = directory.0.join("formal-objects-v1");
    std::fs::create_dir(&native_dir).unwrap();
    std::fs::write(directory.0.join("formal-checkpoint-v1.cbor"), b"checkpoint").unwrap();
    crate::controlled_authority::formal::physical::initialize(&native_dir).unwrap();
    let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
    let fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut objects = Objects {
        memory: memory.clone(),
        values: BTreeMap::new(),
        forbidden: Vec::new(),
    };
    let (native, raw) = f::native_fixture();
    for b in raw {
        std::fs::write(native_dir.join(blake3::hash(&b).to_hex().to_string()), b).unwrap();
    }
    let native_id = objects.put(native.encode().unwrap());
    let wasm = b"\0asm\x01\0\0\0".to_vec();
    let policy = b"original-policy".to_vec();
    let entry = |kind, bytes: &[u8]| ArtifactEntryV1 {
        kind,
        original: OriginalArtifactRefV1 {
            id: *blake3::hash(bytes).as_bytes(),
            byte_len: bytes.len() as u64,
        },
    };
    let wasm_delta = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm)
        .unwrap()
        .append(
            &[entry(ArtifactKindV1::Wasm, &wasm)],
            &EmptyPages(&memory),
            &memory,
            4 << 20,
        )
        .unwrap();
    let rules_delta = ArtifactCatalogV1::empty(ArtifactKindV1::Rules)
        .unwrap()
        .append(
            &[entry(ArtifactKindV1::Rules, &policy)],
            &EmptyPages(&memory),
            &memory,
            4 << 20,
        )
        .unwrap();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut physical = io();
    for node in wasm_delta.new_nodes.iter().chain(&rules_delta.new_nodes) {
        let bytes = memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(node.bytes.clone());
        pages
            .put_page(
                *blake3::hash(&bytes).as_bytes(),
                &bytes,
                &mut physical,
                &stop,
            )
            .unwrap();
    }
    let config = f::genesis_config(native_id);
    let manifest = IwsExecutionManifestV1 {
        scope: config.scope().unwrap(),
        format_profile: config.format_profile,
        native: native_id,
        executable: hex::decode(&native.components[0].content_digest)
            .unwrap()
            .try_into()
            .unwrap(),
        wasm: wasm_delta.after.digest().unwrap(),
        rules: rules_delta.after.digest().unwrap(),
        policy: entry(ArtifactKindV1::Rules, &policy).original,
        mode: ExecutionManifestModeV1::Genesis,
    };
    let manifest_id = objects.put(manifest.canonical_bytes().unwrap());
    objects.put(wasm);
    objects.put(policy);
    objects.put(wasm_delta.after.canonical_bytes().unwrap());
    objects.put(rules_delta.after.canonical_bytes().unwrap());
    let s = f::setup_with_execution_artifacts(
        memory.clone(),
        native_id,
        manifest_id,
        manifest.wasm,
        manifest.rules,
        manifest.policy.id,
    );
    let (blobs, catalog) = f::core_objects(&s, &s.history);
    for b in blobs.iter() {
        objects
            .values
            .insert(*blake3::hash(b).as_bytes(), b.clone());
    }
    for node in &catalog.new_nodes {
        let bytes = memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(node.bytes.clone());
        pages
            .put_page(
                *blake3::hash(&bytes).as_bytes(),
                &bytes,
                &mut physical,
                &stop,
            )
            .unwrap();
    }
    let signed = formal::SignedCanonicalCandidateV1 {
        record: s.proof.decision.record.clone(),
        writer_signature_hex: s.proof.decision.writer_signature_hex.clone(),
    };
    let context = formal::verify_formal_candidate(
        &signed,
        None,
        &s.authority,
        None,
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let state = pages.state_session(io(), MemoryClass::Ordinary).unwrap();
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &context,
            original_authority: &s.authority,
            parent: None,
            max_pending_node_bytes: 4 << 20,
        },
        &mut objects,
        &state,
        &memory,
        &mut work(),
        || false,
    )
    .unwrap();
    let record = &signed.record;
    let candidate = FormalCandidateMaterialV1 {
        material: ClosedRecord {
            payload_digest: record.payload_digest.clone(),
            before_state_root: record.before_state_root.clone(),
            after_state_root: record.after_state_root.clone(),
            roots: record.artifact_roots.clone(),
            objects: Vec::new(),
        },
        candidate: signed,
        frozen: None,
        frozen_material: None,
        branch: formal::FormalCompletionBranchV1::Normal,
    };
    Material {
        s,
        candidate,
        context,
        core,
        objects,
        pages,
        directory,
        manifest,
        _fixture: fixture,
    }
}
fn check(
    m: &mut Material,
    parent: Option<(
        &VerifiedFormalCommitV1,
        &ReservedValue<CandidateExecutionMaterialV1>,
    )>,
) -> Result<ReservedValue<CandidateExecutionMaterialV1>, FormalError> {
    let state = m.pages.state_session(io(), MemoryClass::Ordinary).unwrap();
    verify_in_directory(
        &m.directory.0.join("formal-objects-v1"),
        &m.candidate,
        &m.context,
        &m.core,
        &mut m.objects,
        &m.pages,
        &state,
        parent,
        &mut work(),
        &mut artifacts(),
        &mut bridge(),
        &mut || false,
    )
}
#[test]
fn signed_genesis_material_retains_real_native_artifact_proofs_and_rejects_wrong_true_parent() {
    let mut m = genesis();
    let proof = check(&mut m, None).unwrap();
    assert_eq!(proof.candidate_record_id(), m.s.commit.verified_record_id());
    assert_eq!(proof.body_id(), m.s.history.body.digest().unwrap());
    assert_eq!(proof.manifest_id(), m.manifest.digest().unwrap());
    let other = f::setup();
    assert!(check(&mut m, Some((&other.commit, &proof))).is_err());
    let same = formal::verify_formal_qualified(&m.s.proof, &m.s.authority, None, None).unwrap();
    assert!(check(&mut m, Some((&same, &proof))).is_err());
}

fn normal(m: &mut Material, change_policy: bool) -> VerifiedFormalCommitV1 {
    use crate::controlled_authority::replicated_protocol::ArtifactRole;
    use oasis7_formal_protocol::{incremental_execution::*, incremental_history::*};
    let memory = m.s.memory.clone();
    let state = m.pages.state_session(io(), MemoryClass::Ordinary).unwrap();
    let parent = ExecutionParent {
        commit: m.s.history.body.digest().unwrap(),
        roots: m.s.roots.clone(),
        authority: m.s.history.body.authority,
    };
    let fence = parent.fence().unwrap();
    let operation = [91; 32];
    let input = m.objects.put(b"real normal material input".to_vec());
    let continuation_bytes = b"real continuation".to_vec();
    let continuation = OriginalArtifactRefV1 {
        id: m.objects.put(continuation_bytes.clone()),
        byte_len: continuation_bytes.len() as u64,
    };
    let mut tx = ExecutionTransaction::begin(
        parent,
        operation,
        &state,
        &memory,
        TransactionLimits {
            max_reads: 4,
            max_writes: 2,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    tx.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    let guard = tx.guard();
    let transition = tx.seal(&guard, &fence, 1).unwrap();
    let mut manifest = m.manifest.clone();
    manifest.mode = ExecutionManifestModeV1::Execution {
        parent_commit: m.s.history.body.digest().unwrap(),
        parent_roots: m.s.roots.digest().unwrap(),
        operation,
        input,
        continuation,
    };
    if change_policy {
        let bytes = b"missing changed policy";
        manifest.policy = OriginalArtifactRefV1 {
            id: *blake3::hash(bytes).as_bytes(),
            byte_len: bytes.len() as u64,
        };
    }
    let manifest_id = m.objects.put(manifest.canonical_bytes().unwrap());
    let parent_catalog =
        m.s.history
            .body
            .catalog_before
            .extend(&m.s.history.body, &state, &memory, 4 << 20)
            .unwrap();
    let history = HeaderPreparation {
        config: &m.s.config,
        catalog: &parent_catalog.after,
        roots: &transition.record.after,
        parent: Some(&m.s.history.body),
        parent_history: Some(&m.s.history.prefix),
        transition: Some(&transition.record),
        genesis_facts: &[],
        operation,
        authority: m.s.history.body.authority,
        execution_environment: m.s.config.execution_environment,
        checkpoint: true,
    }
    .prepare(&memory)
    .unwrap();
    let catalog = parent_catalog
        .after
        .extend(&history.body, &state, &memory, 4 << 20)
        .unwrap();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut physical = io();
    for node in &catalog.new_nodes {
        let bytes = memory
            .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(node.bytes.clone());
        m.pages
            .put_page(
                *blake3::hash(&bytes).as_bytes(),
                &bytes,
                &mut physical,
                &stop,
            )
            .unwrap();
    }
    for bytes in [
        transition.record.after.canonical_bytes().unwrap(),
        history.transition_bytes.as_ref().unwrap().to_vec(),
        history.body.canonical_bytes().unwrap(),
        history.prefix.canonical_bytes().unwrap(),
        history.segment.canonical_bytes().unwrap(),
        history.facts.canonical_bytes(&memory).unwrap().to_vec(),
        history
            .checkpoint
            .as_ref()
            .unwrap()
            .canonical_bytes()
            .unwrap(),
        catalog.after.canonical_bytes().unwrap(),
    ] {
        m.objects.put(bytes);
    }
    let mut frozen = fixture::frozen(&m.s.authority, &m.s.commit, None);
    frozen.reservation.body.schema_version = formal::incremental::IWS_FROZEN_SCHEMA;
    frozen.reservation.body.payload_digest = hex::encode(input);
    frozen.reservation.body.request_digest = hex::encode(operation);
    frozen.reservation.body.runtime_manifest_digest = hex::encode(manifest_id);
    frozen.reservation.body.execution_policy_digest = hex::encode(manifest.policy.id);
    let mut frozen_material = ClosedRecord {
        payload_digest: hex::encode(input),
        before_state_root: hex::encode(m.s.roots.digest().unwrap()),
        after_state_root: hex::encode(m.s.roots.digest().unwrap()),
        roots: m.s.commit.record().artifact_roots.clone(),
        objects: Vec::new(),
    };
    frozen_material
        .roots
        .insert(ArtifactRole::Input, hex::encode(input));
    frozen_material
        .roots
        .insert(ArtifactRole::ExecutionManifest, hex::encode(manifest_id));
    frozen.reservation.body.frozen_material_digest =
        formal::frozen_material_digest(&frozen_material).unwrap();
    frozen.reservation.signature_hex = fixture::sign(
        formal::frozen_signing_bytes(&frozen.reservation.body).unwrap(),
        4,
    );
    let reservation = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = reservation.clone();
        receipt.signature_hex = fixture::sign(
            formal::frozen_receipt_signing_bytes(&receipt.body).unwrap(),
            key,
        );
    }
    let mut record = fixture::execution(&m.s.authority, &frozen);
    record.schema_version = formal::incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(transition.record.after.digest().unwrap());
    record.artifact_roots = m.s.commit.record().artifact_roots.clone();
    for (role, id) in [
        (ArtifactRole::Input, input),
        (ArtifactRole::ExecutionManifest, manifest_id),
        (
            ArtifactRole::IncrementalRootSet,
            transition.record.after.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            history.body.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            history.prefix.digest().unwrap(),
        ),
    ] {
        record.artifact_roots.insert(role, hex::encode(id));
    }
    let proof = fixture::proof(
        record,
        &m.s.authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen.clone()),
        4,
        5,
        6,
    );
    let signed = formal::SignedCanonicalCandidateV1 {
        record: proof.decision.record.clone(),
        writer_signature_hex: proof.decision.writer_signature_hex.clone(),
    };
    let context = formal::verify_formal_candidate(
        &signed,
        Some(&frozen),
        &m.s.authority,
        Some(&m.s.commit),
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &context,
            original_authority: &m.s.authority,
            parent: Some(&m.s.commit),
            max_pending_node_bytes: 4 << 20,
        },
        &mut m.objects,
        &state,
        &memory,
        &mut work(),
        || false,
    )
    .unwrap();
    let record = &signed.record;
    m.candidate = FormalCandidateMaterialV1 {
        material: ClosedRecord {
            payload_digest: record.payload_digest.clone(),
            before_state_root: record.before_state_root.clone(),
            after_state_root: record.after_state_root.clone(),
            roots: record.artifact_roots.clone(),
            objects: Vec::new(),
        },
        candidate: signed,
        frozen: Some(frozen),
        frozen_material: Some(frozen_material),
        branch: formal::FormalCompletionBranchV1::Normal,
    };
    m.context = context;
    m.core = core;
    m.manifest = manifest;
    formal::verify_formal_qualified(&proof, &m.s.authority, Some(&m.s.commit), None).unwrap()
}
#[test]
fn signed_ordinary_static_inheritance_reads_no_unchanged_inventory_and_rejects_replaced_lease() {
    let mut m = genesis();
    let baseline = check(&mut m, None).unwrap();
    let parent = formal::verify_formal_qualified(&m.s.proof, &m.s.authority, None, None).unwrap();
    let accepted = normal(&mut m, false);
    let refs = m.core.execution_artifacts();
    m.objects
        .forbidden
        .extend([refs.native, refs.wasm, refs.rules, m.manifest.policy.id]);
    let proof = check(&mut m, Some((&parent, &baseline))).unwrap();
    assert_eq!(proof.candidate_record_id(), accepted.verified_record_id());
    assert!(Arc::ptr_eq(proof.baseline(), baseline.baseline()));
    m.objects.forbidden.clear();
    let native_dir = m.directory.0.join("formal-objects-v1");
    let guard = native_dir.join(".physical-guard-v1");
    std::fs::rename(&guard, native_dir.join("old-guard")).unwrap();
    std::fs::write(&guard, []).unwrap();
    let (_, chunks) = f::native_fixture();
    for bytes in chunks {
        std::fs::remove_file(native_dir.join(blake3::hash(&bytes).to_hex().to_string())).unwrap();
    }
    // Same refs under a different real native lease cannot reuse the old
    // baseline: missing chunk bytes must now be read and rejected.
    assert!(check(&mut m, Some((&parent, &baseline))).is_err());
}
#[test]
fn signed_ordinary_changed_policy_cannot_inherit_or_hide_missing_original() {
    let mut m = genesis();
    let baseline = check(&mut m, None).unwrap();
    let parent = formal::verify_formal_qualified(&m.s.proof, &m.s.authority, None, None).unwrap();
    normal(&mut m, true);
    assert!(check(&mut m, Some((&parent, &baseline))).is_err());
}

#[test]
fn signed_ordinary_replaced_page_lease_cannot_inherit_static_baseline() {
    let mut m = genesis();
    let baseline = check(&mut m, None).unwrap();
    let parent = formal::verify_formal_qualified(&m.s.proof, &m.s.authority, None, None).unwrap();
    normal(&mut m, false);
    let state = m.pages.state_session(io(), MemoryClass::Ordinary).unwrap();
    let lease = m.pages.root().join("lease");
    std::fs::rename(&lease, m.pages.root().join("old-lease")).unwrap();
    std::fs::write(&lease, []).unwrap();
    assert!(
        verify_in_directory(
            &m.directory.0.join("formal-objects-v1"),
            &m.candidate,
            &m.context,
            &m.core,
            &mut m.objects,
            &m.pages,
            &state,
            Some((&parent, &baseline)),
            &mut work(),
            &mut artifacts(),
            &mut bridge(),
            &mut || false,
        )
        .is_err()
    );
}
