use super::*;
use oasis7_distfs::controlled_authority::formal::FormalWorldIdentityV1;
use oasis7_runtime::{EffectIntent, EffectOrigin};
use serde_cbor::Value as CborValue;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct PrivateTestRoot(PathBuf);

impl PrivateTestRoot {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let parent = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary directory");
        let path = parent.join(format!(
            "oasis7-effect-transport-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create private test root");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("set private test root mode");
        Self(path)
    }

    fn store_path(&self) -> PathBuf {
        self.0.join("state")
    }
}

impl Drop for PrivateTestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn route(
    address: SocketAddr,
    sender: &SigningKey,
    receiver: &SigningKey,
) -> FormalEffectReceiverContractV1 {
    let mut senders = vec![hex::encode(sender.verifying_key().to_bytes())];
    senders.sort();
    FormalEffectReceiverContractV1 {
        schema_version: 1,
        target_id: "local.receiver".into(),
        receiver_id: "receiver-one".into(),
        address,
        authorized_sender_public_keys_hex: senders,
        receiver_public_key_hex: hex::encode(receiver.verifying_key().to_bytes()),
        idempotent_post: true,
        complete_signed_query: true,
        signed_rejection_proves_no_effect: true,
        timeout_ms: 2_000,
    }
}

fn obligation(contract: &FormalEffectReceiverContractV1) -> QualifiedEffectObligationV1 {
    let contract_digest = contract.canonical_digest().unwrap();
    let intent = EffectIntent {
        birth_era: Some(4),
        intent_id: "effect-message-4".into(),
        kind: "send_message".into(),
        params: serde_json::json!({
            "schema_version": 1,
            "target_id": contract.target_id,
            "receiver_contract_digest": contract_digest,
            "payload": {"text": "hello receiver"}
        }),
        cap_ref: "cap-message-4".into(),
        origin: EffectOrigin::System,
    };
    super::super::formal_effect_outbox::test_only_qualified_obligation(
        FormalWorldIdentityV1 {
            world_id: "transport-world".into(),
            chain_id: "transport-chain".into(),
            genesis_digest: "a".repeat(64),
        },
        super::super::formal_effect_outbox::QualifiedEffectCommitRefV1 {
            position: 9,
            record_id: "b".repeat(64),
            active_authority_id: "transport-authority".into(),
            finalizing_authority_id: "transport-authority".into(),
        },
        intent,
    )
    .expect("transport protocol fixture only; not a formal qualification test")
}

fn repeated_at_later_commit(
    effect: &QualifiedEffectObligationV1,
    position: u64,
) -> QualifiedEffectObligationV1 {
    super::super::formal_effect_outbox::test_only_qualified_obligation(
        effect.identity.world.clone(),
        super::super::formal_effect_outbox::QualifiedEffectCommitRefV1 {
            position,
            record_id: "d".repeat(64),
            active_authority_id: "transport-authority".into(),
            finalizing_authority_id: "transport-authority".into(),
        },
        effect.intent.clone(),
    )
    .expect("unit fixture for a repeated qualified effect")
}

#[test]
fn formal_effect_receiver_contract_digest_binds_loopback_endpoint_and_preapproved_sender_set() {
    let sender_a = SigningKey::from_bytes(&[31; 32]);
    let sender_b = SigningKey::from_bytes(&[32; 32]);
    let receiver = SigningKey::from_bytes(&[33; 32]);
    let address: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let mut one = route(address, &sender_a, &receiver);
    let digest_one = one.canonical_digest().unwrap();
    let mut two = one.clone();
    two.authorized_sender_public_keys_hex
        .push(hex::encode(sender_b.verifying_key().to_bytes()));
    two.authorized_sender_public_keys_hex.sort();
    assert_ne!(digest_one, two.canonical_digest().unwrap());

    one.address = "0.0.0.0:12345".parse().unwrap();
    assert!(one.validate().is_err());
    two.authorized_sender_public_keys_hex.reverse();
    assert!(two.validate().is_err());
}

