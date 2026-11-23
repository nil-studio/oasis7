//! No business execution occurs in the successor's unique activation record.
use super::*;

pub(in super::super) fn control_candidate(
    authority: &VerifiedAuthorityV1,
    certificate: &SignedRecoveryCertificateV1,
    parent: &FormalQualifiedCommitProofV1,
    original: &FormalCandidateMaterialV1,
    base: &FormalBusinessResultEnvelopeV1,
    source_paths: &[&Path],
    signer: &impl FormalWriterSignerV1,
) -> (FormalCandidateMaterialV1, FormalBusinessResultEnvelopeV1) {
    struct Reader(Vec<FileFormalArchiveSourceV1>);
    impl BusinessIndexNodeReader for Reader {
        fn read_node(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
            for source in &self.0 {
                if let Some(bytes) = source.read_object(id).map_err(|e| e.to_string())? {
                    return Ok(Some(bytes));
                }
            }
            Ok(None)
        }
    }
    let reader = Reader(
        source_paths
            .iter()
            .map(|path| FileFormalArchiveSourceV1::new(path.to_path_buf()).unwrap())
            .collect(),
    );
    assert_eq!(parent.decision.record, original.candidate.record);
    let authorization = authority.recovery_activation().unwrap();
    let mut material = original.material.clone();
    let mut objects: std::collections::BTreeMap<_, _> = material
        .objects
        .iter()
        .map(|object| (object.content_hash.clone(), object.clone()))
        .collect();
    let mut insert = |bytes: Vec<u8>, mut references: Vec<String>| {
        references.sort();
        references.dedup();
        let content_hash = blake3::hash(&bytes).to_hex().to_string();
        objects.insert(
            content_hash.clone(),
            ArtifactObject {
                content_hash: content_hash.clone(),
                bytes,
                references,
            },
        );
        content_hash
    };
    let input = insert(
        recovery_activation_input_bytes(certificate).unwrap(),
        vec![material.roots[&ArtifactRole::Input].clone()],
    );
    material.roots.insert(ArtifactRole::Input, input.clone());
    material.payload_digest = input.clone();
    material.before_state_root = parent.decision.record.after_state_root.clone();
    material.after_state_root = parent.decision.record.after_state_root.clone();
    let mut record = parent.decision.record.clone();
    record.schema_version = if material
        .roots
        .contains_key(&ArtifactRole::NativeExecutionEnvironment)
    {
        5
    } else {
        3
    };
    record.kind = FormalRecordKindV1::RecoveryActivation {
        certificate_digest: authorization.certificate_digest.clone(),
        plan_digest: authorization.plan_digest.clone(),
        pending_handoff_root: authorization.pending_handoff_root.clone(),
    };
    record.position += 1;
    record.parent_record_id = parent.decision.record.record_id().unwrap();
    record.original_authority_id = authority.authority_id().unwrap();
    record.original_epoch = authority.definition().epoch;
    record.activation_ref = authority.activation_ref().into();
    record.request_digest = recovery_activation_request_digest(authority).unwrap();
    record.payload_digest = input;
    record.frozen_reservation_id = None;
    record.before_state_root = record.after_state_root.clone();
    record.handoff = None;
    record.durable_set_transition_digest = None;
    let mut updates = vec![
        BusinessIndexLeafV1 {
            key_digest: metadata_key_digest(),
            kind: BusinessIndexLeafKindV1::CommitMetadata,
            canonical_value: serde_cbor::to_vec(&record.position).unwrap(),
        },
        FormalHistoricalCutV1::from_proof(parent)
            .unwrap()
            .leaf()
            .unwrap(),
    ];
    updates.sort_by(|a, b| a.key_digest.cmp(&b.key_digest));
    let delta = BusinessIndexDeltaV1::build(base.delta.new.clone(), updates, &reader).unwrap();
    record.result_root = delta.new_root.clone();
    let envelope = FormalBusinessResultEnvelopeV1 {
        schema_version: 1,
        domain: FORMAL_BUSINESS_ENVELOPE_DOMAIN_V1.into(),
        execution_result_cbor: base.execution_result_cbor.clone(),
        tree_descriptor_cbor: serde_cbor::to_vec(&delta.new).unwrap(),
        delta,
        retained_history_delta: None,
    };
    let result = insert(
        envelope.encode().unwrap(),
        vec![
            material.roots[&ArtifactRole::Result].clone(),
            material.roots[&ArtifactRole::Snapshot].clone(),
            material.roots[&ArtifactRole::Journal].clone(),
        ],
    );
    material.roots.insert(ArtifactRole::Result, result);
    material.objects = objects.into_values().collect();
    record.artifact_roots = material.roots.clone();
    verify_record_material(&record, &material).unwrap();
    let writer_signature_hex = signer.sign_candidate(&record, None).unwrap();
    (
        FormalCandidateMaterialV1 {
            candidate: SignedCanonicalCandidateV1 {
                record,
                writer_signature_hex,
            },
            material,
            frozen: None,
            frozen_material: None,
            branch: FormalCompletionBranchV1::Normal,
        },
        envelope,
    )
}

pub(in super::super) fn publish_one(
    coordinator: &mut FormalCoordinator<RemoteFormalEndpoint, RemoteFormalEndpoint>,
    candidate: &FormalCandidateMaterialV1,
) -> FormalQualifiedCommitProofV1 {
    let request = &candidate.candidate.record.request_digest;
    let (p, r) = coordinator.endpoints_mut();
    let pp = p.prepare(candidate.clone()).unwrap();
    let rp = r.prepare(candidate.clone()).unwrap();
    let primary = p.decide(request, pp.clone(), rp.clone()).unwrap();
    let replica = r.decide(request, pp, rp).unwrap();
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: candidate.candidate.record.clone(),
        writer_signature_hex: candidate.candidate.writer_signature_hex.clone(),
        frozen: candidate.frozen.clone(),
        primary,
        replica,
        branch: candidate.branch,
    };
    let primary_complete = p.install_proof(decision.clone()).unwrap();
    let replica_complete = r.install_proof(decision.clone()).unwrap();
    let proof = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision,
        primary_complete,
        replica_complete,
    };
    p.publish_qualified(proof.clone()).unwrap();
    proof
}
