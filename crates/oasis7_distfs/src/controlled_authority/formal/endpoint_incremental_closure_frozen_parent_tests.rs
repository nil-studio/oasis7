//! Fresh temporary static/native namespaces and real signed content only.
//! No accepted-cache setter, durable GEN, endpoint receipt or startup permit.
use super::*;
use oasis7_formal_protocol::controlled_authority::{
    formal::{
        FormalFrozenBodyV1, IncrementalFrozenMaterialWorkV1, SignedFormalFrozenV1,
        VerifiedFormalReservationV1, frozen_material_digest_bounded_v1, verify_formal_reservation,
        verify_incremental_frozen_material_v1,
    },
    replicated_protocol::{ArtifactObject, ArtifactRole},
};
use std::collections::BTreeSet;
fn frozen_work() -> IncrementalFrozenMaterialWorkV1 {
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
fn closed(
    m: &Material,
    parent: &VerifiedFormalCommitV1,
    input: [u8; 32],
    manifest_id: [u8; 32],
    manifest: &IwsExecutionManifestV1,
) -> ClosedRecord {
    let ExecutionManifestModeV1::Execution { continuation, .. } = manifest.mode else {
        panic!("real execution manifest")
    };
    let mut roots = parent.record().artifact_roots.clone();
    roots.insert(ArtifactRole::Input, hex::encode(input));
    roots.insert(ArtifactRole::ExecutionManifest, hex::encode(manifest_id));
    let mut needed = roots
        .values()
        .map(|text| hex::decode(text).unwrap().try_into().unwrap())
        .collect::<BTreeSet<[u8; 32]>>();
    needed.insert(manifest.policy.id);
    needed.insert(continuation.id);
    let value = ClosedRecord {
        payload_digest: hex::encode(input),
        before_state_root: parent.record().after_state_root.clone(),
        after_state_root: parent.record().after_state_root.clone(),
        roots,
        objects: needed
            .into_iter()
            .map(|id| {
                let mut references = if id == manifest_id {
                    vec![
                        hex::encode(manifest.policy.id),
                        hex::encode(continuation.id),
                    ]
                } else {
                    vec![]
                };
                references.sort();
                references.dedup();
                ArtifactObject {
                    content_hash: hex::encode(id),
                    bytes: m.objects.values[&id].to_vec(),
                    references,
                }
            })
            .collect(),
    };
    value.validate_iws().unwrap();
    value
}
/// Replace normal()'s historical empty-object Frozen template before any
/// positive material/lease use. Reverify all signatures and actual typed Core.
fn complete_normal(m: &mut Material) -> VerifiedFormalCommitV1 {
    let _template = normal(m, false);
    let mut frozen = m.candidate.frozen.clone().unwrap();
    let input = hex::decode(&frozen.reservation.body.payload_digest)
        .unwrap()
        .try_into()
        .unwrap();
    let manifest_id = m.manifest.digest().unwrap();
    let material = closed(m, &m.s.commit, input, manifest_id, &m.manifest);
    frozen.reservation.body.frozen_material_digest = hex::encode(
        frozen_material_digest_bounded_v1(&material, &m.s.memory, &mut (64 << 20), || false)
            .unwrap(),
    );
    frozen.reservation.signature_hex = fixture::sign(
        formal::frozen_signing_bytes(&frozen.reservation.body).unwrap(),
        4,
    );
    verify_formal_reservation(&frozen.reservation, &m.s.authority, &m.s.commit).unwrap();
    let reservation = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = reservation.clone();
        receipt.signature_hex = fixture::sign(
            formal::frozen_receipt_signing_bytes(&receipt.body).unwrap(),
            key,
        );
    }
    let mut record = m.candidate.candidate.record.clone();
    record.frozen_reservation_id = Some(reservation);
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
    let state = m.pages.state_session(io(), MemoryClass::Ordinary).unwrap();
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &context,
            original_authority: &m.s.authority,
            parent: Some(&m.s.commit),
            max_pending_node_bytes: 4 << 20,
        },
        &mut m.objects,
        &state,
        &m.s.memory,
        &mut work(),
        || false,
    )
    .unwrap();
    m.candidate.material.payload_digest = signed.record.payload_digest.clone();
    m.candidate.material.before_state_root = signed.record.before_state_root.clone();
    m.candidate.material.after_state_root = signed.record.after_state_root.clone();
    m.candidate.material.roots = signed.record.artifact_roots.clone();
    m.candidate.candidate = signed;
    m.candidate.frozen = Some(frozen);
    m.candidate.frozen_material = Some(material);
    m.context = context;
    m.core = core;
    formal::verify_formal_qualified(&proof, &m.s.authority, Some(&m.s.commit), None).unwrap()
}
fn next_frozen(
    m: &mut Material,
    parent: &VerifiedFormalCommitV1,
) -> (SignedFormalFrozenV1, ClosedRecord) {
    let input = m.objects.put(b"next-input".to_vec());
    let continuation_bytes = b"next-continuation".to_vec();
    let continuation = OriginalArtifactRefV1 {
        id: m.objects.put(continuation_bytes.clone()),
        byte_len: continuation_bytes.len() as u64,
    };
    let parent_body =
        hex::decode(&parent.record().artifact_roots[&ArtifactRole::IncrementalCommitBody])
            .unwrap()
            .try_into()
            .unwrap();
    let parent_roots = hex::decode(&parent.record().after_state_root)
        .unwrap()
        .try_into()
        .unwrap();
    let operation = [161; 32];
    let manifest = IwsExecutionManifestV1 {
        mode: ExecutionManifestModeV1::Execution {
            parent_commit: parent_body,
            parent_roots,
            operation,
            input,
            continuation,
        },
        ..m.manifest.clone()
    };
    let manifest_id = m.objects.put(manifest.canonical_bytes().unwrap());
    let material = closed(m, parent, input, manifest_id, &manifest);
    let digest =
        frozen_material_digest_bounded_v1(&material, &m.s.memory, &mut (64 << 20), || false)
            .unwrap();
    let body = FormalFrozenBodyV1 {
        schema_version: formal::incremental::IWS_FROZEN_SCHEMA,
        world: parent.record().world.clone(),
        authority_id: m.s.authority.authority_id().unwrap(),
        epoch: m.s.authority.definition().epoch,
        activation_ref: m.s.authority.activation_ref().to_owned(),
        parent: parent.head(),
        next_position: parent.record().position + 1,
        request_digest: hex::encode(operation),
        payload_digest: hex::encode(input),
        runtime_manifest_digest: hex::encode(manifest_id),
        governing_manifest_digest: parent.record().governing_manifest_digest.clone(),
        execution_policy_digest: parent.record().execution_policy_digest.clone(),
        pre_state_root: parent.record().after_state_root.clone(),
        frozen_material_digest: hex::encode(digest),
        handoff: None,
    };
    let signature_hex = fixture::sign(formal::frozen_signing_bytes(&body).unwrap(), 4);
    (
        SignedFormalFrozenV1 {
            body,
            signature_hex,
        },
        material,
    )
}
fn verified_reservation(
    m: &Material,
    input: &SignedFormalFrozenV1,
    parent: &VerifiedFormalCommitV1,
) -> VerifiedFormalReservationV1 {
    verify_formal_reservation(input, &m.s.authority, parent).unwrap()
}
fn native_directory(m: &Material) -> std::path::PathBuf {
    m.directory.0.join("formal-objects-v1")
}
fn relative(path: &std::path::Path) -> std::path::PathBuf {
    assert!(path.is_absolute());
    let cwd = std::env::current_dir().unwrap();
    let mut result = std::path::PathBuf::new();
    for _ in cwd.components().skip(1) {
        result.push("..");
    }
    for part in path.components().skip(1) {
        result.push(part.as_os_str());
    }
    assert!(!result.is_absolute());
    result
}
#[test]
fn frozen_parent_namespace_lease_accepts_gen_and_inherited_ordinary_static_proofs() {
    let mut m = genesis();
    let baseline = check(&mut m, None).unwrap();
    let genesis_parent = m.s.commit.clone();
    let (input, material) = next_frozen(&mut m, &genesis_parent);
    let reservation = verified_reservation(&m, &input, &genesis_parent);
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        &genesis_parent,
        &material,
        &mut m.objects,
        &m.s.memory,
        &mut frozen_work(),
        || false,
    )
    .unwrap();
    let lease = CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
        &baseline,
        &checked,
        &native_directory(&m),
        &m.pages,
        &m.s.memory,
        &mut || false,
    )
    .unwrap();
    assert_eq!(
        lease.parent_record_id(),
        genesis_parent.verified_record_id()
    );
    assert_eq!(lease.parent_body_id(), baseline.body_id());
    assert_eq!(lease.parent_roots_id(), baseline.roots_id());
    assert_eq!(lease.reservation_id(), checked.reservation_id());
    drop(lease);
    drop(checked);
    let parent = complete_normal(&mut m);
    let current = check(&mut m, Some((&genesis_parent, &baseline))).unwrap();
    assert!(Arc::ptr_eq(&baseline.baseline, &current.baseline));
    assert_ne!(current.body_id(), baseline.body_id());
    let (input, material) = next_frozen(&mut m, &parent);
    let reservation = verified_reservation(&m, &input, &parent);
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        &parent,
        &material,
        &mut m.objects,
        &m.s.memory,
        &mut frozen_work(),
        || false,
    )
    .unwrap();
    let lease = CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
        &current,
        &checked,
        &native_directory(&m),
        &m.pages,
        &m.s.memory,
        &mut || false,
    )
    .unwrap();
    assert_eq!(lease.parent_record_id(), parent.verified_record_id());
}
#[test]
fn frozen_parent_relative_native_namespace_preserves_original_path_compatibility() {
    let mut m = genesis();
    let proof = check(&mut m, None).unwrap();
    let parent = m.s.commit.clone();
    let (input, material) = next_frozen(&mut m, &parent);
    let reservation = verified_reservation(&m, &input, &parent);
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        &parent,
        &material,
        &mut m.objects,
        &m.s.memory,
        &mut frozen_work(),
        || false,
    )
    .unwrap();
    let directory = relative(&native_directory(&m));
    CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
        &proof,
        &checked,
        &directory,
        &m.pages,
        &m.s.memory,
        &mut || false,
    )
    .unwrap();
}
#[test]
fn frozen_parent_replaced_native_guard_or_page_lease_is_rejected() {
    for replace_native in [false, true] {
        let mut m = genesis();
        let proof = check(&mut m, None).unwrap();
        let parent = m.s.commit.clone();
        let (input, material) = next_frozen(&mut m, &parent);
        let reservation = verified_reservation(&m, &input, &parent);
        let checked = verify_incremental_frozen_material_v1(
            &reservation,
            &parent,
            &material,
            &mut m.objects,
            &m.s.memory,
            &mut frozen_work(),
            || false,
        )
        .unwrap();
        let directory = native_directory(&m);
        let path = if replace_native {
            directory.join(".physical-guard-v1")
        } else {
            m.pages.root().join("lease")
        };
        std::fs::rename(&path, path.with_file_name("retained-original-lease")).unwrap();
        std::fs::write(&path, []).unwrap();
        assert!(
            CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
                &proof,
                &checked,
                &directory,
                &m.pages,
                &m.s.memory,
                &mut || false
            )
            .is_err()
        );
    }
}
#[test]
fn frozen_parent_foreign_pool_and_cancel_cannot_return_a_lease() {
    let mut m = genesis();
    let proof = check(&mut m, None).unwrap();
    let parent = m.s.commit.clone();
    let (input, material) = next_frozen(&mut m, &parent);
    let reservation = verified_reservation(&m, &input, &parent);
    let checked = verify_incremental_frozen_material_v1(
        &reservation,
        &parent,
        &material,
        &mut m.objects,
        &m.s.memory,
        &mut frozen_work(),
        || false,
    )
    .unwrap();
    let directory = native_directory(&m);
    let other = MemoryBudget::new(1 << 20, 1 << 19).unwrap();
    assert!(
        CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
            &proof,
            &checked,
            &directory,
            &m.pages,
            &other,
            &mut || false
        )
        .is_err()
    );
    assert_eq!(other.used(), 0);
    let before = m.s.memory.used();
    assert!(
        CandidateExecutionMaterialV1::check_frozen_parent_lease_v1(
            &proof,
            &checked,
            &directory,
            &m.pages,
            &m.s.memory,
            &mut || true
        )
        .is_err()
    );
    assert_eq!(m.s.memory.used(), before);
}
