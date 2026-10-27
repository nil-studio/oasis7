use super::*;
use ed25519_dalek::{Signer, SigningKey};
fn fixture() -> super::super::super::super::model_session::model_session_tests::Fixture {
    let mut f = IdentityService::model_executor_test_fixture();
    f.service.enable_managed_login_test_adapter();
    f
}
fn sync(s: &mut IdentityService, now: u64) -> Result<String, StoreError> {
    let root = s.custody.as_ref().unwrap();
    let config = &s.config;
    s.store.transaction(|tx| guard(tx, root, config, now))
}
fn saved(s: &mut IdentityService) -> Ledger {
    s.store
        .transaction(|tx| Ok(tx.get::<Ledger>(KEY)?.unwrap().value))
        .unwrap()
}
fn pin(seed: [u8; 32], epoch: u64) -> NativeBindingPinV1 {
    NativeBindingPinV1 {
        key_id: format!("owner-native-test-{epoch}"),
        public_key: hex::encode(SigningKey::from_bytes(&seed).verifying_key().to_bytes()),
        environment: "test".into(),
        service_identity: "owner-native-test".into(),
        service_epoch: DecimalU64(epoch),
    }
}
fn rotation(
    s: &mut IdentityService,
    next: NativeBindingPinV1,
    seed: [u8; 32],
    now: u64,
) -> SignedOwnerPinRotationManifestV1 {
    let ledger = saved(s);
    let manifest = OwnerPinRotationManifestV1 {
        api_version: 1,
        from_ledger_digest: ledger.digest().unwrap(),
        identity_namespace: ledger.identity_namespace,
        identity_instance_epoch: DecimalU64(ledger.identity_instance_epoch),
        old_pin: ledger.current,
        new_pin: next,
        operation_id: format!("rotation-{now}"),
        issued_at: DecimalU64(now),
        expires_at: DecimalU64(now + 300_000),
    };
    let root = s.custody.as_ref().unwrap();
    SignedOwnerPinRotationManifestV1 {
        native_root_key_id: root.key_id.clone(),
        native_root_signature: URL_SAFE_NO_PAD
            .encode(root.key.sign(&manifest.signing_bytes().unwrap()).to_bytes()),
        new_owner_signature: URL_SAFE_NO_PAD.encode(
            SigningKey::from_bytes(&seed)
                .sign(&manifest.possession_signing_bytes().unwrap())
                .to_bytes(),
        ),
        manifest,
    }
}
fn configure(s: &mut IdentityService, signed: SignedOwnerPinRotationManifestV1) {
    let managed = s
        .custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap();
    managed.owner = signed.manifest.new_pin.clone();
    managed.rotation = Some(signed);
}
#[test]
fn persistent_floor_rejects_same_epoch_swap_downgrade_retired_reuse_and_native_root_change() {
    let mut f = fixture();
    let s = &mut f.service;
    sync(s, 100).unwrap();
    let original = saved(s).current;
    let next = pin([96; 32], 2);
    s.custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap()
        .owner = next.clone();
    assert!(sync(s, 101).is_err()); // Configuration alone never rotates a nonempty floor.
    let manifest = rotation(s, next.clone(), [96; 32], 101);
    configure(s, manifest);
    sync(s, 102).unwrap();
    s.store = super::super::super::super::store::IdentityStore::open(
        &s.config.sqlite_path,
        s.config.namespace.clone(),
        s.config.rp.clone(),
    )
    .unwrap();
    assert_eq!(saved(s).current, next);
    s.custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap()
        .owner = pin([97; 32], 2);
    assert!(sync(s, 103).is_err());
    s.custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap()
        .owner = original.clone();
    assert!(sync(s, 103).is_err());
    let mut reused = original;
    reused.key_id = "reuse-at-three".into();
    reused.service_epoch = DecimalU64(3);
    let reuse_manifest = rotation(s, reused, [93; 32], 103);
    configure(s, reuse_manifest);
    assert!(sync(s, 104).is_err()); // Even valid Root + retired-key PoP cannot revive it.
    s.custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap()
        .owner = next;
    s.config.rp.identity_instance_epoch += 1;
    assert!(sync(s, 103).is_err());
    s.config.rp.identity_instance_epoch -= 1;
    s.custody.as_mut().unwrap().key = SigningKey::from_bytes(&[99; 32]);
    assert!(sync(s, 103).is_err());
    assert_eq!(saved(s).current.service_epoch.0, 2);
}
#[test]
fn legacy_unknown_and_disabled_feature_preserve_inventory_and_do_not_block_logout() {
    let mut f = fixture();
    let s = &mut f.service;
    s.store
        .transaction(|tx| {
            tx.create(
                "legacy-managed",
                "custody_managed_invalidation",
                None,
                None,
                &json!({"pending":true}),
            )
        })
        .unwrap();
    assert!(sync(s, 100).is_err());
    assert!(
        s.store
            .transaction(|tx| tx.get::<Ledger>(KEY))
            .unwrap()
            .is_none()
    );
    s.logout(Some(&f.token), 101).unwrap();
    assert_eq!(
        s.store
            .transaction(|tx| tx.count_kind("custody_managed_invalidation"))
            .unwrap(),
        1
    );
    let mut fresh = fixture();
    sync(&mut fresh.service, 100).unwrap();
    let before = saved(&mut fresh.service).digest().unwrap();
    let config = fresh
        .service
        .custody
        .as_mut()
        .unwrap()
        .managed_binding
        .take();
    assert!(!fresh.service.managed_login_adapter_enabled());
    fresh.service.logout(Some(&fresh.token), 101).unwrap();
    fresh.service.custody.as_mut().unwrap().managed_binding = config;
    sync(&mut fresh.service, 102).unwrap();
    assert_eq!(saved(&mut fresh.service).digest().unwrap(), before);
}
#[test]
fn manifest_crypto_tamper_domain_expiry_and_atomic_failure_never_advance_floor() {
    let mut f = fixture();
    let s = &mut f.service;
    sync(s, 100).unwrap();
    let signed = rotation(s, pin([96; 32], 2), [96; 32], 101);
    let before = saved(s).digest().unwrap();
    let mut bad = signed.clone();
    bad.native_root_key_id = "wrong-root".into();
    configure(s, bad);
    assert!(sync(s, 102).is_err());
    let mut bad = signed.clone();
    bad.manifest.operation_id.push('x');
    configure(s, bad);
    assert!(sync(s, 102).is_err());
    let mut bad = signed.clone();
    bad.new_owner_signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[97; 32])
            .sign(&bad.manifest.possession_signing_bytes().unwrap())
            .to_bytes(),
    );
    configure(s, bad);
    assert!(sync(s, 102).is_err());
    let mut bad = signed.clone();
    bad.native_root_signature = URL_SAFE_NO_PAD.encode(
        s.custody
            .as_ref()
            .unwrap()
            .key
            .sign(&bad.manifest.possession_signing_bytes().unwrap())
            .to_bytes(),
    );
    configure(s, bad);
    assert!(sync(s, 102).is_err());
    let mut bad = signed.clone();
    bad.new_owner_signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[96; 32])
            .sign(&bad.manifest.signing_bytes().unwrap())
            .to_bytes(),
    );
    configure(s, bad);
    assert!(sync(s, 102).is_err());
    let mut bad = signed.clone();
    bad.native_root_signature.push('=');
    configure(s, bad);
    assert!(sync(s, 102).is_err());
    configure(s, signed.clone());
    assert!(sync(s, 300_101).is_err());
    assert_eq!(saved(s).digest().unwrap(), before);
    let db = rusqlite::Connection::open(&s.config.sqlite_path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_owner_pin_rotation BEFORE UPDATE ON identity_records WHEN OLD.kind='custody_owner_pin_ledger' BEGIN SELECT RAISE(ABORT,'fixture pin rollback'); END;").unwrap();
    assert!(sync(s, 102).is_err());
    assert_eq!(saved(s).digest().unwrap(), before);
    db.execute_batch("DROP TRIGGER fail_owner_pin_rotation")
        .unwrap();
    sync(s, 102).unwrap();
    assert_eq!(saved(s).retired.len(), 1);
    assert!(s.logout(Some(&f.token), 103).is_ok());
}
#[test]
fn rotation_capacity_only_blocks_rotation_not_current_pin_or_logout() {
    let mut f = fixture();
    let s = &mut f.service;
    sync(s, 100).unwrap();
    let mut floor = saved(s);
    let seed = |n: u32| {
        let mut v = [9; 32];
        v[..4].copy_from_slice(&n.to_le_bytes());
        v
    };
    floor.current = pin(seed(999), 257);
    floor.retired = (0..256)
        .map(|i| Retired {
            pin: pin(seed(i), u64::from(i) + 1),
            operation_id: format!("past-{i}"),
            manifest_digest: "11".repeat(32),
        })
        .collect();
    floor.validate().unwrap();
    s.store
        .transaction(|tx| {
            let old = tx.get::<Ledger>(KEY)?.unwrap();
            tx.replace(
                KEY,
                old.row_version,
                "custody_owner_pin_ledger",
                None,
                None,
                &floor,
            )
        })
        .unwrap();
    s.custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap()
        .owner = floor.current.clone();
    sync(s, 101).unwrap();
    s.custody
        .as_mut()
        .unwrap()
        .managed_binding
        .as_mut()
        .unwrap()
        .owner = pin(seed(1000), 258);
    assert!(sync(s, 101).is_err());
    s.logout(Some(&f.token), 102).unwrap();
}
#[test]
fn pending_managed_login_cannot_cross_owner_pin_activation_and_history_never_returns_cookie() {
    let mut f = fixture();
    let s = &mut f.service;
    sync(s, 100).unwrap();
    s.config.registration_enabled = true;
    s.config.machine_registration_enabled = true;
    let key = SigningKey::from_bytes(&[97; 32]);
    let op = random_token();
    let owner = "ab".repeat(32);
    let reg=s.machine_registration_route("POST","/machine-registration/options",&owner,&json!({"operation_id":op,"public_key":hex::encode(key.verifying_key().to_bytes()),"display_name":"ledger"}),"fixture","https://localhost",101).unwrap().unwrap();
    let proof = json!({"operation_id":op,"ceremony_id":op,"signature":URL_SAFE_NO_PAD.encode(key.sign(&URL_SAFE_NO_PAD.decode(reg["options"]["signed_bytes"].as_str().unwrap()).unwrap()).to_bytes())});
    s.machine_registration_route(
        "POST",
        "/machine-registration/complete",
        &owner,
        &proof,
        "fixture",
        "https://localhost",
        102,
    )
    .unwrap()
    .unwrap();
    let id = random_token();
    let options=s.native_key_route("POST","/native-key/options",None,&owner,&json!({"intent":"login","public_key":hex::encode(key.verifying_key().to_bytes()),"managed_operation_id":id}),"fixture","https://localhost",103).unwrap().unwrap().0;
    let complete = json!({"operation_id":id,"ceremony_id":id,"signature":URL_SAFE_NO_PAD.encode(key.sign(&URL_SAFE_NO_PAD.decode(options["options"]["signed_bytes"].as_str().unwrap()).unwrap()).to_bytes())});
    let signed = rotation(s, pin([96; 32], 2), [96; 32], 104);
    configure(s, signed);
    sync(s, 105).unwrap();
    assert!(
        s.native_key_route(
            "POST",
            "/native-key/complete",
            None,
            &owner,
            &complete,
            "fixture",
            "https://localhost",
            106
        )
        .unwrap()
        .is_err()
    );
    assert_eq!(
        s.store
            .transaction(|tx| tx.count_kind("native_managed_login_outcome"))
            .unwrap(),
        0
    );
}

