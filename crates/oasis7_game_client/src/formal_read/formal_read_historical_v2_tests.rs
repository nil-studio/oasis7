use super::fixtures::*;
use super::*;

use crate::remote_world_service::{FormalReadTrustConfigV1, WorldServiceClientConfig};
type RemoteWorldServiceClient =
    crate::remote_world_service::RemoteWorldServiceReader<WorldServiceProjection>;
use oasis7_client_api::world_service::WorldIdentity;
use oasis7_formal_protocol::controlled_authority::formal::{
    business_history::FormalHistoricalCutV1,
    business_index::{
        BusinessIndexDeltaV1, BusinessIndexNodeReader, BusinessIndexNodeWriteV1,
        BusinessIndexRootV1,
    },
    frozen_receipt_signing_bytes,
};
use std::collections::BTreeMap;
use std::time::Duration;

pub(super) struct EmptyIndexNodes;
impl BusinessIndexNodeReader for EmptyIndexNodes {
    fn read_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}

pub(super) struct MemoryIndexNodes(BTreeMap<String, Vec<u8>>);
impl BusinessIndexNodeReader for MemoryIndexNodes {
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.get(digest).cloned())
    }
}

pub(super) fn memory_index_nodes(writes: Vec<BusinessIndexNodeWriteV1>) -> MemoryIndexNodes {
    MemoryIndexNodes(
        writes
            .into_iter()
            .map(|write| (write.digest, write.bytes))
            .collect(),
    )
}

pub(super) fn profile2_initial_fixture() -> (
    fixtures::protocol_world::ProtocolWorld,
    InitialActivationTrustV1,
    VerifiedFormalReadAnchorV1,
    FormalQualifiedCommitProofV1,
) {
    let (world, trust, original, _) = protocol_initial_fixture();
    let mut body = original.initial_certificate().body.clone();
    body.authority.profile_version = 2;
    let certificate = SignedInitialActivationV1 {
        signature_hex: sign(initial_activation_signing_bytes(&body).unwrap(), 9),
        body,
    };
    let authority = verify_initial_activation(&certificate, &trust).unwrap();
    let input = hex::encode(
        blake3::hash(&initial_activation_input_bytes(&certificate).unwrap()).as_bytes(),
    );
    let mut record = original.head().record().clone();
    record.original_authority_id = authority.authority_id().unwrap();
    record.activation_ref = authority.activation_ref().to_owned();
    record.payload_digest = input.clone();
    record.artifact_roots.insert(ArtifactRole::Input, input);
    let proof = qualified_proof(record, None, &authority, None, None, 4, 5, 6);
    let anchor = VerifiedFormalReadAnchorV1::from_initial(&certificate, &trust, &proof).unwrap();
    (world, trust, anchor, proof)
}

fn normal_execution_proof(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    position_salt: u8,
    result_root: String,
    projection_root: &str,
) -> FormalQualifiedCommitProofV1 {
    normal_execution_proof_with_signers(
        authority,
        parent,
        position_salt,
        result_root,
        projection_root,
        [4, 5, 6],
    )
}

pub(super) fn normal_execution_proof_with_signers(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    position_salt: u8,
    result_root: String,
    projection_root: &str,
    signers: [u8; 3],
) -> FormalQualifiedCommitProofV1 {
    let request_digest = h(position_salt);
    let payload_digest = h(position_salt.wrapping_add(1));
    let after_state_root = h(position_salt.wrapping_add(2));
    let runtime_manifest_digest = parent.record().runtime_manifest_digest.clone();
    let governing_manifest_digest = parent.record().governing_manifest_digest.clone();
    let execution_policy_digest = parent.record().execution_policy_digest.clone();
    let body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: parent.record().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().to_owned(),
        parent: parent.head(),
        next_position: parent.record().position + 1,
        request_digest: request_digest.clone(),
        payload_digest: payload_digest.clone(),
        runtime_manifest_digest: runtime_manifest_digest.clone(),
        governing_manifest_digest: governing_manifest_digest.clone(),
        execution_policy_digest: execution_policy_digest.clone(),
        pre_state_root: parent.record().after_state_root.clone(),
        frozen_material_digest: h(position_salt.wrapping_add(3)),
        handoff: None,
    };
    let reservation_id = body.reservation_id().unwrap();
    let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key_id| {
        let receipt_body = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: reservation_id.clone(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&receipt_body).unwrap(), key_id),
            body: receipt_body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&body).unwrap(), signers[0]),
            body,
        },
        primary: frozen_receipt(
            EndpointRole::Primary,
            &authority.definition().primary,
            signers[1],
        ),
        replica: frozen_receipt(
            EndpointRole::Replica,
            &authority.definition().replica,
            signers[2],
        ),
    };
    let mut record = parent.record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position += 1;
    record.parent_record_id = parent.head().record_id;
    record.request_digest = request_digest;
    record.payload_digest = payload_digest.clone();
    record.frozen_reservation_id = Some(reservation_id);
    record.before_state_root = parent.record().after_state_root.clone();
    record.after_state_root = after_state_root.clone();
    record.action_root = h(position_salt.wrapping_add(4));
    record.event_root = h(position_salt.wrapping_add(5));
    record.result_root = result_root;
    record.projection_root = projection_root.to_owned();
    record.original_authority_id = authority.authority_id().unwrap();
    record.original_epoch = authority.definition().epoch;
    record.activation_ref = authority.activation_ref().to_owned();
    record
        .artifact_roots
        .insert(ArtifactRole::Input, payload_digest);
    record
        .artifact_roots
        .insert(ArtifactRole::Snapshot, after_state_root);
    qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(parent),
        None,
        signers[0],
        signers[1],
        signers[2],
    )
}

