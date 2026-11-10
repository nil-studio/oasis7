//! Real signed parent/reservation and original bytes, without World or candidate.
//! These fixtures prove content only, never durable GEN or endpoint acceptance.
use super::super::{
    FormalFrozenBodyV1, SignedFormalFrozenV1, frozen_signing_bytes, test_fixtures as fixture,
    verify_formal_reservation,
};
use super::*;
use crate::controlled_authority::replicated_protocol::ArtifactObject;
use crate::incremental_artifacts::{ArtifactCatalogV1, ArtifactEntryV1, ArtifactKindV1};
use crate::incremental_material::OriginalObjectReader;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, PartialEq, Eq)]
enum SourceError {
    Missing,
    Bound,
    Output,
    Sentinel,
}
impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SourceError {}
struct Objects {
    values: BTreeMap<[u8; 32], Arc<ReservedValue<Vec<u8>>>>,
    memory: MemoryBudget,
    fault: Option<[u8; 32]>,
    _metadata: oasis7_proto::memory_budget::MemoryReservation,
}
impl Objects {
    fn new(memory: &MemoryBudget) -> Self {
        Self {
            values: BTreeMap::new(),
            memory: memory.clone(),
            fault: None,
            _metadata: memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap(),
        }
    }
    fn put(&mut self, bytes: Vec<u8>) -> [u8; 32] {
        let id = *blake3::hash(&bytes).as_bytes();
        let fee = self
            .memory
            .reserve(bytes.capacity() + 128, MemoryClass::Ordinary)
            .unwrap();
        self.values.insert(id, Arc::new(fee.retain(bytes)));
        id
    }
}
impl OriginalObjectReader for Objects {
    type Error = SourceError;
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
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        if self.fault == Some(*id) {
            return Err(SourceError::Sentinel);
        }
        let value = self.values.get(id).ok_or(SourceError::Missing)?;
        if !value.uses_budget(memory) || value.len() > max {
            return Err(SourceError::Bound);
        }
        Ok(Arc::clone(value))
    }
}
impl OriginalObjectStreamReader for Objects {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        out: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        let value = self.read_original(
            id,
            usize::try_from(max).map_err(|_| SourceError::Bound)?,
            memory,
        )?;
        for chunk in value.chunks(3) {
            out.write_all(chunk).map_err(|_| SourceError::Output)?;
        }
        Ok(value.len() as u64)
    }
}
// Actual empty, admitted Merkle source for building the first Rules catalog.
// Any unexpected nonempty parent lookup fails instead of inventing a page.
struct EmptyArtifactPages<'a>(&'a MemoryBudget);
impl crate::incremental_state::MerkleNodeReader for EmptyArtifactPages<'_> {
    type Node = Vec<u8>;
    fn read_node(&self, _: &str, _: usize) -> Result<Option<Self::Node>, String> {
        Err("unexpected nonempty artifact parent".into())
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.0)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(MemoryClass::Ordinary)
    }
}
struct Fixture {
    s: fixture::incremental::Setup,
    signed: SignedFormalFrozenV1,
    material: ClosedRecord,
    objects: Objects,
    input: [u8; 32],
}
fn build(seed: u8) -> Fixture {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let mut objects = Objects::new(&memory);
    let (native, raw) = fixture::incremental::native_fixture();
    let native_id = objects.put(native.encode().unwrap());
    for value in raw {
        objects.put(value);
    }
    let policy = vec![b'p', seed];
    let policy_ref = OriginalArtifactRefV1 {
        id: objects.put(policy.clone()),
        byte_len: policy.len() as u64,
    };
    let wasm = ArtifactCatalogV1::empty(ArtifactKindV1::Wasm).unwrap();
    let wasm_id = objects.put(wasm.canonical_bytes().unwrap());
    let rules = ArtifactCatalogV1::empty(ArtifactKindV1::Rules)
        .unwrap()
        .append(
            &[ArtifactEntryV1 {
                kind: ArtifactKindV1::Rules,
                original: policy_ref,
            }],
            &EmptyArtifactPages(&memory),
            &memory,
            4 << 20,
        )
        .unwrap();
    let rules_id = objects.put(rules.after.canonical_bytes().unwrap());
    let config = fixture::incremental::genesis_config(native_id);
    let parent_manifest = IwsExecutionManifestV1 {
        scope: config.scope().unwrap(),
        format_profile: config.format_profile,
        native: native_id,
        executable: hex::decode(&native.components[0].content_digest)
            .unwrap()
            .try_into()
            .unwrap(),
        wasm: wasm_id,
        rules: rules_id,
        policy: policy_ref,
        mode: ExecutionManifestModeV1::Genesis,
    };
    let parent_manifest_id = objects.put(parent_manifest.canonical_bytes().unwrap());
    let s = fixture::incremental::setup_with_execution_artifacts(
        memory,
        native_id,
        parent_manifest_id,
        wasm_id,
        rules_id,
        policy_ref.id,
    );
    objects.put(s.config.canonical_bytes().unwrap());
    objects.put(s.roots.canonical_bytes().unwrap());
    objects.put(s.history.body.canonical_bytes().unwrap());
    objects.put(s.history.prefix.canonical_bytes().unwrap());
    let input = objects.put(vec![b'i', seed, 0, 1, 2, 3]);
    let operation = [91; 32];
    let continuation_bytes = vec![b'c', seed, 4, 5];
    let continuation = OriginalArtifactRefV1 {
        id: objects.put(continuation_bytes.clone()),
        byte_len: continuation_bytes.len() as u64,
    };
    let manifest = IwsExecutionManifestV1 {
        mode: ExecutionManifestModeV1::Execution {
            parent_commit: s.history.body.digest().unwrap(),
            parent_roots: s.roots.digest().unwrap(),
            operation,
            input,
            continuation,
        },
        ..parent_manifest
    };
    let manifest_id = objects.put(manifest.canonical_bytes().unwrap());
    let mut roots = s.commit.record().artifact_roots.clone();
    roots.insert(ArtifactRole::Input, hex::encode(input));
    roots.insert(ArtifactRole::ExecutionManifest, hex::encode(manifest_id));
    let mut needed = roots
        .values()
        .map(|text| hex::decode(text).unwrap().try_into().unwrap())
        .collect::<BTreeSet<[u8; 32]>>();
    needed.insert(policy_ref.id);
    needed.insert(continuation.id);
    let material = ClosedRecord {
        payload_digest: hex::encode(input),
        before_state_root: s.commit.record().after_state_root.clone(),
        after_state_root: s.commit.record().after_state_root.clone(),
        roots,
        objects: needed
            .into_iter()
            .map(|id| {
                let mut references = if id == manifest_id {
                    vec![hex::encode(policy_ref.id), hex::encode(continuation.id)]
                } else {
                    vec![]
                };
                references.sort();
                references.dedup();
                ArtifactObject {
                    content_hash: hex::encode(id),
                    bytes: objects.values[&id].to_vec(),
                    references,
                }
            })
            .collect(),
    };
    material.validate_iws().unwrap();
    let digest =
        frozen_material_digest_bounded_v1(&material, &s.memory, &mut (64 << 20), || false).unwrap();
    let body = FormalFrozenBodyV1 {
        schema_version: super::super::incremental::IWS_FROZEN_SCHEMA,
        world: s.authority.definition().world.clone(),
        authority_id: s.authority.authority_id().unwrap(),
        epoch: s.authority.definition().epoch,
        activation_ref: s.authority.activation_ref().to_owned(),
        parent: s.commit.head(),
        next_position: s.commit.record().position + 1,
        request_digest: hex::encode(operation),
        payload_digest: hex::encode(input),
        runtime_manifest_digest: hex::encode(manifest_id),
        governing_manifest_digest: s.commit.record().governing_manifest_digest.clone(),
        execution_policy_digest: hex::encode(policy_ref.id),
        pre_state_root: s.commit.record().after_state_root.clone(),
        frozen_material_digest: hex::encode(digest),
        handoff: None,
    };
    let signature_hex = fixture::sign(frozen_signing_bytes(&body).unwrap(), 4);
    let signed = SignedFormalFrozenV1 {
        body,
        signature_hex,
    };
    verify_formal_reservation(&signed, &s.authority, &s.commit).unwrap();
    Fixture {
        s,
        signed,
        material,
        objects,
        input,
    }
}
fn work() -> IncrementalFrozenMaterialWorkV1 {
    IncrementalFrozenMaterialWorkV1 {
        metadata_entries_remaining: 1_000_000,
        comparison_bytes_remaining: 64 << 20,
        hash_bytes_remaining: 32 << 20,
        frozen_wire_bytes_remaining: 64 << 20,
        original_calls_remaining: 16,
        original_bytes_remaining: 16 << 20,
        max_original_bytes: 8 << 20,
    }
}
#[test]
fn signed_frozen_content_needs_only_actual_parent_and_request_originals() {
    let mut f = build(1);
    let reservation = verify_formal_reservation(&f.signed, &f.s.authority, &f.s.commit).unwrap();
    let before = f.s.memory.used();
    let mut budget = work();
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        &f.s.commit,
        &f.material,
        &mut f.objects,
        &f.s.memory,
        &mut budget,
        || false,
    )
    .unwrap();
    assert_eq!(checked.parent_record_id(), f.s.commit.verified_record_id());
    assert_eq!(checked.input().id, f.input);
    assert_eq!(checked.input().byte_len, 6);
    let expected_material: [u8; 32] = hex::decode(&f.signed.body.frozen_material_digest)
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(checked.material_id(), expected_material);
    assert_eq!(
        checked.material().roots[&ArtifactRole::IncrementalCommitBody],
        f.s.commit.record().artifact_roots[&ArtifactRole::IncrementalCommitBody]
    );
    assert!(budget.original_calls_remaining < work().original_calls_remaining);
    assert!(budget.frozen_wire_bytes_remaining < work().frozen_wire_bytes_remaining);
    assert_eq!(f.s.memory.used(), before + checked.reserved_bytes());
    drop(checked);
    assert_eq!(f.s.memory.used(), before);
}
#[test]
fn frozen_wrong_parent_role_and_tampered_material_do_not_return_content_owner() {
    let mut f = build(1);
    let other = build(2);
    let reservation = verify_formal_reservation(&f.signed, &f.s.authority, &f.s.commit).unwrap();
    assert!(matches!(
        verify_incremental_frozen_material_v1(
            &reservation,
            &other.s.commit,
            &f.material,
            &mut f.objects,
            &f.s.memory,
            &mut work(),
            || false
        ),
        Err(Error::Invalid("Frozen parent record"))
    ));
    let mut material = f.material.clone();
    material.roots.insert(
        ArtifactRole::Wasm,
        material.roots[&ArtifactRole::Rules].clone(),
    );
    assert!(matches!(
        verify_incremental_frozen_material_v1(
            &reservation,
            &f.s.commit,
            &material,
            &mut f.objects,
            &f.s.memory,
            &mut work(),
            || false
        ),
        Err(Error::Invalid("Frozen parent role differs"))
    ));
    let mut material = f.material.clone();
    material.objects[0].bytes[0] ^= 1;
    assert!(matches!(
        verify_incremental_frozen_material_v1(
            &reservation,
            &f.s.commit,
            &material,
            &mut f.objects,
            &f.s.memory,
            &mut work(),
            || false
        ),
        Err(Error::Invalid("material original hash"))
    ));
}
#[test]
fn frozen_content_budget_cancel_and_original_source_preserve_exact_failure() {
    let mut f = build(1);
    let reservation = verify_formal_reservation(&f.signed, &f.s.authority, &f.s.commit).unwrap();
    let before = f.s.memory.used();
    let mut exhausted = work();
    exhausted.metadata_entries_remaining = 0;
    assert!(matches!(
        verify_incremental_frozen_material_v1(
            &reservation,
            &f.s.commit,
            &f.material,
            &mut f.objects,
            &f.s.memory,
            &mut exhausted,
            || false
        ),
        Err(Error::ResourceLimited("metadata entries"))
    ));
    assert_eq!(exhausted.metadata_entries_remaining, 0);
    assert!(matches!(
        verify_incremental_frozen_material_v1(
            &reservation,
            &f.s.commit,
            &f.material,
            &mut f.objects,
            &f.s.memory,
            &mut work(),
            || true
        ),
        Err(Error::Cancelled)
    ));
    f.objects.fault = Some(f.input);
    assert!(matches!(
        verify_incremental_frozen_material_v1(
            &reservation,
            &f.s.commit,
            &f.material,
            &mut f.objects,
            &f.s.memory,
            &mut work(),
            || false
        ),
        Err(Error::Source(SourceError::Sentinel))
    ));
    assert_eq!(f.s.memory.used(), before);
}
