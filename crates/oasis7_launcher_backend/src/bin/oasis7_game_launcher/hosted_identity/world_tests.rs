use super::*;

#[test]
fn encrypted_slot_token_cannot_move_between_accounts_or_be_tampered() {
    let secret = [9; 32];
    let value = seal_slot_token(&secret, "control:account:a", "private-slot-token").unwrap();
    assert!(!value.contains("private-slot-token"));
    assert_eq!(
        open_slot_token(&secret, "control:account:a", &value).unwrap(),
        "private-slot-token"
    );
    assert!(open_slot_token(&secret, "control:account:b", &value).is_err());
    assert!(open_slot_token(&[8; 32], "control:account:a", &value).is_err());
    let mut tampered = value.clone();
    tampered.push('0');
    assert!(open_slot_token(&secret, "control:account:a", &tampered).is_err());
}

#[test]
fn world_rpc_rejects_non_loopback_and_wrong_world_before_issuing_authority() {
    assert!(runtime::query("192.0.2.1:5023", "world", "player").is_err());
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        writeln!(
            stream,
            "{}",
            json!({"type":"hello_ack","world_id":"another-world"})
        )
        .unwrap();
    });
    assert!(runtime::query(&addr, "world", "player").is_err());
    server.join().unwrap();
}

fn session() -> AccountSession {
    AccountSession {
        session_id: "parent".into(),
        token_digest: "digest".into(),
        account_id: "account".into(),
        origin_credential_id: "credential".into(),
        origin_credential_version: 1,
        issued_security_epoch: 1,
        identity_instance_epoch: 1,
        auth_method: "webauthn".into(),
        auth_time: 1000,
        expires_at: u64::MAX,
        idle_expires_at: u64::MAX,
        revoked_at: None,
        recovery_context: None,
        csrf_digest: "csrf".into(),
    }
}
fn control() -> WorldControl {
    WorldControl {
        account_id: "account".into(),
        player_id: "player".into(),
        world_id: "world".into(),
        encrypted_slot_token: "ciphertext".into(),
        current: Some(WorldLease {
            parent_session_id: "old-parent".into(),
            public_key: "old-key".into(),
            device_session_id: "old-device".into(),
            join_attempt_id: random_token(),
            world_epoch: 7,
            created_at: 1,
        }),
        pending: None,
        releasing: false,
        retired_attempts: Default::default(),
        last_release: None,
    }
}

#[test]
fn takeover_requires_confirmation_fresh_factor_and_exact_epoch_and_preserves_old_control() {
    let s = session();
    let mut c = control();
    let old = c.current.as_ref().unwrap().device_session_id.clone();
    let fence = runtime::RuntimeFence {
        key: Some("old-key".into()),
        epoch: 7,
    };
    let attempt = random_token();
    let mut p = json!({"join_attempt_id":attempt,"expected_epoch":7});
    assert!(prepare_join(&mut c, &s, &p, "new-key", &fence, 2000).is_err());
    p["takeover"] = json!(true);
    p["expected_epoch"] = json!(6);
    assert!(prepare_join(&mut c, &s, &p, "new-key", &fence, 2000).is_err());
    p["expected_epoch"] = json!(7);
    assert!(prepare_join(&mut c, &s, &p, "new-key", &fence, 302_000).is_err());
    prepare_join(&mut c, &s, &p, "new-key", &fence, 2000).unwrap();
    assert_eq!(c.current.as_ref().unwrap().device_session_id, old);
    assert_eq!(c.pending.as_ref().unwrap().world_epoch, 8);
    assert!(prepare_join(&mut c, &s, &p, "different-key", &fence, 2000).is_err());
}

#[test]
fn stale_release_request_cannot_target_new_controller() {
    let s = session();
    let mut c = control();
    c.current.as_mut().unwrap().parent_session_id = s.session_id.clone();
    let request = json!({"device_session_id":"old-device","expected_epoch":7});
    assert!(validate_control_request(&c, &s, &request, "old-key").is_ok());
    c.current.as_mut().unwrap().world_epoch = 8;
    c.current.as_mut().unwrap().device_session_id = "new-device".into();
    assert!(validate_control_request(&c, &s, &request, "old-key").is_err());
}

