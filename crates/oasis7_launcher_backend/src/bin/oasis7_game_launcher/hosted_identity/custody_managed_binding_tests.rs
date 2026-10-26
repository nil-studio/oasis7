use super::*;
use serde_json::json;
fn setup() -> (
    super::super::super::model_session::model_session_tests::Fixture,
    SigningKey,
    String,
    String,
) {
    let mut f = IdentityService::model_executor_test_fixture();
    f.service.config.registration_enabled = true;
    f.service.config.machine_registration_enabled = true;
    let login = SigningKey::from_bytes(&[71; 32]);
    let owner = SigningKey::from_bytes(&[72; 32]);
    let environment = f.service.config.namespace.environment_id.clone();
    f.service.custody = Some(CustodyConfig {
        key: SigningKey::from_bytes(&[73; 32]),
        key_id: "native-root".into(),
        audience: "owner".into(),
        adapter_secret: [74; 32],
        revocation: None,
        managed_binding: Some(ManagedBindingConfig {
            owner: NativeBindingPinV1 {
                key_id: "owner-binding".into(),
                public_key: hex::encode(owner.verifying_key().to_bytes()),
                environment,
                service_identity: "owner-service".into(),
                service_epoch: DecimalU64(7),
            },
        }),
    });
    let context = "ab".repeat(32);
    let op = random_token();
    let options = f.service.machine_registration_route("POST", "/machine-registration/options", &context, &json!({"operation_id":op,"public_key":hex::encode(login.verifying_key().to_bytes()),"display_name":"managed"}), "source", "https://localhost", 100).unwrap().unwrap();
    let bytes = URL_SAFE_NO_PAD
        .decode(options["options"]["signed_bytes"].as_str().unwrap())
        .unwrap();
    let proof = json!({"operation_id":op,"ceremony_id":op,"signature":URL_SAFE_NO_PAD.encode(login.sign(&bytes).to_bytes())});
    f.service
        .machine_registration_route(
            "POST",
            "/machine-registration/complete",
            &context,
            &proof,
            "source",
            "https://localhost",
            101,
        )
        .unwrap()
        .unwrap();
    (f, owner, op, context)
}
fn challenge(s: &mut IdentityService, op: &str, context: &str) -> SignedNativeBindingChallengeV1 {
    serde_json::from_value(
        s.managed_binding_route(
            "POST",
            "/custody/managed-bindings/options",
            context,
            &json!({"registration_operation_id":op,"binding_operation_id":"bind-1"}),
            "https://localhost",
            102,
        )
        .unwrap(),
    )
    .unwrap()
}
fn statement(
    s: &IdentityService,
    c: NativeBindingChallengeV1,
    owner: &SigningKey,
) -> SignedOwnerNativeBindingStatementV1 {
    let statement = OwnerNativeBindingStatementV1 {
        challenge: c.clone(),
        environment: s.config.namespace.environment_id.clone(),
        custody_account_id: "custody-1".into(),
        provision_id: "provision-1".into(),
        approved_plan_digest: "11".repeat(32),
        login_key_ref: "identity-key-1".into(),
        binding_operation_id: c.challenge_id.clone(),
        permit_id: "permit-1".into(),
        source_revision: "22".repeat(32),
        account_authority_revision: "33".repeat(32),
        service_epoch: s
            .custody
            .as_ref()
            .unwrap()
            .managed_binding
            .as_ref()
            .unwrap()
            .owner
            .service_epoch,
        service_identity: "owner-service".into(),
        signer_key_id: "owner-binding".into(),
        authority_observed_at: DecimalU64(c.issued_at.0 + 1),
        expires_at: DecimalU64(c.issued_at.0 + 5001),
    };
    let signature =
        URL_SAFE_NO_PAD.encode(owner.sign(&statement.signing_bytes().unwrap()).to_bytes());
    SignedOwnerNativeBindingStatementV1 {
        statement,
        signature,
    }
}
fn complete(
    s: &mut IdentityService,
    p: &SignedOwnerNativeBindingStatementV1,
    context: &str,
    now: u64,
) -> Result<Value, StoreError> {
    s.managed_binding_route(
        "POST",
        "/custody/managed-bindings/complete",
        context,
        &serde_json::to_value(NativeBindingCompleteV1 {
            api_version: 1,
            statement: p.clone(),
            login_key_signature: URL_SAFE_NO_PAD.encode(
                SigningKey::from_bytes(&[71; 32])
                    .sign(&p.statement.possession_signing_bytes().unwrap())
                    .to_bytes(),
            ),
        })
        .unwrap(),
        "https://localhost",
        now,
    )
}
#[test]
fn binding_commits_original_native_identity_without_manager_registry_and_replays_only_fact() {
    let (mut f, owner, op, context) = setup();
    let c = challenge(&mut f.service, &op, &context);
    let root = f.service.custody.as_ref().unwrap();
    root.key
        .verifying_key()
        .verify_strict(
            &c.challenge.signing_bytes().unwrap(),
            &Signature::from_slice(&URL_SAFE_NO_PAD.decode(&c.signature).unwrap()).unwrap(),
        )
        .unwrap();
    let signed = statement(&f.service, c.challenge, &owner);
    let result = complete(&mut f.service, &signed, &context, 104).unwrap();
    assert_eq!(result["receipt"]["custody_account_id"], "custody-1");
    assert!(result.get("cookie").is_none());
    f.service.store = super::super::super::store::IdentityStore::open(
        &f.service.config.sqlite_path,
        f.service.config.namespace.clone(),
        f.service.config.rp.clone(),
    )
    .unwrap();
    assert_eq!(
        result,
        complete(&mut f.service, &signed, &context, 100_000).unwrap()
    );
    let mut changed = signed.clone();
    changed.statement.custody_account_id = "other".into();
    changed.signature = URL_SAFE_NO_PAD.encode(
        owner
            .sign(&changed.statement.signing_bytes().unwrap())
            .to_bytes(),
    );
    assert!(complete(&mut f.service, &changed, &context, 104).is_err());
}
#[test]
fn binding_rejects_wrong_pin_owner_original_challenge_expiry_and_changed_credential() {
    let (mut f, owner, op, context) = setup();
    let c = challenge(&mut f.service, &op, &context);
    let signed = statement(&f.service, c.challenge, &owner);
    let wrong_possession = NativeBindingCompleteV1 {
        api_version: 1,
        statement: signed.clone(),
        login_key_signature: URL_SAFE_NO_PAD.encode(
            SigningKey::from_bytes(&[75; 32])
                .sign(&signed.statement.possession_signing_bytes().unwrap())
                .to_bytes(),
        ),
    };
    assert!(
        f.service
            .managed_binding_route(
                "POST",
                "/custody/managed-bindings/complete",
                &context,
                &serde_json::to_value(wrong_possession).unwrap(),
                "https://localhost",
                104
            )
            .is_err()
    );
    assert!(complete(&mut f.service, &signed, &"cd".repeat(32), 104).is_err());
    assert!(complete(&mut f.service, &signed, &context, 5103).is_err());
    let mut wrong = signed.clone();
    wrong.signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[75; 32])
            .sign(&wrong.statement.signing_bytes().unwrap())
            .to_bytes(),
    );
    assert!(complete(&mut f.service, &wrong, &context, 104).is_err());
    let mut wrong = signed.clone();
    wrong.statement.challenge.nonce = "ee".repeat(32);
    wrong.signature = URL_SAFE_NO_PAD.encode(
        owner
            .sign(&wrong.statement.signing_bytes().unwrap())
            .to_bytes(),
    );
    assert!(complete(&mut f.service, &wrong, &context, 104).is_err());
    f.service
        .store
        .transaction(|tx| {
            let key = credential_key(&signed.statement.challenge.native_credential_ref);
            let mut credential = tx.get::<CredentialRecord>(&key)?.unwrap();
            credential.value.state = "revoked".into();
            tx.replace(
                &key,
                credential.row_version,
                "credential",
                Some(&credential.value.account_id),
                None,
                &credential.value,
            )
        })
        .unwrap();
    assert!(complete(&mut f.service, &signed, &context, 104).is_err());
}

