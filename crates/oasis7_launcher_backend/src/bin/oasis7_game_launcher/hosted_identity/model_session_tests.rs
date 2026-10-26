use super::super::{
    ClockGuard, IdentityConfig, IdentityService, digest, email::EmailAdapter, model::*, now_ms,
    random_token, service, store::IdentityStore, webauthn::WebAuthnAdapter,
};
use std::path::PathBuf;

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(crate) struct Fixture {
    pub(crate) service: IdentityService,
    pub(crate) token: String,
    _directory: Directory,
}
fn service(config: IdentityConfig) -> IdentityService {
    let webauthn = WebAuthnAdapter::new(&config.rp.rp_id, &config.rp.allowed_origins).unwrap();
    let store = IdentityStore::open(
        &config.sqlite_path,
        config.namespace.clone(),
        config.rp.clone(),
    )
    .unwrap();
    let email = EmailAdapter::from_env(&config.session_key).unwrap();
    IdentityService {
        custody: None,
        participant_entry: None,
        config,
        store,
        webauthn,
        email,
        clock: ClockGuard::new(),
    }
}
pub(crate) fn fixture() -> Fixture {
    let path = std::env::temp_dir().join(format!("model-principal-{}", random_token()));
    std::fs::create_dir(&path).unwrap();
    let config = IdentityConfig {
        namespace: IdentityNamespace {
            identity_service_id: "model-contract".into(),
            environment_id: "test".into(),
        },
        rp: RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec!["https://localhost".into()],
            version: 1,
            identity_instance_epoch: 4,
        },
        sqlite_path: path.join("identity.sqlite"),
        registration_enabled: false,
        machine_registration_enabled: false,
        session_key: [19; 32],
        world_scope: "test-world".into(),
        runtime_live_bind: String::new(),
        tablestore: None,
        trusted_proxy_loopback: false,
    };
    let mut service = service(config);
    let token = random_token();
    let key = service::session_key(&service.config.session_key, &token);
    let now = now_ms();
    service
        .store
        .transaction(|tx| {
            tx.create(
                "account:a",
                "account",
                Some("a"),
                None,
                &AccountRecord {
                    account_id: "a".into(),
                    user_handle: vec![0; 16],
                    display_name: "Player".into(),
                    status: "active".into(),
                    policy_version: 1,
                    security_epoch: 9,
                    has_ever_public_key: true,
                    created_at: now,
                    last_authenticated_at: None,
                    last_recovery_id: None,
                },
            )?;
            tx.create(
                "credential:p",
                "credential",
                Some("a"),
                None,
                &CredentialRecord {
                    credential_record_id: "p".into(),
                    account_id: "a".into(),
                    state: "active".into(),
                    version: 1,
                    label: "key".into(),
                    created_at: now,
                    last_used_at: None,
                    public_material: CredentialMaterial::WebAuthn {
                        rp_id: "localhost".into(),
                        credential_id: vec![1],
                        user_handle: vec![0; 16],
                        library_record: serde_json::json!({}),
                        library_record_version: 1,
                    },
                    registered_via: "new_account".into(),
                },
            )?;
            tx.create(
                &key,
                "session",
                Some("a"),
                Some(now + 300_000),
                &AccountSession {
                    session_id: "session-one".into(),
                    token_digest: key.clone(),
                    account_id: "a".into(),
                    origin_credential_id: "p".into(),
                    origin_credential_version: 1,
                    issued_security_epoch: 9,
                    identity_instance_epoch: 4,
                    auth_method: "webauthn".into(),
                    auth_time: now,
                    expires_at: now + 300_000,
                    idle_expires_at: now + 300_000,
                    revoked_at: None,
                    recovery_context: None,
                    csrf_digest: digest(&[19; 32], "csrf", &token),
                },
            )?;
            Ok(())
        })
        .unwrap();
    Fixture {
        service,
        token,
        _directory: Directory(path),
    }
}
fn headers(f: &Fixture) -> String {
    format!(
        "POST /v1/agent-client/model-connections HTTP/1.1\r\nCookie: __Host-oasis7_sid={}\r\nOrigin: https://localhost\r\nSec-Fetch-Site: same-origin\r\nContent-Type: application/json; charset=utf-8\r\nX-Oasis7-CSRF: {}\r\n\r\n",
        f.token,
        digest(&[19; 32], "csrf", &f.token)
    )
}
#[test]
fn model_writes_require_exact_origin_csrf_json_and_same_origin_fetch() {
    let mut f = fixture();
    let good = headers(&f);
    let p = f.service.model_principal(&good, true).unwrap();
    assert_eq!(p.account_id, "a");
    assert_eq!(p.session_id, "session-one");
    for bad in [
        good.replace("Origin: https://localhost", "Origin: https://localhost/"),
        good.replace(
            "Origin: https://localhost",
            "Origin: https://localhost.attacker.test",
        ),
        good.replace(
            "Origin: https://localhost",
            "Origin: https://localhost\r\nOrigin: https://localhost",
        ),
        good.replace("same-origin", "cross-site"),
        good.replace("application/json; charset=utf-8", "text/plain"),
        good.replace("X-Oasis7-CSRF:", "X-Not-CSRF:"),
        good.replace("Cookie:", "Missing-Cookie:"),
        good.replace("__Host-oasis7_sid=", "other="),
    ] {
        assert!(
            f.service.model_principal(&bad, true).is_err(),
            "invalid write headers authenticated"
        );
    }
    let read = format!(
        "GET / HTTP/1.1\r\nCookie: __Host-oasis7_sid={}\r\n\r\n",
        f.token
    );
    assert!(f.service.model_principal(&read, false).is_ok());
    assert!(f.service.model_principal(&read, true).is_err());
}
#[test]
fn existing_principal_rechecks_current_database_security_state() {
    for mutation in [
        "revoke",
        "credential_state",
        "credential_version",
        "account_state",
        "security_epoch",
        "expiry",
        "idle_expiry",
        "session_epoch",
    ] {
        let mut f = fixture();
        let h = headers(&f);
        let p = f.service.model_principal(&h, true).unwrap();
        assert!(f.service.model_session_current(&p));
        // A second SQLite connection models revocation by another request/worker.
        let mut db = IdentityStore::open(
            &f.service.config.sqlite_path,
            f.service.config.namespace.clone(),
            f.service.config.rp.clone(),
        )
        .unwrap();
        db.transaction(|tx| {
            if mutation.starts_with("credential") {
                let mut row = tx.get::<CredentialRecord>("credential:p")?.unwrap();
                if mutation == "credential_state" {
                    row.value.state = "revoked".into();
                } else {
                    row.value.version += 1;
                }
                tx.replace(
                    "credential:p",
                    row.row_version,
                    "credential",
                    Some("a"),
                    None,
                    &row.value,
                )?;
            } else if mutation == "account_state" || mutation == "security_epoch" {
                let mut row = tx.get::<AccountRecord>("account:a")?.unwrap();
                if mutation == "account_state" {
                    row.value.status = "disabled".into();
                } else {
                    row.value.security_epoch += 1;
                }
                tx.replace(
                    "account:a",
                    row.row_version,
                    "account",
                    Some("a"),
                    None,
                    &row.value,
                )?;
            } else {
                let mut row = tx.get::<AccountSession>(&p.auth_ref)?.unwrap();
                match mutation {
                    "revoke" => row.value.revoked_at = Some(now_ms()),
                    "expiry" => row.value.expires_at = 1,
                    "idle_expiry" => row.value.idle_expires_at = 1,
                    "session_epoch" => row.value.identity_instance_epoch += 1,
                    _ => unreachable!(),
                }
                tx.replace(
                    &p.auth_ref,
                    row.row_version,
                    "session",
                    Some("a"),
                    Some(row.value.expires_at),
                    &row.value,
                )?;
            }
            Ok(())
        })
        .unwrap();
        assert!(
            !f.service.model_session_current(&p),
            "stale principal survived {mutation}"
        );
        let h = headers(&f);
        assert!(
            f.service.model_principal(&h, true).is_err(),
            "cookie survived {mutation}"
        );
    }
}
#[test]
fn offline_instance_epoch_advance_invalidates_previous_cookie_and_principal() {
    let mut f = fixture();
    let h = headers(&f);
    let p = f.service.model_principal(&h, true).unwrap();
    let Fixture {
        service: old,
        token,
        _directory,
    } = f;
    let mut config = old.config;
    // Fence the old writer before invoking the supported offline epoch operation.
    drop(old.store);
    config.rp.identity_instance_epoch += 1;
    IdentityStore::advance_instance_epoch(
        &config.sqlite_path,
        &config.namespace,
        &config.rp,
        true,
        true,
    )
    .unwrap();
    let mut restarted = service(config);
    assert!(!restarted.model_session_current(&p));
    assert!(restarted.model_principal(&h, true).is_err());
    drop(restarted);
    drop(token);
    drop(_directory);
}