#[test]
fn replay_frozen_origin_qualification_imports_exact_non_normal_token() {
    // Recovery lineage only reaches profile 2 if the external genesis pin is
    // profile 2. The historical-inclusion verifier intentionally rejects a
    // profile-1 current authority even when its root descriptor is otherwise
    // well formed.
    let (world, trust, anchor, _initial_proof) = profile2_initial_fixture();
    let origin = anchor.checkpoint();
    let origin_digest = origin.digest().unwrap();
    let original = anchor.authority();
    let frozen = FormalFrozenBodyV1 {
        schema_version: 1,
        world: original.definition().world.clone(),
        authority_id: original.authority_id().unwrap(),
        epoch: original.definition().epoch,
        activation_ref: original.activation_ref().to_owned(),
        parent: anchor.head().head(),
        next_position: anchor.head().record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: anchor.head().record().runtime_manifest_digest.clone(),
        governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
        pre_state_root: anchor.head().record().after_state_root.clone(),
        frozen_material_digest: h(62),
        handoff: None,
    };
    let frozen_certificate = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&frozen).unwrap(), 4),
            body: frozen.clone(),
        },
        primary: FormalFrozenReceiptV1 {
            signature_hex: sign(
                frozen_receipt_signing_bytes(&FormalFrozenReceiptBodyV1 {
                    authority_id: original.authority_id().unwrap(),
                    role: EndpointRole::Primary,
                    instance_digest: original.definition().primary.instance.digest().unwrap(),
                    reservation_id: frozen.reservation_id().unwrap(),
                })
                .unwrap(),
                5,
            ),
            body: FormalFrozenReceiptBodyV1 {
                authority_id: original.authority_id().unwrap(),
                role: EndpointRole::Primary,
                instance_digest: original.definition().primary.instance.digest().unwrap(),
                reservation_id: frozen.reservation_id().unwrap(),
            },
        },
        replica: FormalFrozenReceiptV1 {
            signature_hex: sign(
                frozen_receipt_signing_bytes(&FormalFrozenReceiptBodyV1 {
                    authority_id: original.authority_id().unwrap(),
                    role: EndpointRole::Replica,
                    instance_digest: original.definition().replica.instance.digest().unwrap(),
                    reservation_id: frozen.reservation_id().unwrap(),
                })
                .unwrap(),
                6,
            ),
            body: FormalFrozenReceiptBodyV1 {
                authority_id: original.authority_id().unwrap(),
                role: EndpointRole::Replica,
                instance_digest: original.definition().replica.instance.digest().unwrap(),
                reservation_id: frozen.reservation_id().unwrap(),
            },
        },
    };
    let recovery_tail = RecoveryTailWitnessV1::Frozen(frozen_certificate.clone());
    let next2 = super::successor(original, 2, 13, 14, 15, 2);
    let minimum =
        oasis7_formal_protocol::controlled_authority::formal::issuer::IssuerAnchorV1::initial(
            original.policy(),
        )
        .unwrap();
    let (step2, issuer_anchor2) = super::recovery_step(
        original,
        anchor.head(),
        next2,
        recovery_tail.clone(),
        None,
        1,
        minimum,
        [5, 6],
    );
    let recovery2 = super::super::history::verify_recovery_step(&step2, &anchor, None)
        .unwrap()
        .recovery;
    let context3 = VerifiedActiveRecoveryContextV2::from_previous(
        &recovery2,
        original,
        anchor.head(),
        &recovery_tail,
    )
    .unwrap();
    let next3 = super::successor(recovery2.successor(), 3, 23, 24, 25, 2);
    let (step3, _) = super::recovery_step(
        recovery2.successor(),
        anchor.head(),
        next3,
        recovery_tail,
        Some(&context3),
        2,
        issuer_anchor2,
        [14, 15],
    );
    let recovery3 = super::super::history::verify_recovery_step(&step3, &anchor, Some(&recovery2))
        .unwrap()
        .recovery;
    let mut record = anchor.head().record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = frozen.next_position;
    record.parent_record_id = frozen.parent.record_id.clone();
    record.request_digest = frozen.request_digest.clone();
    record.payload_digest = frozen.payload_digest.clone();
    record.frozen_reservation_id = Some(frozen.reservation_id().unwrap());
    record.before_state_root = frozen.pre_state_root.clone();
    record.after_state_root = h(63);
    record.action_root = h(64);
    record.event_root = h(65);
    record.result_root = h(66);
    record.projection_root = fixtures::protocol_world::commitments_for_world(&world)
        .unwrap()
        .root()
        .unwrap();
    record.artifact_roots.insert(
        ArtifactRole::ExecutionManifest,
        frozen.runtime_manifest_digest.clone(),
    );
    let candidate = SignedCanonicalCandidateV1 {
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 23),
        record: record.clone(),
    };
    let context = verify_formal_candidate(
        &candidate,
        Some(&frozen_certificate),
        original,
        Some(anchor.head()),
        Some(&recovery3),
        FormalCompletionBranchV1::ReplayFrozen,
    )
    .unwrap();
    let receipt = |role, key_id| FormalFinalReceiptV1 {
        signature_hex: sign(
            formal_final_receipt_signing_bytes(&context.receipt_body(role).unwrap()).unwrap(),
            key_id,
        ),
        body: context.receipt_body(role).unwrap(),
    };
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: record.clone(),
        branch: FormalCompletionBranchV1::ReplayFrozen,
        writer_signature_hex: candidate.writer_signature_hex,
        frozen: Some(frozen_certificate),
        primary: receipt(EndpointRole::Primary, 24),
        replica: receipt(EndpointRole::Replica, 25),
    };
    let verified_decision =
        verify_formal_decision(&decision, original, Some(anchor.head()), Some(&recovery3)).unwrap();
    let completion_receipt = |role, key_id| FormalProofCompleteReceiptV1 {
        signature_hex: sign(
            formal_proof_complete_signing_bytes(
                &verified_decision.complete_receipt_body(role).unwrap(),
            )
            .unwrap(),
            key_id,
        ),
        body: verified_decision.complete_receipt_body(role).unwrap(),
    };
    let completion = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision,
        primary_complete: completion_receipt(EndpointRole::Primary, 24),
        replica_complete: completion_receipt(EndpointRole::Replica, 25),
    };
    let replay = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![
            FormalReadStepV1::Recovery(Box::new(step2)),
            FormalReadStepV1::Recovery(Box::new(step3)),
            FormalReadStepV1::Commit(Box::new(completion.clone())),
        ],
    };
    // The sidecar/request stay anchored at E1 while the CommitRef describes
    // the exact post-replay cut proven by the attached history.
    let mut target_reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    target_reader.replay_history_steps(&replay.steps).unwrap();
    let (request, mut response) = super::response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let committed_ref = commit_ref(&target_reader.current, &response.view);
    response.version.commit = committed_ref.clone();
    response.continuation.commit = committed_ref;
    response.formal_proof.as_mut().unwrap().history_cbor_hex = replay.encode_hex().unwrap();
    let mut reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let observed = reader.verify_view(&response, &request, &public(7)).unwrap();
    let historical_anchor = observed.read_anchor().clone();
    let qualified_origin = reader.current.checkpoint();
    let qualified_origin_digest = qualified_origin.digest().unwrap();
    let completion_digest = formal_qualified_proof_digest(&completion).unwrap();
    let checkpoint = FormalReadCheckpointV2 {
        schema_version: 2,
        origin: qualified_origin.clone(),
        origin_digest: qualified_origin_digest.clone(),
        origin_qualification: Some(FormalReadOriginQualificationV2 {
            base: origin,
            base_digest: origin_digest,
            history: replay,
            origin_record: observed.commit().record().clone(),
            finalizing_authority_id: observed.commit().finalizing_authority_id().to_owned(),
            qualified_proof: completion,
            qualified_proof_digest: completion_digest.clone(),
        }),
        history: FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
        current: qualified_origin,
    };
    let checkpoint_digest = checkpoint.digest().unwrap();
    let mut imported = FormalReadVerifier::from_fixed_checkpoint_v2(
        &trust,
        &checkpoint,
        &checkpoint_digest,
        &qualified_origin_digest,
    )
    .unwrap();
    let token = imported
        .qualified_commits
        .values()
        .find(|entry| entry.proof.decision.branch == FormalCompletionBranchV1::ReplayFrozen)
        .expect("replay import retains exact non-Normal proof token");
    assert_eq!(token.proof.decision.record, *observed.commit().record());
    assert!(token.recovery.is_some());
    assert!(token.retained_context.is_some());
    assert_eq!(imported.verified_recovery_contexts.len(), 2);

    // Evict the exact Recovery3 proof from the bounded commit cache. The
    // reader still has the opaque E2 -> E3 recovery chain produced by replay
    // from its original C0 pin, so the current-root historical inclusion can
    // qualify it without constructing permissions from the response DTO.
    let old_cut = FormalHistoricalCutV1::from_proof(
        &checkpoint
            .origin_qualification
            .as_ref()
            .unwrap()
            .qualified_proof,
    )
    .unwrap();
    let leaf = old_cut.leaf().unwrap();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf.clone()],
        &EmptyIndexNodes,
    )
    .unwrap();
    let descriptor = delta.new.clone();
    let result_root = descriptor.canonical_result_root().unwrap();
    let nodes = memory_index_nodes(delta.node_writes);
    for position in 3..=130u64 {
        let proof = normal_execution_proof_with_signers(
            imported.current.authority(),
            imported.current.head(),
            position as u8,
            result_root.clone(),
            &fixtures::protocol_world::commitments_for_world(&world)
                .unwrap()
                .root()
                .unwrap(),
            [23, 24, 25],
        );
        imported
            .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(proof))])
            .unwrap();
    }
    assert!(!imported.qualified_commits.values().any(|qualification| {
        qualification.proof
            == checkpoint
                .origin_qualification
                .as_ref()
                .unwrap()
                .qualified_proof
    }));

    let inclusion = descriptor.prove(&leaf.key_digest, &nodes).unwrap();
    let historical = VerifiedHistoricalCutInclusionV2::verify(
        &imported.current,
        &descriptor,
        &inclusion,
        historical_anchor.head().record().position,
    )
    .unwrap();
    let (old_request, old_response) = super::response(
        &world,
        &historical_anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: Vec::new(),
        },
    );
    let old = imported
        .verify_committed_historical_view(&historical, &old_response, &old_request, &public(7))
        .unwrap();
    assert_eq!(old.commit().record(), historical_anchor.head().record());
}

