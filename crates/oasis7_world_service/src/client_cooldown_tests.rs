use super::*;
use crate::authority;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use std::time::Instant;

#[test]
fn overload_cooldown_survives_recreated_queries_without_replaying_submit_or_blocking_second_client()
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let service_key = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
    let config = WorldServiceClientConfig {
        formal_read_trust: None,
        endpoint: format!("http://{}", listener.local_addr().unwrap()),
        trusted_service_public_key: hex::encode(service_key.verifying_key().to_bytes()),
        expected_world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([7; 32]),
        timeout: Duration::from_secs(1),
        max_response_bytes: 4096,
    };
    let key = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
    let public = hex::encode(key.verifying_key().to_bytes());
    let mut gameplay = oasis7_proto::viewer::GameplayActionRequest {
        action_id: "claim_first_agent".into(),
        target_agent_id: "starter-agent-0".into(),
        actor_agent_id: None,
        player_id: "player".into(),
        public_key: Some(public.clone()),
        auth: None,
    };
    gameplay.auth = Some(
        oasis7_proto::player_auth::sign_gameplay_action_auth_proof(
            &gameplay,
            19,
            &public,
            &hex::encode([5; 32]),
        )
        .unwrap(),
    );
    let payload = WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&gameplay).unwrap());
    let correlation =
        super::super::derive_correlation(config.expected_world.clone(), &payload).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let observed_count = count.clone();
    let original_payload = payload.clone();
    let expected_correlation = correlation.clone();
    let worker = std::thread::spawn(move || {
        for index in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let (offset, length) = loop {
                let mut buf = [0; 4096];
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(offset) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..offset]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= offset + 4 + length {
                        break (offset + 4, length);
                    }
                }
            };
            observed_count.fetch_add(1, Ordering::SeqCst);
            if index == 0 {
                let submitted: SubmitIntentRequest<WorldServicePayloadV1> =
                    serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
                assert_eq!(submitted.correlation, expected_correlation);
                assert_eq!(
                    serde_json::to_value(submitted.signed_payload).unwrap(),
                    serde_json::to_value(&original_payload).unwrap()
                );
                stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 999999\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            } else {
                let lookup: AuthenticatedLookup =
                    serde_json::from_slice(&bytes[offset..offset + length]).unwrap();
                assert_eq!(lookup.request.key, expected_correlation.key);
                assert_eq!(
                    serde_json::to_value(&lookup.original).unwrap(),
                    serde_json::to_value(&original_payload).unwrap()
                );
                let digest = authority::request_digest(LOOKUP_PATH, &lookup).unwrap();
                let signed = authority::sign_service_response(
                    LOOKUP_PATH,
                    digest,
                    IntentResponse::<Value> {
                        formal_profile: None,
                        execution_evidence_scope: None,
                        contract_version: 1,
                        correlation: expected_correlation.clone(),
                        outcome: IntentOutcome::Pending,
                    },
                    &hex::encode([9; 32]),
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
        }
    });
    let state = WorldServiceQueryState::default();
    let client = RemoteWorldServiceClient::new(config.clone())
        .unwrap()
        .with_query_state(state.clone());
    assert!(
        matches!(client.submit(SubmitIntentRequest { contract_version: 1, correlation: correlation.clone(), deadline_unix_ms: None, signed_payload: payload.clone() }).unwrap(), SubmitObservation::OutcomeUnknown(identity) if identity == correlation)
    );
    let recreated = RemoteWorldServiceClient::new(config.clone())
        .unwrap()
        .with_query_state(state.clone());
    let request = LookupIntentRequest {
        contract_version: 1,
        key: correlation.key.clone(),
    };
    let start = Instant::now();
    assert!(
        matches!(recreated.lookup(request.clone(),payload.clone()), Err(WorldServiceClientError::Cooldown { status:503, retry_after }) if retry_after > Duration::ZERO && retry_after <= Duration::from_secs(2))
    );
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "cooldown must fail fast rather than sleep"
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let independent = RemoteWorldServiceClient::new(config).unwrap();
    assert!(matches!(
        independent
            .lookup(request.clone(), payload.clone())
            .unwrap()
            .outcome,
        IntentOutcome::Pending
    ));
    std::thread::sleep(Duration::from_millis(2100));
    assert!(matches!(
        recreated.lookup(request, payload).unwrap().outcome,
        IntentOutcome::Pending
    ));
    worker.join().unwrap();
    assert_eq!(
        count.load(Ordering::SeqCst),
        3,
        "one Submit and two explicit queries; no automatic replay"
    );
}

#[test]
fn ordinary_unavailable_without_retry_after_does_not_establish_cooldown() {
    let state = WorldServiceQueryState::default();
    assert_eq!(state.observe(503, None).unwrap(), None);
    assert!(state.check().is_ok());
    assert_eq!(
        state.observe(429, Some("not-a-delay")).unwrap(),
        Some(Duration::from_secs(1))
    );
    assert!(matches!(
        state.check(),
        Err(WorldServiceClientError::Cooldown { .. })
    ));
}
