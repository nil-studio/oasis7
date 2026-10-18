use crate::{
    authority,
    client::{
        FormalReadTrustConfigV1, RemoteWorldServiceClient, WorldServiceClientConfig,
        WorldServiceClientError,
    },
    formal_projection::ProjectionAudienceV1,
    formal_read::{FormalReadHistoryV1, FormalReadStepV1, FormalReadVerifier},
    formal_result::{FormalBusinessPageVerification, verify_formal_business_outcome_page},
    projection::WorldServiceProjection,
    wire::{
        AgentSignerDelegationChangeV1, FormalAuthenticatedLookupV1, LOOKUP_PATH, VIEW_PATH,
        WorldServicePayloadV1,
    },
};
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::{formal::*, replicated_protocol::EndpointRole};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use crate::formal_read_fixtures as fixtures;
use fixtures::*;

fn delegation_request(world: WorldIdentity) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let payload = WorldServicePayloadV1::Delegation(
        authority::sign_read_request(
            "delegation",
            AgentSignerDelegationChangeV1 {
                world_id: world.world_id.clone(),
                branch_id: "main".into(),
                agent_id: "agent-a".into(),
                owner_binding: h(70),
                agent_identity_generation: 1,
                generation: 1,
                delegate_public_key: public(8),
                revoked: false,
                nonce: 23,
            },
            &h(7),
        )
        .unwrap(),
    );
    SubmitIntentRequest {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        correlation: crate::derive_correlation(world, &payload).unwrap(),
        deadline_unix_ms: Some(123_456),
        signed_payload: payload,
    }
}

fn progress_page(
    world: &oasis7_runtime::World,
    anchor: &VerifiedFormalReadAnchorV1,
    target_request: ReadWorldViewRequest,
    requested_correlation: RequestCorrelation,
) -> FormalBusinessOutcomePageV1<WorldServiceProjection> {
    let view = projection(world);
    let audience = ProjectionAudienceV1::public();
    let commitments = crate::formal_projection::commitments_for_world(world).unwrap();
    let inclusion = commitments.inclusion(&audience).unwrap();
    let verified_authority = anchor.authority();
    let parent = anchor.head().record();
    let body = FormalFrozenBodyV1 {
        schema_version: 1,
        world: verified_authority.definition().world.clone(),
        authority_id: verified_authority.authority_id().unwrap(),
        epoch: verified_authority.definition().epoch,
        activation_ref: verified_authority.activation_ref().into(),
        parent: anchor.head().head(),
        next_position: parent.position + 1,
        request_digest: h(60),
        payload_digest: h(61),
        runtime_manifest_digest: parent.runtime_manifest_digest.clone(),
        governing_manifest_digest: parent.governing_manifest_digest.clone(),
        execution_policy_digest: parent.execution_policy_digest.clone(),
        pre_state_root: parent.after_state_root.clone(),
        frozen_material_digest: h(64),
        handoff: None,
    };
    let receipt = |role, binding: &EndpointSignerBindingV1, key| {
        let receipt_body = FormalFrozenReceiptBodyV1 {
            authority_id: verified_authority.authority_id().unwrap(),
            role,
            instance_digest: binding.instance.digest().unwrap(),
            reservation_id: body.reservation_id().unwrap(),
        };
        FormalFrozenReceiptV1 {
            signature_hex: sign(frozen_receipt_signing_bytes(&receipt_body).unwrap(), key),
            body: receipt_body,
        }
    };
    let frozen = FormalFrozenCertificateV1 {
        primary: receipt(
            EndpointRole::Primary,
            &verified_authority.definition().primary,
            5,
        ),
        replica: receipt(
            EndpointRole::Replica,
            &verified_authority.definition().replica,
            6,
        ),
        reservation: SignedFormalFrozenV1 {
            signature_hex: sign(frozen_signing_bytes(&body).unwrap(), 4),
            body: body.clone(),
        },
    };
    let mut record = parent.clone();
    record.kind = FormalRecordKindV1::Execution;
    record.position = body.next_position;
    record.parent_record_id = body.parent.record_id.clone();
    record.request_digest = body.request_digest.clone();
    record.payload_digest = body.payload_digest.clone();
    record.frozen_reservation_id = Some(body.reservation_id().unwrap());
    record.projection_root = commitments.root().unwrap();
    let proof = qualified_proof(
        record.clone(),
        Some(frozen),
        verified_authority,
        Some(anchor.head()),
        None,
        4,
        5,
        6,
    );
    let committed =
        verify_formal_qualified(&proof, verified_authority, Some(anchor.head()), None).unwrap();
    let successor = anchor
        .after_verified_commit(verified_authority.clone(), committed)
        .unwrap();
    let from_anchor = FormalReadAnchorRefV1 {
        position: parent.position,
        record_id: parent.record_id().unwrap(),
        active_authority_id: verified_authority.authority_id().unwrap(),
    };
    let continuation_anchor = FormalReadAnchorRefV1 {
        position: record.position,
        record_id: record.record_id().unwrap(),
        active_authority_id: verified_authority.authority_id().unwrap(),
    };
    let commit = commit_ref(&successor, &view);
    let header = view.formal_commit_header.as_ref().unwrap();
    let projection_inclusion = FormalProjectionInclusionV1 {
        schema_version: inclusion.schema_version,
        audience: FormalProjectionAudienceV1 {
            scope_id: inclusion.audience.scope_id,
            reader_public_key: inclusion.audience.reader_public_key,
        },
        leaf_index: inclusion.leaf_index,
        leaf_count: inclusion.leaf_count,
        siblings: inclusion.siblings,
    };
    let target_view = ReadWorldViewResponse {
        formal_profile: Some(FormalReadProfileV1::controlled()),
        formal_proof: Some(FormalReadSidecarV1 {
            schema_version: 1,
            profile: FormalReadProfileV1::controlled(),
            anchor: from_anchor.clone(),
            history_cbor_hex: FormalReadHistoryV1 {
                schema_version: 1,
                steps: vec![FormalReadStepV1::Commit(Box::new(proof))],
            }
            .encode_hex()
            .unwrap(),
            projection: projection_inclusion,
        }),
        execution_evidence_scope: None,
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        version: ProjectionVersion {
            commit: commit.clone(),
            projection_revision: authority::request_digest("projection", &view).unwrap(),
            visibility_scope: target_request.scope_id.clone(),
        },
        logical_tick: view.state.time,
        continuation: EventCursor {
            stream_id: authority::request_digest(
                "formal-events-v1",
                &(&commit.world, &target_request.scope_id),
            )
            .unwrap(),
            scope_id: target_request.scope_id.clone(),
            era: header.event_id_era,
            sequence: header.journal_high_watermark,
            commit,
        },
        view,
    };
    FormalBusinessOutcomePageV1 {
        schema_version: FORMAL_BUSINESS_RESULT_SCHEMA_V1,
        requested_correlation,
        from_anchor,
        target_request,
        target_view,
        result: None,
        historical_cbor_hex: None,
        continuation_anchor: Some(continuation_anchor),
    }
}