#[test]
fn formal_effect_localhost_fixture_reconciles_lost_reply_and_preserves_original_receipt() {
    let sender = SigningKey::from_bytes(&[37; 32]);
    let mut receiver = super::test_support::LocalEffectReceiverFixture::start(
        vec![hex::encode(sender.verifying_key().to_bytes())],
        true,
        true,
    )
    .unwrap();
    let contract = receiver.contract().clone();
    let effect = obligation(&contract);
    let later_effect = repeated_at_later_commit(&effect, 10);
    let root = PrivateTestRoot::new();
    let store = Arc::new(FormalEffectDeliveryStore::open(&root.store_path()).unwrap());
    let transport =
        FormalEffectDeliveryTransport::new(vec![contract], sender.clone(), store.clone()).unwrap();

    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    assert!(matches!(
        transport.drive_qualified_obligation(&later_effect),
        EffectDeliveryDriveResultV1::Delivered(EffectDeliveryReceiptV1 { .. })
    ));
    assert!(receiver.wait_for_execution(Instant::now() + Duration::from_secs(1)));
    assert_eq!(receiver.executions(), 1);
    assert_eq!(receiver.posts(), 1);
    assert!(receiver.wait_for_queries(2, Instant::now() + Duration::from_secs(1)));
    let observations = receiver.observations();
    assert_eq!(observations.len(), 3);
    assert_eq!(observations[0].operation, "query");
    assert_eq!(observations[1].operation, "deliver");
    assert_eq!(observations[2].operation, "query");
    assert_eq!(observations[1].committed_at.as_ref().unwrap().position, 9);
    assert!(observations[2].committed_at.is_none());
    assert!(
        observations
            .iter()
            .all(|observation| observation.identity == effect.identity)
    );
    assert_eq!(
        observations[0].stable_effect_digest,
        observations[2].stable_effect_digest
    );
    assert_eq!(
        observations[2].sender_public_key_hex,
        hex::encode(sender.verifying_key().to_bytes())
    );
    assert!(receiver.last_error().is_none());
    receiver.stop().unwrap();
}

#[test]
fn formal_effect_terminal_state_requires_receiver_signed_matching_outcome() {
    let sender = SigningKey::from_bytes(&[38; 32]);
    let mut receiver = super::test_support::LocalEffectReceiverFixture::start(
        vec![hex::encode(sender.verifying_key().to_bytes())],
        false,
        true,
    )
    .unwrap();
    let contract = receiver.contract().clone();
    let contract_digest = contract.canonical_digest().unwrap();
    let effect = obligation(&contract);
    let stable_digest = stable_effect_request_digest(&effect, &contract_digest).unwrap();
    let root = PrivateTestRoot::new();
    let store = Arc::new(FormalEffectDeliveryStore::open(&root.store_path()).unwrap());
    let transport =
        FormalEffectDeliveryTransport::new(vec![contract.clone()], sender, store.clone()).unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Delivered(EffectDeliveryReceiptV1 { .. })
    ));

    store
        .replace_terminal_response_for_test(&effect.identity, b"damaged response".to_vec())
        .unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));

    let signed_not_found = sign_test_response(
        &SigningKey::from_bytes(&[0xC1; 32]),
        EffectResponseBodyV1 {
            schema_version: 1,
            receiver_id: contract.receiver_id.clone(),
            contract_digest,
            operation: "query".into(),
            response_to_digest: "e".repeat(64),
            effect_request_digest: stable_digest.clone(),
            identity: effect.identity.clone(),
            outcome: EffectResponseOutcomeV1::NotFound { complete: true },
        },
    );
    assert!(
        verify_response(
            &contract,
            &signed_not_found,
            &effect.identity,
            "query",
            &"e".repeat(64),
            &stable_digest,
        )
        .is_ok()
    );
    store
        .replace_terminal_response_for_test(
            &effect.identity,
            serde_cbor::to_vec(&signed_not_found).unwrap(),
        )
        .unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    assert_eq!(receiver.executions(), 1);
    assert_eq!(receiver.posts(), 1);
    receiver.stop().unwrap();
}