#[test]
fn world_cookie_adapter_protocol_child() {
    if std::env::var("OASIS7_TEST_WORLD_ADAPTER_CHILD").is_err() {
        return;
    }
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    listener.set_nonblocking(true).unwrap();
    let state = Arc::new(Mutex::new(runtime::RuntimeFence {
        key: None,
        epoch: 0,
    }));
    let stop = Arc::new(AtomicBool::new(false));
    let thread_state = state.clone();
    let thread_stop = stop.clone();
    let server = std::thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let hello: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(hello["type"], "hello");
            writeln!(stream, "{}", json!({"type":"hello_ack","world_id":"world"})).unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let mode = request["command"]["mode"].as_str().unwrap();
            let mut fence = thread_state.lock().unwrap();
            let ack = if mode == "reconnect_sync" {
                json!({"player_id":request["command"]["request"]["player_id"],"session_pubkey":fence.key,"session_epoch":fence.epoch})
            } else {
                assert_eq!(mode, "release_hosted_session");
                let grant = request["command"]["request"]["authorization_grant"]
                    .as_str()
                    .unwrap();
                let payload = hex::decode(grant.split('.').nth(1).unwrap()).unwrap();
                let context: HostedAuthContextV2 = serde_json::from_slice(&payload).unwrap();
                assert_eq!(context.capabilities, vec!["player_session_release"]);
                assert_eq!(context.session_public_key, fence.key.as_deref().unwrap());
                assert_eq!(context.world_session_epoch, fence.epoch);
                fence.key = None;
                fence.epoch += 1;
                json!({"status":"session_revoked","player_id":context.player_id,"session_epoch":fence.epoch})
            };
            writeln!(
                stream,
                "{}",
                json!({"type":"authoritative_recovery_ack","ack":ack})
            )
            .unwrap();
        }
    });
    let temp = std::env::temp_dir().join(format!("oasis7-world-adapter-{}", random_token()));
    std::fs::create_dir_all(&temp).unwrap();
    let config = super::super::config::IdentityConfig {
        namespace: IdentityNamespace {
            identity_service_id: "identity".into(),
            environment_id: "test".into(),
        },
        rp: RpConfig {
            rp_id: "world.test".into(),
            allowed_origins: vec!["https://world.test".into()],
            version: 1,
            identity_instance_epoch: 1,
        },
        sqlite_path: temp.join("identity.sqlite"),
        registration_enabled: true,
        machine_registration_enabled: false,
        session_key: [9; 32],
        world_scope: "scope".into(),
        runtime_live_bind: addr,
        tablestore: None,
        trusted_proxy_loopback: false,
    };
    let mut service = IdentityService::new(config).unwrap();
    let token = random_token();
    let second_token = random_token();
    let now = now_ms();
    let account = AccountRecord {
        account_id: "account".into(),
        user_handle: vec![1; 16],
        display_name: "player".into(),
        status: "active".into(),
        policy_version: 1,
        security_epoch: 1,
        has_ever_public_key: true,
        created_at: now,
        last_authenticated_at: Some(now),
        last_recovery_id: None,
    };
    let credential = CredentialRecord {
        credential_record_id: "credential".into(),
        account_id: "account".into(),
        state: "active".into(),
        version: 1,
        label: "passkey".into(),
        created_at: now,
        last_used_at: Some(now),
        public_material: CredentialMaterial::WebAuthn {
            rp_id: "world.test".into(),
            credential_id: vec![2; 32],
            user_handle: vec![1; 16],
            library_record: json!({}),
            library_record_version: 1,
        },
        registered_via: "new_account".into(),
    };
    service
        .store
        .transaction(|tx| {
            tx.create(
                "account:account",
                "account",
                Some("account"),
                None,
                &account,
            )?;
            tx.create(
                "credential:credential",
                "credential",
                Some("account"),
                None,
                &credential,
            )?;
            for (token, id) in [(&token, "parent"), (&second_token, "second-parent")] {
                let mut s = session();
                s.session_id = id.into();
                s.auth_time = now;
                s.expires_at = now + 86_400_000;
                s.idle_expires_at = s.expires_at;
                tx.create(
                    &session_key(&[9; 32], token),
                    "session",
                    Some("account"),
                    Some(s.expires_at),
                    &s,
                )?;
            }
            Ok(())
        })
        .unwrap();
    let key1 = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[3; 32])
            .verifying_key()
            .to_bytes(),
    );
    let key2 = hex::encode(
        ed25519_dalek::SigningKey::from_bytes(&[4; 32])
            .verifying_key()
            .to_bytes(),
    );
    let mut issuer = HostedPlayerSessionIssuer::default();
    let request = json!({"public_key":key1,"join_attempt_id":random_token()});
    let join = service
        .world_route(
            "POST",
            "/world/join",
            Some(&token),
            &request,
            &mut issuer,
            now_ms(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(join["world"]["state"], "registration_pending");
    assert!(join["grant"].get("release_token").is_none());
    let device1 = join["grant"]["device_session_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(join["grant"]["world_session_epoch"], 1);
    *state.lock().unwrap() = runtime::RuntimeFence {
        key: Some(key1.clone()),
        epoch: 1,
    };
    let refresh1 = json!({"public_key":key1,"device_session_id":device1,"expected_epoch":1});
    assert_eq!(
        service
            .world_route(
                "POST",
                "/world/refresh",
                Some(&token),
                &refresh1,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .unwrap()["world"]["state"],
        "bound"
    );
    let takeover = json!({"public_key":key2,"join_attempt_id":random_token(),"takeover":true,"expected_epoch":1});
    let candidate = service
        .world_route(
            "POST",
            "/world/join",
            Some(&second_token),
            &takeover,
            &mut issuer,
            now_ms(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(candidate["grant"]["world_session_epoch"], 2);
    // Pending candidates reconcile only by replaying their original join.
    // Refresh is intentionally current-only, including before runtime registration.
    let pending_refresh = json!({"public_key":key2,"device_session_id":candidate["grant"]["device_session_id"],"expected_epoch":2});
    assert!(
        service
            .world_route(
                "POST",
                "/world/refresh",
                Some(&second_token),
                &pending_refresh,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .is_err()
    );
    let replay = service
        .world_route(
            "POST",
            "/world/join",
            Some(&second_token),
            &takeover,
            &mut issuer,
            now_ms(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(replay["world"]["state"], "registration_pending");
    assert_eq!(
        replay["grant"]["device_session_id"],
        candidate["grant"]["device_session_id"]
    );
    // The candidate has not bound: original controller and slot still work.
    assert!(
        service
            .world_route(
                "POST",
                "/world/refresh",
                Some(&token),
                &refresh1,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .is_ok()
    );
    *state.lock().unwrap() = runtime::RuntimeFence {
        key: Some(key2.clone()),
        epoch: 2,
    };
    assert!(
        service
            .world_route(
                "POST",
                "/world/release",
                Some(&token),
                &refresh1,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .is_err()
    );
    assert_eq!(state.lock().unwrap().key.as_deref(), Some(key2.as_str()));
    // Lost register ACK: original attempt now reports authoritative bound.
    let recovered = service
        .world_route(
            "POST",
            "/world/join",
            Some(&second_token),
            &takeover,
            &mut issuer,
            now_ms(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(recovered["world"]["state"], "bound");
    assert_eq!(recovered["join_attempt_id"], takeover["join_attempt_id"]);
    assert_eq!(
        recovered["grant"]["device_session_id"],
        candidate["grant"]["device_session_id"]
    );
    let refresh2 = json!({"public_key":key2,"device_session_id":candidate["grant"]["device_session_id"],"expected_epoch":2});
    assert!(
        service
            .world_route(
                "POST",
                "/world/refresh",
                Some(&second_token),
                &refresh2,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .is_ok()
    );
    assert_eq!(
        service
            .world_route(
                "POST",
                "/world/release",
                Some(&second_token),
                &refresh2,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .unwrap()["world"]["state"],
        "released"
    );
    assert!(state.lock().unwrap().key.is_none());
    assert!(
        service
            .world_route(
                "POST",
                "/world/release",
                Some(&second_token),
                &refresh2,
                &mut issuer,
                now_ms()
            )
            .unwrap()
            .is_ok()
    );
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
    drop(service);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn world_adapter_roundtrip_keeps_parent_and_fences_late_release() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "hosted_identity::world::tests::world_cookie_adapter_protocol_child",
            "--nocapture",
        ])
        .env("OASIS7_TEST_WORLD_ADAPTER_CHILD", "1")
        .env("OASIS7_HOSTED_WORLD_ID", "world")
        .env("OASIS7_HOSTED_WORLD_ADMISSION_MODE", "open_preview")
        .env(
            "OASIS7_HOSTED_AUTHORIZATION_ISSUER_PRIVATE_KEY",
            hex::encode([7; 32]),
        )
        .env_remove("OASIS7_HOSTED_IDENTITY_LEGACY_SNAPSHOT")
        .env_remove("OASIS7_HOSTED_ACCOUNT_STORE_PATH")
        .env_remove("OASIS7_HOSTED_EMAIL_ENABLED")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
