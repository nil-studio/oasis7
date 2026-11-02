//! Reuses the real formally qualified progress fixture from the owning service.
use super::*;

fn serve_one_verified_progress_page(
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
        let page = progress_page(
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
fn one_page_lookup_returns_verified_progress_after_exactly_one_http() {
    let (world, trust, anchor, _) = actual_world_initial_fixture();
    let scope_world = commit_ref(&anchor, &projection(&world)).world;
    let checkpoint = anchor.checkpoint();
    let original = delegation_request(scope_world.clone());
    // This server owns exactly one accepted HTTP connection and then drops
    // its listener. Any hidden continuation attempts produce an error.
    let (endpoint, task) = serve_one_verified_progress_page(world, trust.clone(), anchor);
    let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        formal_read_trust: Some(FormalReadTrustConfigV1 {
            initial_trust: trust,
            checkpoint_digest: checkpoint.digest().unwrap(),
            checkpoint,
        }),
        endpoint,
        trusted_service_public_key: public(9),
        expected_world: scope_world,
        scope_id: "public".into(),
        read_private_key_hex: h(7),
        timeout: std::time::Duration::from_secs(2),
        max_response_bytes: 4 * 1024 * 1024,
    })
    .unwrap();
    let before = client.formal_checkpoint().unwrap();
    let page = client.lookup_formal_original_page(original).unwrap();
    task.join().unwrap();
    assert!(matches!(
        page,
        oasis7_game_client::formal_business::FormalBusinessPageVerification::Progress(_)
    ));
    assert_ne!(client.formal_checkpoint().unwrap(), before);
}