#[test]
fn formal_effect_retry_revalidates_persisted_not_found_against_exact_query() {
    let sender = SigningKey::from_bytes(&[47; 32]);
    let mut receiver = super::test_support::LocalEffectReceiverFixture::start(
        vec![hex::encode(sender.verifying_key().to_bytes())],
        false,
        true,
    )
    .unwrap();
    let contract = receiver.contract().clone();
    let contract_digest = contract.canonical_digest().unwrap();
    let effect = obligation(&contract);
    let stable_digest = stable_effect_request_digest(&effect, &contract_digest).unwrap();
    let delivery_key = delivery_identity_key(&effect.identity).unwrap();
    let query = sign_request(
        EffectRequestBodyV1 {
            schema_version: 1,
            receiver_id: contract.receiver_id.clone(),
            contract_digest: contract_digest.clone(),
            operation: "query".into(),
            identity: effect.identity.clone(),
            target_id: effect.target_id.clone(),
            delivery_key,
            payload_digest: effect.payload_digest.clone(),
            effect_request_digest: stable_digest.clone(),
            intent: None,
            committed_at: None,
        },
        &sender,
    )
    .unwrap();
    let exact_query_digest = request_digest(&query.body, &query.sender_public_key_hex).unwrap();
    let root = PrivateTestRoot::new();
    let store = Arc::new(FormalEffectDeliveryStore::open(&root.store_path()).unwrap());
    store.observe(&effect).unwrap();
    store
        .begin_query(&effect, &exact_query_digest, &stable_digest)
        .unwrap();
    let mismatched_not_found = sign_test_response(
        &SigningKey::from_bytes(&[0xC1; 32]),
        EffectResponseBodyV1 {
            schema_version: 1,
            receiver_id: contract.receiver_id.clone(),
            contract_digest,
            operation: "query".into(),
            response_to_digest: "f".repeat(64),
            effect_request_digest: stable_digest,
            identity: effect.identity.clone(),
            outcome: EffectResponseOutcomeV1::NotFound { complete: true },
        },
    );
    assert!(
        verify_response(
            &contract,
            &mismatched_not_found,
            &effect.identity,
            "query",
            &"f".repeat(64),
            &stable_effect_request_digest(&effect, &contract.canonical_digest().unwrap()).unwrap(),
        )
        .is_ok()
    );
    store
        .authorize_retry(&effect, serde_cbor::to_vec(&mismatched_not_found).unwrap())
        .unwrap();

    let transport = FormalEffectDeliveryTransport::new(vec![contract], sender, store).unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    assert_eq!(receiver.posts(), 0);
    assert_eq!(receiver.queries(), 0);
    assert_eq!(receiver.executions(), 0);
    receiver.stop().unwrap();
}

#[test]
fn formal_effect_slow_fragmented_response_retains_unknown_delivery_state() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sender = SigningKey::from_bytes(&[48; 32]);
    let receiver = SigningKey::from_bytes(&[49; 32]);
    let mut contract = route(address, &sender, &receiver);
    contract.timeout_ms = 170;
    let effect = obligation(&contract);
    let server_contract = contract.clone();
    let server_receiver = receiver.clone();
    let server = thread::spawn(move || {
        let mut stream = accept_deadline_test_stream(&listener);
        let request_bytes = read_test_http_request(&mut stream).unwrap();
        let request: SignedEffectRequestV1 = serde_cbor::from_slice(&request_bytes).unwrap();
        let rpc_digest = request_digest(&request.body, &request.sender_public_key_hex).unwrap();
        let response = sign_test_response(
            &server_receiver,
            EffectResponseBodyV1 {
                schema_version: 1,
                receiver_id: server_contract.receiver_id.clone(),
                contract_digest: server_contract.canonical_digest().unwrap(),
                operation: "query".into(),
                response_to_digest: rpc_digest,
                effect_request_digest: request.body.effect_request_digest,
                identity: request.body.identity,
                outcome: EffectResponseOutcomeV1::NotFound { complete: true },
            },
        );
        let bytes = serde_cbor::to_vec(&response).unwrap();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        for byte in bytes.iter().take(3) {
            thread::sleep(Duration::from_millis(90));
            if stream.write_all(std::slice::from_ref(byte)).is_err() {
                break;
            }
        }
    });
    let root = PrivateTestRoot::new();
    let store = Arc::new(FormalEffectDeliveryStore::open(&root.store_path()).unwrap());
    let transport =
        FormalEffectDeliveryTransport::new(vec![contract], sender, store.clone()).unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    // The durable drive includes store/fsync work outside the HTTP deadline.
    // Its safety contract is Unknown with the original identity retained.
    assert!(matches!(
        store.read(&effect.identity).unwrap().unwrap().phase,
        DeliveryPhaseV1::Unknown { .. }
    ));
    server.join().unwrap();
}