#[test]
fn retired_binding_cannot_report_active_or_mint_managed_sid_but_original_facts_and_logout_survive()
{
    let mut f = fixture();
    let s = &mut f.service;
    sync(s, 100).unwrap();
    s.config.registration_enabled = true;
    s.config.machine_registration_enabled = true;
    let login = SigningKey::from_bytes(&[97; 32]);
    let owner = SigningKey::from_bytes(&[93; 32]);
    let context = "ab".repeat(32);
    let reg_id = random_token();
    let reg=s.machine_registration_route("POST","/machine-registration/options",&context,&json!({"operation_id":reg_id,"public_key":hex::encode(login.verifying_key().to_bytes()),"display_name":"bound ledger"}),"fixture","https://localhost",101).unwrap().unwrap();
    let proof = json!({"operation_id":reg_id,"ceremony_id":reg_id,"signature":URL_SAFE_NO_PAD.encode(login.sign(&URL_SAFE_NO_PAD.decode(reg["options"]["signed_bytes"].as_str().unwrap()).unwrap()).to_bytes())});
    s.machine_registration_route(
        "POST",
        "/machine-registration/complete",
        &context,
        &proof,
        "fixture",
        "https://localhost",
        102,
    )
    .unwrap()
    .unwrap();
    let challenge: SignedNativeBindingChallengeV1 = serde_json::from_value(
        s.managed_binding_route(
            "POST",
            "/custody/managed-bindings/options",
            &context,
            &json!({"registration_operation_id":reg_id,"binding_operation_id":"pin-binding"}),
            "https://localhost",
            103,
        )
        .unwrap(),
    )
    .unwrap();
    let statement = OwnerNativeBindingStatementV1 {
        challenge: challenge.challenge,
        environment: "test".into(),
        custody_account_id: "custody".into(),
        provision_id: "provision".into(),
        approved_plan_digest: "11".repeat(32),
        login_key_ref: "key".into(),
        binding_operation_id: "pin-binding".into(),
        permit_id: "permit".into(),
        source_revision: "22".repeat(32),
        account_authority_revision: "33".repeat(32),
        service_epoch: DecimalU64(1),
        service_identity: "owner-native-test".into(),
        signer_key_id: "owner-native-test".into(),
        authority_observed_at: DecimalU64(104),
        expires_at: DecimalU64(5104),
    };
    let complete = NativeBindingCompleteV1 {
        api_version: 1,
        login_key_signature: URL_SAFE_NO_PAD.encode(
            login
                .sign(&statement.possession_signing_bytes().unwrap())
                .to_bytes(),
        ),
        statement: SignedOwnerNativeBindingStatementV1 {
            signature: URL_SAFE_NO_PAD
                .encode(owner.sign(&statement.signing_bytes().unwrap()).to_bytes()),
            statement,
        },
    };
    let binding = s
        .managed_binding_route(
            "POST",
            "/custody/managed-bindings/complete",
            &context,
            &serde_json::to_value(&complete).unwrap(),
            "https://localhost",
            105,
        )
        .unwrap();
    let login_id = random_token();
    let login_request = json!({"intent":"login","public_key":hex::encode(login.verifying_key().to_bytes()),"managed_operation_id":login_id});
    let options = s
        .native_key_route(
            "POST",
            "/native-key/options",
            None,
            &context,
            &login_request,
            "fixture",
            "https://localhost",
            106,
        )
        .unwrap()
        .unwrap()
        .0;
    let login_proof = json!({"operation_id":login_id,"ceremony_id":login_id,"signature":URL_SAFE_NO_PAD.encode(login.sign(&URL_SAFE_NO_PAD.decode(options["options"]["signed_bytes"].as_str().unwrap()).unwrap()).to_bytes())});
    let (_, sid) = s
        .native_key_route(
            "POST",
            "/native-key/complete",
            None,
            &context,
            &login_proof,
            "fixture",
            "https://localhost",
            107,
        )
        .unwrap()
        .unwrap();
    let sid = sid.unwrap();
    let nonce = json!({"nonce":"44".repeat(32)});
    assert_eq!(
        s.managed_session_status(Some(&sid), &nonce, 108).unwrap()["status"]["active"],
        true
    );
    let signed = rotation(s, pin([96; 32], 2), [96; 32], 109);
    configure(s, signed);
    sync(s, 110).unwrap();
    let status = s.managed_session_status(Some(&sid), &nonce, 111).unwrap();
    assert_eq!(status["status"]["active"], false);
    assert_eq!(status["status"]["frozen"], true);
    assert_eq!(status["status"]["pending"], true);
    let fresh = json!({"intent":"login","public_key":hex::encode(login.verifying_key().to_bytes()),"managed_operation_id":random_token()});
    assert!(
        s.native_key_route(
            "POST",
            "/native-key/options",
            None,
            &context,
            &fresh,
            "fixture",
            "https://localhost",
            111
        )
        .unwrap()
        .is_err()
    );
    assert!(
        s.native_key_route(
            "POST",
            "/native-key/complete",
            None,
            &context,
            &login_proof,
            "fixture",
            "https://localhost",
            111
        )
        .unwrap()
        .unwrap()
        .1
        .is_none()
    );
    assert_eq!(
        s.managed_binding_route(
            "POST",
            "/custody/managed-bindings/lookup",
            &context,
            &json!({"registration_operation_id":reg_id}),
            "https://localhost",
            100_000
        )
        .unwrap(),
        binding
    );
    assert_eq!(
        s.managed_binding_route(
            "POST",
            "/custody/managed-bindings/complete",
            &context,
            &serde_json::to_value(&complete).unwrap(),
            "https://localhost",
            100_000
        )
        .unwrap(),
        binding
    );
    s.logout(Some(&sid), 112).unwrap();
}
use serde_json::json;

