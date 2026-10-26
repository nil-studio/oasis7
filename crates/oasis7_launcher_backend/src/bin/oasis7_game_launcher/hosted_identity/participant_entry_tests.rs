use super::super::{IdentityService, model::AccountRecord, now_ms};
use super::*;
use oasis7_client_api::world_service::{sign_service_response, verify_read_request};
use std::{io::Write, net::TcpListener, thread};
fn owner(key: u8, operation: &str) -> SignedReadRequest<GameParticipantRegisterV1> {
    let secret = hex::encode([key; 32]);
    let public = sign_read_request("public", (), &secret)
        .unwrap()
        .subject_public_key;
    sign_read_request(
        GAME_PARTICIPANT_REGISTER_DOMAIN,
        GameParticipantRegisterV1 {
            api_rev: 1,
            client_operation_id: operation.into(),
            identity_namespace: GameIdentityNamespaceV1 {
                service_id: "model-contract".into(),
                environment_id: "test".into(),
                instance_epoch: DecimalU64(4),
            },
            account_id: "a".into(),
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
            owner_public_key: public,
            owner_key_version: DecimalU64(1),
        },
        &secret,
    )
    .unwrap()
}
fn entry(endpoint: String) -> ParticipantEntry {
    ParticipantEntry::new(
        EntryTrust {
            endpoint,
            service_public_key: sign_read_request("public", (), &hex::encode([90; 32]))
                .unwrap()
                .subject_public_key,
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
        },
        Zeroizing::new(hex::encode([91; 32])),
    )
    .unwrap()
}
fn configure(
    s: &mut IdentityService,
    change: impl FnOnce(&mut Value) + Send + 'static,
) -> thread::JoinHandle<()> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let configured = entry(format!("http://{}", listener.local_addr().unwrap()));
    let issuer = configured.issuer_public_key.clone();
    s.participant_entry = Some(configured);
    thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let (start, len) = loop {
            let mut chunk = [0u8; 2048];
            let n = socket.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 65536);
            if let Some(start) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..start]).to_ascii_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if bytes.len() >= start + 4 + len {
                    break (start + 4, len);
                }
            }
        };
        let request: SignedReadRequest<ParticipantPolicyRequestV1> =
            parse_request(&bytes[start..start + len]).unwrap();
        request.request.validate().unwrap();
        verify_read_request(PARTICIPANT_POLICY_PATH, &request).unwrap();
        let mut view = json!({"available":true,"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","logical_tick":"20",
            "policy":{"api_rev":1,"enabled":true,"registry_complete":true,"identity_namespace":owner(8,"op").request.identity_namespace,
                "issuer_public_key":issuer,"world_id":"world","genesis_digest":"genesis","generation":1,"max_grant_ticks":30,"max_participants":100}});
        change(&mut view);
        let response = sign_service_response(
            PARTICIPANT_POLICY_PATH,
            request_digest(PARTICIPANT_POLICY_PATH, &request).unwrap(),
            view,
            &hex::encode([90; 32]),
        )
        .unwrap();
        let body = serde_json::to_vec(&response).unwrap();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(&body).unwrap();
    })
}
fn authorize(
    s: &mut IdentityService,
    token: &str,
    r: &SignedReadRequest<GameParticipantRegisterV1>,
) -> Result<Value, StoreError> {
    s.participant_route(
        "POST",
        "/participant/authorize",
        Some(token),
        &serde_json::to_value(r).unwrap(),
        now_ms(),
    )
    .unwrap()
}
#[test]
fn policy_original_preserves_caller_nonce_and_independently_verifiable_world_signature() {
    let mut f = IdentityService::model_executor_test_fixture();
    let nonce = hex::encode([0xab; 32]);
    let server = configure(&mut f.service, |_| {});
    let value = f
        .service
        .participant_route(
            "POST",
            "/participant/policy-original",
            Some(&f.token),
            &json!({"observation_nonce":nonce}),
            now_ms(),
        )
        .unwrap()
        .unwrap();
    server.join().unwrap();
    let request: SignedReadRequest<ParticipantPolicyRequestV1> =
        serde_json::from_value(value["request"].clone()).unwrap();
    let response: SignedServiceResponse<Value> =
        serde_json::from_value(value["response"].clone()).unwrap();
    assert_eq!(request.request.observation_nonce, nonce);
    assert_eq!(request.request.expected_world.world_id, "world");
    verify_read_request(PARTICIPANT_POLICY_PATH, &request).unwrap();
    let pin = &f
        .service
        .participant_entry
        .as_ref()
        .unwrap()
        .trust
        .service_public_key;
    verify_service_response(
        PARTICIPANT_POLICY_PATH,
        &request_digest(PARTICIPANT_POLICY_PATH, &request).unwrap(),
        &response,
        pin,
    )
    .unwrap();
    let mut changed = request;
    changed.request.observation_nonce = hex::encode([18; 32]);
    assert!(
        verify_service_response(
            PARTICIPANT_POLICY_PATH,
            &request_digest(PARTICIPANT_POLICY_PATH, &changed).unwrap(),
            &response,
            pin
        )
        .is_err()
    );
    for payload in [
        json!({"observation_nonce":"aa"}),
        json!({"observation_nonce":nonce.to_uppercase()}),
        json!({"observation_nonce":nonce,"world_id":"caller-world"}),
    ] {
        assert!(
            f.service
                .participant_route(
                    "POST",
                    "/participant/policy-original",
                    Some(&f.token),
                    &payload,
                    now_ms()
                )
                .unwrap()
                .is_err()
        );
    }
    assert!(
        f.service
            .participant_route(
                "POST",
                "/participant/policy-original",
                None,
                &json!({"observation_nonce":nonce}),
                now_ms()
            )
            .unwrap()
            .is_err()
    );
}
#[test]
fn current_account_grant_is_separate_from_owner_signature_and_original_binding_survives_restart() {
    let mut f = IdentityService::model_executor_test_fixture();
    let r = owner(8, "original-op");
    let server = configure(&mut f.service, |_| {});
    let result = authorize(&mut f.service, &f.token, &r).unwrap();
    server.join().unwrap();
    let admission: SignedReadRequest<GameParticipantAdmissionV1> =
        serde_json::from_value(result["admission"].clone()).unwrap();
    verify_read_request(GAME_PARTICIPANT_ADMISSION_DOMAIN, &admission).unwrap();
    assert_ne!(admission.subject_public_key, r.subject_public_key);
    assert_eq!(admission.request.valid_from_tick, DecimalU64(20));
    assert_eq!(admission.request.valid_until_tick, DecimalU64(50));
    assert_eq!(result["state"], "pending");
    f.service.store = super::super::store::IdentityStore::open(
        &f.service.config.sqlite_path,
        f.service.config.namespace.clone(),
        f.service.config.rp.clone(),
    )
    .unwrap();
    let server = configure(&mut f.service, |_| {});
    assert!(
        matches!(authorize(&mut f.service,&f.token,&owner(9,"original-op")),Err(StoreError::Rejected(code)) if code=="idempotency_conflict")
    );
    server.join().unwrap();
}
#[test]
fn wrong_subject_or_namespace_and_disabled_entry_fail_before_any_upstream_call() {
    let mut f = IdentityService::model_executor_test_fixture();
    let r = owner(8, "op");
    assert!(authorize(&mut f.service, &f.token, &r).is_err());
    f.service.participant_entry = Some(entry("http://127.0.0.1:9".into()));
    for field in ["account_id", "world_id", "namespace"] {
        let mut bad = r.clone();
        match field {
            "account_id" => bad.request.account_id = "other".into(),
            "world_id" => bad.request.world_id = "other".into(),
            _ => bad.request.identity_namespace.instance_epoch = DecimalU64(5),
        };
        assert!(
            matches!(authorize(&mut f.service,&f.token,&bad),Err(StoreError::Rejected(code)) if code=="participant_context_mismatch")
        );
    }
}
#[test]
fn incomplete_policy_wrong_issuer_or_world_never_issue_admission() {
    for field in ["registry_complete", "issuer_public_key", "genesis_digest"] {
        let mut f = IdentityService::model_executor_test_fixture();
        let server = configure(&mut f.service, move |view| {
            view["policy"][field] = if field == "registry_complete" {
                json!(false)
            } else {
                json!("wrong")
            };
        });
        assert!(authorize(&mut f.service, &f.token, &owner(8, "op")).is_err());
        server.join().unwrap();
    }
}
#[test]
fn closed_new_registration_still_issues_recovery_proof_for_canonical_reconciliation() {
    let mut f = IdentityService::model_executor_test_fixture();
    let server = configure(&mut f.service, |view| {
        view["available"] = json!(false);
        view["policy"]["enabled"] = json!(false);
    });
    let result = authorize(&mut f.service, &f.token, &owner(8, "old-op")).unwrap();
    server.join().unwrap();
    assert_eq!(result["new_registration_enabled"], false);
    assert_eq!(result["state"], "pending");
}
#[test]
fn revocation_during_policy_query_is_rechecked_before_signing() {
    for policy_original in [false, true] {
        let mut f = IdentityService::model_executor_test_fixture();
        let path = f.service.config.sqlite_path.clone();
        let ns = f.service.config.namespace.clone();
        let rp = f.service.config.rp.clone();
        let server = configure(&mut f.service, move |_| {
            let mut store = super::super::store::IdentityStore::open(&path, ns, rp).unwrap();
            store
                .transaction(|tx| {
                    let mut row = tx.get::<AccountRecord>("account:a")?.unwrap();
                    row.value.security_epoch += 1;
                    tx.replace(
                        "account:a",
                        row.row_version,
                        "account",
                        Some("a"),
                        None,
                        &row.value,
                    )
                })
                .unwrap();
        });
        let result = if policy_original {
            f.service
                .participant_route(
                    "POST",
                    "/participant/policy-original",
                    Some(&f.token),
                    &json!({"observation_nonce":hex::encode([0xab;32])}),
                    now_ms(),
                )
                .unwrap()
        } else {
            authorize(&mut f.service, &f.token, &owner(8, "op"))
        };
        assert!(matches!(result,Err(StoreError::Rejected(code)) if code=="session_expired"));
        server.join().unwrap();
    }
}
#[test]
fn upstream_configuration_rejects_public_hosts_paths_weak_keys_and_credentials() {
    for endpoint in [
        "http://example.invalid",
        "https://127.0.0.1",
        "http://127.0.0.1/path",
        "http://u:p@127.0.0.1",
        "http://127.0.0.1?x=1",
    ] {
        let trust = EntryTrust {
            endpoint: endpoint.into(),
            service_public_key: sign_read_request("public", (), &hex::encode([90; 32]))
                .unwrap()
                .subject_public_key,
            world: WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
        };
        assert!(ParticipantEntry::new(trust, Zeroizing::new(hex::encode([91; 32]))).is_err());
    }
}

#[test]
fn discovery_does_not_advertise_mismatched_or_disabled_world_authority() {
    for mismatch in [false, true] {
        let mut f = IdentityService::model_executor_test_fixture();
        let server = configure(&mut f.service, move |view| {
            if mismatch {
                view["policy"]["issuer_public_key"] = json!("wrong");
            }
        });
        let e = f.service.participant_entry.as_mut().unwrap();
        e.owner_control_enabled = true;
        e.executor_control_enabled = true;
        let v = f.service.entry_discovery("local", now_ms()).unwrap();
        server.join().unwrap();
        for key in [
            "participant_registration",
            "participant_recovery",
            "owner_control",
            "executor_control",
        ] {
            assert_eq!(v["phase_capabilities"][key], !mismatch, "{key}");
        }
        assert_eq!(v["phase_capabilities"]["complete_cold_entry"], false);
        assert_eq!(v["runtime_profiles"], json!([]));
    }
}