fn serve_valid_page_then_change_read_deadline(
    world: oasis7_runtime::World,
    trust: InitialActivationTrustV1,
    anchor: VerifiedFormalReadAnchorV1,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let (body_start, body_len) = loop {
            let mut chunk = [0; 1024];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break (end + 4, length);
                }
            }
        };
        let message: FormalAuthenticatedLookupV1 =
            serde_json::from_slice(&bytes[body_start..body_start + body_len]).unwrap();
        authority::verify_read_request(VIEW_PATH, &message.read).unwrap();
        let original_request = SubmitIntentRequest {
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: crate::derive_correlation(
                message.read.request.world.clone(),
                &message.lookup.original,
            )
            .unwrap(),
            deadline_unix_ms: None,
            signed_payload: message.lookup.original.clone(),
        };
        let mut page = progress_page(
            &world,
            &anchor,
            message.read.request.clone(),
            original_request.correlation.clone(),
        );
        page.validate().unwrap();
        let mut verifier = FormalReadVerifier::new(&trust, anchor).unwrap();
        assert!(matches!(
            verify_formal_business_outcome_page(
                &mut verifier,
                &page,
                &original_request,
                &message.read.subject_public_key,
            )
            .unwrap(),
            FormalBusinessPageVerification::Progress(_)
        ));
        page.target_request.deadline_unix_ms = Some(55_555);
        page.validate()
            .expect("changing only the read deadline leaves the signed projection valid");
        let response_digest = authority::request_digest(LOOKUP_PATH, &message).unwrap();
        let response = authority::sign_service_response(
            LOOKUP_PATH,
            response_digest,
            serde_json::to_value(page).unwrap(),
            &h(9),
        )
        .unwrap();
        let response = serde_json::to_vec(&response).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        )
        .unwrap();
        stream.write_all(&response).unwrap();
    });
    (endpoint, task)
}

#[test]
fn lookup_rejects_formal_page_for_a_different_signed_read_without_advancing_cache() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let scope_world = commit_ref(&anchor, &projection(&world)).world;
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust.clone(),
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
        endpoint: "http://127.0.0.1:0".into(),
        trusted_service_public_key: public(9),
        expected_world: scope_world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(10),
        max_response_bytes: 4 * 1024 * 1024,
    };
    let original = delegation_request(scope_world);
    let (endpoint, task) = serve_valid_page_then_change_read_deadline(world, trust, anchor);
    let mut config = config;
    config.endpoint = endpoint;
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let before = client.formal_checkpoint().unwrap();
    let error = client.lookup_formal_original(original).unwrap_err();
    task.join().unwrap();
    assert!(matches!(
        error,
        WorldServiceClientError::Assurance(message)
            if message == "formal lookup response differs from the signed read request"
    ));
    assert_eq!(client.formal_checkpoint().unwrap(), before);
}

