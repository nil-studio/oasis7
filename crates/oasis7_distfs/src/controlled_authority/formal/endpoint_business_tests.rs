use super::*;
use crate::controlled_authority::formal::business_index::*;
struct Empty;
impl BusinessIndexNodeReader for Empty {
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
pub(super) fn envelope(delta: BusinessIndexDeltaV1) -> FormalBusinessResultEnvelopeV1 {
    FormalBusinessResultEnvelopeV1 {
        schema_version: 1,
        domain: FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1.into(),
        execution_result_cbor: serde_cbor::to_vec(&"opaque fixture result").unwrap(),
        tree_descriptor_cbor: serde_cbor::to_vec(&delta.new).unwrap(),
        delta,
        retained_history_delta: None,
    }
}
pub(super) fn replace_result(material: &mut ClosedRecord, bytes: Vec<u8>) {
    let old = material.roots[&ArtifactRole::Result].clone();
    let digest = blake3::hash(&bytes).to_hex().to_string();
    material.objects.retain(|object| object.content_hash != old);
    material.roots.insert(ArtifactRole::Result, digest.clone());
    material.objects.push(ArtifactObject {
        content_hash: digest,
        bytes,
        references: vec![],
    });
    material
        .objects
        .sort_by(|a, b| a.content_hash.cmp(&b.content_hash));
}
pub(super) fn fixture() -> (Fixture, FormalBusinessResultEnvelopeV1) {
    fixture_with_snapshot(b"state0".to_vec())
}
pub(super) fn fixture_with_snapshot(
    snapshot: Vec<u8>,
) -> (Fixture, FormalBusinessResultEnvelopeV1) {
    let mut fixture = Fixture::new();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: b"initial metadata".to_vec(),
        }],
        &Empty,
    )
    .unwrap();
    let envelope = envelope(delta);
    let bytes = envelope.encode().unwrap();
    let mut body = fixture.authority.initial_body().cloned().unwrap();
    body.authority.profile_version = 2;
    body.initial_state_root = blake3::hash(&snapshot).to_hex().to_string();
    body.artifact_roots
        .insert(ArtifactRole::Snapshot, body.initial_state_root.clone());
    body.artifact_roots.insert(
        ArtifactRole::Result,
        blake3::hash(&bytes).to_hex().to_string(),
    );
    let certificate = SignedInitialActivationV1 {
        signature_hex: signed(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    fixture.authority = verify_initial_activation(
        &certificate,
        &InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: certificate.body.authority.world.clone(),
            initial_state_root: certificate.body.initial_state_root.clone(),
            governing_manifest_digest: certificate.body.governing_manifest_digest.clone(),
            recovery_policy_digest: certificate.body.recovery_policy.digest().unwrap(),
        },
    )
    .unwrap();
    let mut closed = material(
        initial_activation_input_bytes(&certificate).unwrap(),
        snapshot,
        b"manifest".to_vec(),
    );
    replace_result(&mut closed, bytes);
    let mut record = fixture.initial.candidate.record.clone();
    record.original_authority_id = fixture.authority.authority_id().unwrap();
    record.activation_ref = fixture.authority.activation_ref().into();
    record.payload_digest = closed.payload_digest.clone();
    record.before_state_root = closed.before_state_root.clone();
    record.after_state_root = closed.after_state_root.clone();
    record.artifact_roots = closed.roots.clone();
    record.result_root = envelope.delta.new_root.clone();
    fixture.initial = FormalCandidateMaterialV1 {
        candidate: SignedCanonicalCandidateV1 {
            writer_signature_hex: signed(canonical_candidate_signing_bytes(&record).unwrap(), 4),
            record,
        },
        material: closed,
        frozen: None,
        frozen_material: None,
        branch: FormalCompletionBranchV1::Normal,
    };
    (fixture, envelope)
}
#[test]
fn profile2_prepares_durable_business_nodes_and_rejects_wrong_base_or_missing_envelope() {
    let (fixture, initial_envelope) = fixture();
    let mut coordinator = fixture.coordinator();
    let proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let parent = verify_formal_qualified(&proof, &fixture.authority, None, None).unwrap();
    for dir in [&fixture.pdir, &fixture.rdir] {
        for write in &initial_envelope.delta.node_writes {
            assert!(
                FileFormalArchiveSourceV1::new(dir.clone())
                    .unwrap()
                    .read_object(&write.digest)
                    .unwrap()
                    .unwrap()
                    == write.bytes,
                "logical CAS object differs"
            );
        }
    }
    let (input, material) = fixture.frozen(&parent, 91);
    let executable = coordinator.freeze(input, material).unwrap();
    let plain = fixture.candidate(&executable);
    assert!(
        coordinator
            .endpoints_mut()
            .0
            .prepare(plain.clone())
            .is_err()
    );
    let (primary, _) = coordinator.endpoints_mut();
    let mut updates = vec![
        BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: b"next metadata".to_vec(),
        },
        BusinessIndexLeafV1 {
            key_digest: h(92),
            kind: BusinessIndexLeafKindV1::Outcome,
            canonical_value: b"permanent original result".to_vec(),
        },
    ];
    updates.push(
        FormalHistoricalCutV1::from_proof(&proof)
            .unwrap()
            .leaf()
            .unwrap(),
    );
    updates.sort_by(|a, b| a.key_digest.cmp(&b.key_digest));
    let delta =
        BusinessIndexDeltaV1::build(initial_envelope.delta.new.clone(), updates, &primary.store)
            .unwrap();
    let env = envelope(delta);
    let mut candidate = plain;
    replace_result(&mut candidate.material, env.encode().unwrap());
    candidate.candidate.record.artifact_roots = candidate.material.roots.clone();
    candidate.candidate.record.result_root = env.delta.new_root.clone();
    candidate.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&candidate.candidate.record).unwrap(),
        4,
    );
    let mut wrong = env.clone();
    wrong.delta.base_root = h(93);
    let mut changed = candidate.clone();
    replace_result(&mut changed.material, wrong.encode().unwrap());
    changed.candidate.record.artifact_roots = changed.material.roots.clone();
    changed.candidate.writer_signature_hex = signed(
        canonical_candidate_signing_bytes(&changed.candidate.record).unwrap(),
        4,
    );
    assert!(primary.prepare(changed).is_err());
    let result = coordinator.submit(candidate).unwrap();
    verify_formal_qualified(&result, &fixture.authority, Some(&parent), None).unwrap();
    drop(coordinator);
    let endpoint = fixture.endpoint(EndpointRole::Primary);
    assert_eq!(
        endpoint.parent.as_ref().unwrap().head().record_id,
        result.decision.record.record_id().unwrap()
    );
    drop(endpoint);
    let missing = fixture
        .pdir
        .join("formal-objects-v1")
        .join(&env.delta.node_writes[0].digest);
    if missing.exists() {
        fs::remove_file(missing).unwrap();
    } else {
        // A missing locator is recoverable from a durable pack recipe. Damage
        // the real selected descriptor to model loss of original node material.
        fs::write(
            fixture
                .pdir
                .join("formal-objects-v1")
                .join(".representation-v1")
                .join(&env.delta.node_writes[0].digest),
            b"lost original node representation",
        )
        .unwrap();
    }
    assert!(
        FileFormalEndpoint::open_inner(
            &fixture.pdir,
            FormalEndpointAuthorizationV1 {
                authority: fixture.authority.clone(),
                original: fixture.authority.clone(),
                recovery: None,
                parent: None
            },
            EndpointRole::Primary,
            key(5),
            fixture.provider.clone(),
            true,
            None
        )
        .is_err()
    );
}

