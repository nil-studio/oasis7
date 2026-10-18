use crate::authority;
use crate::projection::WorldServiceProjection;
use oasis7_client_api::formal_projection::{ProjectionAudienceV1, ProjectionCommitmentsV1};
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::*;
use oasis7_game_client::formal_read::*;
type FormalReadVerifier =
    oasis7_game_client::formal_read::FormalReadVerifier<WorldServiceProjection>;

use crate::formal_read_fixtures as fixtures;
use fixtures::*;

pub(super) use crate::formal_read_response_fixture::response;

#[path = "game_client_rebase_conformance.rs"]
mod rebase_tests;

#[test]
fn formal_header_scope_and_proof_absence_fail_closed_without_cache_changes() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let (request, response) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let before = verifier.anchor_ref().unwrap();
    for edit in 0..9 {
        let mut value = response.clone();
        match edit {
            0 => value.formal_proof = None,
            1 => value.version.commit.binding.authority_generation += 1,
            2 => value.version.commit.binding.permission_generation += 1,
            3 => {
                value.execution_evidence_scope =
                    Some(crate::verified_view::CONTROLLED_PREREQUISITE_READ_SCOPE.into())
            }
            4 => {
                value
                    .formal_proof
                    .as_mut()
                    .unwrap()
                    .projection
                    .audience
                    .reader_public_key = Some(public(7))
            }
            5 => value.continuation.era = value.continuation.era.wrapping_add(1),
            6 => value.continuation.sequence = value.continuation.sequence.wrapping_add(1),
            7 => value.continuation.stream_id = "unproved-stream".into(),
            _ => value.continuation.commit.binding.permission_generation += 1,
        }
        assert!(verifier.verify_view(&value, &request, &public(7)).is_err());
        assert_eq!(verifier.anchor_ref().unwrap(), before);
    }
    let mut fixed = request.clone();
    fixed.fixed_commit = Some(response.version.commit.clone());
    verifier.verify_view(&response, &fixed, &public(7)).unwrap();
    assert_eq!(verifier.anchor_ref().unwrap(), before);
    fixed.fixed_commit.as_mut().unwrap().position += 1;
    assert!(
        verifier
            .fixed_anchor_ref(fixed.fixed_commit.as_ref().unwrap())
            .unwrap_err()
            .starts_with("HistoryUnavailable")
    );
    let excessive = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![
            FormalReadStepV1::Commit(Box::new(actual_world_initial_fixture().3));
            MAX_FORMAL_HISTORY_STEPS + 1
        ],
    };
    assert!(excessive.encode_hex().is_err());
    let bytes = serde_cbor::to_vec(&excessive).unwrap();
    assert!(FormalReadHistoryV1::decode_hex(&hex::encode(bytes)).is_err());
}

#[test]
fn configured_checkpoint_cannot_be_replaced_by_another_higher_pinned_anchor() {
    use crate::client::{
        FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServiceClientConfig,
    };
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust.clone(),
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint: checkpoint.clone(),
        }),
        endpoint: "http://127.0.0.1:9".into(),
        trusted_service_public_key: public(9),
        expected_world: commit_ref(&anchor, &projection(&world)).world,
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(1),
        max_response_bytes: 1_048_576,
    };
    // An independently pinned checkpoint is valid only under its own external
    // pin. Its height cannot prove continuity with the client's configured pin.
    let mut other = checkpoint;
    other.head_record.position += 1;
    other.head_record.kind = FormalRecordKindV1::Execution;
    other.head_record.parent_record_id = h(88);
    other.head_record.frozen_reservation_id = Some(h(89));
    let other_anchor =
        VerifiedFormalReadAnchorV1::from_fixed_pin(&other, &other.digest().unwrap(), &trust)
            .unwrap();
    assert!(RemoteWorldServiceClient::new_formal(config, trust, other_anchor).is_err());
    let mut cache = FormalReadVerifier::new(
        &InitialActivationTrustV1 {
            issuer_public_key: public(9),
            world: anchor.authority().definition().world.clone(),
            initial_state_root: anchor.initial_certificate().body.initial_state_root.clone(),
            governing_manifest_digest: anchor
                .initial_certificate()
                .body
                .governing_manifest_digest
                .clone(),
            recovery_policy_digest: anchor.authority().policy().digest().unwrap(),
        },
        anchor,
    )
    .unwrap();
    let current = cache.anchor_ref().unwrap();
    let mut sibling = current.clone();
    sibling.record_id = h(90);
    assert!(!cache.contains_verified_anchor(&sibling).unwrap());
    assert!(cache.contains_verified_anchor(&current).unwrap());
    let (request, reply) = response(
        &world,
        cache.current_anchor(),
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    cache.verify_view(&reply, &request, &public(7)).unwrap();
    let mut minimum = request;
    minimum.min_commit = Some(reply.version.commit.clone());
    cache.verify_view(&reply, &minimum, &public(7)).unwrap();
    minimum
        .min_commit
        .as_mut()
        .unwrap()
        .binding
        .permission_generation += 1;
    assert!(cache.verify_view(&reply, &minimum, &public(7)).is_err());
}