fn accept_deadline_test_stream(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "deadline-test receiver never accepted a request"
                );
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("deadline-test accept failed: {error}"),
        }
    }
}

#[test]
fn formal_effect_http_slow_fragmented_response_obeys_one_absolute_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let sender = SigningKey::from_bytes(&[48; 32]);
    let receiver = SigningKey::from_bytes(&[49; 32]);
    let mut contract = route(listener.local_addr().unwrap(), &sender, &receiver);
    contract.timeout_ms = 170;
    let body = b"actual bounded HTTP request";
    let server = thread::spawn(move || {
        let mut stream = accept_deadline_test_stream(&listener);
        let received =
            read_test_http_request(&mut stream).expect("receive complete actual request");
        assert_eq!(received, body);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut fragments_written = 0;
        for byte in b"abc" {
            thread::sleep(Duration::from_millis(90));
            // A correctly timed-out client may have closed before a late write.
            if stream.write_all(std::slice::from_ref(byte)).is_err() {
                break;
            }
            fragments_written += 1;
        }
        // Never send the fourth byte: resetting 170 ms on each fragment would
        // reach this EOF around 270 ms instead of the absolute deadline.
        (received, fragments_written)
    });
    let started = Instant::now();
    let result = http_post_bytes(&contract, "/v1/effects/query", body);
    let elapsed = started.elapsed();
    let (received, fragments_written) = server.join().expect("bounded HTTP receiver finished");
    assert_eq!(
        received, body,
        "deadline regression must reach the actual receiver"
    );
    assert!(
        fragments_written > 0,
        "receiver must exercise a fragmented response"
    );
    let error = result.expect_err("incomplete fragmented response cannot succeed");
    let lower = error.to_ascii_lowercase();
    assert!(
        lower.contains("operation deadline expired while reading")
            || (lower.contains("read effect http response body failed")
                && (lower.contains("timed out")
                    || lower.contains("temporarily unavailable")
                    || lower.contains("operation would block"))),
        "expected an actual read deadline/timeout, never EOF or disconnect: {error}; elapsed={elapsed:?}; fragments={fragments_written}"
    );
}

