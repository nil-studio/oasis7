//! Existing Service profile consumer of the real formal Game Client transport.
use super::*;
use crate::client::WorldServicePort;
use oasis7_game_client::operation_lifecycle::*;

fn read_lookup(stream: &mut std::net::TcpStream) -> (String, FormalAuthenticatedLookupV1) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
            let len: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            if bytes.len() >= end + 4 + len {
                return (
                    head.lines().next().unwrap().into(),
                    serde_json::from_slice(&bytes[end + 4..end + 4 + len]).unwrap(),
                );
            }
        }
    }
}

#[test]
fn formal_query_only_consumer_keeps_original_signs_reads_and_projects_verified_unknown() {
    // Both supported public lookup consumers must stay on the formal route.
    for (generic, bad_proof) in [(false, false), (true, false), (false, true), (true, true)] {
        let (world, trust, anchor, _) = actual_world_initial_fixture();
        let scope_world = commit_ref(&anchor, &projection(&world)).world;
        let original = delegation_request(scope_world.clone());
        let original_bytes = serde_json::to_vec(&original).unwrap();
        let expected = original.clone();
        let checkpoint = anchor.checkpoint();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
            formal_read_trust: Some(FormalReadTrustConfigV1 {
                initial_trust: trust.clone(),
                checkpoint_digest: checkpoint.digest().unwrap(),
                checkpoint,
            }),
            endpoint: format!("http://{}", listener.local_addr().unwrap()),
            trusted_service_public_key: public(9),
            expected_world: scope_world,
            scope_id: "public".into(),
            read_private_key_hex: h(7),
            timeout: std::time::Duration::from_secs(10),
            max_response_bytes: 4 * 1024 * 1024,
        })
        .unwrap();
        let before = client.formal_checkpoint().unwrap();
        let task = thread::spawn(move || {
            let mut anchor = anchor;
            let mut verifier = FormalReadVerifier::new(&trust, anchor.clone()).unwrap();
            let mut submits = 0;
            let mut lookups = 0;
            for _ in 0..if bad_proof { 1 } else { 8 } {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(connection) => break connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "missing formal lookup page"
                            );
                            thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(error) => panic!("formal fixture accept: {error}"),
                    }
                };
                let (line, message) = read_lookup(&mut stream);
                submits += usize::from(line.starts_with("post /v1/world/submit "));
                assert_eq!(line, format!("post {LOOKUP_PATH} http/1.1"));
                lookups += 1;
                authority::verify_read_request(VIEW_PATH, &message.read).unwrap();
                assert_eq!(message.read.subject_public_key, public(7));
                assert_eq!(message.lookup.original, expected.signed_payload);
                assert_eq!(message.lookup.request.key, expected.correlation.key);
                assert_eq!(
                    crate::derive_correlation(
                        message.read.request.world.clone(),
                        &message.lookup.original
                    )
                    .unwrap(),
                    expected.correlation
                );
                let mut page = progress_page(
                    &world,
                    &anchor,
                    message.read.request.clone(),
                    expected.correlation.clone(),
                );
                let FormalBusinessPageVerification::Progress(verified) =
                    verify_formal_business_outcome_page(
                        &mut verifier,
                        &page,
                        &expected,
                        &message.read.subject_public_key,
                    )
                    .unwrap()
                else {
                    panic!("fixture must be a verified progress page")
                };
                anchor = verified.read_anchor().clone();
                if bad_proof {
                    let proof = page.target_view.formal_proof.as_mut().unwrap();
                    let mut history = hex::decode(&proof.history_cbor_hex).unwrap();
                    *history.last_mut().unwrap() ^= 1;
                    proof.history_cbor_hex = hex::encode(history);
                }
                let signed = authority::sign_service_response(
                    LOOKUP_PATH,
                    authority::request_digest(LOOKUP_PATH, &message).unwrap(),
                    serde_json::to_value(page).unwrap(),
                    &h(9),
                )
                .unwrap();
                let body = serde_json::to_vec(&signed).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            (lookups, submits)
        });
        let result = if generic {
            client.lookup(
                LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
        } else {
            client.lookup_formal_original(original.clone())
        };
        let counts = task.join().unwrap();
        assert_eq!(counts, (if bad_proof { 1 } else { 8 }, 0));
        assert_eq!(serde_json::to_vec(&original).unwrap(), original_bytes);
        if bad_proof {
            assert!(matches!(result, Err(WorldServiceClientError::Assurance(_))));
            assert_eq!(client.formal_checkpoint().unwrap(), before);
        } else {
            let response = result.unwrap();
            assert_eq!(response.correlation, original.correlation);
            assert!(matches!(response.outcome, IntentOutcome::Unknown));
            assert_eq!(
                response.formal_profile,
                Some(FormalReadProfileV1::controlled())
            );
            assert_ne!(client.formal_checkpoint().unwrap(), before);
        }
        // Transport/proof failure is never a verified rejection or permission
        // to submit. Eight verified pages without a result remain Unknown.
        let input = RecoveryInput {
            intent: RecoveryIntent::QueryOnly,
            policy: OperationPolicy::QueryOnly,
            phase: DeliveryPhase::Unknown,
            disposition: DomainDisposition::Unknown,
            proof_verified: !bad_proof,
            original_present: true,
            scope_current: true,
            read_allowed: true,
            write_allowed: true,
            stopped: false,
        };
        assert_eq!(next_action(input), RecoveryAction::QueryOriginal);
    }
}
