//! Developer-only loopback authority fixture. Uses the real HTTP handler and isolated SQLite.
use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    time::{Duration, Instant},
};

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    // Accepted sockets can inherit the polling listener's nonblocking mode on
    // Darwin. Reset it so a fragmented body waits within the existing timeout
    // instead of panicking on WouldBlock and killing the fixture authority.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut buffer = [0_u8; 2048];
        let count = stream.read(&mut buffer).unwrap();
        assert!(
            count > 0 && bytes.len() + count <= 65536,
            "bounded fixture request"
        );
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(position) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])
        .unwrap()
        .to_owned();
    let length = headers
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 32768);
    while bytes.len() < header_end + length {
        let mut buffer = [0_u8; 2048];
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    (headers, bytes[header_end..header_end + length].to_vec())
}

/// Run with explicit ready/stop paths; no model, deployed authority, credential or world calls.
#[test]
#[ignore = "developer loopback HTTP fixture; invoked by native-identity.integration.test.mjs"]
fn serve_native_identity_http_fixture() {
    let Some(ready) = std::env::var_os("OASIS7_CUSTODY_NATIVE_HTTP_READY") else {
        return;
    };
    let ready = PathBuf::from(ready);
    let stop = PathBuf::from(
        std::env::var_os("OASIS7_CUSTODY_NATIVE_HTTP_STOP").expect("explicit fixture stop path"),
    );
    let (mut fixture, key) = fixture();
    let managed_binding = std::env::var("OASIS7_CUSTODY_NATIVE_HTTP_MANAGED_BINDING")
        .ok()
        .as_deref()
        == Some("true");
    if managed_binding {
        fixture.service.custody.as_mut().unwrap().managed_binding =
            Some(managed_binding::ManagedBindingConfig::native_http_fixture());
        fixture
            .service
            .custody
            .as_mut()
            .unwrap()
            .managed_binding_state = managed_binding::ManagedBindingConfigState::Ready;
        fixture.service.config.registration_enabled = true;
        fixture.service.config.machine_registration_enabled = true;
    }
    {
        let custody = fixture.service.custody.as_mut().unwrap();
        custody.revocation = Some(revocation::RevocationConfig::http_fixture(
            custody.audience.clone(),
        ));
        custody
            .attach_revocation(
                &mut fixture.service.store,
                fixture.service.config.namespace.key(),
                fixture.service.config.namespace.environment_id.clone(),
                fixture.service.config.rp.identity_instance_epoch,
            )
            .unwrap();
    }
    let now = now_ms();
    let start = fixture
        .service
        .custody_options(Some(&fixture.token), &options(now), "fixture", now)
        .unwrap();
    let complete = fixture
        .service
        .custody_complete(
            Some(&fixture.token),
            &complete_payload(&start, &key, 0x05),
            "fixture",
            now,
        )
        .unwrap();
    let request = verify_request(&complete);
    let mut authenticator_counter = 1_u32;
    let authority = fixture.service.custody.as_ref().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let data = json!({
        "fixture_label":"public developer data in isolated native SQLite; never deployment credentials",
        "managed_binding_owner_pin": authority.managed_binding.as_ref().map(|c| c.fixture_pin()),
        "managed_binding_fixture_seed_note": managed_binding.then_some("Public test-only Ed25519 seed [93;32]; isolated developer fixture, never deployment credentials"),
        "address": format!("http://{}", listener.local_addr().unwrap()),
        "cookie": format!("__Host-oasis7_sid={}", fixture.token),
        "revocation_poll_secret":hex::encode([83_u8;32]),
        "revocation_ack_secret":hex::encode([84_u8;32]),
        "owner_source_key_id":"owner-source-test",
        "owner_source_public_key":hex::encode(SigningKey::from_bytes(&[92;32]).verifying_key().to_bytes()),
        "csrf": digest(&fixture.service.config.session_key, "csrf", &fixture.token),
        "verify_request": request,
        "namespace": fixture.service.config.namespace.key(),
        "identity_instance_epoch": fixture.service.config.rp.identity_instance_epoch.to_string(),
        "key_id": authority.key_id,
        "public_key": hex::encode(authority.key.verifying_key().to_bytes()),
        "audience": authority.audience,
        "adapter_secret": hex::encode(authority.adapter_secret),
    });
    let mut file = std::fs::OpenOptions::new();
    file.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        file.mode(0o600);
    }
    file.open(&ready)
        .unwrap()
        .write_all(&serde_json::to_vec(&data).unwrap())
        .unwrap();
    let mut issuer = crate::hosted_player_session::HostedPlayerSessionIssuer::default();
    let deadline = Instant::now() + Duration::from_secs(180);
    while !stop.exists() && Instant::now() < deadline {
        let (mut stream, _) = match listener.accept() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        };
        let (headers, body) = read_request(&mut stream);
        let mut first = headers.lines().next().unwrap().split_whitespace();
        let method = first.next().unwrap();
        let path = first.next().unwrap();
        let payload = if body.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&body).unwrap()
        };
        let reply = if method == "POST" && path == "/_fixture/approve-intent" {
            // Test-only authenticator owns this synthetic Passkey. The real
            // options/UV completion and SQLite proof persistence are unchanged.
            let now = now_ms();
            match fixture.service.custody_options(
                Some(&fixture.token),
                &payload,
                "provision-fixture",
                now,
            ) {
                Ok(start) => {
                    authenticator_counter = authenticator_counter.checked_add(1).unwrap();
                    let mut payload = complete_payload(&start, &key, 0x05);
                    payload["response"] =
                        assertion_with_counter(&start, &key, 0x05, authenticator_counter);
                    match fixture.service.custody_complete(
                        Some(&fixture.token),
                        &payload,
                        "provision-fixture",
                        now,
                    ) {
                        Ok(value) => crate::hosted_identity::service::IdentityReply::ok(value),
                        Err(error) => crate::hosted_identity::service::IdentityReply::error(error),
                    }
                }
                Err(error) => crate::hosted_identity::service::IdentityReply::error(error),
            }
        } else if method == "GET" && path == "/_fixture/revocation-status" {
            let (pending, acknowledged) = fixture
                .service
                .store
                .transaction(|tx| {
                    let rows =
                        tx.all::<revocation::SourceOutbox>("custody_source_revocation", None)?;
                    Ok((
                        rows.iter()
                            .filter(|(_, r)| r.value.acknowledgement.is_none())
                            .count(),
                        rows.iter()
                            .filter(|(_, r)| r.value.acknowledgement.is_some())
                            .count(),
                    ))
                })
                .unwrap();
            crate::hosted_identity::service::IdentityReply::ok(
                json!({"pending":pending,"acknowledged":acknowledged}),
            )
        } else if method == "POST" && path == "/_fixture/revoke-manager" {
            crate::hosted_identity::service::IdentityReply::ok(
                fixture
                    .service
                    .logout(Some(&fixture.token), now_ms())
                    .unwrap(),
            )
        } else if method == "POST"
            && matches!(
                path,
                "/_fixture/expire" | "/_fixture/native-key" | "/_fixture/reset"
            )
        {
            let storage_key = session_key(&fixture.service.config.session_key, &fixture.token);
            fixture
                .service
                .store
                .transaction(|tx| {
                    let mut saved = tx.get::<AccountSession>(&storage_key)?.unwrap();
                    if path == "/_fixture/expire" {
                        saved.value.expires_at = now_ms() - 1;
                    } else if path == "/_fixture/native-key" {
                        saved.value.auth_method = "native_key".into();
                    } else {
                        saved.value.auth_method = "webauthn".into();
                        saved.value.expires_at = now_ms() + 300_000;
                        saved.value.idle_expires_at = now_ms() + 300_000;
                    }
                    tx.replace(
                        &storage_key,
                        saved.row_version,
                        "session",
                        Some("a"),
                        Some(saved.value.expires_at),
                        &saved.value,
                    )
                })
                .unwrap();
            crate::hosted_identity::service::IdentityReply::ok(json!({"fixture_updated":true}))
        } else {
            fixture.service.handle(
                method,
                path,
                &headers,
                payload,
                "loopback-fixture",
                &mut issuer,
            )
        };
        let bytes = serde_json::to_vec(&reply.body).unwrap();
        write!(stream, "HTTP/1.1 {} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n", reply.status, bytes.len()).unwrap();
        for cookie in &reply.cookies {
            assert!(
                cookie.bytes().all(|byte| byte != b'\r' && byte != b'\n'),
                "fixture cookie must be one header value"
            );
            write!(stream, "Set-Cookie: {cookie}\r\n").unwrap();
        }
        stream.write_all(b"\r\n").unwrap();
        stream.write_all(&bytes).unwrap();
    }
    std::fs::remove_file(ready).unwrap();
}