#[test]
fn e2_normal_result_remains_readable_from_e3_current_root() {
    let (world, trust, anchor, _) = profile2_initial_fixture();
    let original = anchor.authority();
    let projection_root = fixtures::protocol_world::commitments_for_world(&world)
        .unwrap()
        .root()
        .unwrap();
    let empty_tail = RecoveryTailWitnessV1::Empty;
    let minimum = issuer::IssuerAnchorV1::initial(original.policy()).unwrap();
    let next2 = super::successor(original, 2, 13, 14, 15, 2);
    let (recovery2_step, issuer_anchor2) = super::recovery_step(
        original,
        anchor.head(),
        next2,
        empty_tail.clone(),
        None,
        1,
        minimum,
        [5, 6],
    );

    let mut reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    reader
        .replay_history_steps(&[FormalReadStepV1::Recovery(Box::new(recovery2_step))])
        .unwrap();
    let e2_proof = normal_execution_proof_with_signers(
        reader.current.authority(),
        reader.current.head(),
        51,
        h(70),
        &projection_root,
        [13, 14, 15],
    );
    let e2_record = e2_proof.decision.record.clone();
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(e2_proof.clone()))])
        .unwrap();

    let old_cut = FormalHistoricalCutV1::from_proof(&e2_proof).unwrap();
    let leaf = old_cut.leaf().unwrap();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf.clone()],
        &EmptyIndexNodes,
    )
    .unwrap();
    let descriptor = delta.new.clone();
    let result_root = descriptor.canonical_result_root().unwrap();
    let nodes = memory_index_nodes(delta.node_writes);

    let next3 = super::successor(reader.current.authority(), 3, 23, 24, 25, 3);
    let (recovery3_step, _) = super::recovery_step(
        reader.current.authority(),
        reader.current.head(),
        next3,
        empty_tail,
        None,
        2,
        issuer_anchor2,
        [14, 15],
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Recovery(Box::new(recovery3_step))])
        .unwrap();
    let e3_proof = normal_execution_proof_with_signers(
        reader.current.authority(),
        reader.current.head(),
        52,
        result_root,
        &projection_root,
        [23, 24, 25],
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(e3_proof))])
        .unwrap();

    let inclusion = descriptor.prove(&leaf.key_digest, &nodes).unwrap();
    let e2_authority = reader
        .verified_authorities
        .get(&e2_record.original_authority_id)
        .unwrap();
    let included = VerifiedHistoricalCutInclusionV2::verify(
        &reader.current,
        &descriptor,
        &inclusion,
        e2_record.position,
    )
    .unwrap();
    let historical = included
        .qualify_normal_with_verified_authority(&reader.current, e2_authority)
        .unwrap();
    let old_anchor =
        VerifiedFormalReadAnchorV1::from_committed_historical_cut_with_verified_authority(
            &reader.current,
            &historical,
        )
        .unwrap();
    let (old_request, old_response) = super::response(
        &world,
        &old_anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: Vec::new(),
        },
    );
    let old = reader
        .verify_committed_historical_view(&included, &old_response, &old_request, &public(7))
        .unwrap();
    assert_eq!(old.commit().record(), &e2_record);

    let e1_authority = reader
        .verified_authorities
        .get(&original.authority_id().unwrap())
        .unwrap();
    assert!(
        VerifiedHistoricalCutInclusionV2::verify(
            &reader.current,
            &descriptor,
            &inclusion,
            e2_record.position,
        )
        .unwrap()
        .qualify_normal_with_verified_authority(&reader.current, e1_authority)
        .is_err()
    );
}

