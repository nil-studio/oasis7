use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn request(key: &SigningKey) -> Value {
    json!({"operation_id":random_token(),"public_key":hex::encode(key.verifying_key().to_bytes()),"display_name":"自主参与者"})
}
fn options(s: &mut IdentityService, p: &Value) -> Value {
    s.machine_registration_options("owner", p, "source", "https://localhost", 100)
        .unwrap()
}
fn proof(start: &Value, key: &SigningKey) -> Value {
    let bytes = URL_SAFE_NO_PAD
        .decode(start["options"]["signed_bytes"].as_str().unwrap())
        .unwrap();
    json!({"operation_id":start["operation_id"],"ceremony_id":start["ceremony_id"],"signature":URL_SAFE_NO_PAD.encode(key.sign(&bytes).to_bytes())})
}
fn complete(s: &mut IdentityService, p: &Value, now: u64) -> Result<Value, StoreError> {
    s.machine_registration_complete("owner", p, "https://localhost", now)
}
fn enable(s: &mut IdentityService) {
    s.config.registration_enabled = true;
    s.config.machine_registration_enabled = true;
}
#[test]
fn registration_replays_original_receipt_after_restart_and_does_not_issue_sessions_or_world_effects()
 {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    enable(s);
    let key = SigningKey::from_bytes(&[76; 32]);
    let p = request(&key);
    let start = options(s, &p);
    assert_eq!(start, options(s, &p));
    let original = proof(&start, &key);
    let result = complete(s, &original, 101).unwrap();
    assert_eq!(result["phase"], "account_registered");
    assert_eq!(result["world"]["state"], "not_requested");
    assert!(result.get("token").is_none());
    let account = result["receipt"]["account_id"].as_str().unwrap();
    assert!(
        s.store
            .transaction(|tx| tx.get::<AccountRecord>(&format!("account:{account}")))
            .unwrap()
            .is_some()
    );
    s.store = super::super::store::IdentityStore::open(
        &s.config.sqlite_path,
        s.config.namespace.clone(),
        s.config.rp.clone(),
    )
    .unwrap();
    s.config.registration_enabled = false;
    s.config.machine_registration_enabled = false;
    assert_eq!(result, complete(s, &original, 1_000_000).unwrap());
    assert_eq!(result, options(s, &p));
    // Registered machine key logs into the same existing account with registration closed.
    let login = s
        .native_key_route(
            "POST",
            "/native-key/options",
            None,
            "owner",
            &json!({"public_key":p["public_key"],"intent":"login"}),
            "source",
            "https://localhost",
            1_000_001,
        )
        .unwrap()
        .unwrap()
        .0;
    let reply = s
        .native_key_route(
            "POST",
            "/native-key/complete",
            None,
            "owner",
            &proof(&login, &key),
            "source",
            "https://localhost",
            1_000_002,
        )
        .unwrap()
        .unwrap();
    assert_eq!(reply.0["account"]["account_id"], account);
    assert!(reply.1.is_some());
}
#[test]
fn both_registration_switches_are_required_and_rechecked_at_commit() {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    let key = SigningKey::from_bytes(&[76; 32]);
    let p = request(&key);
    for (general, machine) in [(false, false), (true, false), (false, true)] {
        s.config.registration_enabled = general;
        s.config.machine_registration_enabled = machine;
        assert!(
            s.machine_registration_options("owner", &p, "source", "https://localhost", 100)
                .is_err()
        );
    }
    enable(s);
    let start = options(s, &p);
    s.config.registration_enabled = false;
    assert!(complete(s, &proof(&start, &key), 101).is_err());
    s.config.registration_enabled = true;
    s.config.machine_registration_enabled = false;
    assert!(complete(s, &proof(&start, &key), 101).is_err());
    enable(s);
    assert!(complete(s, &proof(&start, &key), 101).is_ok());
}
#[test]
fn registration_rejects_cross_owner_origin_purpose_namespace_expiry_and_altered_original() {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    enable(s);
    let key = SigningKey::from_bytes(&[76; 32]);
    let p = request(&key);
    let start = options(s, &p);
    let original = proof(&start, &key);
    assert!(
        s.machine_registration_complete("other", &original, "https://localhost", 101)
            .is_err()
    );
    assert!(
        s.machine_registration_complete("owner", &original, "https://other", 101)
            .is_err()
    );
    s.config.namespace.environment_id = "other".into();
    assert!(complete(s, &original, 101).is_err());
    s.config.namespace.environment_id = "test".into();
    // Fixture's environment is test; expired proof cannot create the account.
    assert!(complete(s, &original, 120100).is_err());
    let wrong = proof(&start, &SigningKey::from_bytes(&[77; 32]));
    assert!(complete(s, &wrong, 101).is_err());
    let result = complete(s, &original, 101).unwrap();
    let mut altered = original.clone();
    altered["signature"] = wrong["signature"].clone();
    assert!(complete(s, &altered, 102).is_err());
    assert_eq!(result, complete(s, &original, 102).unwrap());
    let mut changed = p.clone();
    changed["display_name"] = json!("different");
    assert!(
        s.machine_registration_options("owner", &changed, "source", "https://localhost", 102)
            .is_err()
    );
}
#[test]
fn concurrent_challenges_for_one_public_key_cannot_create_duplicate_accounts() {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    enable(s);
    let key = SigningKey::from_bytes(&[76; 32]);
    let first = options(s, &request(&key));
    let second = options(s, &request(&key));
    let result = complete(s, &proof(&first, &key), 101).unwrap();
    assert!(complete(s, &proof(&second, &key), 102).is_err());
    let lookup = s
        .store
        .transaction(|tx| {
            tx.get::<CredentialLookup>(&native_key::lookup_key(&key.verifying_key().to_bytes()))
        })
        .unwrap()
        .unwrap();
    assert_eq!(lookup.value.account_id, result["receipt"]["account_id"]);
    // Credential tombstones remain reserved, preventing account recreation.
    s.store
        .transaction(|tx| {
            let mut c = tx
                .get::<CredentialRecord>(&credential_key(&lookup.value.credential_record_id))?
                .unwrap();
            c.value.state = "revoked".into();
            tx.replace(
                &credential_key(&c.value.credential_record_id),
                c.row_version,
                "credential",
                Some(&c.value.account_id),
                None,
                &c.value,
            )
        })
        .unwrap();
    let third = options(s, &request(&key));
    assert!(complete(s, &proof(&third, &key), 103).is_err());
}
#[test]
fn invalid_requests_and_exhausted_proof_attempts_are_rejected() {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    enable(s);
    let key = SigningKey::from_bytes(&[76; 32]);
    let p = request(&key);
    for field in ["operation_id", "public_key", "display_name"] {
        let mut bad = p.clone();
        bad[field] = json!("");
        assert!(
            s.machine_registration_options("owner", &bad, "source", "https://localhost", 100)
                .is_err()
        );
    }
    let mut bad = p.clone();
    bad["account_id"] = json!("a");
    assert!(
        s.machine_registration_options("owner", &bad, "source", "https://localhost", 100)
            .is_err()
    );
    let start = options(s, &p);
    let wrong = proof(&start, &SigningKey::from_bytes(&[77; 32]));
    for _ in 0..5 {
        assert!(complete(s, &wrong, 101).is_err());
    }
    assert!(complete(s, &proof(&start, &key), 101).is_err());
}

