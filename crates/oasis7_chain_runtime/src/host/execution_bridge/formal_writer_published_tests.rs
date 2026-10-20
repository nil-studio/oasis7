use super::*;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> (PathBuf, Published) {
    let (_, _, _, proof) = crate::formal_read_fixtures::actual_world_initial_fixture();
    let record = proof.decision.record.clone();
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-parsed-proof-memo-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("published")).unwrap();
    for path in [&root, &root.join("published")] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let marker = root.to_string_lossy().as_bytes().to_vec();
    let publication = Published {
        schema_version: 1,
        candidate: FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record: record.clone(),
                writer_signature_hex: proof.decision.writer_signature_hex.clone(),
            },
            material: ClosedRecord {
                payload_digest: record.payload_digest.clone(),
                before_state_root: record.before_state_root.clone(),
                after_state_root: record.after_state_root.clone(),
                roots: record.artifact_roots.clone(),
                objects: vec![
                    oasis7_distfs::controlled_authority::replicated_protocol::ArtifactObject {
                        content_hash: blake3_hex(&marker),
                        bytes: marker,
                        references: Vec::new(),
                    },
                ],
            },
            frozen: proof.decision.frozen.clone(),
            frozen_material: None,
            branch: proof.decision.branch,
        },
        proof,
    };
    (root, publication)
}

fn save(root: &Path, publication: &Published) -> PathBuf {
    let path = root.join("published").join(format!(
        "{:020}.cbor",
        publication.proof.decision.record.position
    ));
    std::fs::write(&path, serde_cbor::to_vec(publication).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

#[test]
fn parsed_proof_memo_reuses_decode_but_rechecks_current_bytes_position_and_path() {
    let (root, mut publication) = fixture();
    let position = publication.proof.decision.record.position;
    let path = save(&root, &publication);
    let before = decode_count();
    for _ in 0..2 {
        assert_eq!(
            read_published_proof_at(&root, position).unwrap(),
            Some(publication.proof.clone())
        );
    }
    assert_eq!(decode_count(), before + 1);

    let wrong = root
        .join("published")
        .join(format!("{:020}.cbor", position + 1));
    std::fs::copy(&path, &wrong).unwrap();
    assert!(read_published_proof_at(&root, position + 1).is_err());
    std::fs::remove_file(wrong).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_published_proof_at(&root, position).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    std::fs::write(&path, [0xff]).unwrap();
    assert!(read_published_proof_at(&root, position).is_err());
    assert_eq!(decode_count(), before + 2);
    publication.schema_version = 2;
    save(&root, &publication);
    assert!(read_published_proof_at(&root, position).is_err());
    publication.schema_version = 1;
    publication.candidate.candidate.record.request_digest = "0".repeat(64);
    save(&root, &publication);
    assert!(read_published_proof_at(&root, position).is_err());
    publication.candidate.candidate.record = publication.proof.decision.record.clone();
    publication.candidate.material.objects[0].bytes.push(7);
    save(&root, &publication);
    let before_seed = decode_count();
    read_published_candidate_at(&root, position).unwrap();
    assert_eq!(decode_count(), before_seed + 1);
    assert_eq!(
        read_published_proof_at(&root, position).unwrap(),
        Some(publication.proof)
    );
    assert_eq!(decode_count(), before_seed + 1);
    std::fs::remove_file(&path).unwrap();
    assert!(read_published_proof_at(&root, position).unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn parsed_proof_memo_never_supplies_signature_qualification() {
    let (root, mut publication) = fixture();
    let (_, _, anchor, _) = crate::formal_read_fixtures::actual_world_initial_fixture();
    let position = publication.proof.decision.record.position;
    assert!(verify_formal_qualified(&publication.proof, anchor.authority(), None, None).is_ok());
    publication.proof.decision.primary.signature_hex = "00".repeat(64);
    save(&root, &publication);
    let before = decode_count();
    for _ in 0..2 {
        let untrusted = read_published_proof_at(&root, position).unwrap().unwrap();
        assert!(verify_formal_qualified(&untrusted, anchor.authority(), None, None).is_err());
    }
    assert_eq!(decode_count(), before + 1);
    std::fs::remove_dir_all(root).unwrap();
}
