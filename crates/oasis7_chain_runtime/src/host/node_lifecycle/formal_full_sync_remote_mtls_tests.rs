//! Real TLS over TCP for the ordinary V3 Host evidence adapter.
use super::*;
use crate::formal_live_config::FormalLiveReadAuthorizationV3;
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, ExtendedKeyUsagePurpose, IsCa, KeyPair,
};
use rustls::{
    RootCertStore, ServerConfig, ServerConnection, StreamOwned, pki_types::PrivatePkcs8KeyDer,
    server::WebPkiClientVerifier,
};
use std::io::{self, Write};
use std::net::TcpListener;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink};
use std::path::Path;

fn private_pem(path: &Path, pem: &str) {
    std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .unwrap()
        .write_all(pem.as_bytes())
        .unwrap();
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
    usage: ExtendedKeyUsagePurpose,
) -> (Certificate, KeyPair) {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(vec![name.into()]).unwrap();
    params.extended_key_usages = vec![usage];
    (
        params.signed_by(&key, &authority.0, &authority.1).unwrap(),
        key,
    )
}

#[test]
fn configured_evidence_mtls_requires_server_pin_hostname_and_client_certificate() {
    let (_, trust, _, _, _) = full_sync_fixture();
    let fixture = issue_lease_fixture(trust.initial_trust.clone(), now_ms());
    let witness = Arc::new(SignedEvidenceSource::new(
        &fixture,
        fixture.lease_not_before,
    ));
    let authority = ca();
    let unrelated = ca();
    let (server_cert, server_key) =
        leaf(&authority, "localhost", ExtendedKeyUsagePurpose::ServerAuth);
    let (client_cert, client_key) = leaf(
        &authority,
        "evidence-provider",
        ExtendedKeyUsagePurpose::ClientAuth,
    );
    let (wrong_client_cert, wrong_client_key) = leaf(
        &unrelated,
        "evidence-provider",
        ExtendedKeyUsagePurpose::ClientAuth,
    );

    let files = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let server_ca_file = files.path().join("server-ca.pem");
    private_pem(&server_ca_file, &authority.0.pem());
    let unrelated_ca_file = files.path().join("unrelated-ca.pem");
    private_pem(&unrelated_ca_file, &unrelated.0.pem());
    let client_identity_file = files.path().join("provider.pem");
    private_pem(
        &client_identity_file,
        &(client_cert.pem() + &client_key.serialize_pem()),
    );
    let wrong_identity_file = files.path().join("unrelated-provider.pem");
    private_pem(
        &wrong_identity_file,
        &(wrong_client_cert.pem() + &wrong_client_key.serialize_pem()),
    );

    let loose_identity_file = files.path().join("nonprivate-provider.pem");
    std::fs::copy(&client_identity_file, &loose_identity_file).unwrap();
    std::fs::set_permissions(&loose_identity_file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let symlink_identity_file = files.path().join("symlink-provider.pem");
    symlink(&client_identity_file, &symlink_identity_file).unwrap();

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut clients = RootCertStore::empty();
    clients.add(authority.0.der().clone()).unwrap();
    let client_verifier =
        WebPkiClientVerifier::builder_with_provider(Arc::new(clients), provider.clone())
            .build()
            .unwrap();
    let server = Arc::new(
        ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(
                vec![server_cert.der().clone()],
                PrivatePkcs8KeyDer::from(server_key.serialize_der()).into(),
            )
            .unwrap(),
    );
    for (hostname, server_ca, client_identity, success) in [
        ("127.0.0.1", &server_ca_file, &client_identity_file, false),
        (
            "localhost",
            &unrelated_ca_file,
            &client_identity_file,
            false,
        ),
        ("localhost", &server_ca_file, &wrong_identity_file, false),
        ("localhost", &server_ca_file, &client_identity_file, true),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let config: FormalLiveReadAuthorizationV3 = serde_json::from_value(serde_json::json!({
            "evidence_endpoint": format!("https://{hostname}:{port}/read"),
            "evidence_mtls": {
                "server_ca_file": server_ca,
                "client_identity_file": client_identity,
            },
            "signed_v2_policy": fixture.signed_v2,
            "signed_v3_policy": fixture.signed_v3,
            "minimum_anchor": fixture.minimum_v3,
        }))
        .unwrap();
        let mut plaintext = config.clone();
        plaintext.evidence_endpoint = format!("http://127.0.0.1:{port}/read");
        assert!(plaintext.build(&fixture.trust, &key(PROVIDER_KEY)).is_err());
        if success {
            for invalid in [&loose_identity_file, &symlink_identity_file] {
                let mut invalid_config = config.clone();
                invalid_config
                    .evidence_mtls
                    .as_mut()
                    .unwrap()
                    .client_identity_file = invalid.clone();
                assert!(
                    invalid_config
                        .build(&fixture.trust, &key(PROVIDER_KEY))
                        .is_err()
                );
            }
        }
        let source = config
            .build(&fixture.trust, &key(PROVIDER_KEY))
            .unwrap()
            .source;
        let server = server.clone();
        let expected_peer = client_cert.der().clone();
        let witness = witness.clone();
        // The relay closes the first successful exchange. The second request
        // uses the same configured client and must authenticate on reconnect.
        let attempts = if success { 2 } else { 1 };
        let worker = std::thread::spawn(move || {
            for _ in 0..attempts {
                let until = Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e)
                            if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < until =>
                        {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("bounded evidence TLS accept: {error}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut tls = ServerConnection::new(server.clone()).unwrap();
                while tls.is_handshaking() {
                    if tls.complete_io(&mut socket).is_err() {
                        assert!(!success, "authorized mTLS handshake failed");
                        return;
                    }
                }
                assert!(success, "unauthorized TLS handshake succeeded");
                assert_eq!(
                    tls.peer_certificates().unwrap(),
                    std::slice::from_ref(&expected_peer)
                );
                let mut stream = StreamOwned::new(tls, socket);
                serve_relay(&mut stream, &witness, false);
                stream.flush().unwrap();
            }
        });
        for i in 0..attempts {
            let started_at = Instant::now();
            let request = FullSyncIssuerReadEvidenceRequestV3 {
                observation_nonce: [93 + i as u8; 32],
                minimum_anchor: fixture.minimum_v3.clone(),
                provider_public_key: public(PROVIDER_KEY),
                provider_incarnation: issuer_hash(PROVIDER_INCARNATION),
                target: fixture.target.clone(),
                target_generation: 1,
                started_at,
                deadline: started_at + Duration::from_secs(5),
            };
            let result = source.fetch(&request);
            if success {
                assert_eq!(
                    result.unwrap().query.body.observation_nonce,
                    hex::encode(request.observation_nonce)
                );
            } else {
                assert!(matches!(
                    result,
                    Err(FormalReadError::HistoryUnavailable(_))
                ));
            }
        }
        worker.join().unwrap();
    }
}