#[test]
fn e2_recovery_result_remains_readable_after_e2_and_e3_normal_commits() {
    let (world, trust, anchor, _) = profile2_initial_fixture();
    let original = anchor.authority();
    let projection_root = fixtures::protocol_world::commitments_for_world(&world)
        .unwrap()
        .root()
        .unwrap();
    let frozen_body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: original.definition().world.clone(),
        authority_id: original.authority_id().unwrap(),
        epoch: original.definition().epoch,
        activation_ref: original.activation_ref().to_owned(),
        parent: anchor.head().head(),
        next_position: anchor.head().record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: anchor.head().record().runtime_manifest_digest.clone(),
        governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
        pre_state_root: anchor.head().record().after_state_root.clone(),
        frozen_material_digest: h(62),
        handoff: None,
    };
    let reservation_id = frozen_body.reservation_id().unwrap();
    let frozen_receipt = |role, binding: &EndpointSignerBindingV1, key_id| {
        let body = FormalFrozenReceiptBodyV1 {
            authority_id: original.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: reservation_id.clone(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&frozen_body).unwrap(), 4),
            body: frozen_body.clone(),
        },
        primary: frozen_receipt(EndpointRole::Primary, &original.definition().primary, 5),
        replica: frozen_receipt(EndpointRole::Replica, &original.definition().replica, 6),
    };
    let minimum = issuer::IssuerAnchorV1::initial(original.policy()).unwrap();
    let next2 = super::successor(original, 2, 13, 14, 15, 2);
    let (recovery2_step, issuer_anchor2) = super::recovery_step(
        original,
        anchor.head(),
        next2,
        RecoveryTailWitnessV1::Frozen(frozen.clone()),
        None,
        1,
        minimum,
        [5, 6],
    );
    let mut reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    reader
        .replay_history_steps(&[FormalReadStepV1::Recovery(Box::new(recovery2_step))])
        .unwrap();
    let recovery2 = reader.pending.as_ref().unwrap().clone();

    let mut record = anchor.head().record().clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = frozen_body.next_position;
    record.parent_record_id = frozen_body.parent.record_id.clone();
    record.request_digest = frozen_body.request_digest.clone();
    record.payload_digest = frozen_body.payload_digest.clone();
    record.frozen_reservation_id = Some(reservation_id);
    record.runtime_manifest_digest = frozen_body.runtime_manifest_digest.clone();
    record.governing_manifest_digest = frozen_body.governing_manifest_digest.clone();
    record.execution_policy_digest = frozen_body.execution_policy_digest.clone();
    record.before_state_root = frozen_body.pre_state_root.clone();
    record.after_state_root = h(63);
    record.action_root = h(64);
    record.event_root = h(65);
    record.result_root = h(66);
    record.projection_root = projection_root.clone();
    record.artifact_roots.insert(
        ArtifactRole::ExecutionManifest,
        frozen_body.runtime_manifest_digest.clone(),
    );
    let candidate = SignedCanonicalCandidateV1 {
        writer_signature_hex: sign(canonical_candidate_signing_bytes(&record).unwrap(), 13),
        record,
    };
    let context = verify_formal_candidate(
        &candidate,
        Some(&frozen),
        original,
        Some(anchor.head()),
        Some(&recovery2),
        FormalCompletionBranchV1::ReplayFrozen,
    )
    .unwrap();
    let final_receipt = |role, key_id| {
        let body = context.receipt_body(role).unwrap();
        FormalFinalReceiptV1 {
            signature_hex: sign(formal_final_receipt_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let decision = FormalDecisionProofV1 {
        schema_version: 1,
        record: candidate.record.clone(),
        branch: FormalCompletionBranchV1::ReplayFrozen,
        writer_signature_hex: candidate.writer_signature_hex.clone(),
        frozen: Some(frozen),
        primary: final_receipt(EndpointRole::Primary, 14),
        replica: final_receipt(EndpointRole::Replica, 15),
    };
    let verified_decision =
        verify_formal_decision(&decision, original, Some(anchor.head()), Some(&recovery2)).unwrap();
    let completion_receipt = |role, key_id| {
        let body = verified_decision.complete_receipt_body(role).unwrap();
        FormalProofCompleteReceiptV1 {
            signature_hex: sign(formal_proof_complete_signing_bytes(&body).unwrap(), key_id),
            body,
        }
    };
    let e2_recovery_proof = FormalQualifiedCommitProofV1 {
        schema_version: 1,
        decision,
        primary_complete: completion_receipt(EndpointRole::Primary, 14),
        replica_complete: completion_receipt(EndpointRole::Replica, 15),
    };
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(
            e2_recovery_proof.clone(),
        ))])
        .unwrap();

    let old_cut = FormalHistoricalCutV1::from_proof(&e2_recovery_proof).unwrap();
    let leaf = old_cut.leaf().unwrap();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf.clone()],
        &EmptyIndexNodes,
    )
    .unwrap();
    let descriptor = delta.new.clone();
    let result_root = descriptor.canonical_result_root().unwrap();
    let nodes = memory_index_nodes(delta.node_writes);
    let e2_normal = normal_execution_proof_with_signers(
        reader.current.authority(),
        reader.current.head(),
        67,
        result_root.clone(),
        &projection_root,
        [13, 14, 15],
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(e2_normal))])
        .unwrap();

    let next3 = super::successor(reader.current.authority(), 3, 23, 24, 25, 4);
    let (recovery3_step, _) = super::recovery_step(
        reader.current.authority(),
        reader.current.head(),
        next3,
        RecoveryTailWitnessV1::Empty,
        None,
        2,
        issuer_anchor2,
        [14, 15],
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Recovery(Box::new(recovery3_step))])
        .unwrap();
    let e3_normal = normal_execution_proof_with_signers(
        reader.current.authority(),
        reader.current.head(),
        68,
        result_root,
        &projection_root,
        [23, 24, 25],
    );
    reader
        .replay_history_steps(&[FormalReadStepV1::Commit(Box::new(e3_normal))])
        .unwrap();

    let inclusion = descriptor.prove(&leaf.key_digest, &nodes).unwrap();
    let historical = VerifiedHistoricalCutInclusionV2::verify(
        &reader.current,
        &descriptor,
        &inclusion,
        e2_recovery_proof.decision.record.position,
    )
    .unwrap();
    let contexts: Vec<_> = reader
        .verified_recovery_contexts
        .iter()
        .map(|(recovery, predecessor)| (recovery, predecessor))
        .collect();
    let committed = historical
        .qualify_with_verified_recovery_contexts(&reader.current, &contexts)
        .unwrap();
    let old_anchor = VerifiedFormalReadAnchorV1::from_committed_historical_recovery_cut(
        &reader.current,
        &committed,
    )
    .unwrap();
    let (old_request, old_response) = super::response(
        &world,
        &old_anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: Vec::new(),
        },
    );
    let old = reader
        .verify_committed_historical_view(&historical, &old_response, &old_request, &public(7))
        .unwrap();
    assert_eq!(old.commit().record(), &e2_recovery_proof.decision.record);

    let truncated_contexts = &contexts[..1];
    assert!(
        historical
            .qualify_with_verified_recovery_contexts(&reader.current, truncated_contexts)
            .is_err()
    );
}

