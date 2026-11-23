use super::*;

#[test]
fn formal_reader_uses_direct_http_policy() {
    crate::formal_http::tests::with_environment_proxy(
        "formal_read::tests::http_transport_tests::formal_reader_uses_direct_http_policy",
        "HTTP/1.1 400 Bad Request\r\n",
        |endpoint| {
            use crate::remote_world_service::{
                FormalReadTrustConfigV1, RemoteWorldServiceReader, WorldServiceClientConfig,
            };
            let (world, trust, anchor, _) = protocol_initial_fixture();
            let checkpoint = anchor.checkpoint();
            let (request, _) = response(
                &world,
                &anchor,
                FormalReadHistoryV1 {
                    schema_version: 1,
                    steps: Vec::new(),
                },
            );
            let client =
                RemoteWorldServiceReader::<WorldServiceProjection>::new(WorldServiceClientConfig {
                    formal_read_trust: Some(FormalReadTrustConfigV1 {
                        initial_trust: trust,
                        checkpoint_digest: checkpoint.digest().unwrap(),
                        checkpoint,
                    }),
                    endpoint,
                    trusted_service_public_key: public(9),
                    expected_world: request.world.clone(),
                    scope_id: "public".into(),
                    read_private_key_hex: h(7),
                    timeout: std::time::Duration::from_secs(10),
                    max_response_bytes: 1_048_576,
                })
                .unwrap();
            // A real formal read reaches the configured origin despite proxy
            // environment variables. The unsigned error is still rejected;
            // this transport fixture does not claim an accepted World proof.
            match client.read_view(request) {
                Err(crate::WorldServiceClientError::Rejected { status: 400, .. }) => {}
                Err(error) => panic!("expected direct-origin HTTP 400 refusal, got {error}"),
                Ok(_) => panic!("unsigned HTTP 400 was accepted"),
            }
        },
    );
}
