//! Real HTTP relay plus the same signed-history gate; no protected hardware claim.
use super::*;
use crate::formal_live_service::full_sync_read_gate::{
    BftCandidateReadSubjectV1,
    remote_evidence::{FullSyncReadEvidenceQueryV3, RemoteFullSyncReadEvidenceSourceV3},
};
use std::io::{Read, Write};

#[test]
fn remote_evidence_endpoint_rejects_ambient_or_credential_urls() {
    for endpoint in [
        "http://example.com/read",
        "file:///read",
        "https://user:secret@example.com/read",
        "https://example.com/read?token=secret",
        "https://example.com/read#fragment",
    ] {
        assert!(RemoteFullSyncReadEvidenceSourceV3::new(endpoint).is_err());
    }
    assert!(RemoteFullSyncReadEvidenceSourceV3::new("https://example.com/read").is_ok());
    assert!(RemoteFullSyncReadEvidenceSourceV3::new("http://127.0.0.1:1/read").is_ok());
}

fn relay_once(
    source: Arc<SignedEvidenceSource>,
    bad_nonce: bool,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/read", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
            assert!(header.len() <= 8192);
        }
        let header = String::from_utf8(header).unwrap();
        assert!(header.starts_with("POST /read HTTP/1.1"));
        let size: usize = header
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        assert!(size < 16 * 1024);
        let mut body = vec![0; size];
        stream.read_exact(&mut body).unwrap();
        let query: FullSyncReadEvidenceQueryV3 = serde_json::from_slice(&body).unwrap();
        assert_eq!(query.schema_version, 3);
        let start = Instant::now();
        source.return_bad_nonce(bad_nonce);
        let evidence = source
            .fetch(&FullSyncIssuerReadEvidenceRequestV3 {
                observation_nonce: hex::decode(query.observation_nonce)
                    .unwrap()
                    .try_into()
                    .unwrap(),
                minimum_anchor: query.minimum_anchor,
                provider_public_key: query.provider_public_key,
                provider_incarnation: query.provider_incarnation,
                target: query.target,
                target_generation: query.target_generation,
                started_at: start,
                deadline: start + Duration::from_secs(3),
            })
            .unwrap();
        let body = serde_json::to_vec(&evidence).unwrap();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        stream.write_all(&body).unwrap();
    });
    (endpoint, thread)
}

#[test]
fn remote_evidence_relay_preserves_fresh_nonce_and_signed_gate_authority() {
    for bad_nonce in [false, true] {
        let (_, trust, _, _, _) = full_sync_fixture();
        let fixture = issue_lease_fixture_role(
            trust.initial_trust.clone(),
            now_ms(),
            FullSyncReadRoleV2::BftPendingCandidateV1,
        );
        let source = Arc::new(SignedEvidenceSource::new(
            &fixture,
            fixture.lease_not_before,
        ));
        let (endpoint, server) = relay_once(source, bad_nonce);
        let gate = FullSyncV3ReadGate::new(
            fixture.trust.clone(),
            fixture.signed_v2.clone(),
            fixture.signed_v3.clone(),
            fixture.minimum_v3.clone(),
            public(PROVIDER_KEY),
            issuer_hash(PROVIDER_INCARNATION),
            Arc::new(RemoteFullSyncReadEvidenceSourceV3::new(&endpoint).unwrap()),
        )
        .unwrap();
        let subject = BftCandidateReadSubjectV1 {
            world: oasis7_client_api::world_service::WorldIdentity {
                world_id: fixture.trust.world.world_id.clone(),
                genesis_digest: fixture.trust.world.genesis_digest.clone(),
            },
            chain_id: fixture.trust.world.chain_id.clone(),
            authority_epoch: fixture.verified_v3.body().authority_epoch,
            node_public_key: fixture.target.node_public_key.clone(),
            policy_digest: fixture.verified_v3.policy_digest().into(),
            grant_generation: 1,
        };
        let calls = AtomicU64::new(0);
        let result = gate.authorize_candidate_with_response(&subject, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![7])
        });
        assert_eq!(result.is_ok(), !bad_nonce);
        assert_eq!(calls.load(Ordering::SeqCst), u64::from(!bad_nonce));
        server.join().unwrap();
    }
}