#[test]
fn malformed_managed_config_keeps_native_logout_available() {
    let marker = "OASIS7_OWNER_PIN_CONFIG_CHILD";
    if let Ok(case) = std::env::var(marker) {
        let mut f = IdentityService::model_executor_test_fixture();
        let original = &f.service.config;
        let config = super::super::super::super::config::IdentityConfig {
            namespace: original.namespace.clone(),
            rp: original.rp.clone(),
            sqlite_path: original.sqlite_path.clone(),
            tablestore: None,
            trusted_proxy_loopback: original.trusted_proxy_loopback,
            registration_enabled: original.registration_enabled,
            machine_registration_enabled: original.machine_registration_enabled,
            session_key: original.session_key,
            world_scope: original.world_scope.clone(),
            runtime_live_bind: original.runtime_live_bind.clone(),
        };
        let mut service = IdentityService::new(config).unwrap();
        let root = service.custody.as_ref().unwrap();
        assert!(root.managed_binding.is_none());
        let expected = if case == "pin" {
            "owner_pin_configuration_invalid_pending"
        } else {
            "owner_pin_rotation_configuration_invalid_pending"
        };
        assert_eq!(root.managed_binding_state.pending_code(), Some(expected));
        assert_eq!(
            root.key.verifying_key().to_bytes(),
            SigningKey::from_bytes(&[95; 32]).verifying_key().to_bytes()
        );
        let csrf =
            super::super::super::super::digest(&service.config.session_key, "csrf", &f.token);
        let headers = format!(
            "POST /api/public/hosted-account/v2/custody/managed-bindings/options HTTP/1.1\r\nOrigin: https://localhost\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\nCookie: __Host-oasis7_sid={}\r\n",
            f.token
        );
        let mut issuer = crate::hosted_player_session::HostedPlayerSessionIssuer::default();
        let unavailable = service.handle(
            "POST",
            "/api/public/hosted-account/v2/custody/managed-bindings/options",
            &headers,
            json!({"registration_operation_id":"missing","binding_operation_id":"missing"}),
            "fixture",
            &mut issuer,
        );
        assert_eq!(unavailable.status, 409);
        assert_eq!(unavailable.body["error_code"], expected);
        let logout = service.handle(
            "POST",
            "/api/public/hosted-account/v2/logout",
            &headers.replacen("/custody/managed-bindings/options", "/logout", 1),
            json!({}),
            "fixture",
            &mut issuer,
        );
        assert_eq!(logout.status, 200);
        assert_eq!(logout.body["ok"], true);
        service
            .store
            .transaction(|tx| {
                assert!(tx.get::<Ledger>(KEY)?.is_none());
                let key = super::super::super::super::service::session_key(
                    &service.config.session_key,
                    &f.token,
                );
                assert!(
                    tx.get::<AccountSession>(&key)?
                        .unwrap()
                        .value
                        .revoked_at
                        .is_some()
                );
                Ok(())
            })
            .unwrap();
        // Keep the original isolated directory/connection owned until this service is closed.
        drop(service);
        let _ = &mut f;
        return;
    }
    let full = concat!(
        module_path!(),
        "::malformed_managed_config_keeps_native_logout_available"
    );
    let test = full.split_once("::").unwrap().1;
    for case in ["pin", "manifest"] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", test, "--nocapture"]);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("OASIS7_CUSTODY_")
                || name.to_string_lossy().starts_with("OASIS7_HOSTED_ACCOUNT_")
            {
                command.env_remove(name);
            }
        }
        command
            .env(marker, case)
            .env("OASIS7_CUSTODY_APPROVAL_SIGNING_KEY", hex::encode([95; 32]))
            .env("OASIS7_CUSTODY_APPROVAL_KEY_ID", "native-managed-test-root")
            .env(
                "OASIS7_CUSTODY_ACCOUNT_AUTHORITY_AUDIENCE",
                "native-managed-test",
            )
            .env(
                "OASIS7_CUSTODY_NATIVE_ADAPTER_SECRET",
                hex::encode([94; 32]),
            )
            .env("OASIS7_CUSTODY_MANAGED_BINDINGS_ENABLED", "true")
            .env(
                "OASIS7_CUSTODY_MANAGED_BINDING_OWNER_PIN",
                if case == "pin" {
                    "{".into()
                } else {
                    serde_json::to_string(&ManagedBindingConfig::native_http_fixture().owner)
                        .unwrap()
                },
            );
        if case == "manifest" {
            command.env(
                "OASIS7_CUSTODY_MANAGED_BINDING_OWNER_ROTATION_MANIFEST",
                "{",
            );
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "isolated malformed {case} config child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "isolated test selection must execute the actual case"
        );
    }
}