#[test]
fn typed_qualified_successor_allows_bound_governance_and_permission_evolution() {
    use crate::formal_projection::projection_leaf;
    use oasis7_distfs::controlled_authority::replicated_protocol::{ArtifactRole, EndpointRole};
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    let (mut request, initial) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    verifier
        .verify_view(&initial, &request, &public(7))
        .unwrap();
    // These test-authority commitments exercise the light consumer contract.
    // They do not claim to execute a real governance action or qualify custody.
    let mut view = initial.view.clone();
    view.runtime_binding.as_mut().unwrap().runtime_manifest_hash =
        serde_json::from_value(serde_json::json!(format!("blake3:{}", h(91)))).unwrap();
    view.formal_commit_header
        .as_mut()
        .unwrap()
        .permission_generation += 1;
    let audience = ProjectionAudienceV1::public();
    let tree = ProjectionCommitmentsV1::from_leaves(vec![(
        audience.clone(),
        projection_leaf(&audience, &view).unwrap(),
    )])
    .unwrap();
    let authority = anchor.authority();
    let body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: authority.definition().world.clone(),
        authority_id: authority.authority_id().unwrap(),
        epoch: authority.definition().epoch,
        activation_ref: authority.activation_ref().into(),
        parent: anchor.head().head(),
        next_position: anchor.head().record().position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: h(62),
        governing_manifest_digest: anchor.head().record().governing_manifest_digest.clone(),
        execution_policy_digest: anchor.head().record().execution_policy_digest.clone(),
        pre_state_root: anchor.head().record().after_state_root.clone(),
        frozen_material_digest: h(64),
        handoff: None,
    };
    let receipt = |role, instance: &EndpointSignerBindingV1, key| {
        let receipt = FormalFrozenReceiptBodyV1 {
            authority_id: authority.authority_id().unwrap(),
            role,
            instance_digest: instance.instance.digest().unwrap(),
            reservation_id: body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&receipt).unwrap(), key),
            body: receipt,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        primary: receipt(EndpointRole::Primary, &authority.definition().primary, 5),
        replica: receipt(EndpointRole::Replica, &authority.definition().replica, 6),
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&body).unwrap(), 4),
            body: body.clone(),
        },
    };
    let mut record = anchor.head().record().clone();
    let reservation_id = body.reservation_id().unwrap();
    record.kind = FormalRecordKindV1::Execution;
    record.position = body.next_position;
    record.parent_record_id = body.parent.record_id;
    record.request_digest = body.request_digest;
    record.payload_digest = body.payload_digest;
    record.frozen_reservation_id = Some(reservation_id);
    record.runtime_manifest_digest = h(62);
    record
        .artifact_roots
        .insert(ArtifactRole::ExecutionManifest, h(62));
    record.governing_manifest_digest = h(91);
    record.projection_root = tree.root().unwrap();
    let proof = qualified_proof(
        record,
        Some(frozen),
        authority,
        Some(anchor.head()),
        None,
        4,
        5,
        6,
    );
    let committed = verify_formal_qualified(&proof, authority, Some(anchor.head()), None).unwrap();
    let successor = anchor
        .after_verified_commit(authority.clone(), committed)
        .unwrap();
    let mut reply = initial.clone();
    reply.view = view;
    reply.version.commit = commit_ref(&successor, &reply.view);
    reply.version.projection_revision =
        authority::request_digest("projection", &reply.view).unwrap();
    reply.continuation.commit = reply.version.commit.clone();
    let inclusion = tree.inclusion(&audience).unwrap();
    let sidecar = reply.formal_proof.as_mut().unwrap();
    sidecar.history_cbor_hex = FormalReadHistoryV1 {
        schema_version: 1,
        steps: vec![FormalReadStepV1::Commit(Box::new(proof))],
    }
    .encode_hex()
    .unwrap();
    sidecar.projection = FormalProjectionInclusionV1 {
        schema_version: inclusion.schema_version,
        audience: FormalProjectionAudienceV1 {
            scope_id: inclusion.audience.scope_id,
            reader_public_key: inclusion.audience.reader_public_key,
        },
        leaf_index: inclusion.leaf_index,
        leaf_count: inclusion.leaf_count,
        siblings: inclusion.siblings,
    };
    request.min_commit = Some(initial.version.commit.clone());
    assert!(
        reply.validate(&request).is_err(),
        "prototype binding equality remains strict"
    );
    let before = verifier.anchor_ref().unwrap();
    let mut fork = reply.clone();
    fork.version.commit.execution_block_hash = h(92);
    fork.continuation.commit = fork.version.commit.clone();
    assert!(verifier.verify_view(&fork, &request, &public(7)).is_err());
    let mut counter = reply.clone();
    counter.version.commit.binding.permission_generation += 1;
    counter.continuation.commit = counter.version.commit.clone();
    assert!(
        verifier
            .verify_view(&counter, &request, &public(7))
            .is_err()
    );
    assert_eq!(verifier.anchor_ref().unwrap(), before);
    verifier.verify_view(&reply, &request, &public(7)).unwrap();
    assert_eq!(
        verifier.anchor_ref().unwrap().record_id,
        reply.version.commit.execution_block_hash
    );
}

