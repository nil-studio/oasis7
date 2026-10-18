use super::*;
use crate::formal_read::{FormalReadHistoryV1, FormalReadReplayCursorV1, FormalReadStepV1};
use crate::formal_read_fixtures as fixtures;
use oasis7_distfs::controlled_authority::{
    formal::*,
    replicated_protocol::{ArtifactRole, EndpointRole},
};
fn normal_execution_proof(
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    salt: u8,
    signer_keys: [u8; 3],
) -> FormalQualifiedCommitProofV1 {
    let request_digest = fixtures::h(salt);
    let payload_digest = fixtures::h(salt.wrapping_add(1));
    let after_state_root = fixtures::h(salt.wrapping_add(2));
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
        frozen_material_digest: fixtures::h(salt.wrapping_add(3)),
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
            signature_hex: fixtures::sign(
                frozen_signing_bytes(&frozen_body).unwrap(),
                signer_keys[0],
            ),
            body: frozen_body,
        },
        primary: frozen_receipt(
            EndpointRole::Primary,
            &authority.definition().primary,
            signer_keys[1],
        ),
        replica: frozen_receipt(
            EndpointRole::Replica,
            &authority.definition().replica,
            signer_keys[2],
        ),
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
    record.action_root = fixtures::h(salt.wrapping_add(4));
    record.event_root = fixtures::h(salt.wrapping_add(5));
    record.result_root = fixtures::h(salt.wrapping_add(6));
    record
        .artifact_roots
        .insert(ArtifactRole::Input, payload_digest);
    record
        .artifact_roots
        .insert(ArtifactRole::Snapshot, after_state_root);
    fixtures::qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(parent),
        None,
        signer_keys[0],
        signer_keys[1],
        signer_keys[2],
    )
}

fn cursor(anchor: &VerifiedFormalReadAnchorV1) -> FormalReadReplayCursorV1 {
    FormalReadReplayCursorV1 {
        schema_version: 1,
        anchor: oasis7_client_api::world_service::FormalReadAnchorRefV1 {
            position: anchor.head().record().position,
            record_id: anchor.head().head().record_id,
            active_authority_id: anchor.authority().authority_id().unwrap(),
        },
        checkpoint_digest: anchor.checkpoint().digest().unwrap(),
        last_recovery_digest: None,
        pending_recovery_digest: None,
        requires_v2_checkpoint: false,
    }
}
fn page(
    anchor: &VerifiedFormalReadAnchorV1,
    count: usize,
    salt: u8,
) -> (FormalReadHistoryPageV1, VerifiedFormalReadAnchorV1) {
    let mut current = anchor.clone();
    let mut steps = Vec::new();
    for n in 0..count {
        let proof = normal_execution_proof(
            current.authority(),
            current.head(),
            salt.wrapping_add(n as u8),
            [4, 5, 6],
        );
        let commit =
            verify_formal_qualified(&proof, current.authority(), Some(current.head()), None)
                .unwrap();
        current = current
            .after_verified_commit(current.authority().clone(), commit)
            .unwrap();
        steps.push(FormalReadStepV1::Commit(Box::new(proof)));
    }
    (
        FormalReadHistoryPageV1 {
            schema_version: 1,
            start: cursor(anchor),
            history: FormalReadHistoryV1 {
                schema_version: 1,
                steps,
            },
            end: cursor(&current),
        },
        current,
    )
}
#[test]
fn formal_recovery_lineage_streams_more_than_128_steps_without_replacing_origin() {
    let (_, trust, origin, _) = fixtures::actual_world_initial_fixture();
    let (first, middle) = page(&origin, 64, 10);
    let (second, next) = page(&middle, 64, 80);
    let (third, target) = page(&next, 2, 150);
    let pinned = origin.checkpoint();
    let result = verify_recovery_predecessor_lineage(
        &trust,
        &origin,
        &target.checkpoint(),
        [Ok(first), Ok(second), Ok(third)],
    )
    .unwrap();
    assert_eq!(result.checkpoint(), target.checkpoint());
    assert_eq!(origin.checkpoint(), pinned);
    assert_eq!(result.head().record().position, 131);
}
#[test]
fn formal_recovery_lineage_rejects_missing_discontinuous_fork_and_authority_mismatch() {
    let (_, trust, origin, _) = fixtures::actual_world_initial_fixture();
    let (first, middle) = page(&origin, 1, 10);
    let (second, target) = page(&middle, 1, 30);
    assert!(
        verify_recovery_predecessor_lineage(
            &trust,
            &origin,
            &target.checkpoint(),
            [Ok(first.clone())]
        )
        .is_err()
    );
    assert!(
        verify_recovery_predecessor_lineage(
            &trust,
            &origin,
            &target.checkpoint(),
            [Ok(second.clone())]
        )
        .is_err()
    );
    let (fork, _) = page(&origin, 2, 90); // All signatures valid, but a different branch.
    assert!(
        verify_recovery_predecessor_lineage(&trust, &origin, &target.checkpoint(), [Ok(fork)])
            .is_err()
    );
    let mut wrong = target.checkpoint();
    wrong.active_authority.writer_public_key = fixtures::public(10);
    assert!(
        verify_recovery_predecessor_lineage(
            &trust,
            &origin,
            &wrong,
            [Ok(first.clone()), Ok(second.clone())]
        )
        .is_err()
    );
    let mut altered = first;
    altered.end.checkpoint_digest = fixtures::h(250);
    assert!(
        verify_recovery_predecessor_lineage(
            &trust,
            &origin,
            &target.checkpoint(),
            [Ok(altered), Ok(second)]
        )
        .is_err()
    );
}