fn raw_evidence_request(timeout: Duration) -> FullSyncIssuerReadEvidenceRequestV3 {
    let (_, trust, _, _, _) = full_sync_fixture();
    let fixture = issue_lease_fixture_role(
        trust.initial_trust.clone(),
        now_ms(),
        FullSyncReadRoleV2::BftPendingCandidateV1,
    );
    let started_at = Instant::now();
    FullSyncIssuerReadEvidenceRequestV3 {
        observation_nonce: [93; 32],
        minimum_anchor: fixture.minimum_v3,
        provider_public_key: public(PROVIDER_KEY),
        provider_incarnation: issuer_hash(PROVIDER_INCARNATION),
        target: fixture.target,
        target_generation: 1,
        started_at,
        deadline: started_at + timeout,
    }
}

// Consume the complete small POST before responding, so transport rejection
// cannot accidentally be explained by a reset caused by unread request bytes.
fn raw_relay_once(
    response: Vec<u8>,
    hold_after_response: Duration,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/read", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            header.push(byte[0]);
            assert!(header.len() <= 8192);
        }
        let header = String::from_utf8(header).unwrap();
        let size: usize = header
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse().unwrap())
            })
            .unwrap();
        assert!(size < 16 * 1024);
        stream.read_exact(&mut vec![0; size]).unwrap();
        stream.write_all(&response).unwrap();
        stream.flush().unwrap();
        std::thread::sleep(hold_after_response);
    });
    (endpoint, server)
}

#[test]
fn remote_evidence_slow_body_cannot_extend_the_original_deadline() {
    // Headers and an incomplete JSON byte arrive immediately; the body stays
    // open beyond the original deadline rather than merely returning bad JSON.
    let (endpoint, server) = raw_relay_once(
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{".to_vec(),
        Duration::from_millis(600),
    );
    let source = RemoteFullSyncReadEvidenceSourceV3::new(&endpoint).unwrap();
    let request = raw_evidence_request(Duration::from_millis(200));
    let result = source.fetch(&request);
    assert!(matches!(
        result,
        Err(FormalReadError::HistoryUnavailable(_))
    ));
    assert!(
        request.started_at.elapsed() < Duration::from_millis(550),
        "body timeout must not wait for the server to close"
    );
    server.join().unwrap();
}

#[test]
fn remote_evidence_refuses_oversized_content_length_before_reading_the_body() {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        68_u64 * 1024 * 1024 + 1,
    )
    .into_bytes();
    let (endpoint, server) = raw_relay_once(response, Duration::ZERO);
    let source = RemoteFullSyncReadEvidenceSourceV3::new(&endpoint).unwrap();
    let result = source.fetch(&raw_evidence_request(Duration::from_secs(3)));
    assert!(
        matches!(result, Err(FormalReadError::HistoryUnavailable(reason)) if reason.contains("response rejected"))
    );
    server.join().unwrap();
}

#[test]
fn remote_evidence_does_not_follow_redirects_to_another_endpoint() {
    let redirected = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    redirected.set_nonblocking(true).unwrap();
    let response = format!(
        "HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/redirected\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        redirected.local_addr().unwrap(),
    ).into_bytes();
    let (endpoint, server) = raw_relay_once(response, Duration::ZERO);
    let source = RemoteFullSyncReadEvidenceSourceV3::new(&endpoint).unwrap();
    let result = source.fetch(&raw_evidence_request(Duration::from_secs(3)));
    assert!(
        matches!(result, Err(FormalReadError::HistoryUnavailable(reason)) if reason.contains("response rejected"))
    );
    assert!(
        matches!(redirected.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
    server.join().unwrap();
}