#[test]
fn formal_cursor_restore_failure_is_atomic_and_existing_projection_cache_cannot_rewind() {
    use crate::client::{
        FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServiceClientConfig,
    };
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let (request, reply) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
    verifier.verify_view(&reply, &request, &public(7)).unwrap();
    let valid = verifier.export_cursor().unwrap();
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:9".into(),
        trusted_service_public_key: public(9),
        expected_world: reply.version.commit.world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(1),
        max_response_bytes: 16 * 1024 * 1024,
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
    };
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let origin = client.formal_anchor_ref().unwrap();
    let mut invalid = valid.clone();
    invalid.frames.push(valid.frames[0].clone());
    assert!(client.restore_formal_cursor(&invalid).is_err());
    assert_eq!(client.formal_anchor_ref().unwrap(), origin);
    assert!(
        client.formal_cursor().is_err(),
        "no partially authenticated frame journal"
    );
    let mut invalid = valid.clone();
    invalid.commit.binding.permission_generation += 1;
    assert!(client.restore_formal_cursor(&invalid).is_err());
    assert_eq!(client.formal_anchor_ref().unwrap(), origin);
    assert!(client.formal_cursor().is_err());
    client.restore_formal_cursor(&valid).unwrap();
    let before =
        authority::request_digest("cursor-state-test", &client.formal_cursor().unwrap()).unwrap();
    assert!(
        client.restore_formal_cursor(&valid).is_err(),
        "restore is fresh-client-only even at equal height"
    );
    assert_eq!(
        authority::request_digest("cursor-state-test", &client.formal_cursor().unwrap()).unwrap(),
        before
    );
}
