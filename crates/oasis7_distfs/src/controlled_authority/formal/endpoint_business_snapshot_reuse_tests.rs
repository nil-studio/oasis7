use super::*;
use crate::controlled_authority::formal::business_material::{
    VerifiedCandidateMaterialClosureV1, verify_formal_business_material_with_closure,
};
use oasis7_formal_protocol::snapshot_delta::{SnapshotArtifactV1, SnapshotDeltaV1};

fn snapshot(cursor: u64) -> Vec<u8> {
    serde_cbor::to_vec(&serde_json::json!({"cursor": cursor, "state": {"time": 0}})).unwrap()
}
fn replace_with_delta(material: &mut ClosedRecord, base: &[u8], target: &[u8]) {
    let old = material.roots[&ArtifactRole::Snapshot].clone();
    material.objects.retain(|object| object.content_hash != old);
    let base_ref = blake3::hash(base).to_hex().to_string();
    let encoded = SnapshotArtifactV1::new(
        base_ref.clone(),
        1,
        SnapshotDeltaV1::between_bytes(base, target).unwrap(),
    )
    .unwrap()
    .encode()
    .unwrap();
    let target_ref = blake3::hash(&encoded).to_hex().to_string();
    material.objects.extend([
        ArtifactObject {
            content_hash: base_ref.clone(),
            bytes: base.to_vec(),
            references: vec![],
        },
        ArtifactObject {
            content_hash: target_ref.clone(),
            bytes: encoded,
            references: vec![base_ref],
        },
    ]);
    material
        .objects
        .sort_by(|left, right| left.content_hash.cmp(&right.content_hash));
    material.roots.insert(ArtifactRole::Snapshot, target_ref);
    material.after_state_root = blake3::hash(target).to_hex().to_string();
}

#[test]
fn prepare_original_delta_snapshot_reuse_matches_full_verification_and_rejects_wrong_sources() {
    let before_bytes = snapshot(1);
    let after_bytes = snapshot(2);
    let (fixture, initial_envelope) = fixture_with_snapshot(before_bytes.clone());
    let mut coordinator = fixture.coordinator();
    let initial_proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let parent = verify_formal_qualified(&initial_proof, &fixture.authority, None, None).unwrap();
    let (mut input, mut frozen_material) = fixture.frozen(&parent, 96);
    replace_with_delta(&mut frozen_material, &snapshot(0), &before_bytes);
    frozen_material.before_state_root = frozen_material.after_state_root.clone();
    input.body.schema_version = 2;
    input.body.pre_state_root = frozen_material.after_state_root.clone();
    input.body.frozen_material_digest = frozen_material_digest(&frozen_material).unwrap();
    input.signature_hex = signed(frozen_signing_bytes(&input.body).unwrap(), 4);
    let executable = coordinator.freeze(input, frozen_material).unwrap();
    let mut candidate = fixture.candidate(&executable);
    replace_with_delta(&mut candidate.material, &before_bytes, &after_bytes);
    candidate.material.before_state_root = blake3::hash(&before_bytes).to_hex().to_string();
    let primary = coordinator.endpoints_mut().0;
    let mut updates = vec![
        BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: b"changed metadata".to_vec(),
        },
        FormalHistoricalCutV1::from_proof(&initial_proof)
            .unwrap()
            .leaf()
            .unwrap(),
    ];
    updates.sort_by(|left, right| left.key_digest.cmp(&right.key_digest));
    let envelope = envelope(
        BusinessIndexDeltaV1::build(initial_envelope.delta.new, updates, &primary.store).unwrap(),
    );
    replace_result(&mut candidate.material, envelope.encode().unwrap());
    candidate.candidate.record.schema_version = 3;
    candidate.candidate.record.before_state_root = candidate.material.before_state_root.clone();
    candidate.candidate.record.after_state_root = candidate.material.after_state_root.clone();
    candidate.candidate.record.artifact_roots = candidate.material.roots.clone();
    candidate.candidate.record.result_root = envelope.delta.new_root.clone();
    candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
        4,
    );
    let parent = VerifiedFormalBusinessParentV1::from_endpoint(primary).unwrap();
    let token = VerifiedCandidateMaterialClosureV1::verify_retaining_snapshots(&candidate).unwrap();
    let retained = verify_formal_business_material_with_closure(
        &fixture.authority,
        &candidate,
        parent.as_ref(),
        &primary.store,
        &token,
    )
    .unwrap()
    .unwrap();
    let original = verify_formal_business_material(
        &fixture.authority,
        &candidate.candidate.record,
        &candidate.material,
        parent.as_ref(),
        candidate.frozen_material.as_ref(),
        &primary.store,
    )
    .unwrap()
    .unwrap();
    assert_eq!(retained.envelope(), original.envelope());
    let clone = candidate.clone();
    assert!(
        verify_formal_business_material_with_closure(
            &fixture.authority,
            &clone,
            parent.as_ref(),
            &primary.store,
            &token,
        )
        .is_err()
    );
    for before in [false, true] {
        let mut corrupt = candidate.clone();
        let material = if before {
            corrupt.frozen_material.as_mut().unwrap()
        } else {
            &mut corrupt.material
        };
        let root = material.roots[&ArtifactRole::Snapshot].clone();
        material
            .objects
            .iter_mut()
            .find(|object| object.content_hash == root)
            .unwrap()
            .bytes
            .push(0);
        assert!(VerifiedCandidateMaterialClosureV1::verify_retaining_snapshots(&corrupt).is_err());
        assert!(
            verify_formal_business_material_with_closure(
                &fixture.authority,
                &corrupt,
                parent.as_ref(),
                &primary.store,
                &token,
            )
            .is_err()
        );
        let mut missing = candidate.clone();
        let material = if before {
            missing.frozen_material.as_mut().unwrap()
        } else {
            &mut missing.material
        };
        let root = material.roots[&ArtifactRole::Snapshot].clone();
        let dependency = material
            .objects
            .iter()
            .find(|object| object.content_hash == root)
            .unwrap()
            .references[0]
            .clone();
        material
            .objects
            .retain(|object| object.content_hash != dependency);
        assert!(VerifiedCandidateMaterialClosureV1::verify_retaining_snapshots(&missing).is_err());
    }
    // The same candidate still passes every normal Prepare/certificate check;
    // the reusable token itself never grants writer or qualification authority.
    primary.prepare(candidate.clone()).unwrap();
}
