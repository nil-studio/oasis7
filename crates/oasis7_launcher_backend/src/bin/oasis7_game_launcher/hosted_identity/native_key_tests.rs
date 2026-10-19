use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn fixture() -> (IdentityService, String, SigningKey) {
    let config = super::super::config::IdentityConfig {
        namespace: IdentityNamespace {
            identity_service_id: "native-test".into(),
            environment_id: "test".into(),
        },
        rp: RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec!["https://localhost".into()],
            version: 1,
            identity_instance_epoch: 1,
        },
        sqlite_path: std::env::temp_dir().join(format!("native-{}.sqlite", random_token())),
        tablestore: None,
        trusted_proxy_loopback: false,
        registration_enabled: false,
        session_key: [71; 32],
        world_scope: "test".into(),
        runtime_live_bind: String::new(),
    };
    let mut service = IdentityService::new(config).unwrap();
    let token = random_token();
    let key = SigningKey::from_bytes(&[42; 32]);
    let a = AccountRecord {
        account_id: "a".into(),
        user_handle: vec![0; 16],
        display_name: "玩家".into(),
        status: "active".into(),
        policy_version: 1,
        security_epoch: 1,
        has_ever_public_key: true,
        created_at: 100,
        last_authenticated_at: None,
        last_recovery_id: None,
    };
    let c = CredentialRecord {
        credential_record_id: "key".into(),
        account_id: "a".into(),
        state: "active".into(),
        version: 1,
        label: "导入密钥".into(),
        created_at: 100,
        last_used_at: None,
        public_material: CredentialMaterial::NativeKey {
            public_key: key.verifying_key().to_bytes(),
        },
        registered_via: "test".into(),
    };
    service
        .store
        .transaction(|tx| {
            tx.create("account:a", "account", Some("a"), None, &a)?;
            tx.create("credential:key", "credential", Some("a"), None, &c)?;
            let bytes = key.verifying_key().to_bytes();
            tx.create(
                &lookup_key(&bytes),
                "lookup",
                Some("a"),
                None,
                &CredentialLookup {
                    scope: "native:ed25519:v1".into(),
                    key_digest: String::new(),
                    canonical_key: bytes.to_vec(),
                    credential_record_id: "key".into(),
                    account_id: "a".into(),
                    state: "active".into(),
                },
            )?;
            create_session(tx, &service.config, &a, &c, &token, 100)?;
            Ok(())
        })
        .unwrap();
    (service, token, key)
}
fn key_hex(key: &SigningKey) -> String {
    key.verifying_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn options(
    service: &mut IdentityService,
    token: Option<&str>,
    key: &SigningKey,
    intent: &str,
    action: Option<&str>,
) -> Value {
    service
        .native_options(
            token,
            "owner",
            &json!({"intent":intent,"public_key":key_hex(key),"action_id":action}),
            "source",
            "https://localhost",
            101,
        )
        .unwrap()
}
fn proof(started: &Value, key: &SigningKey) -> Value {
    let bytes = URL_SAFE_NO_PAD
        .decode(started["options"]["signed_bytes"].as_str().unwrap())
        .unwrap();
    json!({"ceremony_id":started["ceremony_id"],"operation_id":started["operation_id"],"signature":URL_SAFE_NO_PAD.encode(key.sign(&bytes).to_bytes())})
}
fn complete(
    service: &mut IdentityService,
    token: Option<&str>,
    p: &Value,
    now: u64,
) -> Result<(Value, Option<String>), StoreError> {
    service.native_complete(token, "owner", p, "https://localhost", now)
}
#[test]
fn login_works_with_registration_closed_and_replay_does_not_issue_another_session() {
    let (mut s, _, key) = fixture();
    let started = options(&mut s, None, &key, "login", None);
    let p = proof(&started, &key);
    let (_, token) = complete(&mut s, None, &p, 102).unwrap();
    let (a, session) = s.session(token.as_deref(), 103).unwrap();
    assert_eq!(a.account_id, "a");
    assert_eq!(session.auth_method, "native_key");
    assert!(complete(&mut s, None, &p, 103).unwrap().1.is_none());
    let mut altered = p.clone();
    altered["signature"] = json!("x".repeat(86));
    assert!(complete(&mut s, None, &altered, 103).is_err());
}
#[test]
fn invalid_proof_owner_origin_expiry_and_exhaustion_are_rejected() {
    let (mut s, _, key) = fixture();
    let started = options(&mut s, None, &key, "login", None);
    let p = proof(&started, &key);
    assert!(
        s.native_complete(None, "other", &p, "https://localhost", 102)
            .is_err()
    );
    assert!(
        s.native_complete(None, "owner", &p, "https://other", 102)
            .is_err()
    );
    assert!(complete(&mut s, None, &p, 120101).is_err());
    let bad = proof(&started, &SigningKey::from_bytes(&[43; 32]));
    for _ in 0..5 {
        assert!(complete(&mut s, None, &bad, 102).is_err());
    }
    assert!(complete(&mut s, None, &p, 102).is_err());
}
#[test]
fn unbound_revoked_and_frozen_credentials_cannot_login() {
    let (mut s, _, key) = fixture();
    let other = SigningKey::from_bytes(&[43; 32]);
    let started = options(&mut s, None, &other, "login", None);
    assert!(complete(&mut s, None, &proof(&started, &other), 102).is_err());
    for (table, field) in [("credential:key", "state"), ("account:a", "status")] {
        s.store
            .transaction(|tx| {
                let mut v = tx.get::<Value>(table)?.unwrap();
                v.value[field] = json!("revoked");
                tx.replace(
                    table,
                    v.row_version,
                    if field == "state" {
                        "credential"
                    } else {
                        "account"
                    },
                    Some("a"),
                    None,
                    &v.value,
                )
            })
            .unwrap();
        let started = options(&mut s, None, &key, "login", None);
        assert!(complete(&mut s, None, &proof(&started, &key), 102).is_err());
        s.store
            .transaction(|tx| {
                let mut v = tx.get::<Value>(table)?.unwrap();
                v.value[field] = json!("active");
                tx.replace(
                    table,
                    v.row_version,
                    if field == "state" {
                        "credential"
                    } else {
                        "account"
                    },
                    Some("a"),
                    None,
                    &v.value,
                )
            })
            .unwrap();
    }
}
fn authorize(
    s: &mut IdentityService,
    token: &str,
    key: &SigningKey,
    kind: &str,
    target: Option<&str>,
) -> String {
    let action = s
        .create_action(
            Some(token),
            &json!({"action_type":kind,"target":target}),
            101,
        )
        .unwrap();
    let aid = action["action_id"].as_str().unwrap();
    let started = options(s, Some(token), key, "reauth", Some(aid));
    complete(s, Some(token), &proof(&started, key), 102).unwrap();
    aid.into()
}
#[test]
fn binding_requires_reauth_and_retains_account_then_revoke_invalidates_sessions() {
    let (mut s, token, key) = fixture();
    let new = SigningKey::from_bytes(&[43; 32]);
    assert!(
        s.native_options(
            Some(&token),
            "owner",
            &json!({"intent":"bind","public_key":key_hex(&new)}),
            "source",
            "https://localhost",
            101
        )
        .is_err()
    );
    let aid = authorize(&mut s, &token, &key, "bind", None);
    let started = options(&mut s, Some(&token), &new, "bind", Some(&aid));
    assert!(complete(&mut s, None, &proof(&started, &new), 103).is_err());
    complete(&mut s, Some(&token), &proof(&started, &new), 103).unwrap();
    let started = options(&mut s, None, &new, "login", None);
    let (_, ntoken) = complete(&mut s, None, &proof(&started, &new), 104).unwrap();
    assert_eq!(s.session(ntoken.as_deref(), 105).unwrap().0.account_id, "a");
    let cid = s
        .store
        .transaction(|tx| {
            Ok(native_credential(tx, &new.verifying_key().to_bytes())?
                .value
                .credential_record_id)
        })
        .unwrap();
    let aid = authorize(&mut s, &token, &key, "revoke_credential", Some(&cid));
    s.handle_credentials(
        "POST",
        &format!("/credentials/{cid}/revoke"),
        Some(&token),
        &json!({"action_id":aid,"operation_id":aid,"expected_version":1}),
        106,
        "owner",
    )
    .unwrap();
    assert!(s.session(ntoken.as_deref(), 107).is_err());
    let started = options(&mut s, None, &new, "login", None);
    assert!(complete(&mut s, None, &proof(&started, &new), 108).is_err());
    let aid = authorize(&mut s, &token, &key, "revoke_credential", Some("key"));
    assert!(
        s.handle_credentials(
            "POST",
            "/credentials/key/revoke",
            Some(&token),
            &json!({"action_id":aid,"operation_id":aid,"expected_version":1}),
            109,
            "owner"
        )
        .is_err()
    );
}
#[test]
fn protocol_vector_is_stable() {
    let (s, _, _) = fixture();
    let bytes = message(
        &s.config,
        "https://localhost",
        "login",
        "",
        "op",
        &[1; 32],
        &[2; 32],
        100,
        120100,
    );
    // Shared with the browser test; a byte change is a protocol change.
    assert_eq!(
        URL_SAFE_NO_PAD.encode(&bytes),
        "b2FzaXM3LmlkZW50aXR5Lm5hdGl2ZS52MQABAAAAC25hdGl2ZS10ZXN0AAAABHRlc3QAAAARaHR0cHM6Ly9sb2NhbGhvc3QAAAAFbG9naW4AAAAAAAAAAm9wAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQECAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgAAAAAAAABkAAAAAAAB1SQ"
    );
}

#[test]
fn purposes_namespace_config_and_binding_uniqueness_are_enforced() {
    let (mut s, token, key) = fixture();
    assert!(
        s.native_options(
            None,
            "owner",
            &json!({"intent":"new_account","public_key":key_hex(&key)}),
            "source",
            "https://localhost",
            101
        )
        .is_err()
    );
    let started = options(&mut s, None, &key, "login", None);
    let p = proof(&started, &key);
    s.config.rp.version += 1;
    assert!(complete(&mut s, None, &p, 102).is_err());
    s.config.rp.version -= 1;
    s.config.namespace.environment_id = "other".into();
    assert!(complete(&mut s, None, &p, 102).is_err());
    s.config.namespace.environment_id = "test".into();
    let aid = authorize(&mut s, &token, &key, "bind", None);
    let started = options(&mut s, Some(&token), &key, "bind", Some(&aid));
    assert!(complete(&mut s, Some(&token), &proof(&started, &key), 102).is_err());
    // Duplicate binding is rejected without consuming the existing mapping.
    assert_eq!(
        s.store
            .transaction(
                |tx| Ok(native_credential(tx, &key.verifying_key().to_bytes())?
                    .value
                    .account_id)
            )
            .unwrap(),
        "a"
    );
}
