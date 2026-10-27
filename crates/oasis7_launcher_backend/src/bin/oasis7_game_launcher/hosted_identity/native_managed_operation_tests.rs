use super::*;
fn request(id: &str, key: &SigningKey) -> Value {
    json!({"intent":"login","public_key":key_hex(key),"managed_operation_id":id})
}
fn start(s: &mut IdentityService, p: &Value, now: u64) -> Result<Value, StoreError> {
    s.native_options(None, "owner", p, "source", "https://localhost", now)
}
#[test]
fn managed_id_keeps_original_challenge_and_success_tombstone_never_recovers_cookie() {
    let (mut s, _, key) = fixture();
    s.enable_managed_login_test_adapter();
    let id = random_token();
    let p = request(&id, &key);
    let started = start(&mut s, &p, 101).unwrap();
    // RED on the old implementation: the reply used an unrelated random ID.
    assert_eq!(started["operation_id"], id);
    assert_eq!(started, start(&mut s, &p, 102).unwrap());
    let signed = proof(&started, &key);
    let (reply, cookie) = complete(&mut s, None, &signed, 103).unwrap();
    assert_eq!(reply["phase"], "authenticated");
    assert!(cookie.is_some());
    drop(cookie); // Simulate losing the successful response without retaining its Cookie.
    s.store = super::super::super::store::IdentityStore::open(
        &s.config.sqlite_path,
        s.config.namespace.clone(),
        s.config.rp.clone(),
    )
    .unwrap();
    let replay = complete(&mut s, None, &signed, 104).unwrap();
    assert_eq!(replay.0["phase"], "operation_committed");
    assert!(replay.1.is_none());
    let future = 9 * 86_400_000;
    for _ in 0..10 {
        if s.store.cleanup_expired(future).unwrap() == 0 {
            break;
        }
    }
    s.store
        .transaction(|tx| {
            assert!(
                tx.get::<CeremonyRecord>(&format!("ceremony:{id}"))?
                    .is_none()
            );
            assert!(
                tx.get::<OperationReceipt>(&format!("receipt:{id}"))?
                    .is_none()
            );
            let outcome = tx
                .get::<Value>(&format!("native_managed_login_outcome:{id}"))?
                .unwrap()
                .value;
            assert!(
                outcome.get("cookie").is_none()
                    && outcome.get("token").is_none()
                    && outcome.get("sid").is_none()
            );
            Ok(())
        })
        .unwrap();
    let after_cleanup = start(&mut s, &p, future).unwrap();
    assert_eq!(after_cleanup["phase"], "operation_committed");
    assert!(after_cleanup.get("options").is_none());
    assert!(complete(&mut s, None, &signed, future).unwrap().1.is_none());
}
#[test]
fn managed_pending_scope_is_exact_and_expiry_never_renews_nonce() {
    let (mut s, _, key) = fixture();
    s.enable_managed_login_test_adapter();
    let id = random_token();
    let p = request(&id, &key);
    let started = start(&mut s, &p, 101).unwrap();
    assert!(
        s.native_options(None, "other", &p, "source", "https://localhost", 102)
            .is_err()
    );
    assert!(
        s.native_options(None, "owner", &p, "source", "https://other", 102)
            .is_err()
    );
    assert!(
        start(
            &mut s,
            &request(&id, &SigningKey::from_bytes(&[77; 32])),
            102
        )
        .is_err()
    );
    s.config.rp.identity_instance_epoch += 1;
    assert!(start(&mut s, &p, 102).is_err());
    assert!(complete(&mut s, None, &proof(&started, &key), 102).is_err());
    s.config.rp.identity_instance_epoch -= 1;
    s.config.namespace.environment_id = "other".into();
    assert!(start(&mut s, &p, 102).is_err());
    s.config.namespace.environment_id = "test".into();
    assert!(start(&mut s, &p, 120_101).is_err());
    s.store
        .transaction(|tx| {
            let original = tx
                .get::<CeremonyRecord>(&format!("ceremony:{id}"))?
                .unwrap()
                .value;
            assert_eq!(
                original.server_state.unwrap()["nonce"],
                started["options"]["nonce"]
            );
            assert_eq!(original.expires_at, 120_101);
            assert_eq!(tx.count_kind("native_managed_login_outcome")?, 0);
            Ok(())
        })
        .unwrap();
}
#[test]
fn managed_id_is_explicit_opt_in_and_does_not_change_ordinary_operation_id_behavior() {
    let (mut s, token, key) = fixture();
    let id = random_token();
    assert!(start(&mut s, &request(&id, &key), 101).is_err());
    let ordinary = json!({"intent":"login","public_key":key_hex(&key),"operation_id":id});
    let first = start(&mut s, &ordinary, 101).unwrap();
    let second = start(&mut s, &ordinary, 102).unwrap();
    assert_ne!(first["operation_id"], id);
    assert_ne!(first["operation_id"], second["operation_id"]);
    s.enable_managed_login_test_adapter();
    for bad in [json!(null), json!("x"), json!(format!("{id}="))] {
        let mut p = request(&id, &key);
        p["managed_operation_id"] = bad;
        assert!(start(&mut s, &p, 103).is_err());
    }
    for purpose in ["bind", "reauth"] {
        let mut p = request(&id, &key);
        p["intent"] = json!(purpose);
        p["action_id"] = json!(random_token());
        assert!(
            s.native_options(
                Some(&token),
                "owner",
                &p,
                "source",
                "https://localhost",
                103
            )
            .is_err()
        );
    }
}
#[test]
fn successful_managed_persistence_is_atomic_with_session_and_no_tombstone_before_pop() {
    let (mut s, _, key) = fixture();
    s.enable_managed_login_test_adapter();
    let id = random_token();
    let started = start(&mut s, &request(&id, &key), 101).unwrap();
    let signed = proof(&started, &key);
    let before = s.store.transaction(|tx| tx.count_kind("session")).unwrap();
    let db = rusqlite::Connection::open(&s.config.sqlite_path).unwrap();
    db.execute_batch("CREATE TRIGGER fail_managed_outcome BEFORE INSERT ON identity_records WHEN NEW.kind='native_managed_login_outcome' BEGIN SELECT RAISE(ABORT,'fixture managed outcome failure'); END;").unwrap();
    assert!(complete(&mut s, None, &signed, 102).is_err());
    s.store
        .transaction(|tx| {
            assert_eq!(tx.count_kind("session")?, before);
            assert_eq!(tx.count_kind("native_managed_login_outcome")?, 0);
            assert_eq!(
                tx.get::<CeremonyRecord>(&format!("ceremony:{id}"))?
                    .unwrap()
                    .value
                    .status,
                "pending"
            );
            Ok(())
        })
        .unwrap();
    db.execute_batch("DROP TRIGGER fail_managed_outcome")
        .unwrap();
    assert!(complete(&mut s, None, &signed, 103).unwrap().1.is_some());
    assert!(complete(&mut s, None, &signed, 104).unwrap().1.is_none());
}
