use super::*;
use oasis7_distfs::controlled_authority::formal::endpoint::FormalCandidateMaterialV1;
use oasis7_distfs::controlled_authority::{
    formal::*,
    replicated_protocol::{ArtifactObject, ArtifactRole, ClosedRecord, EndpointRole},
};

use crate::formal_read_fixtures as fixtures;

fn normal_execution_commit(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
) -> VerifiedFormalCommitV1 {
    let request_digest = fixtures::h(60);
    let payload_digest = fixtures::h(61);
    let after_state_root = fixtures::h(62);
    let frozen_body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: parent.record().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().to_owned(),
        parent: parent.head(),
        next_position: parent.record().position + 1,
        request_digest: request_digest.clone(),
        payload_digest: payload_digest.clone(),
        runtime_manifest_digest: parent.record().runtime_manifest_digest.clone(),
        governing_manifest_digest: parent.record().governing_manifest_digest.clone(),
        execution_policy_digest: parent.record().execution_policy_digest.clone(),
        pre_state_root: parent.record().after_state_root.clone(),
        frozen_material_digest: fixtures::h(63),
        handoff: None,
    };
    let reservation_id = frozen_body.reservation_id().unwrap();
    let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key_id| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: reservation_id.clone(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: fixtures::sign(frozen_receipt_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: fixtures::sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
            body: frozen_body,
        },
        primary: frozen_receipt(EndpointRole::Primary, &authority.definition().primary, 5),
        replica: frozen_receipt(EndpointRole::Replica, &authority.definition().replica, 6),
    };
    let mut record = parent.record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position += 1;
    record.parent_record_id = parent.head().record_id;
    record.original_authority_id = authority.authority_id().unwrap();
    record.original_epoch = authority.definition().epoch;
    record.activation_ref = authority.activation_ref().to_owned();
    record.request_digest = request_digest;
    record.payload_digest = payload_digest.clone();
    record.frozen_reservation_id = Some(reservation_id);
    record.before_state_root = parent.record().after_state_root.clone();
    record.after_state_root = after_state_root.clone();
    record.action_root = fixtures::h(64);
    record.event_root = fixtures::h(65);
    record.result_root = fixtures::h(66);
    record
        .artifact_roots
        .insert(ArtifactRole::Input, payload_digest);
    record
        .artifact_roots
        .insert(ArtifactRole::Snapshot, after_state_root);
    let proof =
        fixtures::qualified_proof(record, Some(frozen), authority, Some(parent), None, 4, 5, 6);
    verify_formal_qualified(&proof, authority, Some(parent), None).unwrap()
}

fn initial_candidate_with_corrupt_material(
    anchor: &VerifiedFormalReadAnchorV1,
    proof: &FormalQualifiedCommitProofV1,
) -> FormalCandidateMaterialV1 {
    let record = anchor.head().record().clone();
    let mut objects: Vec<_> = record
        .artifact_roots
        .values()
        .map(|digest| ArtifactObject {
            content_hash: digest.clone(),
            bytes: b"corrupt initial closure bytes".to_vec(),
            references: Vec::new(),
        })
        .collect();
    objects.sort_by(|left, right| left.content_hash.cmp(&right.content_hash));
    FormalCandidateMaterialV1 {
        candidate: SignedCanonicalCandidateV1 {
            record: record.clone(),
            writer_signature_hex: proof.decision.writer_signature_hex.clone(),
        },
        material: ClosedRecord {
            payload_digest: record.payload_digest.clone(),
            before_state_root: record.before_state_root.clone(),
            after_state_root: record.after_state_root.clone(),
            roots: record.artifact_roots.clone(),
            objects,
        },
        frozen: None,
        frozen_material: None,
        branch: FormalCompletionBranchV1::Normal,
    }
}

#[test]
fn formal_predecessor_initial_cache_rejects_wrong_anchor_record() {
    let (_, _, anchor, proof) = fixtures::actual_world_initial_fixture();
    let mut candidate = initial_candidate_with_corrupt_material(&anchor, &proof);
    candidate.candidate.record.action_root = fixtures::h(231);
    let error = readback::initial_business_nodes_from_verified_anchor(&anchor, &candidate, &proof)
        .unwrap_err();
    assert!(error.contains("exact external anchor"));
}

#[test]
fn formal_predecessor_initial_cache_rejects_invalid_qualified_proof() {
    let (_, _, anchor, proof) = fixtures::actual_world_initial_fixture();
    let candidate = initial_candidate_with_corrupt_material(&anchor, &proof);
    let mut corrupt_proof = proof;
    corrupt_proof.primary_complete.signature_hex = "00".repeat(64);
    let error =
        readback::initial_business_nodes_from_verified_anchor(&anchor, &candidate, &corrupt_proof)
            .unwrap_err();
    assert!(error.contains("initial proof"));
}

#[test]
fn formal_predecessor_initial_cache_rejects_corrupt_closure_bytes() {
    let (_, _, anchor, proof) = fixtures::actual_world_initial_fixture();
    let candidate = initial_candidate_with_corrupt_material(&anchor, &proof);
    let error = readback::initial_business_nodes_from_verified_anchor(&anchor, &candidate, &proof)
        .unwrap_err();
    assert!(error.contains("initial closure"));
}

#[test]
fn formal_signed_successor_cut_requires_post_cut_execution_by_active_authority() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let authority = anchor.authority();
    let initial = anchor.head();
    assert!(readback::validate_successor_published_commit(initial, authority, 0).is_err());

    let execution = normal_execution_commit(authority, initial);
    readback::validate_successor_published_commit(&execution, authority, initial.record().position)
        .unwrap();
    assert!(
        readback::validate_successor_published_commit(
            &execution,
            authority,
            execution.record().position,
        )
        .is_err()
    );
}

