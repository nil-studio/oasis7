//! Local-only real browser WebAuthn fixture. No synthetic credential/proof/status route.
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    time::{Duration, Instant},
};
fn receive(stream: &mut TcpStream) -> (String, Vec<u8>) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut data = Vec::new();
    let end = loop {
        let mut b = [0; 2048];
        let n = stream.read(&mut b).unwrap();
        assert!(
            n > 0 && data.len() + n <= 65536,
            "bounded local HTTP request"
        );
        data.extend_from_slice(&b[..n]);
        if let Some(i) = data.windows(4).position(|b| b == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let head = String::from_utf8(data[..end].to_vec()).unwrap();
    let length = head
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    let execution_prepare = head.lines().next().is_some_and(|line| {
        line.split_whitespace().nth(1)
            == Some(
                oasis7_custody_core::native_execution_prepare_v2::NATIVE_EXECUTION_PREPARE_PATH_V2,
            )
    });
    assert!(length <= if execution_prepare { 65536 } else { 32768 });
    while data.len() < end + length {
        let mut b = [0; 2048];
        let n = stream.read(&mut b).unwrap();
        assert!(n > 0);
        data.extend_from_slice(&b[..n]);
    }
    (head, data[end..end + length].to_vec())
}
fn reply(stream: &mut TcpStream, value: crate::hosted_identity::IdentityReply) {
    let bytes = serde_json::to_vec(&value.body).unwrap();
    write!(stream,"HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",value.status,bytes.len()).unwrap();
    for cookie in value.cookies {
        assert!(!cookie.contains(['\r', '\n']));
        write!(stream, "Set-Cookie: {cookie}\r\n").unwrap();
    }
    stream.write_all(b"\r\n").unwrap();
    stream.write_all(&bytes).unwrap();
}
#[test]
#[ignore = "explicit local browser harness only; owns no deployed credentials"]
fn serve_custody_browser_uv_fixture() {
    let Some(ready) = std::env::var_os("OASIS7_CUSTODY_BROWSER_READY") else {
        return;
    };
    let ready = PathBuf::from(ready);
    let stop =
        PathBuf::from(std::env::var_os("OASIS7_CUSTODY_BROWSER_STOP").expect("explicit stop"));
    let origin =
        std::env::var("OASIS7_CUSTODY_BROWSER_ORIGIN").expect("explicit HTTPS loopback origin");
    let u = url::Url::parse(&origin).unwrap();
    assert!(
        u.scheme() == "https"
            && u.host_str() == Some("localhost")
            && u.origin().ascii_serialization() == origin
    );
    let world3 = std::env::var("OASIS7_CUSTODY_BROWSER_WORLD3").as_deref() == Ok("true");
    let callback_root = world3.then(|| {
        let path =
            std::env::var_os("OASIS7_CUSTODY_BROWSER_CALLBACK_CA").expect("explicit fixture CA");
        std::fs::read(path).expect("public fixture CA bytes")
    });
    let root = ready.parent().unwrap();
    let database = root.join("browser-native.sqlite");
    assert!(!database.exists(), "fresh isolated SQLite required");
    let config = crate::hosted_identity::IdentityConfig {
        namespace: crate::hosted_identity::model::IdentityNamespace {
            identity_service_id: "model-contract".into(),
            environment_id: "test".into(),
        },
        rp: crate::hosted_identity::model::RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec![origin.clone()],
            version: 1,
            identity_instance_epoch: 4,
        },
        sqlite_path: database,
        registration_enabled: true,
        machine_registration_enabled: true,
        session_key: [19; 32],
        world_scope: "test-world".into(),
        runtime_live_bind: String::new(),
        tablestore: None,
        trusted_proxy_loopback: false,
    };
    let mut service = crate::hosted_identity::IdentityService::new(config).unwrap();
    service.custody = Some(CustodyConfig {
        key: SigningKey::from_bytes(&[51; 32]),
        key_id: "approval-test-1".into(),
        audience: "account-authority:test".into(),
        adapter_secret: [52; 32],
        revocation: None,
        managed_binding: Some(managed_binding::ManagedBindingConfig::native_http_fixture()),
        managed_binding_state: managed_binding::ManagedBindingConfigState::Ready,
        model_budget_summary_contract_enabled: std::env::var("OASIS7_CUSTODY_BROWSER_SESSION_V2")
            .as_deref()
            == Ok("true"),
        full_session_v2_summary_contract_enabled: std::env::var(
            "OASIS7_CUSTODY_BROWSER_SESSION_V2",
        )
        .as_deref()
            == Ok("true"),
    });
    service.custody_management_enabled = true;
    let trusted: Value = serde_json::from_str(
        &std::env::var("OASIS7_CUSTODY_BROWSER_WORLD_TRUST").expect("actual World fixture trust"),
    )
    .unwrap();
    let mut participant = crate::hosted_identity::participant_entry::ParticipantEntry::new(
        crate::hosted_identity::participant_entry::EntryTrust {
            endpoint: trusted["endpoint"].as_str().unwrap().into(),
            service_public_key: trusted["world"]["service_public_key"]
                .as_str()
                .unwrap()
                .into(),
            world: serde_json::from_value(trusted["world"].clone()).unwrap(),
        },
        zeroize::Zeroizing::new(hex::encode([95; 32])),
    )
    .unwrap();
    participant.owner_control_enabled = true;
    service.participant_entry = Some(participant);
    if world3 {
        cleanup_config::MachineConfig::from_env().expect("fixed World3 machine pins before MFA");
        service
            .store
            .attach_machine_restore_guard_from_env(&service.config.sqlite_path)
            .unwrap();
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let authority = service.custody.as_ref().unwrap();
    let data = json!({"api_version":1,"address":format!("http://{}",listener.local_addr().unwrap()),"namespace":service.config.namespace.key(),"identity_instance_epoch":"4",
        "key_id":authority.key_id,"public_key":hex::encode(authority.key.verifying_key().to_bytes()),"audience":authority.audience,
        "managed_binding_owner_pin":authority.managed_binding.as_ref().unwrap().fixture_pin()});
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(&ready)
        .unwrap()
        .write_all(&serde_json::to_vec(&data).unwrap())
        .unwrap();
    let shared = std::sync::Arc::new(std::sync::Mutex::new(Some(service)));
    let mut issuer = crate::hosted_player_session::HostedPlayerSessionIssuer::default();
    let end = Instant::now() + Duration::from_secs(600);
    let mut manager: Option<String> = None;
    let mut bypass = 0u64;
    while !stop.exists() && Instant::now() < end {
        let (mut stream, _) = match listener.accept() {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(_) => panic!("fixture accept failed"),
        };
        let (headers, body) = receive(&mut stream);
        let mut first = headers.lines().next().unwrap().split_whitespace();
        let method = first.next().unwrap();
        let path = first.next().unwrap();
        let payload = if body.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&body).unwrap()
        };
        use oasis7_custody_core::native_cleanup_v2::{
            NATIVE_ACTIVATE_V2_PATH, NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
            NATIVE_PREFLIGHT_POLICY_V2_PATH, NATIVE_REGISTER_V2_PATH,
        };
        let owned_route = [
            NATIVE_PREFLIGHT_POLICY_V2_PATH,
            NATIVE_PREFLIGHT_AUTHORIZE_V2_PATH,
            NATIVE_REGISTER_V2_PATH,
            NATIVE_ACTIVATE_V2_PATH,
            oasis7_custody_core::native_execution_prepare_v2::NATIVE_EXECUTION_PREPARE_PATH_V2,
        ]
        .contains(&path);
        let value = if world3 && owned_route {
            assert_eq!(method, "POST");
            // No service guard crosses actual World/callback HTTP. The production
            // static dispatcher owns prepare/send/reentry and its real mutex.
            // The internal socket helper owns framing; retain all original
            // authority headers but avoid duplicate hop-local length fields.
            let forwarding = headers
                .lines()
                .filter(|line| {
                    !line.split_once(':').is_some_and(|(key, _)| {
                        key.eq_ignore_ascii_case("content-length")
                            || key.eq_ignore_ascii_case("connection")
                    })
                })
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join("\r\n")
                + "\r\n\r\n";
            crate::static_http::cleanup_dispatch_with_root_for_test(
                shared.clone(),
                path,
                &forwarding,
                &body,
                callback_root.clone(),
            )
        } else {
            let mut guard = shared.lock().unwrap();
            let service = guard.as_mut().unwrap();
            if path == "/_fixture/approve-intent" {
                bypass += 1;
                crate::hosted_identity::IdentityReply::error(reject("fixture_mfa_bypass_forbidden"))
            } else if method == "GET" && path == "/_fixture/browser-facts" {
                crate::hosted_identity::IdentityReply::ok(
                    json!({"bypass_attempts":bypass,"manager_configured":manager.is_some(),
                    "world3_enabled":world3,"protected_storage_ready":world3 && service.store.require_guarded_machine_storage().is_ok()}),
                )
            } else if method == "POST" && path == "/_fixture/browser-manager" {
                let token =
                    crate::hosted_identity::service::cookie_value(&headers, "__Host-oasis7_sid")
                        .unwrap();
                assert_eq!(
                    crate::hosted_identity::header(&headers, "origin"),
                    Some(origin.as_str())
                );
                let expected = digest(&service.config.session_key, "csrf", token);
                assert!(
                    crate::hosted_identity::header(&headers, "x-oasis7-csrf")
                        == Some(expected.as_str()),
                    "actual browser CSRF required"
                );
                let (account, session) = service.session(Some(token), now_ms()).unwrap();
                assert_eq!(session.auth_method, "webauthn");
                assert!(
                    manager.as_ref().is_none_or(|m| m == &account.account_id),
                    "manager fixed after real browser registration"
                );
                if manager.is_none() {
                    let custody = service.custody.as_mut().unwrap();
                    custody.revocation = Some(revocation::RevocationConfig::browser_uv_fixture(
                        custody.audience.clone(),
                        account.account_id.clone(),
                    ));
                    custody
                        .attach_revocation(
                            &mut service.store,
                            service.config.namespace.key(),
                            "test".into(),
                            4,
                        )
                        .unwrap();
                    manager = Some(account.account_id.clone());
                }
                crate::hosted_identity::IdentityReply::ok(
                    json!({"manager_account_id":account.account_id}),
                )
            } else if path.starts_with("/_fixture/") {
                crate::hosted_identity::IdentityReply::error(reject("fixture_route_closed"))
            } else {
                service.handle(
                    method,
                    path,
                    &headers,
                    payload,
                    "loopback-browser-fixture",
                    &mut issuer,
                )
            }
        };
        reply(&mut stream, value);
    }
}