#[test]
fn lookup_preserves_signed_service_unavailable_reason() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let scope_world = commit_ref(&anchor, &projection(&world)).world;
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
        endpoint: String::new(),
        trusted_service_public_key: public(9),
        expected_world: scope_world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(10),
        max_response_bytes: 4 * 1024 * 1024,
    };
    let original = delegation_request(scope_world);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let (body_start, body_len) = loop {
            let mut chunk = [0; 1024];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= end + 4 + length {
                    break (end + 4, length);
                }
            }
        };
        let message: FormalAuthenticatedLookupV1 =
            serde_json::from_slice(&bytes[body_start..body_start + body_len]).unwrap();
        authority::verify_read_request(VIEW_PATH, &message.read).unwrap();
        let digest = authority::request_digest(LOOKUP_PATH, &message).unwrap();
        let signed = authority::sign_service_response(
            LOOKUP_PATH,
            digest,
            serde_json::json!({"message": "native business CAS read failed"}),
            &h(9),
        )
        .unwrap();
        let body = serde_json::to_vec(&signed).unwrap();
        write!(
            stream,
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
    });
    let mut config = config;
    config.endpoint = endpoint;
    let client = RemoteWorldServiceClient::new(config).unwrap();

    let error = client.lookup_formal_original(original).unwrap_err();
    task.join().unwrap();
    assert!(matches!(
        error,
        WorldServiceClientError::Rejected { status: 503, message }
            if message == "native business CAS read failed"
    ));
}

#[test]
fn fixed_original_lookup_signs_exact_selector_and_rejects_substitution_without_cache_advance() {
    for substitute in [false, true] {
        let (world, trust, anchor, _) = actual_world_initial_fixture();
        let fixed = commit_ref(&anchor, &projection(&world));
        let original = delegation_request(fixed.world.clone());
        let checkpoint = anchor.checkpoint();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = WorldServiceClientConfig {
            formal_read_trust: Some(FormalReadTrustConfigV1 {
                initial_trust: trust,
                checkpoint_digest: checkpoint.digest().unwrap(),
                checkpoint,
            }),
            endpoint: format!("http://{}", listener.local_addr().unwrap()),
            trusted_service_public_key: public(9),
            expected_world: fixed.world.clone(),
            scope_id: "public".into(),
            read_private_key_hex: h(7),
            timeout: std::time::Duration::from_secs(10),
            max_response_bytes: 4 * 1024 * 1024,
        };
        let expected_fixed = fixed.clone();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut bytes = Vec::new();
            let (start, len) = loop {
                let mut chunk = [0; 1024];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let len = head
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                    if bytes.len() >= end + 4 + len {
                        break (end + 4, len);
                    }
                }
            };
            let message: FormalAuthenticatedLookupV1 =
                serde_json::from_slice(&bytes[start..start + len]).unwrap();
            authority::verify_read_request(VIEW_PATH, &message.read).unwrap();
            assert_eq!(
                message.read.request.fixed_commit.as_ref(),
                Some(&expected_fixed)
            );
            let (status, payload) = if substitute {
                let correlation = crate::derive_correlation(
                    message.read.request.world.clone(),
                    &message.lookup.original,
                )
                .unwrap();
                let mut page =
                    progress_page(&world, &anchor, message.read.request.clone(), correlation);
                page.target_request
                    .fixed_commit
                    .as_mut()
                    .unwrap()
                    .state_root_ref = h(231);
                ("200 OK", serde_json::to_value(page).unwrap())
            } else {
                (
                    "503 Service Unavailable",
                    serde_json::json!({"message": "exact fixed selector observed"}),
                )
            };
            let signed = authority::sign_service_response(
                LOOKUP_PATH,
                authority::request_digest(LOOKUP_PATH, &message).unwrap(),
                payload,
                &h(9),
            )
            .unwrap();
            let body = serde_json::to_vec(&signed).unwrap();
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });
        let client = RemoteWorldServiceClient::new(config).unwrap();
        let before = client.formal_checkpoint().unwrap();
        let error = client
            .lookup_formal_original_at(original, fixed)
            .unwrap_err();
        task.join().unwrap();
        if substitute {
            assert!(
                matches!(error, WorldServiceClientError::Assurance(message) if message == "formal lookup response differs from the signed read request")
            );
        } else {
            assert!(
                matches!(error, WorldServiceClientError::Rejected { status: 503, message } if message == "exact fixed selector observed")
            );
        }
        assert_eq!(client.formal_checkpoint().unwrap(), before);
    }
}