#[test]
fn historical_normal_lookup_survives_qualification_cache_eviction() {
    let (world, trust, anchor, _initial_proof) = profile2_initial_fixture();
    let projection_root = fixtures::protocol_world::commitments_for_world(&world)
        .unwrap()
        .root()
        .unwrap();
    let old_proof = normal_execution_proof(
        anchor.authority(),
        anchor.head(),
        60,
        h(70),
        &projection_root,
    );
    let old_commit =
        verify_formal_qualified(&old_proof, anchor.authority(), Some(anchor.head()), None).unwrap();
    let old_anchor = anchor
        .after_verified_commit(anchor.authority().clone(), old_commit)
        .unwrap();
    let old_cut = FormalHistoricalCutV1::from_proof(&old_proof).unwrap();
    let leaf = old_cut.leaf().unwrap();
    let delta = BusinessIndexDeltaV1::build(
        BusinessIndexRootV1::empty().unwrap(),
        vec![leaf.clone()],
        &EmptyIndexNodes,
    )
    .unwrap();
    let index_root = delta.new.clone();
    let final_result_root = index_root.canonical_result_root().unwrap();
    let nodes = memory_index_nodes(delta.node_writes);
    let mut live = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let mut reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let mut page_start = reader.replay_cursor().unwrap();
    let mut page_steps = vec![FormalReadStepV1::Commit(Box::new(old_proof.clone()))];
    let mut persisted_pages = Vec::new();
    live.replay_history_steps(&[FormalReadStepV1::Commit(Box::new(old_proof.clone()))])
        .unwrap();
    for position in 3..=130u64 {
        let result_root = if position == 130 {
            final_result_root.clone()
        } else {
            h(position as u8)
        };
        let proof = normal_execution_proof(
            live.current.authority(),
            live.current.head(),
            position as u8,
            result_root,
            &projection_root,
        );
        let step = FormalReadStepV1::Commit(Box::new(proof));
        live.replay_history_steps(std::slice::from_ref(&step))
            .unwrap();
        page_steps.push(step);
        if page_steps.len() == MAX_FORMAL_HISTORY_STEPS || position == 130 {
            let page = FormalReadHistoryPageV1 {
                schema_version: 1,
                start: page_start,
                history: FormalReadHistoryV1 {
                    schema_version: 1,
                    steps: std::mem::take(&mut page_steps),
                },
                end: live.replay_cursor().unwrap(),
            };
            let bytes = page.encode_bytes().unwrap();
            let digest = hex::encode(blake3::hash(&bytes).as_bytes());
            reader.replay_history_page(&page).unwrap();
            assert_eq!(reader.replay_cursor().unwrap(), page.end);
            persisted_pages.push((digest, bytes));
            page_start = page.end;
        }
    }
    assert_eq!(
        persisted_pages.len(),
        2,
        "129 commits remain two bounded pages"
    );
    assert_eq!(
        FormalReadHistoryPageV1::decode_bytes(&persisted_pages[0].1)
            .unwrap()
            .history
            .steps
            .len(),
        MAX_FORMAL_HISTORY_STEPS
    );
    assert_eq!(
        FormalReadHistoryPageV1::decode_bytes(&persisted_pages[1].1)
            .unwrap()
            .history
            .steps
            .len(),
        1
    );
    assert!(persisted_pages[0].1.len() <= oasis7_client_api::world_service::MAX_FORMAL_PROOF_BYTES);

    let first_page = FormalReadHistoryPageV1::decode_bytes(&persisted_pages[0].1).unwrap();
    let mut first_page_reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    first_page_reader.replay_history_page(&first_page).unwrap();
    let (page_request, mut page_response) =
        super::response(&world, &anchor, first_page.history.clone());
    let page_commit = commit_ref(&first_page_reader.current, &page_response.view);
    page_response.version.commit = page_commit.clone();
    page_response.continuation.commit = page_commit;
    let mut page_response_reader = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let page_observation = page_response_reader
        .verify_view(&page_response, &page_request, &public(7))
        .unwrap();
    assert_eq!(page_observation.history_page(), Some(&first_page));

    // A fresh process starts at the same externally trusted origin and
    // reconstructs the exact cursor by replaying immutable hash-addressed
    // pages; the persisted cursor itself is only a consistency check.
    let mut restarted = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    for (digest, bytes) in &persisted_pages {
        assert_eq!(hex::encode(blake3::hash(bytes).as_bytes()), *digest);
        let page = FormalReadHistoryPageV1::decode_bytes(bytes).unwrap();
        restarted.replay_history_page(&page).unwrap();
    }
    assert_eq!(
        restarted.replay_cursor().unwrap(),
        live.replay_cursor().unwrap()
    );
    assert_eq!(
        restarted.current.head().record(),
        live.current.head().record()
    );
    assert_eq!(
        restarted.qualified_commits.len(),
        live.qualified_commits.len()
    );
    let mut random_suffix = [0u8; 8];
    getrandom::fill(&mut random_suffix).unwrap();
    let history_store_root = std::env::temp_dir().join(format!(
        "oasis7-formal-history-pages-{}-{}",
        std::process::id(),
        hex::encode(random_suffix)
    ));
    std::fs::create_dir(&history_store_root).unwrap();
    let history_store = history_store_root.clone();
    for (digest, bytes) in &persisted_pages {
        std::fs::write(history_store.join(digest), bytes).unwrap();
    }

    let origin_checkpoint = anchor.checkpoint();
    let origin_digest = origin_checkpoint.digest().unwrap();
    let client = RemoteWorldServiceClient::new_formal(
        WorldServiceClientConfig {
            formal_read_trust: Some(FormalReadTrustConfigV1 {
                initial_trust: trust.clone(),
                checkpoint: origin_checkpoint,
                checkpoint_digest: origin_digest.clone(),
            }),
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: public(9),
            expected_world: WorldIdentity {
                world_id: trust.world.world_id.clone(),
                genesis_digest: trust.world.genesis_digest.clone(),
            },
            scope_id: "public".into(),
            read_private_key_hex: hex::encode(key(7).to_bytes()),
            timeout: Duration::from_secs(1),
            max_response_bytes: 16 * 1024 * 1024,
        },
        trust.clone(),
        anchor.clone(),
    )
    .unwrap();
    // Preserve the complete portable cursor across the same serialization
    // boundary used by a reopened external client.
    let expected_cursor =
        serde_json::from_value(serde_json::to_value(live.replay_cursor().unwrap()).unwrap())
            .unwrap();
    client
        .resume_formal_history_pages(
            &origin_digest,
            persisted_pages.iter().map(|(digest, _)| {
                std::fs::read(history_store.join(digest))
                    .map(|bytes| {
                        assert_eq!(blake3::hash(&bytes).to_hex().as_str(), digest);
                        (digest.clone(), bytes)
                    })
                    .map_err(|error| format!("{error:?}"))
            }),
            &expected_cursor,
        )
        .unwrap();
    assert_eq!(client.formal_replay_cursor().unwrap(), expected_cursor);
    assert_eq!(
        serde_json::to_value(client.formal_replay_cursor().unwrap()).unwrap(),
        serde_json::to_value(live.replay_cursor().unwrap()).unwrap()
    );
    std::fs::remove_dir_all(&history_store_root).unwrap();
    reader = restarted;
    let old_key = QualifiedCommitKey {
        position: old_anchor.head().record().position,
        record_id: old_anchor.head().head().record_id.clone(),
        proof_digest: formal_qualified_proof_digest(&old_proof).unwrap(),
    };
    assert!(!reader.qualified_commits.contains_key(&old_key));
    assert!(
        reader
            .verified_authorities
            .contains_key(&anchor.authority().authority_id().unwrap())
    );
    assert_eq!(reader.current.head().record().position, 130);
    let inclusion = index_root.prove(&leaf.key_digest, &nodes).unwrap();
    let historical = VerifiedHistoricalCutInclusionV2::verify(
        &reader.current,
        &index_root,
        &inclusion,
        old_anchor.head().record().position,
    )
    .unwrap();
    let (old_request, old_response) = super::response(
        &world,
        &old_anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let observed = reader
        .verify_committed_historical_view(&historical, &old_response, &old_request, &public(7))
        .unwrap();
    assert_eq!(observed.commit().record(), old_anchor.head().record());
}
