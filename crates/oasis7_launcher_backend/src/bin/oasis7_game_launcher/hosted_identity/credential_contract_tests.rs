use super::super::{config::IdentityConfig, random_token, service::create_session};
use super::*;

fn fixture() -> (IdentityService, String) {
    let config = IdentityConfig {
        namespace: IdentityNamespace {
            identity_service_id: "credential-contract".into(),
            environment_id: "test".into(),
        },
        rp: RpConfig {
            rp_id: "localhost".into(),
            allowed_origins: vec!["https://localhost".into()],
            version: 1,
            identity_instance_epoch: 1,
        },
        sqlite_path: std::env::temp_dir()
            .join(format!("credential-contract-{}.sqlite", random_token())),
        tablestore: None,
        trusted_proxy_loopback: false,
        registration_enabled: true,
        machine_registration_enabled: false,
        session_key: [71; 32],
        world_scope: "test".into(),
        runtime_live_bind: "127.0.0.1:5023".into(),
    };
    let mut service = IdentityService::new(config).unwrap();
    let token = random_token();
    let account = AccountRecord {
        account_id: "a".into(),
        user_handle: vec![0; 16],
        display_name: "玩家".into(),
        status: "active".into(),
        policy_version: 1,
        security_epoch: 1,
        has_ever_public_key: true,
        created_at: 100,
        last_authenticated_at: Some(100),
        last_recovery_id: None,
    };
    service
        .store
        .transaction(|tx| {
            tx.create("account:a", "account", Some("a"), None, &account)?;
            for id in ["p", "q"] {
                let c = CredentialRecord {
                    credential_record_id: id.into(),
                    account_id: "a".into(),
                    state: "active".into(),
                    version: 1,
                    label: "test".into(),
                    created_at: 100,
                    last_used_at: None,
                    public_material: CredentialMaterial::WebAuthn {
                        rp_id: "localhost".into(),
                        credential_id: vec![0],
                        user_handle: vec![0; 16],
                        library_record: json!({}),
                        library_record_version: 1,
                    },
                    registered_via: "test-domain-fixture".into(),
                };
                tx.create(&credential_key(id), "credential", Some("a"), None, &c)?;
                if id == "p" {
                    create_session(tx, &service.config, &account, &c, &token, 100)?;
                }
            }
            Ok(())
        })
        .unwrap();
    (service, token)
}
#[test]
fn history_beyond_one_page_remains_visible_and_can_be_revoked_idempotently() {
    let (mut service, token) = fixture();
    let (a, s) = service.session(Some(&token), 101).unwrap();
    service
        .store
        .transaction(|tx| {
            for i in 0..110 {
                let mut history = s.clone();
                history.session_id = format!("{i:043}");
                tx.create(
                    &format!("history:{i:043}"),
                    "session",
                    Some("a"),
                    Some(history.expires_at),
                    &history,
                )?;
            }
            Ok(())
        })
        .unwrap();
    let first = service
        .list_account_records(Some(&token), "/sessions", &json!({}), 101)
        .unwrap();
    assert_eq!(first["sessions"].as_array().unwrap().len(), 100);
    let second = service
        .list_account_records(
            Some(&token),
            "/sessions",
            &json!({"cursor":first["next_cursor"]}),
            101,
        )
        .unwrap();
    assert_eq!(second["sessions"].as_array().unwrap().len(), 11);
    let target = format!("{:043}", 109);
    let action_id = random_token();
    let action = SensitiveAction {
        action_id: action_id.clone(),
        account_id: a.account_id.clone(),
        session_id: s.session_id,
        action_type: "revoke_session".into(),
        target_digest: target.clone(),
        account_policy_version: 1,
        proof_credential_id: Some("q".into()),
        proof_time: Some(101),
        expires_at: 300000,
        state: "authorized".into(),
    };
    service
        .store
        .transaction(|tx| {
            tx.create(
                &format!("action:{action_id}"),
                "action",
                Some("a"),
                Some(action.expires_at),
                &action,
            )
        })
        .unwrap();
    let body = json!({"action_id":action_id,"operation_id":action_id});
    let path = format!("/sessions/{target}/revoke");
    assert!(
        service
            .handle_credentials("POST", &path, Some(&token), &body, 102, "owner")
            .unwrap()["ok"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        service
            .handle_credentials("POST", &path, Some(&token), &body, 103, "owner")
            .unwrap()["phase"],
        "operation_committed"
    );
    service
        .store
        .transaction(|tx| {
            assert!(
                tx.get::<AccountSession>(&format!("history:{target}"))?
                    .unwrap()
                    .value
                    .revoked_at
                    .is_some()
            );
            assert_eq!(
                tx.get::<AccountRecord>("account:a")?
                    .unwrap()
                    .value
                    .policy_version,
                2
            );
            Ok(())
        })
        .unwrap();
}