#[test]
fn binding_audit_failure_rolls_back_nonce_and_binding_then_original_retry_commits() {
    let (mut f, owner, op, context) = setup();
    let c = challenge(&mut f.service, &op, &context);
    let signed = statement(&f.service, c.challenge.clone(), &owner);
    let db = rusqlite::Connection::open(&f.service.config.sqlite_path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_managed_audit BEFORE INSERT ON identity_records WHEN NEW.kind='outbox' AND json_extract(NEW.payload,'$.event_type')='custody_native_bound' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END;").unwrap();
    assert!(complete(&mut f.service, &signed, &context, 104).is_err());
    f.service
        .store
        .transaction(|tx| {
            assert!(
                tx.get::<ManagedBindingRecord>(&format!(
                    "custody_managed_binding:{}",
                    c.challenge.native_account_id
                ))?
                .is_none()
            );
            assert!(
                tx.get::<ChallengeRecord>(&"custody_managed_challenge:bind-1".to_owned())?
                    .unwrap()
                    .value
                    .consumed_statement
                    .is_none()
            );
            Ok(())
        })
        .unwrap();
    db.execute_batch("DROP TRIGGER fail_managed_audit").unwrap();
    assert!(complete(&mut f.service, &signed, &context, 104).is_ok());
}

#[test]
fn native_session_status_and_logout_create_only_permanent_account_pending_invalidation() {
    let (mut f, owner, op, context) = setup();
    let c = challenge(&mut f.service, &op, &context);
    let signed = statement(&f.service, c.challenge.clone(), &owner);
    complete(&mut f.service, &signed, &context, 104).unwrap();
    let login_key = SigningKey::from_bytes(&[71; 32]);
    let options = f.service.native_key_route("POST", "/native-key/options", None, &context, &json!({"public_key":hex::encode(login_key.verifying_key().to_bytes()),"intent":"login"}), "source", "https://localhost", 105).unwrap().unwrap().0;
    let bytes = URL_SAFE_NO_PAD
        .decode(options["options"]["signed_bytes"].as_str().unwrap())
        .unwrap();
    let proof = json!({"operation_id":options["operation_id"],"ceremony_id":options["ceremony_id"],"signature":URL_SAFE_NO_PAD.encode(login_key.sign(&bytes).to_bytes())});
    let (_, sid) = f
        .service
        .native_key_route(
            "POST",
            "/native-key/complete",
            None,
            &context,
            &proof,
            "source",
            "https://localhost",
            106,
        )
        .unwrap()
        .unwrap();
    let sid = sid.unwrap();
    let request = json!({"nonce":"44".repeat(32)});
    let active = f
        .service
        .managed_session_status(Some(&sid), &request, 107)
        .unwrap();
    assert_eq!(active["status"]["active"], true);
    assert_eq!(active["status"]["service_epoch"], "7");
    assert_ne!(
        active["status"]["receipt"]["identity_instance_epoch"],
        active["status"]["service_epoch"]
    );
    assert_eq!(active["status"]["generation"], "1");
    let key = session_key(&f.service.config.session_key, &sid);
    f.service
        .store
        .transaction(|tx| {
            let mut session = tx.get::<AccountSession>(&key)?.unwrap();
            session.value.expires_at += 100;
            session.value.idle_expires_at += 100;
            tx.replace(
                &key,
                session.row_version,
                "session",
                Some(&session.value.account_id),
                None,
                &session.value,
            )
        })
        .unwrap();
    let renewed = f
        .service
        .managed_session_status(Some(&sid), &request, 107)
        .unwrap();
    assert_eq!(renewed["status"]["active"], true);
    assert_eq!(renewed["status"]["generation"], "1");
    f.service
        .store
        .transaction(|tx| {
            let mut session = tx.get::<AccountSession>(&key)?.unwrap();
            session.value.revoked_at = Some(108);
            tx.replace(
                &key,
                session.row_version,
                "session",
                Some(&session.value.account_id),
                None,
                &session.value,
            )
        })
        .unwrap();
    let inactive = f
        .service
        .managed_session_status(Some(&sid), &request, 109)
        .unwrap();
    assert_eq!(inactive["status"]["active"], false);
    assert_eq!(inactive["status"]["frozen"], true);
    assert_eq!(inactive["status"]["pending"], true);
    assert_eq!(inactive["status"]["generation"], "2");
    let fact = f
        .service
        .managed_invalidation(
            &context,
            &json!({"registration_operation_id":op}),
            "https://localhost",
        )
        .unwrap();
    assert_eq!(fact["invalidation"]["reason"], "session_invalidated");
    assert_eq!(fact["invalidation"]["service_epoch"], "7");
    assert_eq!(
        fact["invalidation"]["receipt"]["custody_account_id"],
        "custody-1"
    );
    f.service
        .store
        .transaction(|tx| {
            assert_eq!(tx.count_kind("custody_source_revocation")?, 0);
            assert!(
                tx.get::<ManagedInvalidationPending>(&format!(
                    "custody_managed_invalidation:{}",
                    c.challenge.native_account_id
                ))?
                .unwrap()
                .value
                .pending
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn disabled_bindings_refuse_and_original_challenge_is_never_renewed() {
    let (mut f, _, op, context) = setup();
    let original = challenge(&mut f.service, &op, &context);
    let expired = f
        .service
        .managed_binding_route(
            "POST",
            "/custody/managed-bindings/options",
            &context,
            &json!({"registration_operation_id":op,"binding_operation_id":"bind-1"}),
            "https://localhost",
            100_000,
        )
        .unwrap();
    assert_eq!(serde_json::to_value(original).unwrap(), expired);
    f.service.custody.as_mut().unwrap().managed_binding = None;
    assert!(
        f.service
            .managed_binding_route(
                "POST",
                "/custody/managed-bindings/options",
                &context,
                &json!({"registration_operation_id":op,"binding_operation_id":"bind-1"}),
                "https://localhost",
                104
            )
            .is_err()
    );
}

#[test]
fn distinct_registered_attempts_refresh_nonce_but_preserve_native_account_and_key() {
    let (mut f, owner, op, context) = setup();
    let old = challenge(&mut f.service, &op, &context);
    let request = json!({"registration_operation_id":op,"binding_operation_id":"bind-1"});
    let retry = f
        .service
        .managed_binding_route(
            "POST",
            "/custody/managed-bindings/options",
            &context,
            &request,
            "https://localhost",
            40_000,
        )
        .unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap(), retry);
    let fresh: SignedNativeBindingChallengeV1 = serde_json::from_value(
        f.service
            .managed_binding_route(
                "POST",
                "/custody/managed-bindings/options",
                &context,
                &json!({"registration_operation_id":op,"binding_operation_id":"bind-2"}),
                "https://localhost",
                40_000,
            )
            .unwrap(),
    )
    .unwrap();
    assert_ne!(fresh.challenge.nonce, old.challenge.nonce);
    assert_eq!(fresh.challenge.challenge_id, "bind-2");
    assert_eq!(
        fresh.challenge.native_account_id,
        old.challenge.native_account_id
    );
    assert_eq!(
        fresh.challenge.login_public_key,
        old.challenge.login_public_key
    );
    let mut signed = statement(&f.service, fresh.challenge, &owner);
    signed.statement.authority_observed_at = DecimalU64(40_001);
    signed.statement.expires_at = DecimalU64(45_001);
    signed.signature = URL_SAFE_NO_PAD.encode(
        owner
            .sign(&signed.statement.signing_bytes().unwrap())
            .to_bytes(),
    );
    let original = complete(&mut f.service, &signed, &context, 40_002).unwrap();
    let lookup = f
        .service
        .managed_binding_route(
            "POST",
            "/custody/managed-bindings/lookup",
            &context,
            &json!({"registration_operation_id":op}),
            "https://localhost",
            100_000,
        )
        .unwrap();
    assert_eq!(lookup, original);
    assert_eq!(
        complete(&mut f.service, &signed, &context, 100_000).unwrap(),
        original
    );
    assert!(
        f.service
            .managed_binding_route(
                "POST",
                "/custody/managed-bindings/options",
                &context,
                &json!({"registration_operation_id":op,"binding_operation_id":"bind-3"}),
                "https://localhost",
                100_000
            )
            .is_err()
    );
}
#[test]
fn challenge_capacity_is_per_registration_and_never_blocks_original_fact_or_revoke() {
    let (mut f, _, op, context) = setup();
    let first = challenge(&mut f.service, &op, &context);
    for attempt in 2..=16 {
        f.service.managed_binding_route("POST", "/custody/managed-bindings/options", &context, &json!({"registration_operation_id":op,"binding_operation_id":format!("bind-{attempt}")}), "https://localhost", 102).unwrap();
    }
    assert!(
        f.service
            .managed_binding_route(
                "POST",
                "/custody/managed-bindings/options",
                &context,
                &json!({"registration_operation_id":op,"binding_operation_id":"bind-17"}),
                "https://localhost",
                102
            )
            .is_err()
    );
    assert_eq!(challenge(&mut f.service, &op, &context), first);
    f.service
        .store
        .transaction(|tx| {
            let key = credential_key(&first.challenge.native_credential_ref);
            let mut credential = tx.get::<CredentialRecord>(&key)?.unwrap();
            credential.value.state = "revoked".into();
            tx.replace(
                &key,
                credential.row_version,
                "credential",
                Some(&credential.value.account_id),
                None,
                &credential.value,
            )
        })
        .unwrap();
}