#[test]
fn failed_registration_keeps_operation_binding_after_expiry_cleanup() {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    enable(s);
    let key = SigningKey::from_bytes(&[76; 32]);
    let p = request(&key);
    let start = options(s, &p);
    assert!(complete(s, &proof(&start, &SigningKey::from_bytes(&[77; 32])), 101).is_err());
    s.store.cleanup_expired(1_000_000).unwrap();
    let mut changed = p.clone();
    changed["display_name"] = json!("changed after cleanup");
    assert!(
        s.machine_registration_options("owner", &changed, "source", "https://localhost", 1_000_000)
            .is_err(),
        "expired original operation must not become a new registration"
    );
}

#[test]
fn independent_connections_race_to_register_one_key_and_preserve_one_account() {
    let mut f = IdentityService::model_executor_test_fixture();
    enable(&mut f.service);
    let key = SigningKey::from_bytes(&[76; 32]);
    let first = proof(&options(&mut f.service, &request(&key)), &key);
    let second = proof(&options(&mut f.service, &request(&key)), &key);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for original in [first, second] {
        let c = &f.service.config;
        let config = super::super::config::IdentityConfig {
            namespace: c.namespace.clone(),
            rp: c.rp.clone(),
            sqlite_path: c.sqlite_path.clone(),
            registration_enabled: true,
            machine_registration_enabled: true,
            session_key: c.session_key,
            world_scope: c.world_scope.clone(),
            runtime_live_bind: String::new(),
            tablestore: None,
            trusted_proxy_loopback: false,
        };
        let ready = barrier.clone();
        workers.push(std::thread::spawn(move || {
            let mut service = IdentityService::new(config).unwrap();
            ready.wait();
            complete(&mut service, &original, 101)
        }));
    }
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    let committed = results.iter().find_map(|r| r.as_ref().ok()).unwrap();
    let lookup = f
        .service
        .store
        .transaction(|tx| {
            tx.get::<CredentialLookup>(&native_key::lookup_key(&key.verifying_key().to_bytes()))
        })
        .unwrap()
        .unwrap();
    assert_eq!(lookup.value.account_id, committed["receipt"]["account_id"]);
}

