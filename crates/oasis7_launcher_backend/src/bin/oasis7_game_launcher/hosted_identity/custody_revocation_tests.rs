use super::super::super::{model_session::model_session_tests::Fixture, service::session_key};
use super::*;
fn fixture() -> Fixture {
    let (f, _) = super::super::tests::fixture();
    f
}
fn headers(poll: bool) -> String {
    format!(
        "POST / HTTP/1.1\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\n",
        hex::encode(if poll { [83; 32] } else { [84; 32] })
    )
}
fn poll(f: &mut Fixture, limit: u16) -> Vec<SignedSourceRevocationEventV1> {
    let now = now_ms();
    let value = f
        .service
        .custody_revocation_route(
            "POST",
            "/custody/revocations/poll",
            &headers(true),
            &serde_json::json!({"api_version":1,"limit":limit}),
            now,
        )
        .unwrap();
    serde_json::from_value::<SourceRevocationsPollResponseV1>(value)
        .unwrap()
        .events
}
fn change_account(f: &mut Fixture) {
    f.service
        .store
        .transaction(|tx| {
            let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
            a.value.policy_version += 1;
            tx.replace(
                "account:a",
                a.row_version,
                "account",
                Some("a"),
                None,
                &a.value,
            )
        })
        .unwrap();
}
fn ack(event: &SignedSourceRevocationEventV1) -> SignedSourceClosureAckV1 {
    let event = &event.event;
    let ack = SourceClosureAckV1 {
        version: 1,
        key_id: "owner-source-test".into(),
        event_id: event.event_id.clone(),
        event_digest: String::new(),
        identity_namespace: event.identity_namespace.clone(),
        environment: event.environment.clone(),
        identity_instance_epoch: event.identity_instance_epoch,
        manager_account_id: event.manager_account_id.clone(),
        owner_audience: event.owner_audience.clone(),
        closure_digest: "a".repeat(64),
        inventory_digest: "b".repeat(64),
        closed_at: DecimalU64(now_ms()),
        status: SourceClosureStateV1::Closed,
    };
    SignedSourceClosureAckV1 {
        ack,
        signature: String::new(),
    }
}
fn sign_ack(ack: &mut SignedSourceClosureAckV1) {
    ack.signature = URL_SAFE_NO_PAD.encode(
        SigningKey::from_bytes(&[92; 32])
            .sign(&ack.ack.signing_bytes().unwrap())
            .to_bytes(),
    );
}
#[test]
fn source_writes_and_signed_originals_commit_or_roll_back_together() {
    let mut f = fixture();
    let before = f
        .service
        .store
        .transaction(|tx| {
            Ok(tx
                .get::<AccountRecord>("account:a")?
                .unwrap()
                .value
                .policy_version)
        })
        .unwrap();
    let failed: Result<(), StoreError> = f.service.store.transaction(|tx| {
        let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
        a.value.policy_version += 1;
        tx.replace(
            "account:a",
            a.row_version,
            "account",
            Some("a"),
            None,
            &a.value,
        )?;
        Err(StoreError::Conflict)
    });
    assert!(failed.is_err());
    assert!(poll(&mut f, 8).is_empty());
    assert_eq!(
        f.service
            .store
            .transaction(|tx| Ok(tx
                .get::<AccountRecord>("account:a")?
                .unwrap()
                .value
                .policy_version))
            .unwrap(),
        before
    );
    change_account(&mut f);
    let events = poll(&mut f, 8);
    assert_eq!(events.len(), 1);
    assert_eq!(poll(&mut f, 8), events);
    let config = f.service.custody.as_ref().unwrap();
    let event = &events[0];
    config
        .key
        .verifying_key()
        .verify_strict(
            &event.event.signing_bytes().unwrap(),
            &Signature::from_slice(&URL_SAFE_NO_PAD.decode(&event.signature).unwrap()).unwrap(),
        )
        .unwrap();
    assert_eq!(event.event.manager_security_version.0, 9);
}
#[test]
fn ordinary_activity_and_extension_do_not_emit_but_logout_and_shortening_do() {
    let mut f = fixture();
    let key = session_key(&f.service.config.session_key, &f.token);
    f.service
        .store
        .transaction(|tx| {
            let mut s = tx.get::<AccountSession>(&key)?.unwrap();
            s.value.idle_expires_at += 1000;
            tx.replace(
                &key,
                s.row_version,
                "session",
                Some("a"),
                Some(s.value.expires_at),
                &s.value,
            )
        })
        .unwrap();
    assert!(poll(&mut f, 8).is_empty());
    f.service
        .store
        .transaction(|tx| {
            let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
            a.value.last_authenticated_at = Some(now_ms());
            tx.replace(
                "account:a",
                a.row_version,
                "account",
                Some("a"),
                None,
                &a.value,
            )
        })
        .unwrap();
    assert!(poll(&mut f, 8).is_empty());
    f.service
        .store
        .transaction(|tx| {
            let mut s = tx.get::<AccountSession>(&key)?.unwrap();
            s.value.idle_expires_at -= 1;
            tx.replace(
                &key,
                s.row_version,
                "session",
                Some("a"),
                Some(s.value.expires_at),
                &s.value,
            )
        })
        .unwrap();
    assert_eq!(poll(&mut f, 8).len(), 1);
    f.service.logout(Some(&f.token), now_ms()).unwrap();
    let original = poll(&mut f, 8);
    assert_eq!(original.len(), 2);
    f.service.logout(Some(&f.token), now_ms() + 1).unwrap();
    let retried = poll(&mut f, 8);
    assert_eq!(retried.len(), 2);
    for event in original {
        assert!(retried.contains(&event));
    }
}
#[test]
fn poll_round_robin_preserves_signatures_and_does_not_starve_unacknowledged_events() {
    let mut f = fixture();
    for _ in 0..3 {
        change_account(&mut f)
    }
    let a = poll(&mut f, 1).pop().unwrap();
    let b = poll(&mut f, 1).pop().unwrap();
    let c = poll(&mut f, 1).pop().unwrap();
    assert_ne!(a.event.event_id, b.event.event_id);
    assert_ne!(b.event.event_id, c.event.event_id);
    assert_ne!(a.event.event_id, c.event.event_id);
    assert_eq!(poll(&mut f, 1), vec![a]);
}
#[test]
fn acknowledgements_require_independent_auth_owner_signature_and_exact_event_binding() {
    let mut f = fixture();
    change_account(&mut f);
    let event = poll(&mut f, 8).pop().unwrap();
    let mut signed = ack(&event);
    signed.ack.event_digest = event.digest().unwrap();
    sign_ack(&mut signed);
    let path = "/custody/revocations/ack";
    let now = now_ms();
    let value = serde_json::to_value(&signed).unwrap();
    assert!(
        f.service
            .custody_revocation_route("POST", path, &headers(true), &value, now)
            .is_err()
    );
    for mutate in 0..4 {
        let mut wrong = signed.clone();
        match mutate {
            0 => wrong.ack.manager_account_id = "other".into(),
            1 => wrong.ack.event_digest = "c".repeat(64),
            2 => wrong.ack.closed_at = DecimalU64(event.event.observed_at.0 - 1),
            _ => wrong.ack.key_id = "other-key".into(),
        };
        sign_ack(&mut wrong);
        assert!(
            f.service
                .custody_revocation_route(
                    "POST",
                    path,
                    &headers(false),
                    &serde_json::to_value(wrong).unwrap(),
                    now
                )
                .is_err()
        );
    }
    let original = f
        .service
        .custody_revocation_route("POST", path, &headers(false), &value, now)
        .unwrap();
    assert_eq!(
        f.service
            .custody_revocation_route("POST", path, &headers(false), &value, now)
            .unwrap(),
        original
    );
    assert!(poll(&mut f, 8).is_empty());
    assert!(
        f.service
            .custody_revocation_route(
                "POST",
                path,
                &headers(false),
                &serde_json::json!({"ack":true}),
                now
            )
            .is_err()
    );
}
#[test]
fn complete_configuration_requires_independent_credentials_and_role_keys() {
    let (f, _) = super::super::tests::fixture();
    let config = f.service.custody.as_ref().unwrap();
    let good = vec![
        Some(hex::encode([83; 32])),
        Some(hex::encode([84; 32])),
        Some("owner-source-test".into()),
        Some(hex::encode(
            SigningKey::from_bytes(&[92; 32]).verifying_key().to_bytes(),
        )),
        Some(config.audience.clone()),
        Some("[\"a\"]".into()),
    ];
    assert!(
        RevocationConfig::from_values(config, &f.service.config.session_key, vec![None; 6])
            .unwrap()
            .is_none()
    );
    assert!(
        RevocationConfig::from_values(config, &f.service.config.session_key, good.clone())
            .unwrap()
            .is_some()
    );
    for index in 0..6 {
        let mut bad = good.clone();
        bad[index] = None;
        assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
    }
    for registry in ["[\"a\",\"a\"]", "[\"\"]", "{}"] {
        let mut bad = good.clone();
        bad[5] = Some(registry.into());
        assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
    }
    let mut empty = good.clone();
    empty[5] = Some("[]".into());
    assert!(
        RevocationConfig::from_values(config, &f.service.config.session_key, empty)
            .unwrap()
            .is_some()
    );
    let mut bad = good.clone();
    bad[1] = bad[0].clone();
    assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
    let mut bad = good.clone();
    bad[0] = Some(hex::encode(config.adapter_secret));
    assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
    let mut bad = good;
    bad[3] = Some(hex::encode(config.key.verifying_key().to_bytes()));
    assert!(RevocationConfig::from_values(config, &f.service.config.session_key, bad).is_err());
}
#[test]
fn offline_epoch_requires_real_signed_closed_inventory_not_boolean_attestations() {
    use super::super::super::store::IdentityStore;
    let mut f = fixture();
    let path = f.service.config.sqlite_path.clone();
    let namespace = f.service.config.namespace.clone();
    let mut rp = f.service.config.rp.clone();
    rp.identity_instance_epoch += 1;
    assert!(IdentityStore::advance_instance_epoch(&path, &namespace, &rp, true, true).is_err());
    change_account(&mut f);
    let event = poll(&mut f, 8).pop().unwrap();
    assert!(IdentityStore::advance_instance_epoch(&path, &namespace, &rp, true, true).is_err());
    let mut signed = ack(&event);
    signed.ack.event_digest = event.digest().unwrap();
    sign_ack(&mut signed);
    f.service
        .custody_revocation_route(
            "POST",
            "/custody/revocations/ack",
            &headers(false),
            &serde_json::to_value(&signed).unwrap(),
            now_ms(),
        )
        .unwrap();
    IdentityStore::advance_instance_epoch(&path, &namespace, &rp, true, true).unwrap();
    assert!(IdentityStore::open(&path, namespace.clone(), f.service.config.rp.clone()).is_err());
    assert!(IdentityStore::open(&path, namespace, rp).is_ok());
}
#[test]
fn source_deletion_and_kind_reclassification_cannot_bypass_hook_or_block_ceremony_cleanup() {
    let mut f = fixture();
    let session = session_key(&f.service.config.session_key, &f.token);
    for key in ["account:a".to_owned(), session] {
        assert!(
            f.service
                .store
                .transaction(|tx| {
                    let record = tx.get::<Value>(&key)?.unwrap();
                    tx.delete(&key, record.row_version)
                })
                .is_err()
        );
        assert!(
            f.service
                .store
                .transaction(|tx| {
                    let record = tx.get::<Value>(&key)?.unwrap();
                    tx.replace(
                        &key,
                        record.row_version,
                        "unrelated",
                        Some("a"),
                        None,
                        &record.value,
                    )
                })
                .is_err()
        );
    }
    f.service
        .store
        .transaction(|tx| {
            tx.create(
                "expired-test-ceremony",
                "ceremony",
                Some("a"),
                Some(1),
                &serde_json::json!({"test":true}),
            )
        })
        .unwrap();
    assert!(f.service.store.cleanup_expired(now_ms()).unwrap() > 0);
    assert!(poll(&mut f, 8).is_empty());
}
#[test]
fn missing_base_configuration_does_not_ignore_revocation_group() {
    const CHILD: &str = "OASIS7_REVOCATION_MISSING_BASE_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        assert!(CustodyConfig::from_env(&[19; 32]).is_err());
        return;
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args(["hosted_identity::custody::revocation::tests::missing_base_configuration_does_not_ignore_revocation_group","--exact"]);
    for name in [
        "OASIS7_CUSTODY_APPROVAL_SIGNING_KEY",
        "OASIS7_CUSTODY_APPROVAL_KEY_ID",
        "OASIS7_CUSTODY_ACCOUNT_AUTHORITY_AUDIENCE",
        "OASIS7_CUSTODY_NATIVE_ADAPTER_SECRET",
        "OASIS7_CUSTODY_REVOCATION_ACK_SECRET",
        "OASIS7_CUSTODY_REVOCATION_OWNER_KEY_ID",
        "OASIS7_CUSTODY_REVOCATION_OWNER_PUBLIC_KEY",
        "OASIS7_CUSTODY_REVOCATION_OWNER_AUDIENCE",
    ] {
        command.env_remove(name);
    }
    let output = command
        .env(CHILD, "1")
        .env(
            "OASIS7_CUSTODY_REVOCATION_POLL_SECRET",
            hex::encode([83; 32]),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated missing-base configuration test failed"
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("1 passed")
    );
}
#[test]
fn credential_security_mutation_emits_only_public_source_versions() {
    let mut f = fixture();
    f.service
        .store
        .transaction(|tx| {
            let (key, mut credential) = tx
                .all::<CredentialRecord>("credential", Some("a"))?
                .into_iter()
                .find(|(_, c)| {
                    matches!(c.value.public_material, CredentialMaterial::WebAuthn { .. })
                })
                .unwrap();
            credential.value.state = "revoked".into();
            credential.value.version += 1;
            tx.replace(
                &key,
                credential.row_version,
                "credential",
                Some("a"),
                None,
                &credential.value,
            )
        })
        .unwrap();
    let events = poll(&mut f, 8);
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].event.source,
        RevokedSourceV1::Credential { .. }
    ));
    let raw = serde_json::to_string(&events).unwrap();
    assert!(!raw.contains("library_record"));
    assert!(!raw.contains("token_digest"));
}
fn fill_journal(f: &mut Fixture) {
    f.service
        .store
        .transaction(|tx| {
            for _ in 0..MAX_SOURCE_EVENTS {
                let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
                a.value.policy_version += 1;
                tx.replace(
                    "account:a",
                    a.row_version,
                    "account",
                    Some("a"),
                    None,
                    &a.value,
                )?;
            }
            Ok(())
        })
        .unwrap();
}
#[test]
fn full_journal_preserves_logout_and_credential_revocation_with_permanent_checkpoint() {
    let mut f = fixture();
    fill_journal(&mut f);
    f.service.logout(Some(&f.token), now_ms()).unwrap();
    assert!(f.service.session(Some(&f.token), now_ms()).is_err());
    let first = f
        .service
        .store
        .transaction(|tx| {
            Ok(tx
                .get::<overflow::OverflowRecord>(&overflow::record_key("a"))?
                .unwrap()
                .value)
        })
        .unwrap();
    assert!(matches!(
        first.trigger.original.event.source,
        RevokedSourceV1::Session { .. }
    ));
    assert_eq!(first.checkpoint.checkpoint.watermark.0, 1);
    f.service
        .store
        .transaction(|tx| {
            let mut c = tx.get::<CredentialRecord>("credential:p")?.unwrap();
            c.value.state = "revoked".into();
            c.value.version += 1;
            tx.replace(
                "credential:p",
                c.row_version,
                "credential",
                Some("a"),
                None,
                &c.value,
            )
        })
        .unwrap();
    let second = f
        .service
        .store
        .transaction(|tx| {
            assert_eq!(
                tx.count_kind("custody_source_revocation")?,
                MAX_SOURCE_EVENTS
            );
            assert_eq!(tx.count_kind(overflow::KIND)?, 1);
            assert_eq!(
                tx.get::<CredentialRecord>("credential:p")?
                    .unwrap()
                    .value
                    .state,
                "revoked"
            );
            assert!(overflow::require_participant(tx, "a").is_err());
            Ok(tx
                .get::<overflow::OverflowRecord>(&overflow::record_key("a"))?
                .unwrap()
                .value)
        })
        .unwrap();
    assert_eq!(first.trigger.original, second.trigger.original);
    assert_eq!(second.checkpoint.checkpoint.watermark.0, 2);
    assert_eq!(
        second.checkpoint.checkpoint.previous_checkpoint_digest,
        first.checkpoint.digest().unwrap()
    );
    assert_ne!(
        first.checkpoint.checkpoint.chain_digest,
        second.checkpoint.checkpoint.chain_digest
    );
    let delivered = poll(&mut f, 1).pop().unwrap();
    assert_eq!(delivered, first.trigger.original);
    let next = poll(&mut f, 1).pop().unwrap();
    assert_ne!(next.event.event_id, delivered.event.event_id);
    let mut signed = ack(&first.trigger.original);
    signed.ack.event_digest = first.trigger.original.digest().unwrap();
    sign_ack(&mut signed);
    f.service
        .custody_revocation_route(
            "POST",
            "/custody/revocations/ack",
            &headers(false),
            &serde_json::to_value(&signed).unwrap(),
            now_ms(),
        )
        .unwrap();
    f.service
        .store
        .transaction(|tx| {
            assert!(overflow::require_participant(tx, "a").is_err());
            let row = tx
                .get::<overflow::OverflowRecord>(&overflow::record_key("a"))?
                .unwrap();
            assert_eq!(row.value.checkpoint, second.checkpoint);
            assert_eq!(row.value.trigger.acknowledgement, Some(signed));
            Ok(())
        })
        .unwrap();
}
#[test]
fn registered_bridge_missing_metadata_is_not_a_legacy_unregistered_store() {
    use super::super::super::store::IdentityStore;
    let mut f = fixture();
    f.service
        .store
        .transaction(|tx| {
            let record = tx.get::<Value>("custody_revocation_bridge_meta")?.unwrap();
            tx.delete("custody_revocation_bridge_meta", record.row_version)
        })
        .unwrap();
    let mut rp = f.service.config.rp.clone();
    rp.identity_instance_epoch += 1;
    assert!(
        IdentityStore::advance_instance_epoch(
            &f.service.config.sqlite_path,
            &f.service.config.namespace,
            &rp,
            true,
            true
        )
        .is_err()
    );
    let mut reopened = IdentityStore::open(
        &f.service.config.sqlite_path,
        f.service.config.namespace.clone(),
        f.service.config.rp.clone(),
    )
    .unwrap();
    assert!(
        reopened
            .require_revocation_configuration_if_registered()
            .is_err()
    );
    assert!(
        f.service
            .custody
            .as_ref()
            .unwrap()
            .attach_revocation(
                &mut reopened,
                f.service.config.namespace.key(),
                f.service.config.namespace.environment_id.clone(),
                f.service.config.rp.identity_instance_epoch
            )
            .is_err()
    );
}