#[test]
fn model_executor_ownership_uses_existing_scoped_player_binding_without_allocating() {
    let mut f = fixture();
    assert!(!f.service.model_account_owns_player("a", "player-a"));
    let scope = f.service.config.world_scope.clone();
    let key = format!("binding:a:{scope}");
    f.service
        .store
        .transaction(|tx| {
            tx.create(
                &key,
                "binding",
                Some("a"),
                None,
                &PlayerBinding {
                    account_id: "a".into(),
                    world_identity_scope: scope.clone(),
                    player_id: "player-a".into(),
                    source: "identity_v2".into(),
                    binding_version: 1,
                },
            )
        })
        .unwrap();
    assert!(f.service.model_account_owns_player("a", "player-a"));
    assert!(!f.service.model_account_owns_player("a", "player-b"));
    assert!(!f.service.model_account_owns_player("b", "player-a"));
    f.service
        .store
        .transaction(|tx| {
            let mut row = tx.get::<PlayerBinding>(&key)?.unwrap();
            row.value.world_identity_scope = "another-scope".into();
            tx.replace(
                &key,
                row.row_version,
                "binding",
                Some("a"),
                None,
                &row.value,
            )
        })
        .unwrap();
    assert!(!f.service.model_account_owns_player("a", "player-a"));
}