#[test]
fn machine_http_guards_and_cancel_preserve_original_binding() {
    let mut f = IdentityService::model_executor_test_fixture();
    let s = &mut f.service;
    enable(s);
    let pre = random_token();
    let owner = digest(&s.config.session_key, "preauth", &pre);
    let csrf = digest(&s.config.session_key, "csrf", &pre);
    let headers = format!(
        "POST / HTTP/1.1\r\nOrigin: https://localhost\r\nContent-Type: application/json\r\nX-Oasis7-Csrf: {csrf}\r\nCookie: __Host-oasis7_pre={pre}\r\n\r\n"
    );
    let key = SigningKey::from_bytes(&[76; 32]);
    let p = request(&key);
    let now = super::super::now_ms();
    let mut issuer = crate::hosted_player_session::HostedPlayerSessionIssuer::with_ledger_path(
        s.config
            .sqlite_path
            .parent()
            .unwrap()
            .join("registration-test-ledger.json"),
    )
    .unwrap();
    let options_path = format!(
        "{}/machine-registration/options",
        super::super::service::PREFIX
    );
    let rejected = s.handle(
        "POST",
        &options_path,
        &headers.replace("https://localhost", "https://evil"),
        p.clone(),
        "source",
        &mut issuer,
    );
    assert_eq!(rejected.status, 403);
    let rejected = s.handle(
        "POST",
        &options_path,
        &headers.replace(&csrf, "wrong-csrf"),
        p.clone(),
        "source",
        &mut issuer,
    );
    assert_eq!(rejected.status, 403);
    let started = s.handle(
        "POST",
        &options_path,
        &headers,
        p.clone(),
        "source",
        &mut issuer,
    );
    assert_eq!(started.status, 200);
    assert!(started.cookies.is_empty());
    let cancel_path = format!(
        "{}/ceremonies/{}/cancel",
        super::super::service::PREFIX,
        p["operation_id"].as_str().unwrap()
    );
    let cancelled = s.handle(
        "POST",
        &cancel_path,
        &headers,
        json!({}),
        "source",
        &mut issuer,
    );
    assert_eq!(cancelled.status, 200);
    s.store.cleanup_expired(now + 1_000_000).unwrap();
    let mut changed = p.clone();
    changed["display_name"] = json!("cancelled original changed");
    assert!(
        s.machine_registration_options(
            &owner,
            &changed,
            "source",
            "https://localhost",
            now + 1_000_000
        )
        .is_err()
    );
}