#[test]
fn unregistered_account_invalidation_and_cleanup_do_not_enter_custody_barrier() {
    use super::super::super::store::IdentityStore;
    let mut f = fixture();
    f.service
        .store
        .transaction(|tx| {
            let mut other = tx.get::<AccountRecord>("account:a")?.unwrap().value;
            other.account_id = "ordinary".into();
            tx.create(
                "account:ordinary",
                "account",
                Some("ordinary"),
                None,
                &other,
            )?;
            other.security_epoch += 1;
            let row = tx.get::<AccountRecord>("account:ordinary")?.unwrap();
            tx.replace(
                "account:ordinary",
                row.row_version,
                "account",
                Some("ordinary"),
                None,
                &other,
            )?;
            let mut session = tx
                .get::<AccountSession>(&session_key(&f.service.config.session_key, &f.token))?
                .unwrap()
                .value;
            session.account_id = "ordinary".into();
            session.session_id = "ordinary-session".into();
            session.expires_at = 1;
            session.idle_expires_at = 1;
            session.revoked_at = None;
            tx.create(
                &session_key(&f.service.config.session_key, "ordinary-token"),
                "session",
                Some("ordinary"),
                Some(1),
                &session,
            )?;
            assert_eq!(tx.count_kind("custody_source_revocation")?, 0);
            Ok(())
        })
        .unwrap();
    f.service.logout(Some("ordinary-token"), now_ms()).unwrap();
    assert_eq!(
        f.service
            .store
            .transaction(|tx| tx.count_kind("custody_source_revocation"))
            .unwrap(),
        0
    );
    assert_eq!(f.service.store.cleanup_expired(now_ms()).unwrap(), 1);
    assert!(
        f.service
            .store
            .transaction(|tx| overflow::require_participant(tx, "ordinary"))
            .is_err()
    );
    // Only participant a needs a closure, even with another account in the same native store.
    f.service.logout(Some(&f.token), now_ms()).unwrap();
    let event = poll(&mut f, 8).pop().unwrap();
    let mut signed = ack(&event);
    signed.ack.event_digest = event.digest().unwrap();
    sign_ack(&mut signed);
    f.service
        .custody_revocation_route(
            "POST",
            "/custody/revocations/ack",
            &headers(false),
            &serde_json::to_value(signed).unwrap(),
            now_ms(),
        )
        .unwrap();
    let mut next = f.service.config.rp.clone();
    next.identity_instance_epoch += 1;
    IdentityStore::advance_instance_epoch(
        &f.service.config.sqlite_path,
        &f.service.config.namespace,
        &next,
        true,
        true,
    )
    .unwrap();
}
#[test]
fn participant_configuration_is_canonical_immutable_and_checks_existing_accounts() {
    let mut f = fixture();
    let custody = f.service.custody.as_ref().unwrap();
    let mut signer = custody.revocation.as_ref().unwrap().signer(
        custody,
        f.service.config.namespace.key(),
        f.service.config.namespace.environment_id.clone(),
        f.service.config.rp.identity_instance_epoch,
    );
    signer.metadata.participants = vec![];
    assert!(f.service.store.enable_source_revocations(signer).is_err());
    let mut signer = custody.revocation.as_ref().unwrap().signer(
        custody,
        f.service.config.namespace.key(),
        f.service.config.namespace.environment_id.clone(),
        f.service.config.rp.identity_instance_epoch,
    );
    signer.metadata.participants = vec!["missing".into()];
    assert!(f.service.store.enable_source_revocations(signer).is_err());
}
#[test]
fn full_checkpoint_is_verified_on_restart_and_watermark_saturates() {
    use super::super::super::store::IdentityStore;
    let mut f = fixture();
    fill_journal(&mut f);
    f.service.logout(Some(&f.token), now_ms()).unwrap();
    let before = f
        .service
        .store
        .transaction(|tx| {
            let mut row = tx
                .get::<overflow::OverflowRecord>(&overflow::record_key("a"))?
                .unwrap();
            row.value.checkpoint.checkpoint.watermark = DecimalU64(u64::MAX);
            row.value.checkpoint.signature = URL_SAFE_NO_PAD.encode(
                SigningKey::from_bytes(&[51; 32])
                    .sign(&row.value.checkpoint.checkpoint.signing_bytes().unwrap())
                    .to_bytes(),
            );
            tx.replace(
                &overflow::record_key("a"),
                row.row_version,
                overflow::KIND,
                Some("a"),
                None,
                &row.value,
            )?;
            Ok(row.value)
        })
        .unwrap();
    let custody = f.service.custody.as_ref().unwrap();
    let signer = custody.revocation.as_ref().unwrap().signer(
        custody,
        f.service.config.namespace.key(),
        f.service.config.namespace.environment_id.clone(),
        f.service.config.rp.identity_instance_epoch,
    );
    let mut reopened = IdentityStore::open(
        &f.service.config.sqlite_path,
        f.service.config.namespace.clone(),
        f.service.config.rp.clone(),
    )
    .unwrap();
    reopened.enable_source_revocations(signer).unwrap();
    reopened
        .transaction(|tx| {
            let mut a = tx.get::<AccountRecord>("account:a")?.unwrap();
            a.value.security_epoch += 1;
            tx.replace(
                "account:a",
                a.row_version,
                "account",
                Some("a"),
                None,
                &a.value,
            )?;
            let after = tx
                .get::<overflow::OverflowRecord>(&overflow::record_key("a"))?
                .unwrap();
            assert_eq!(after.value.checkpoint.checkpoint.watermark.0, u64::MAX);
            assert_eq!(
                after.value.checkpoint.checkpoint.previous_checkpoint_digest,
                before.checkpoint.digest().unwrap()
            );
            assert_ne!(
                after.value.checkpoint.checkpoint.chain_digest,
                before.checkpoint.checkpoint.chain_digest
            );
            let meta = tx
                .get::<BridgeMetadata>("custody_revocation_bridge_meta")?
                .unwrap();
            overflow::verify_checkpoint(&after.value, &meta.value)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn custody_management_without_registered_bridge_is_closed_but_native_session_works() {
    let mut f = super::super::super::model_session::model_session_tests::fixture();
    assert!(f.service.session(Some(&f.token), now_ms()).is_ok());
    let key = session_key(&f.service.config.session_key, &f.token);
    let epoch = f.service.config.rp.identity_instance_epoch;
    assert!(
        f.service
            .store
            .transaction(|tx| super::super::management_session(tx, &key, epoch, now_ms()))
            .is_err()
    );
    f.service.logout(Some(&f.token), now_ms()).unwrap();
    assert!(f.service.session(Some(&f.token), now_ms()).is_err());
}
