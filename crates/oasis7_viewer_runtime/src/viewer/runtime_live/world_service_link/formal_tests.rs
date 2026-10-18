use super::*;
#[path = "../../../../../oasis7/src/world_service/formal_read_fixtures.rs"]
mod fixtures;
use crate::world_service::{
    SignedReadRequest, authority, client::FormalReadTrustConfigV1,
    formal_projection::ProjectionAudienceV1, formal_read::*, projection::WorldServiceProjection,
};
use fixtures::*;
use oasis7_client_api::world_service::*;
use oasis7_distfs::controlled_authority::formal::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn response(
    world: &oasis7::runtime::World,
    anchor: &VerifiedFormalReadAnchorV1,
    history: FormalReadHistoryV1,
) -> (
    ReadWorldViewRequest,
    ReadWorldViewResponse<WorldServiceProjection>,
) {
    let view = projection(world);
    let commit = commit_ref(anchor, &view);
    let cache = FormalReadVerifier::new(
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
        anchor.clone(),
    )
    .unwrap();
    let anchor_ref = cache.anchor_ref().unwrap();
    let inclusion = crate::world_service::formal_projection::commitments_for_world(world)
        .unwrap()
        .inclusion(&ProjectionAudienceV1::public())
        .unwrap();
    let request = ReadWorldViewRequest {
        observation_nonce: Some("07".repeat(32)),
        formal_anchor: Some(anchor_ref.clone()),
        contract_version: 1,
        world: commit.world.clone(),
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let response = ReadWorldViewResponse {
        formal_profile: Some(FormalReadProfileV1::controlled()),
        formal_proof: Some(FormalReadSidecarV1 {
            schema_version: 1,
            profile: FormalReadProfileV1::controlled(),
            anchor: anchor_ref,
            history_cbor_hex: history.encode_hex().unwrap(),
            projection: FormalProjectionInclusionV1 {
                schema_version: inclusion.schema_version,
                audience: FormalProjectionAudienceV1 {
                    scope_id: inclusion.audience.scope_id,
                    reader_public_key: inclusion.audience.reader_public_key,
                },
                leaf_index: inclusion.leaf_index,
                leaf_count: inclusion.leaf_count,
                siblings: inclusion.siblings,
            },
        }),
        execution_evidence_scope: None,
        contract_version: 1,
        logical_tick: view.state.time,
        version: ProjectionVersion {
            commit: commit.clone(),
            projection_revision: authority::request_digest("projection", &view).unwrap(),
            visibility_scope: "public".into(),
        },
        continuation: EventCursor {
            stream_id: authority::request_digest("formal-events-v1", &(&commit.world, &"public"))
                .unwrap(),
            scope_id: "public".into(),
            era: view.formal_commit_header.as_ref().unwrap().event_id_era,
            sequence: view
                .formal_commit_header
                .as_ref()
                .unwrap()
                .journal_high_watermark,
            commit,
        },
        view,
    };
    (request, response)
}

#[test]
fn configured_formal_viewer_refresh_consumes_qualified_projection_without_changes() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let (_, reply) = response(
        &world,
        &anchor,
        FormalReadHistoryV1 {
            schema_version: 1,
            steps: vec![],
        },
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let (offset, length) = loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    assert!(
                        header.starts_with("post /v1/world/view "),
                        "unexpected path: {header}"
                    );
                    let length: usize = header
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length:"))
                        .unwrap()
                        .trim()
                        .parse()
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break (end + 4, length);
                    }
                }
            };
            let signed: SignedReadRequest<ReadWorldViewRequest> =
                serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
            authority::verify_read_request("/v1/world/view", &signed).unwrap();
            assert_eq!(
                signed.request.formal_anchor,
                reply
                    .formal_proof
                    .as_ref()
                    .map(|proof| proof.anchor.clone())
            );
            let digest = authority::request_digest("/v1/world/view", &signed).unwrap();
            let signed_reply =
                authority::sign_service_response("/v1/world/view", digest, reply.clone(), &h(9))
                    .unwrap();
            let body = serde_json::to_vec(&signed_reply).unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        }
    });
    let checkpoint = anchor.checkpoint();
    let config = WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
        endpoint,
        trusted_service_public_key: public(9),
        expected_world: commit_ref(&anchor, &projection(&world)).world,
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: Duration::from_secs(5),
        max_response_bytes: 16 * 1024 * 1024,
    };
    let first =
        prepare_world_service_update(config.clone(), None, vec![], Default::default()).unwrap();
    let first_view = first.verified_view.unwrap();
    assert!(first_view.formal_commit().is_some());
    assert_eq!(
        first_view.projection().state.time,
        world.snapshot().state.time
    );
    let second =
        prepare_world_service_update(config, Some(first_view), vec![], Default::default()).unwrap();
    assert!(second.verified_view.unwrap().formal_commit().is_some());
    assert!(second.service_events.unwrap().is_empty());
    server.join().unwrap();
}