#[test]
fn prepare_material_closure_token_rejects_corruption_and_other_borrows() {
    let (fixture, _) = fixture();
    let candidate = &fixture.initial;
    let token =
        crate::controlled_authority::formal::business_material::VerifiedCandidateMaterialClosureV1::verify(candidate)
            .unwrap();

    assert!(token.matches_candidate(candidate));

    let cloned = candidate.clone();
    assert!(!token.matches_candidate(&cloned));
    assert!(
        crate::controlled_authority::formal::business_material::verify_formal_business_material_with_closure(
            &fixture.authority,
            &cloned,
            None,
            &Empty,
            &token,
        )
        .is_err()
    );

    let mut corrupt = candidate.clone();
    corrupt.material.objects[0].bytes[0] ^= 1;
    assert!(
        crate::controlled_authority::formal::business_material::VerifiedCandidateMaterialClosureV1::verify(&corrupt,)
            .is_err()
    );
    assert!(
        crate::controlled_authority::formal::business_material::verify_formal_business_material_with_closure(
            &fixture.authority,
            &corrupt,
            None,
            &Empty,
            &token,
        )
        .is_err()
    );
}

#[test]
fn prepare_material_closure_token_binds_exact_frozen_body_and_material() {
    let (fixture, _) = fixture();
    let mut coordinator = fixture.coordinator();
    let initial_proof = coordinator.submit(fixture.initial.clone()).unwrap();
    let parent = verify_formal_qualified(&initial_proof, &fixture.authority, None, None).unwrap();
    let (input, frozen_material) = fixture.frozen(&parent, 94);
    let executable = coordinator.freeze(input, frozen_material).unwrap();
    let candidate = fixture.candidate(&executable);
    let token =
        crate::controlled_authority::formal::business_material::VerifiedCandidateMaterialClosureV1::verify(&candidate)
            .unwrap();

    assert!(token.matches_candidate(&candidate));

    let mut changed_body = candidate.clone();
    changed_body
        .frozen
        .as_mut()
        .unwrap()
        .reservation
        .body
        .request_digest = h(95);
    assert!(!token.matches_candidate(&changed_body));
    let mut changed_material = candidate.clone();
    changed_material.frozen_material.as_mut().unwrap().objects[0].bytes[0] ^= 1;
    assert!(!token.matches_candidate(&changed_material));

    let mut corrupt_frozen = candidate.clone();
    corrupt_frozen.frozen_material.as_mut().unwrap().objects[0].bytes[0] ^= 1;
    assert!(
        crate::controlled_authority::formal::business_material::VerifiedCandidateMaterialClosureV1::verify(
            &corrupt_frozen,
        )
        .is_err()
    );
}