#[test]
fn formal_effect_localhost_lost_reply_fresh_store_and_successor_queries_receipt_without_duplicate_effect()
 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sender = SigningKey::from_bytes(&[41; 32]);
    let successor = SigningKey::from_bytes(&[43; 32]);
    let receiver = SigningKey::from_bytes(&[42; 32]);
    let mut contract = route(address, &sender, &receiver);
    contract
        .authorized_sender_public_keys_hex
        .push(hex::encode(successor.verifying_key().to_bytes()));
    contract.authorized_sender_public_keys_hex.sort();
    let effect = obligation(&contract);
    let later_effect = repeated_at_later_commit(&effect, 10);
    let key = delivery_identity_key(&effect.identity).unwrap();
    let contract_digest = contract.canonical_digest().unwrap();
    let stable_digest = stable_effect_request_digest(&effect, &contract_digest).unwrap();
    let store_path = PrivateTestRoot::new();
    let store_directory = store_path.store_path();
    let deliveries = Arc::new(AtomicUsize::new(0));
    let queries = Arc::new(AtomicUsize::new(0));
    let attempt_preceded_post = Arc::new(AtomicBool::new(false));
    let server_deliveries = deliveries.clone();
    let server_queries = queries.clone();
    let server_attempt_preceded_post = attempt_preceded_post.clone();
    let server_receiver_key = receiver.clone();
    let server_contract = contract.clone();
    let server_store_directory = store_directory.clone();
    let server_stable_digest = stable_digest.clone();

    let server = thread::spawn(move || {
        let mut receiver_ledger: HashMap<String, (String, JsonValue)> = HashMap::new();
        let mut first_delivery_rpc_digest = None;
        for request_index in 0..3 {
            let (mut stream, _) = listener.accept().expect("accept effect request");
            let request_bytes = read_test_http_request(&mut stream).expect("read request");
            let request: SignedEffectRequestV1 =
                serde_cbor::from_slice(&request_bytes).expect("decode signed request");
            verify_test_request(&request, &server_contract, &contract_digest);
            let body = &request.body;
            assert_eq!(body.effect_request_digest, server_stable_digest);
            if request_index == 0 {
                assert_eq!(body.operation, "query");
                server_queries.fetch_add(1, Ordering::SeqCst);
                let query_digest = request_digest(body, &request.sender_public_key_hex).unwrap();
                let signed = sign_test_response(
                    &server_receiver_key,
                    EffectResponseBodyV1 {
                        schema_version: 1,
                        receiver_id: server_contract.receiver_id.clone(),
                        contract_digest: contract_digest.clone(),
                        operation: "query".into(),
                        response_to_digest: query_digest,
                        effect_request_digest: body.effect_request_digest.clone(),
                        identity: body.identity.clone(),
                        outcome: EffectResponseOutcomeV1::NotFound { complete: true },
                    },
                );
                write_test_http_response(&mut stream, &signed);
                continue;
            }

            if request_index == 1 {
                assert_eq!(body.operation, "deliver");
                assert_eq!(body.delivery_key, key);
                first_delivery_rpc_digest =
                    Some(request_digest(body, &request.sender_public_key_hex).unwrap());
                let bytes =
                    fs::read(server_store_directory.join(format!("{}.cbor", body.delivery_key)))
                        .expect("AttemptStarted is durable before receiver accepts POST");
                server_attempt_preceded_post.store(
                    cbor_record_phase_is(&bytes, "attempt_started"),
                    Ordering::SeqCst,
                );
                let receipt = serde_json::json!({"receiver_receipt_id":"receipt-once"});
                let inserted = receiver_ledger
                    .entry(body.delivery_key.clone())
                    .or_insert_with(|| {
                        server_deliveries.fetch_add(1, Ordering::SeqCst);
                        (body.effect_request_digest.clone(), receipt.clone())
                    });
                assert_eq!(inserted.0, body.effect_request_digest);
                // The receiver completed the effect but the response is lost.
                // Closing this real TCP connection forces client uncertainty.
                drop(stream);
                continue;
            }

            assert_eq!(body.operation, "query");
            server_queries.fetch_add(1, Ordering::SeqCst);
            assert!(body.committed_at.is_none());
            assert_ne!(
                request_digest(body, &request.sender_public_key_hex).unwrap(),
                first_delivery_rpc_digest
                    .as_deref()
                    .expect("first delivery was observed")
            );
            let outcome = match receiver_ledger.get(&body.delivery_key) {
                Some((digest, receipt)) => {
                    assert_eq!(digest, &body.effect_request_digest);
                    EffectResponseOutcomeV1::Delivered {
                        receipt_payload: receipt.clone(),
                    }
                }
                None => EffectResponseOutcomeV1::NotFound { complete: true },
            };
            let query_digest = request_digest(body, &request.sender_public_key_hex).unwrap();
            let signed = sign_test_response(
                &server_receiver_key,
                EffectResponseBodyV1 {
                    schema_version: 1,
                    receiver_id: server_contract.receiver_id.clone(),
                    contract_digest: contract_digest.clone(),
                    operation: "query".into(),
                    response_to_digest: query_digest,
                    effect_request_digest: body.effect_request_digest.clone(),
                    identity: body.identity.clone(),
                    outcome,
                },
            );
            write_test_http_response(&mut stream, &signed);
        }
    });

    let first_store = Arc::new(FormalEffectDeliveryStore::open(&store_directory).unwrap());
    let first_transport = FormalEffectDeliveryTransport::new(
        vec![contract.clone()],
        sender.clone(),
        first_store.clone(),
    )
    .unwrap();
    assert!(matches!(
        first_transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    drop(first_transport);
    drop(first_store);

    let successor_store_path = PrivateTestRoot::new();
    let reopened_store =
        Arc::new(FormalEffectDeliveryStore::open(&successor_store_path.store_path()).unwrap());
    let reopened_transport =
        FormalEffectDeliveryTransport::new(vec![contract], successor, reopened_store.clone())
            .unwrap();
    assert!(matches!(
        reopened_transport.drive_qualified_obligation(&later_effect),
        EffectDeliveryDriveResultV1::Delivered(EffectDeliveryReceiptV1 { .. })
    ));
    drop(reopened_transport);
    drop(reopened_store);
    server.join().unwrap();

    assert_eq!(deliveries.load(Ordering::SeqCst), 1);
    assert_eq!(queries.load(Ordering::SeqCst), 2);
    assert!(attempt_preceded_post.load(Ordering::SeqCst));
}

#[test]
fn formal_effect_signed_complete_not_found_authorizes_only_same_identity_idempotent_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let sender = SigningKey::from_bytes(&[44; 32]);
    let receiver = SigningKey::from_bytes(&[45; 32]);
    let contract = route(address, &sender, &receiver);
    let effect = obligation(&contract);
    let later_effect = repeated_at_later_commit(&effect, 10);
    let expected_identity = effect.identity.clone();
    let contract_digest = contract.canonical_digest().unwrap();
    let stable_digest = stable_effect_request_digest(&effect, &contract_digest).unwrap();
    let side_effects = Arc::new(AtomicUsize::new(0));
    let post_count = Arc::new(AtomicUsize::new(0));
    let query_count = Arc::new(AtomicUsize::new(0));
    let server_effects = side_effects.clone();
    let server_posts = post_count.clone();
    let server_queries = query_count.clone();
    let server_contract = contract.clone();
    let server_stable_digest = stable_digest.clone();
    let server_receiver = receiver.clone();
    let server = thread::spawn(move || {
        let mut completed: Option<(String, JsonValue)> = None;
        let mut first_post_rpc_digest = None;
        for step in 0..4 {
            let (mut stream, _) = listener.accept().expect("accept retry protocol request");
            let bytes = read_test_http_request(&mut stream).unwrap();
            let request: SignedEffectRequestV1 = serde_cbor::from_slice(&bytes).unwrap();
            verify_test_request(&request, &server_contract, &contract_digest);
            let body = &request.body;
            assert_eq!(body.effect_request_digest, server_stable_digest);
            if step == 0 || step == 2 {
                assert_eq!(body.operation, "query");
                server_queries.fetch_add(1, Ordering::SeqCst);
                assert!(completed.is_none());
                let query_digest = request_digest(body, &request.sender_public_key_hex).unwrap();
                let response = sign_test_response(
                    &server_receiver,
                    EffectResponseBodyV1 {
                        schema_version: 1,
                        receiver_id: server_contract.receiver_id.clone(),
                        contract_digest: contract_digest.clone(),
                        operation: "query".into(),
                        response_to_digest: query_digest,
                        effect_request_digest: body.effect_request_digest.clone(),
                        identity: body.identity.clone(),
                        outcome: EffectResponseOutcomeV1::NotFound { complete: true },
                    },
                );
                write_test_http_response(&mut stream, &response);
                continue;
            }

            assert_eq!(body.operation, "deliver");
            server_posts.fetch_add(1, Ordering::SeqCst);
            if step == 1 {
                first_post_rpc_digest =
                    Some(request_digest(body, &request.sender_public_key_hex).unwrap());
                drop(stream); // No effect occurred and the response is lost.
                continue;
            }
            assert_eq!(
                body.delivery_key,
                delivery_identity_key(&expected_identity).unwrap()
            );
            let rpc_digest = request_digest(body, &request.sender_public_key_hex).unwrap();
            assert!(valid_digest(&rpc_digest));
            assert_ne!(
                rpc_digest,
                first_post_rpc_digest
                    .as_deref()
                    .expect("first delivery was observed")
            );
            let receipt = serde_json::json!({"receiver_receipt_id":"retry-receipt"});
            if let Some((prior_digest, prior_receipt)) = &completed {
                assert_eq!(prior_digest, &body.effect_request_digest);
                assert_eq!(prior_receipt, &receipt);
            } else {
                server_effects.fetch_add(1, Ordering::SeqCst);
                completed = Some((body.effect_request_digest.clone(), receipt.clone()));
            }
            let response = sign_test_response(
                &server_receiver,
                EffectResponseBodyV1 {
                    schema_version: 1,
                    receiver_id: server_contract.receiver_id.clone(),
                    contract_digest: contract_digest.clone(),
                    operation: "deliver".into(),
                    response_to_digest: rpc_digest,
                    effect_request_digest: body.effect_request_digest.clone(),
                    identity: body.identity.clone(),
                    outcome: EffectResponseOutcomeV1::Delivered {
                        receipt_payload: receipt,
                    },
                },
            );
            write_test_http_response(&mut stream, &response);
        }
    });

    let private_root = PrivateTestRoot::new();
    let store_path = private_root.store_path();
    let store = Arc::new(FormalEffectDeliveryStore::open(&store_path).unwrap());
    let first =
        FormalEffectDeliveryTransport::new(vec![contract.clone()], sender.clone(), store.clone())
            .unwrap();
    assert!(matches!(
        first.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    drop(first);
    drop(store);

    let reopened = Arc::new(FormalEffectDeliveryStore::open(&store_path).unwrap());
    let retry =
        FormalEffectDeliveryTransport::new(vec![contract], sender, reopened.clone()).unwrap();
    assert!(matches!(
        retry.drive_qualified_obligation(&later_effect),
        EffectDeliveryDriveResultV1::Delivered(EffectDeliveryReceiptV1 { .. })
    ));
    drop(retry);
    drop(reopened);
    server.join().unwrap();
    assert_eq!(side_effects.load(Ordering::SeqCst), 1);
    assert_eq!(post_count.load(Ordering::SeqCst), 2);
    assert_eq!(query_count.load(Ordering::SeqCst), 2);
}

#[test]
fn formal_effect_non_idempotent_contract_is_blocked_before_first_post() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut contract = route(
        listener.local_addr().unwrap(),
        &SigningKey::from_bytes(&[51; 32]),
        &SigningKey::from_bytes(&[52; 32]),
    );
    contract.idempotent_post = false;
    let sender = SigningKey::from_bytes(&[51; 32]);
    let effect = obligation(&contract);
    let private_root = PrivateTestRoot::new();
    let store = Arc::new(FormalEffectDeliveryStore::open(&private_root.store_path()).unwrap());
    let transport =
        FormalEffectDeliveryTransport::new(vec![contract], sender, store.clone()).unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Blocked { .. }
    ));
    assert!(store.read(&effect.identity).unwrap().unwrap().phase == DeliveryPhaseV1::Ready);
    assert!(matches!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

#[test]
fn formal_effect_signed_not_found_does_not_retry_without_idempotent_post_guarantee() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let probe_listener = listener.try_clone().unwrap();
    let address = listener.local_addr().unwrap();
    let sender = SigningKey::from_bytes(&[61; 32]);
    let receiver = SigningKey::from_bytes(&[62; 32]);
    let mut contract = route(address, &sender, &receiver);
    contract.idempotent_post = false;
    let contract_digest = contract.canonical_digest().unwrap();
    let effect = obligation(&contract);
    let server_receiver = receiver.clone();
    let server_contract = contract.clone();
    let server_contract_digest = contract_digest.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept recovery query");
        let bytes = read_test_http_request(&mut stream).unwrap();
        let request: SignedEffectRequestV1 = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(request.body.operation, "query");
        verify_test_request(&request, &server_contract, &server_contract_digest);
        let query_digest = request_digest(&request.body, &request.sender_public_key_hex).unwrap();
        let response = sign_test_response(
            &server_receiver,
            EffectResponseBodyV1 {
                schema_version: 1,
                receiver_id: server_contract.receiver_id.clone(),
                contract_digest: server_contract_digest,
                operation: "query".into(),
                response_to_digest: query_digest,
                effect_request_digest: request.body.effect_request_digest.clone(),
                identity: request.body.identity.clone(),
                outcome: EffectResponseOutcomeV1::NotFound { complete: true },
            },
        );
        write_test_http_response(&mut stream, &response);
    });

    let private_root = PrivateTestRoot::new();
    let store = Arc::new(FormalEffectDeliveryStore::open(&private_root.store_path()).unwrap());
    store.observe(&effect).unwrap();
    let delivery = build_effect_request(&effect, &contract, &contract_digest, &sender).unwrap();
    let effect_request_digest = delivery.body.effect_request_digest.clone();
    store
        .begin_attempt(&effect, &effect_request_digest)
        .unwrap();
    let transport =
        FormalEffectDeliveryTransport::new(vec![contract], sender, store.clone()).unwrap();
    assert!(matches!(
        transport.drive_qualified_obligation(&effect),
        EffectDeliveryDriveResultV1::Unknown { .. }
    ));
    assert!(matches!(
        store.read(&effect.identity).unwrap().unwrap().phase,
        DeliveryPhaseV1::Unknown { .. }
    ));
    server.join().unwrap();
    probe_listener.set_nonblocking(true).unwrap();
    assert!(matches!(
        probe_listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

fn verify_test_request(
    request: &SignedEffectRequestV1,
    contract: &FormalEffectReceiverContractV1,
    contract_digest: &str,
) {
    assert!(
        contract
            .authorized_sender_public_keys_hex
            .contains(&request.sender_public_key_hex)
    );
    assert_eq!(request.body.receiver_id, contract.receiver_id);
    assert_eq!(request.body.contract_digest, contract_digest);
    assert_eq!(
        request.body.delivery_key,
        delivery_identity_key(&request.body.identity).unwrap()
    );
    if request.body.operation == "deliver" {
        assert_eq!(
            request.body.effect_request_digest,
            stable_effect_request_digest_fields(
                &request.body.identity,
                request.body.intent.as_ref().unwrap(),
                &request.body.payload_digest,
                &request.body.target_id,
                contract_digest,
            )
            .unwrap()
        );
    } else {
        assert_eq!(request.body.operation, "query");
        assert!(valid_digest(&request.body.effect_request_digest));
        assert!(valid_digest(
            &request_digest(&request.body, &request.sender_public_key_hex).unwrap()
        ));
    }
    let signature = decode_signature(&request.signature_hex).unwrap();
    let bytes = serde_cbor::to_vec(&(REQUEST_DOMAIN, &request.body)).unwrap();
    decode_verifying_key(&request.sender_public_key_hex)
        .unwrap()
        .verify_strict(&bytes, &signature)
        .unwrap();
}

fn sign_test_response(receiver: &SigningKey, body: EffectResponseBodyV1) -> SignedEffectResponseV1 {
    let bytes = serde_cbor::to_vec(&(RESPONSE_DOMAIN, &body)).unwrap();
    SignedEffectResponseV1 {
        body,
        signature_hex: hex::encode(receiver.sign(&bytes).to_bytes()),
    }
}

fn read_test_http_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    const MAX_TEST_HEADER: usize = 16 * 1024;
    let mut header = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if header.len() >= MAX_TEST_HEADER {
            return Err("test receiver request header too large".into());
        }
        stream
            .read_exact(&mut byte)
            .map_err(|error| error.to_string())?;
        header.push(byte[0]);
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&header).map_err(|error| error.to_string())?;
    let length = text
        .split("\r\n")
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or("test receiver Content-Length missing")?;
    if length > MAX_HTTP_BODY_BYTES {
        return Err("test receiver body exceeds bound".into());
    }
    let mut body = vec![0; length];
    stream
        .read_exact(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(body)
}

fn write_test_http_response(stream: &mut TcpStream, response: &SignedEffectResponseV1) {
    let body = serde_cbor::to_vec(response).unwrap();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/cbor\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
    stream.flush().unwrap();
}

fn cbor_record_phase_is(bytes: &[u8], expected: &str) -> bool {
    let Ok(CborValue::Map(record)) = serde_cbor::from_slice::<CborValue>(bytes) else {
        return false;
    };
    let Some(CborValue::Map(phase)) = record.get(&CborValue::Text("phase".into())) else {
        return false;
    };
    phase.get(&CborValue::Text("state".into())) == Some(&CborValue::Text(expected.into()))
}
