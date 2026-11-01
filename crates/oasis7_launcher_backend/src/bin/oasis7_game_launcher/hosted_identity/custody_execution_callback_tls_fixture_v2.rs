//! Real localhost TLS using the existing OpenSSL dev dependency. No shell,
//! system trust edits, external server process or relaxed hostname verification.
//! This endpoint is an Account receipt fixture, not a real CF Account Actor.
use super::*;
use openssl::{
    asn1::{Asn1Integer, Asn1Time},
    bn::BigNum,
    hash::MessageDigest,
    pkey::{PKey, Private},
    rsa::Rsa,
    ssl::{SslAcceptor, SslFiletype, SslMethod},
    x509::{
        X509, X509NameBuilder,
        extension::{
            AuthorityKeyIdentifier, BasicConstraints, ExtendedKeyUsage, KeyUsage,
            SubjectAlternativeName, SubjectKeyIdentifier,
        },
    },
};
use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

pub(super) struct Certificates {
    directory: PathBuf,
}
fn key() -> PKey<Private> {
    PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap()
}
fn name(common: &str) -> openssl::x509::X509Name {
    let mut n = X509NameBuilder::new().unwrap();
    n.append_entry_by_text("CN", common).unwrap();
    n.build()
}
fn serial(value: u32) -> Asn1Integer {
    Asn1Integer::from_bn(&BigNum::from_u32(value).unwrap()).unwrap()
}
impl Certificates {
    pub(super) fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("native-callback-tls-{}", random_token()));
        std::fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let ca_key = key();
        let ca_name = name("Native callback component CA");
        let before = Asn1Time::days_from_now(0).unwrap();
        let after = Asn1Time::days_from_now(1).unwrap();
        let mut ca = X509::builder().unwrap();
        ca.set_version(2).unwrap();
        ca.set_serial_number(&serial(1)).unwrap();
        ca.set_subject_name(&ca_name).unwrap();
        ca.set_issuer_name(&ca_name).unwrap();
        ca.set_pubkey(&ca_key).unwrap();
        ca.set_not_before(&before).unwrap();
        ca.set_not_after(&after).unwrap();
        ca.append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        ca.append_extension(
            KeyUsage::new()
                .critical()
                .key_cert_sign()
                .crl_sign()
                .build()
                .unwrap(),
        )
        .unwrap();
        let ski = SubjectKeyIdentifier::new()
            .build(&ca.x509v3_context(None, None))
            .unwrap();
        ca.append_extension(ski).unwrap();
        ca.sign(&ca_key, MessageDigest::sha256()).unwrap();
        let ca = ca.build();
        let leaf_key = key();
        let leaf_name = name("localhost");
        let mut leaf = X509::builder().unwrap();
        leaf.set_version(2).unwrap();
        leaf.set_serial_number(&serial(2)).unwrap();
        leaf.set_subject_name(&leaf_name).unwrap();
        leaf.set_issuer_name(ca.subject_name()).unwrap();
        leaf.set_pubkey(&leaf_key).unwrap();
        leaf.set_not_before(&before).unwrap();
        leaf.set_not_after(&after).unwrap();
        leaf.append_extension(BasicConstraints::new().critical().build().unwrap())
            .unwrap();
        leaf.append_extension(
            KeyUsage::new()
                .critical()
                .digital_signature()
                .key_encipherment()
                .build()
                .unwrap(),
        )
        .unwrap();
        leaf.append_extension(ExtendedKeyUsage::new().server_auth().build().unwrap())
            .unwrap();
        let san = SubjectAlternativeName::new()
            .dns("localhost")
            .build(&leaf.x509v3_context(Some(&ca), None))
            .unwrap();
        leaf.append_extension(san).unwrap();
        let aki = AuthorityKeyIdentifier::new()
            .keyid(true)
            .issuer(true)
            .build(&leaf.x509v3_context(Some(&ca), None))
            .unwrap();
        leaf.append_extension(aki).unwrap();
        leaf.sign(&ca_key, MessageDigest::sha256()).unwrap();
        std::fs::write(directory.join("ca.pem"), ca.to_pem().unwrap()).unwrap();
        let leaf = leaf.build();
        let mut anchors = openssl::x509::store::X509StoreBuilder::new().unwrap();
        anchors.add_cert(ca.clone()).unwrap();
        anchors
            .set_purpose(openssl::x509::X509PurposeId::SSL_SERVER)
            .unwrap();
        let mut hostname = openssl::x509::verify::X509VerifyParam::new().unwrap();
        hostname.set_host("localhost").unwrap();
        anchors.set_param(&hostname).unwrap();
        let anchors = anchors.build();
        let chain = openssl::stack::Stack::new().unwrap();
        let mut verify = openssl::x509::X509StoreContext::new().unwrap();
        assert!(
            verify
                .init(&anchors, &leaf, &chain, |context| context.verify_cert())
                .unwrap(),
            "fixture CA/leaf/time/EKU/localhost verification"
        );
        assert!(leaf.public_key().unwrap().public_eq(&leaf_key));
        let mut chain = leaf.to_pem().unwrap();
        chain.extend(ca.to_pem().unwrap());
        std::fs::write(directory.join("leaf.pem"), chain).unwrap();
        let key_path = directory.join("leaf.key");
        std::fs::write(&key_path, leaf_key.private_key_to_pem_pkcs8().unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        Self { directory }
    }
    pub(super) fn path(&self) -> &Path {
        &self.directory
    }
}
impl Drop for Certificates {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Counts {
    pub tls_connections: usize,
    pub http_requests: usize,
}
pub(super) struct TlsServer {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Counts>>,
}
impl TlsServer {
    pub(super) fn start(
        directory: &Path,
        port: u16,
        ready: &ReadyNativeRegistryV2,
        expected: NativeExecutionPrepareRequestV2,
        scenario: String,
    ) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut tls = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        tls.set_private_key_file(directory.join("leaf.key"), SslFiletype::PEM)
            .unwrap();
        tls.set_certificate_chain_file(directory.join("leaf.pem"))
            .unwrap();
        tls.check_private_key().unwrap();
        let tls = tls.build();
        let service = ready.service.clone();
        let sid = ready
            .headers
            .split("__Host-oasis7_sid=")
            .nth(1)
            .unwrap()
            .split('\r')
            .next()
            .unwrap()
            .to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut counts = Counts {
                tls_connections: 0,
                http_requests: 0,
            };
            let until = Instant::now() + Duration::from_secs(10);
            while !stopping.load(Ordering::SeqCst) && Instant::now() < until {
                let (stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(e) => panic!("TLS fixture accept {:?}", e.kind()),
                };
                counts.tls_connections += 1;
                assert!(
                    counts.tls_connections <= 8,
                    "unexpected transport retry loop"
                );
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let stream = match tls.accept(stream) {
                    Ok(v) => v,
                    Err(_) if scenario == "untrusted" => continue,
                    Err(openssl::ssl::HandshakeError::Failure(mid)) => {
                        let reasons: Vec<_> = mid
                            .error()
                            .ssl_error()
                            .into_iter()
                            .flat_map(|s| s.errors())
                            .filter_map(|e| e.reason())
                            .collect();
                        panic!(
                            "fixture TLS handshake code {:?}; static reasons {:?}",
                            mid.error().code(),
                            reasons
                        );
                    }
                    Err(_) => panic!("fixture TLS handshake setup/nonblocking category"),
                };
                let mut incoming = BufReader::new(stream);
                let mut line = String::new();
                assert!(incoming.read_line(&mut line).unwrap() > 0);
                assert_eq!(
                    line.trim_end(),
                    format!("POST {} HTTP/1.1", NATIVE_EXECUTION_REENTRY_PATH_V2)
                );
                let mut length = None;
                let mut host = None;
                loop {
                    line.clear();
                    assert!(incoming.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    let (name, value) = line.split_once(':').unwrap();
                    if name.eq_ignore_ascii_case("content-length") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                    if name.eq_ignore_ascii_case("host") {
                        host = Some(value.trim().to_owned());
                    }
                }
                assert_eq!(host.as_deref(), Some(format!("localhost:{port}").as_str()));
                let length = length.unwrap();
                assert!(length <= 128 * 1024);
                let mut raw = vec![0; length];
                incoming.read_exact(&mut raw).unwrap();
                let index = counts.http_requests;
                counts.http_requests += 1;
                assert!(
                    service.try_lock().is_ok(),
                    "callback HTTPS must release actual Native mutex"
                );
                let callback: NativeExecutionDispatchCallbackV2 =
                    serde_json::from_slice(&raw).unwrap();
                assert!(
                    callback.request.as_ref() == &expected,
                    "full exact Native request retained"
                );
                callback
                    .query
                    .verify_native(
                        &expected,
                        "approval-test-1",
                        &hex::encode(SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
                        &format!("https://localhost:{port}"),
                        now_ms(),
                    )
                    .unwrap();
                let edge = if index == 0 {
                    Edge::NativeToWorldCurrent
                } else {
                    Edge::NativeToWorldPrepare
                };
                assert_eq!(callback.query.query.leaf.edge, edge);
                let body = if index == 0 {
                    serde_json::to_vec(&expected.original.access.read).unwrap()
                } else {
                    serde_json::to_vec(&expected.original.request).unwrap()
                };
                assert_eq!(callback.query.query.body_digest, bytes_digest(&body));
                let outgoing = incoming.get_mut();
                if scenario == "unknown" {
                    outgoing.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 999\r\nConnection: close\r\n\r\n{").unwrap();
                    outgoing.flush().unwrap();
                    // Drop the actual TLS stream with an incomplete HTTP body.
                    // Keep accepting until finish so an illicit retry is counted.
                    continue;
                }
                if index >= 1 {
                    outgoing.write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                    outgoing.flush().unwrap();
                    continue;
                }
                if scenario == "executor_disabled" {
                    service
                        .lock()
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .participant_entry
                        .as_mut()
                        .unwrap()
                        .executor_control_enabled = false;
                }
                if scenario == "logout" {
                    service
                        .lock()
                        .unwrap()
                        .as_mut()
                        .unwrap()
                        .logout(Some(&sid), now_ms())
                        .unwrap();
                }
                let mut signed = request_fixture::claim(
                    &expected,
                    edge,
                    if scenario == "wrong_body" {
                        b"different World request"
                    } else {
                        &body
                    },
                    108 + index as u64,
                );
                if scenario == "wrong_signature" {
                    signed.signature_hex = "00".repeat(64);
                }
                let response = serde_json::to_vec(&signed).unwrap();
                write!(outgoing,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",response.len()).unwrap();
                outgoing.write_all(&response).unwrap();
                outgoing.flush().unwrap();
            }
            counts
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
    pub(super) fn finish(mut self) -> Counts {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap()
    }
}
impl Drop for TlsServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
