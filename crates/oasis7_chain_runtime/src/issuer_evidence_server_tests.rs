//! Transport regression; these tests do not qualify a protected witness.
use super::*;
use oasis7_distfs::controlled_authority::formal::full_sync_issuer::{
    FullSyncIssuerAnchorV3, FullSyncReadRoleV2,
};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
};
use rustls::pki_types::PrivatePkcs8KeyDer;
use std::{
    net::TcpListener,
    sync::atomic::{AtomicUsize, Ordering},
};

fn hash(byte: u8) -> String {
    hex::encode([byte; 32])
}

fn ca() -> (Certificate, KeyPair) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    (params.self_signed(&key).unwrap(), key)
}

fn leaf(
    authority: &(Certificate, KeyPair),
    name: &str,
    purpose: ExtendedKeyUsagePurpose,
) -> (Certificate, KeyPair) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(vec![name.into()]).unwrap();
    params.extended_key_usages = vec![purpose];
    (
        params.signed_by(&key, &authority.0, &authority.1).unwrap(),
        key,
    )
}

fn query() -> FullSyncReadEvidenceQueryV3 {
    FullSyncReadEvidenceQueryV3 {
        schema_version: 3,
        observation_nonce: hash(1),
        minimum_anchor: FullSyncIssuerAnchorV3 {
            schema_version: 3,
            revision: 1,
            scope_digest: hash(2),
            log_digest: hash(3),
            state_digest: hash(4),
            policy_digest: hash(5),
            signer_generation: 1,
        },
        provider_public_key: hash(6),
        provider_incarnation: hash(7),
        target: FullSyncIssuerTargetV2 {
            node_public_key: hash(8),
            role: FullSyncReadRoleV2::FullReadOnly,
        },
        target_generation: 1,
        provider_lease_request: None,
    }
}

#[test]
fn actual_mtls_listener_rejects_unlisted_leaf_and_mismatched_grant_before_dispatch() {
    let authority = ca();
    let (server_cert, server_key) =
        leaf(&authority, "localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let (allowed_cert, allowed_key) =
        leaf(&authority, "provider-a", ExtendedKeyUsagePurpose::ClientAuth);
    let (other_cert, other_key) =
        leaf(&authority, "provider-b", ExtendedKeyUsagePurpose::ClientAuth);
    let request = query();
    let grant = IssuerEvidenceGrantV3 {
        client_certificate_sha256: certificate_pin(allowed_cert.der().as_ref()),
        provider_public_key: request.provider_public_key.clone(),
        provider_incarnation: request.provider_incarnation.clone(),
        policy_digest: request.minimum_anchor.policy_digest.clone(),
        target: request.target.clone(),
        target_generation: request.target_generation,
    };
    let mut roots = RootCertStore::empty();
    roots.add(authority.0.der().clone()).unwrap();
    let server = Arc::new(
        IssuerEvidenceServerV3::new(
            roots,
            vec![server_cert.der().clone()],
            PrivatePkcs8KeyDer::from(server_key.serialize_der()).into(),
            vec![grant],
        )
        .unwrap(),
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let mut wrong = request.clone();
    wrong.provider_public_key = hash(9);
    let mut wrong_generation = request.clone();
    wrong_generation.target_generation = 2;
    let mut wrong_incarnation = request.clone();
    wrong_incarnation.provider_incarnation = hash(12);
    let mut wrong_policy = request.clone();
    wrong_policy.minimum_anchor.policy_digest = hash(10);
    let mut wrong_target = request.clone();
    wrong_target.target.node_public_key = hash(11);
    for (certificate, key, input, expected_status) in [
        (&other_cert, &other_key, &request, 403),
        (&allowed_cert, &allowed_key, &wrong, 403),
        (&allowed_cert, &allowed_key, &wrong_generation, 403),
        (&allowed_cert, &allowed_key, &wrong_incarnation, 403),
        (&allowed_cert, &allowed_key, &wrong_policy, 403),
        (&allowed_cert, &allowed_key, &wrong_target, 403),
        (&allowed_cert, &allowed_key, &request, 200),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = server.clone();
        let observed = calls.clone();
        let thread = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            server
                .serve_with(
                    socket,
                    Instant::now() + Duration::from_secs(5),
                    |grant, input, deadline| {
                        assert!(Instant::now() < deadline);
                        assert_eq!(grant.provider_public_key, input.provider_public_key);
                        assert_eq!(grant.provider_incarnation, input.provider_incarnation);
                        assert_eq!(grant.policy_digest, input.minimum_anchor.policy_digest);
                        observed.fetch_add(1, Ordering::SeqCst);
                        Ok(b"{}".to_vec())
                    },
                )
                .unwrap();
        });
        let ca = reqwest::Certificate::from_pem(authority.0.pem().as_bytes()).unwrap();
        let identity =
            reqwest::Identity::from_pem((certificate.pem() + &key.serialize_pem()).as_bytes())
                .unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .tls_certs_only(vec![ca])
            .identity(identity)
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let reply = client
            .post(format!("https://localhost:{port}/read"))
            .json(input)
            .send()
            .unwrap();
        assert_eq!(reply.status().as_u16(), expected_status);
        assert_eq!(reply.headers()["cache-control"], "no-store");
        assert_eq!(reply.text().unwrap(), "{}");
        thread.join().unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn framing_rejects_duplicate_length_transfer_encoding_and_overlong_body() {
    let json = serde_json::to_vec(&query()).unwrap();
    let head = format!(
        "POST /read HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        json.len()
    );
    let complete = |suffix: &str| {
        let mut bytes = format!("{head}{suffix}\r\n").into_bytes();
        bytes.extend_from_slice(&json);
        bytes
    };
    assert_eq!(read_query(&mut complete("").as_slice()).unwrap(), query());
    for invalid in [
        "Content-Length: 0\r\n",
        "Transfer-Encoding: chunked\r\n",
        "Expect: 100-continue\r\n",
    ] {
        assert!(read_query(&mut complete(invalid).as_slice()).is_err());
    }
    let oversized = format!(
        "POST /read HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        MAX_REQUEST + 1
    );
    assert!(read_query(&mut oversized.as_bytes()).is_err());
}