#[test]
fn formal_handoff_readback_retries_only_verified_lagging_baselines_within_deadline() {
    let (_, _, anchor, _) = fixtures::actual_world_initial_fixture();
    let authority = anchor.authority();
    let execution = normal_execution_commit(authority, anchor.head());
    let reads = std::cell::Cell::new(0);
    let observed =
        readback::poll_until_successor_execution(std::time::Duration::from_secs(1), |_| {
            let attempt = reads.get() + 1;
            reads.set(attempt);
            if attempt == 1 {
                Ok(None)
            } else {
                Ok(Some(execution.clone()))
            }
        })
        .unwrap();
    assert_eq!(reads.get(), 2);
    assert_eq!(observed.record(), execution.record());
}

#[test]
fn formal_handoff_readback_does_not_retry_authority_or_reader_errors() {
    let reads = std::cell::Cell::new(0);
    let error = readback::poll_until_successor_execution(std::time::Duration::from_secs(1), |_| {
        reads.set(reads.get() + 1);
        Err("FENCING_UNPROVEN: ordinary signed reader uses the wrong active authority".into())
    })
    .unwrap_err();
    assert!(error.contains("wrong active authority"));
    assert_eq!(reads.get(), 1);
}

#[test]
fn formal_handoff_readback_timeout_keeps_a_lagging_baseline_unqualified() {
    let reads = std::cell::Cell::new(0);
    let error = readback::poll_until_successor_execution(std::time::Duration::ZERO, |_| {
        reads.set(reads.get() + 1);
        Ok(None)
    })
    .unwrap_err();
    assert!(error.contains("ORIGINAL_OUTCOME_UNKNOWN"));
    assert_eq!(reads.get(), 0);
}

fn predecessor() -> FormalRuntimeInstanceV1 {
    FormalRuntimeInstanceV1 {
        api_version: "oasis7.formal-runtime-owner/v1".into(),
        operation_id: "predecessor-op".into(),
        request_digest: "a".repeat(64),
        incarnation: "0".repeat(32),
        config_digest: "b".repeat(64),
        node_id: "node-a".into(),
        world_id: "world-a".into(),
        chain_id: "chain-a".into(),
        authority_id: "c".repeat(64),
        authority_epoch: 7,
        profile_version: 2,
        process_id: 123,
    }
}

fn request(predecessor: &FormalRuntimeInstanceV1) -> Request {
    Request {
        api_version: "oasis7.local-operation/v1".into(),
        operation_id: "handoff-op".into(),
        world_id: predecessor.world_id.clone(),
        chain_id: predecessor.chain_id.clone(),
        kind: Kind::ProtectedSignerMigration,
        config: PathBuf::from("/private/old/formal.json"),
        expected_generation: predecessor.authority_epoch,
        expected_config_digest: predecessor.config_digest.clone(),
        expected_instance: None,
        managed_target: None,
        migration: None,
        authorization: Some(OperationAuthorizationV1::ProtectedSignerTransfer {
            formal_live_config_file: PathBuf::from("/private/new/formal.json"),
            target_signer_public_key: "d".repeat(64),
            expected_predecessor_instance: Some(Box::new(predecessor.clone())),
        }),
        package_upgrade: None,
    }
}

fn plan(request: &Request, predecessor: &FormalRuntimeInstanceV1) -> ProtectedSignerHandoffPlanV1 {
    ProtectedSignerHandoffPlanV1 {
        api_version: PLAN_API.into(),
        request_digest: io::digest(request).unwrap(),
        predecessor_config_file: request.config.clone(),
        predecessor_config_digest: "b".repeat(64),
        predecessor_input_bundle_digest: "e".repeat(64),
        predecessor_recovery_digest: None,
        predecessor_writer_directory: PathBuf::from("/private/old/writer"),
        predecessor_instance: predecessor.clone(),
        successor_config_file: PathBuf::from("/private/new/formal.json"),
        successor_config_digest: "f".repeat(64),
        successor_input_bundle_digest: "1".repeat(64),
        recovery_permission_digest: "2".repeat(64),
        recovery_cut_position: 10,
        successor_writer_directory: PathBuf::from("/private/new/writer"),
        target_signer_public_key: "d".repeat(64),
    }
}

#[test]
fn persisted_plan_binds_exact_authorized_request_and_predecessor() {
    let predecessor = predecessor();
    let request = request(&predecessor);
    let plan = plan(&request, &predecessor);

    validate_persisted_plan(&request, &plan).unwrap();

    let mut changed = request.clone();
    if let Some(OperationAuthorizationV1::ProtectedSignerTransfer {
        target_signer_public_key,
        ..
    }) = changed.authorization.as_mut()
    {
        target_signer_public_key.replace_range(..2, "ff");
    }
    assert!(validate_persisted_plan(&changed, &plan).is_err());

    let mut changed = request.clone();
    if let Some(OperationAuthorizationV1::ProtectedSignerTransfer {
        expected_predecessor_instance,
        ..
    }) = changed.authorization.as_mut()
        && let Some(instance) = expected_predecessor_instance
    {
        instance.incarnation.push_str("-replacement");
    }
    assert!(validate_persisted_plan(&changed, &plan).is_err());
}

#[test]
fn persisted_plan_rejects_nested_writer_directories() {
    let predecessor = predecessor();
    let request = request(&predecessor);
    let mut plan = plan(&request, &predecessor);
    plan.successor_writer_directory = plan.predecessor_writer_directory.join("successor");

    assert!(validate_persisted_plan(&request, &plan).is_err());
}
