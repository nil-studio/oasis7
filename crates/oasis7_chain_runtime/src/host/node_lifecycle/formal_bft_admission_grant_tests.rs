//! Signed issuer/role component evidence. This does not prove confidential
//! material verification, shipping backend persistence or physical custody.
use super::*;
use crate::formal_bft_admission_grant::{endpoint::AdmissionGrantIssuerV1, wire::GrantRequest};
use ed25519_dalek::Signer;
use oasis7_distfs::protected_bft_v3::{
    BftPrivateRetentionAdmissionInputRefV1, BftPrivateRetentionAdmissionReadGrantBodyV1 as Body,
    BftPrivateRetentionAdmissionReadGrantV1 as Grant, BftPrivateRetentionAdmissionReadPurposeV1,
    BftPrivateRetentionKeyV1, BftPrivateRetentionSourceV1, BftSignerErrorV3, BftSigningKeyHandleV3,
    BftSuccessorWindowArchiveRefV1,
};
struct CountedIssuer {
    calls: Arc<AtomicU64>,
}
impl BftSigningKeyHandleV3 for CountedIssuer {
    fn public_key_hex(&self) -> String {
        public(9)
    }
    fn sign_preimage(&self, bytes: &[u8]) -> Result<[u8; 64], BftSignerErrorV3> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(key(9).sign(bytes).to_bytes())
    }
}
fn request(fixture: &IssuerFixture, base: u64) -> GrantRequest {
    let requester = oasis7_net::Libp2pNetwork::new(Default::default());
    let serving = oasis7_net::Libp2pNetwork::new(Default::default());
    let world = &fixture.verified_v3.body().world;
    GrantRequest::sign(
        Body {
            schema_version: 1,
            purpose: BftPrivateRetentionAdmissionReadPurposeV1::OriginalAdmissionInput,
            scope: oasis7_proto::distributed_bft::v2::BftScopeV2 {
                world_id: world.world_id.clone(),
                chain_id: world.chain_id.clone(),
                genesis_hash: world.genesis_digest.clone(),
            },
            reference: BftPrivateRetentionAdmissionInputRefV1 {
                source: BftPrivateRetentionSourceV1 {
                    binding_digest: issuer_hash(70),
                    handoff_digest: issuer_hash(71),
                    material_source_digest: issuer_hash(72),
                },
                key: BftPrivateRetentionKeyV1 {
                    ledger_origin_digest: issuer_hash(73),
                    height: 5,
                    header_id: issuer_hash(74),
                },
                object: BftSuccessorWindowArchiveRefV1 {
                    digest: issuer_hash(75),
                    bytes: 1,
                },
                admission_digest: issuer_hash(76),
            },
            subject_public_key: public(READER_KEY),
            requester_peer_id: requester.peer_id().to_string(),
            serving_peer_id: serving.peer_id().to_string(),
            schedule_digest: issuer_hash(77),
            view_generation: 0,
            request_nonce: issuer_hash(78),
            grant_epoch: 1,
            not_before_unix_ms: base.saturating_sub(1_000),
            expires_unix_ms: base + 60_000,
        },
        &key(READER_KEY),
    )
    .unwrap()
}
#[test]
fn admission_live_issuer_preserves_exact_grant_and_stops_signing_after_revoke() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture_role(
        trust.initial_trust.clone(),
        base,
        FullSyncReadRoleV2::BftOriginalAdmissionInputV1,
    );
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let calls = Arc::new(AtomicU64::new(0));
    let issuer = AdmissionGrantIssuerV1::new(
        gate,
        Arc::new(CountedIssuer {
            calls: calls.clone(),
        }),
    );
    let request = request(&fixture, base);
    let raw = serde_json::to_vec(&request).unwrap();
    let first = issuer
        .issue_reply(&raw, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let grant: Grant = serde_json::from_slice(&first).unwrap();
    grant
        .verify_signature(&trust.initial_trust.issuer_public_key)
        .unwrap();
    assert_eq!(grant.body, request.body);
    let retry = issuer
        .issue_reply(&raw, Instant::now() + Duration::from_secs(2))
        .unwrap();
    assert_eq!(
        retry, first,
        "the original issuer request cannot mint a different grant"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let before = source.fetch_count();
    let mut tampered = request.clone();
    tampered.body.reference.key.height += 1;
    assert!(
        issuer
            .issue_reply(
                &serde_json::to_vec(&tampered).unwrap(),
                Instant::now() + Duration::from_secs(2)
            )
            .is_err()
    );
    assert_eq!(
        source.fetch_count(),
        before,
        "bad subject signature must not invoke witness work"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    source.revoke_target(&fixture);
    assert!(
        issuer
            .issue_reply(&raw, Instant::now() + Duration::from_secs(2))
            .is_err()
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "revoked role must fail before issuer signing"
    );
}
#[test]
fn admission_live_issuer_does_not_convert_full_read_only_or_extend_deadline() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let base = now_ms();
    let fixture = issue_lease_fixture_role(
        trust.initial_trust.clone(),
        base,
        FullSyncReadRoleV2::FullReadOnly,
    );
    let source = Arc::new(SignedEvidenceSource::new(&fixture, base + 2_000));
    let gate = test_gate(&fixture, source.clone(), PROVIDER_INCARNATION);
    let calls = Arc::new(AtomicU64::new(0));
    let issuer = AdmissionGrantIssuerV1::new(
        gate,
        Arc::new(CountedIssuer {
            calls: calls.clone(),
        }),
    );
    let raw = serde_json::to_vec(&request(&fixture, base)).unwrap();
    assert!(issuer.issue_reply(&raw, Instant::now()).is_err());
    assert!(
        issuer
            .issue_reply(&raw, Instant::now() + Duration::from_secs(2))
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        source.fetch_count(),
        0,
        "old role cannot reach new-purpose witness work"
    );
}

#[test]
fn admission_issuer_http_rejects_oversized_content_length_before_waiting_for_body() {
    use crate::formal_live_service::http::read_http_request_with_body_limit;
    use std::io::Write;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut accepted, _) = listener.accept().unwrap();
    let worker = std::thread::spawn(move || {
        read_http_request_with_body_limit(
            &mut accepted,
            Instant::now() + Duration::from_secs(2),
            64 * 1024,
        )
    });
    // Keep the writing half open and send no body at all. A parser that uses
    // the old 256KiB cap would wait for bytes and return a deadline error.
    client.write_all(format!("POST /oasis7/bft/original-admission-input/grant/v1 HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n", 64 * 1024 + 1).as_bytes()).unwrap();
    let result = worker.join().unwrap();
    assert_eq!(result.unwrap_err(), "HTTP body exceeds bound");
    drop(client);
}
