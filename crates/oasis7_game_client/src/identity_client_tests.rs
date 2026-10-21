use super::*;
use std::{
    io::Write,
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn directory() -> Directory {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    Directory(
        std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("identity-sdk-{}", hex::encode(nonce))),
    )
}
const PASSWORD: &[u8] = b"synthetic identity test unlock";
fn config(root: &Directory) -> IdentityClientConfig {
    IdentityClientConfig {
        endpoint: "https://identity.example".into(),
        identity_service_id: "identity-test".into(),
        environment_id: "test".into(),
        audience: "https://identity.example".into(),
        state_directory: root.0.clone(),
        test_loopback: false,
    }
}
fn challenge(client: &IdentityClient, purpose: NativeIdentityPurposeV1, operation: &str) -> Value {
    let now = http::now_ms();
    let key = client.signing_key.verifying_key().to_bytes();
    let nonce = [2; 32];
    let bytes = NativeIdentityChallengeV1 {
        identity_service_id: &client.config.identity_service_id,
        environment_id: &client.config.environment_id,
        audience: &client.config.audience,
        purpose,
        account_context: "",
        operation_id: operation,
        public_key: &key,
        nonce: &nonce,
        issued_at: now,
        expires_at: now + 120000,
    }
    .signing_bytes()
    .unwrap();
    json!({"api_version":2,"ok":true,"operation_id":operation,"ceremony_id":operation,"expires_at":now+120000,"options":{"identity_service_id":client.config.identity_service_id,"environment_id":client.config.environment_id,"audience":client.config.audience,"purpose":purpose.as_str(),"account_context":"","operation_id":operation,"public_key":client.public_key(),"nonce":URL_SAFE_NO_PAD.encode(nonce),"issued_at":now,"expires_at":now+120000,"signed_bytes":URL_SAFE_NO_PAD.encode(bytes)}})
}
#[test]
fn password_vault_preserves_identity_and_fails_closed_on_replacement_or_wrong_unlock() {
    let d = directory();
    let c = config(&d);
    let public = IdentityClient::initialize(&c, PASSWORD).unwrap();
    assert!(IdentityClient::initialize(&c, PASSWORD).is_err());
    assert!(IdentityClient::new(c.clone(), b"wrong synthetic password").is_err());
    let first = IdentityClient::new(c.clone(), PASSWORD).unwrap();
    assert_eq!(first.public_key(), public);
    let winner: Value = first
        .store
        .put_original(
            &first.scope,
            "op",
            "request",
            &json!({"signature":"synthetic-original-proof"}),
        )
        .unwrap();
    drop(first);
    let reopened = IdentityClient::new(c, PASSWORD).unwrap();
    assert_eq!(reopened.public_key(), public);
    assert_eq!(
        reopened
            .store
            .read_original::<Value>(&reopened.scope, "op", "request")
            .unwrap(),
        Some(winner)
    );
    assert!(
        reopened
            .store
            .read_original::<Value>("other-service", "op", "request")
            .unwrap()
            .is_none()
    );
    // Neither the seed nor the original proof occurs in plaintext on disk.
    for entry in std::fs::read_dir(&d.0).unwrap() {
        let p = entry.unwrap().path();
        if p.is_file() {
            let b = std::fs::read(p).unwrap();
            assert!(!b.windows(24).any(|w| w == b"synthetic-original-proof"));
            assert!(!b.windows(32).any(|w| w == reopened.store.login_seed()));
        }
    }
}
#[test]
fn client_recomputes_every_trusted_challenge_binding_before_signing() {
    let d = directory();
    let c = config(&d);
    IdentityClient::initialize(&c, PASSWORD).unwrap();
    let client = IdentityClient::new(c, PASSWORD).unwrap();
    let id = URL_SAFE_NO_PAD.encode([9; 32]);
    let good = challenge(&client, NativeIdentityPurposeV1::Register, &id);
    let signed = client
        .proof(&good, NativeIdentityPurposeV1::Register, http::now_ms())
        .unwrap();
    let sig = ed25519_dalek::Signature::from_slice(
        &URL_SAFE_NO_PAD
            .decode(signed["signature"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    client
        .signing_key
        .verifying_key()
        .verify_strict(
            &URL_SAFE_NO_PAD
                .decode(good["options"]["signed_bytes"].as_str().unwrap())
                .unwrap(),
            &sig,
        )
        .unwrap();
    for (field, value) in [
        ("identity_service_id", json!("evil")),
        ("environment_id", json!("evil")),
        ("audience", json!("https://evil")),
        ("purpose", json!("login")),
        ("account_context", json!("victim")),
        ("public_key", json!("03".repeat(32))),
        ("operation_id", json!(URL_SAFE_NO_PAD.encode([3; 32]))),
        ("signed_bytes", json!("bad")),
        ("expires_at", json!(1)),
    ] {
        let mut bad = good.clone();
        bad["options"][field] = value;
        assert!(
            client
                .proof(&bad, NativeIdentityPurposeV1::Register, http::now_ms())
                .is_err(),
            "{field}"
        );
    }
}

#[test]
fn trust_configuration_and_plaintext_credentials_are_rejected_before_unlock_or_network() {
    let d = directory();
    let mut c = config(&d);
    for endpoint in [
        "http://identity.example",
        "https://user:secret@identity.example",
        "https://identity.example/other",
        "https://identity.example?token=secret",
        "https://identity.example#fragment",
    ] {
        c.endpoint = endpoint.into();
        assert!(IdentityClient::initialize(&c, PASSWORD).is_err());
        assert!(!d.0.exists());
    }
    let mut wire = serde_json::to_value(config(&d)).unwrap();
    wire["private_key"] = json!("forbidden");
    assert!(serde_json::from_value::<IdentityClientConfig>(wire).is_err());
    let mut wire = serde_json::to_value(config(&d)).unwrap();
    wire["test_loopback"] = json!(true);
    assert!(serde_json::from_value::<IdentityClientConfig>(wire).is_err());
}

#[test]
fn status_projects_known_login_without_exposing_cookie_or_claiming_current_authority() {
    let d = directory();
    let c = config(&d);
    IdentityClient::initialize(&c, PASSWORD).unwrap();
    let client = IdentityClient::new(c, PASSWORD).unwrap();
    let session = IdentitySession {
        account_id: "stable-account".into(),
        cookie: "synthetic-private-session".into(),
    };
    let _: IdentitySession = client
        .store
        .put_original(&client.scope, "login-op", "login-session", &session)
        .unwrap();
    let result = client.status("login-op").unwrap();
    assert_eq!(result.state, "confirmed");
    assert_eq!(
        result.next_action,
        "revalidate_current_session_before_world_admission"
    );
    assert!(
        !serde_json::to_string(&result)
            .unwrap()
            .contains(&session.cookie)
    );
}
#[test]
fn registration_lost_commit_reply_reuses_exact_original_after_client_restart() {
    let d = directory();
    let mut c = config(&d);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    c.endpoint = format!("http://{}", listener.local_addr().unwrap());
    c.test_loopback = true;
    IdentityClient::initialize(&c, PASSWORD).unwrap();
    let client = IdentityClient::new(c.clone(), PASSWORD).unwrap();
    let id = URL_SAFE_NO_PAD.encode([9; 32]);
    let reply = challenge(&client, NativeIdentityPurposeV1::Register, &id);
    let public = client.public_key();
    let id2 = id.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let server = thread::spawn(move || {
        for n in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut input = Vec::new();
            let mut byte = [0];
            while !input.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                input.push(byte[0]);
                assert!(input.len() < 16384);
            }
            let headers = String::from_utf8(input).unwrap();
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            recorded
                .lock()
                .unwrap()
                .push((headers.clone(), body.clone()));
            let (result, cookie) = match n {
                0 => (
                    json!({"api_version":2,"ok":true,"csrf_token":"a".repeat(64),"native_key":{"available":true,"algorithm":"Ed25519","identity_service_id":"identity-test","environment_id":"test"},"machine_registration":{"available":true}}),
                    format!(
                        "Set-Cookie: __Host-oasis7_pre={}; Secure; HttpOnly; Path=/\r\n",
                        URL_SAFE_NO_PAD.encode([4; 32])
                    ),
                ),
                1 => {
                    let p: Value = serde_json::from_str(&body).unwrap();
                    assert_eq!(p["public_key"], public);
                    assert_eq!(p["operation_id"], id2);
                    (reply.clone(), String::new())
                }
                2 => continue, // Authority accepted the request, but transport lost the reply.
                3 => (
                    json!({"api_version":2,"ok":true,"phase":"account_registered","operation_id":id2,"receipt":{"state":"committed","account_id":"stable-account"},"next_action":"fresh_authentication"}),
                    String::new(),
                ),
                _ => unreachable!(),
            };
            let text = serde_json::to_string(&result).unwrap();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{}",text.len(),cookie,text).unwrap();
        }
    });
    assert!(client.register(&id, "自主参与者").is_err());
    drop(client);
    let resumed = IdentityClient::new(c, PASSWORD).unwrap();
    let progress = resumed.register(&id, "自主参与者").unwrap();
    assert_eq!(progress.state, "confirmed");
    assert_eq!(progress.account_id.as_deref(), Some("stable-account"));
    assert!(resumed.register(&id, "different intent").is_err());
    server.join().unwrap();
    let log = requests.lock().unwrap();
    assert_eq!(log.len(), 4);
    assert_eq!(log[2], log[3]);
}
