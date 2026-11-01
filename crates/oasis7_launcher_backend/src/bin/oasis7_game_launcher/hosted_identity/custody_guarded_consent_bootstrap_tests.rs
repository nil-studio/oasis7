//! Real Native UV/signature + logout + supported SQLite backup rollback.
//! This targets options-time guard bootstrap, not Account prepay/World dispatch.
use super::*;
use oasis7_custody_core::approval_admission::*;
struct StrictBookCrypto;
impl ApprovalSignatureVerifier for StrictBookCrypto {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        for job in checks.checks() {
            let key: [u8; 32] = hex::decode(job.public_key_hex())
                .unwrap()
                .try_into()
                .unwrap();
            let signature: [u8; 64] = URL_SAFE_NO_PAD
                .decode(job.signature_base64url())
                .unwrap()
                .try_into()
                .unwrap();
            ed25519_dalek::VerifyingKey::from_bytes(&key)
                .unwrap()
                .verify_strict(job.canonical_bytes(), &Signature::from_bytes(&signature))
                .unwrap();
        }
        Ok(())
    }
}
#[test]
fn guarded_full_mfa_revoke_then_database_restore_requires_external_lineage() {
    if std::env::var_os("OASIS7_BOOTSTRAP_TEST_CHILD").is_none() {
        let directory = std::env::temp_dir().join(format!("guarded-consent-{}", random_token()));
        std::fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let now = now_ms();
        let pin = |purpose: &str| json!({"namespace":"account","service_id":"account-service","environment":"test","key_id":"account-key","public_key":hex::encode(SigningKey::from_bytes(&[81;32]).verifying_key().to_bytes()),"issuer_epoch":"1","config_generation":"1","not_before":now.saturating_sub(1000).to_string(),"not_after":(now+600000).to_string(),"native_audience_origin":"https://localhost","allowed_purpose":purpose});
        let config = json!({"api_version":2,"pending_pin":pin("consumed_pending"),"active_pin":pin("active_three_roots"),"current_pin":pin("native_current_attempt"),"term_current_pin":pin("native_term_current_attempt"),"issuer_floor":{"issuer_epoch":"1","config_generation":"1"},"native_origin":"https://localhost"});
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("guarded_full_mfa_revoke_then_database_restore_requires_external_lineage")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("OASIS7_BOOTSTRAP_TEST_CHILD", "1")
            .env("OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED", "true")
            .env(
                "OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG",
                config.to_string(),
            )
            .env(
                "OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH",
                directory.join("lineage"),
            )
            .status()
            .unwrap();
        let _ = std::fs::remove_dir_all(directory);
        assert!(status.success(), "guarded consent child failed");
        return;
    }
    let (mut f, key) = fixture();
    let now = now_ms();
    f.service
        .custody
        .as_mut()
        .unwrap()
        .full_session_v2_summary_contract_enabled = true;
    f.service
        .store
        .attach_machine_restore_guard_from_env(&f.service.config.sqlite_path)
        .unwrap();
    let trust = serde_json::from_value(json!({"endpoint":"http://127.0.0.1:9","service_public_key":hex::encode(SigningKey::from_bytes(&[82;32]).verifying_key().to_bytes()),"world":{"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":0}})).unwrap();
    f.service.participant_entry = Some(
        super::super::super::super::participant_entry::ParticipantEntry::new(
            trust,
            zeroize::Zeroizing::new(hex::encode([83; 32])),
        )
        .unwrap(),
    );
    let start = f
        .service
        .custody_options(Some(&f.token), &session_options(now), "source", now)
        .unwrap();
    let completed = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&start, &key, 0x05),
            "source",
            now + 1,
        )
        .unwrap();
    let signed: SignedCustodyApprovalV1 =
        serde_json::from_value(completed["approval"].clone()).unwrap();
    let signature: [u8; 64] = URL_SAFE_NO_PAD
        .decode(&signed.signature)
        .unwrap()
        .try_into()
        .unwrap();
    SigningKey::from_bytes(&[51; 32])
        .verifying_key()
        .verify_strict(
            &signed.proof.signing_bytes().unwrap(),
            &Signature::from_bytes(&signature),
        )
        .unwrap();
    let (_, manager) = f.service.session(Some(&f.token), now + 2).unwrap();
    let proof = &signed.proof;
    let input = ApprovalAdmissionInput {
        operation_id: "session".into(),
        request: serde_json::from_value(session_options(now)).unwrap(),
        approval: signed.clone(),
    };
    let context = ApprovalAdmissionContext {
        identity_namespace: proof.identity_namespace.clone(),
        identity_instance_epoch: proof.identity_instance_epoch,
        authority: proof.context.clone(),
        allowed_managers: [proof.manager_account_id.clone()].into_iter().collect(),
        source: ApprovalSourceVersions {
            manager_account_id: proof.manager_account_id.clone(),
            manager_session_ref: proof.manager_session_ref.clone(),
            origin_credential_id: proof.origin_credential_id.clone(),
            origin_credential_version: proof.origin_credential_version,
            passkey_credential_id: proof.passkey_credential_id.clone(),
            passkey_credential_version: proof.passkey_credential_version,
            manager_policy_version: proof.manager_policy_version,
            manager_security_version: proof.manager_security_version,
            session_expires_at: DecimalU64(manager.expires_at),
            session_idle_expires_at: DecimalU64(manager.idle_expires_at),
        },
        native_key_pins: [(
            proof.key_id.clone(),
            hex::encode(SigningKey::from_bytes(&[51; 32]).verifying_key().to_bytes()),
        )]
        .into_iter()
        .collect(),
        fence: oasis7_custody_core::recovery::RecoveryFence::new(
            proof.context.service_epoch.0,
            false,
        ),
        frozen: false,
        handoff_until: proof.expires_at,
    };
    let mut book = ApprovalAdmissionBook::default();
    let admitted = book
        .begin(
            input,
            "77".repeat(32),
            &context,
            now + 2,
            &mut StrictBookCrypto,
        )
        .unwrap();
    let verify = admitted.native_verify_request().unwrap();
    let headers = format!(
        "POST / HTTP/1.1\r\nAuthorization: Bearer {}\r\n",
        hex::encode([52; 32])
    );
    let verification = f
        .service
        .custody_verify_route(
            "POST",
            &headers,
            &serde_json::to_value(verify).unwrap(),
            "source",
            now + 3,
        )
        .unwrap();
    let consumed = book
        .commit(
            "session",
            serde_json::from_value(verification).unwrap(),
            &context,
            now + 3,
            &mut StrictBookCrypto,
        )
        .unwrap();
    assert_eq!(consumed.stage(), ApprovalAdmissionStage::Consumed);
    let backup = f
        .service
        .config
        .sqlite_path
        .with_file_name("before-revoke.sqlite");
    f.service.store.backup(&backup).unwrap();
    // Same backup path is a legal cold restore before the later revocation.
    // The target below therefore cannot pass merely because the filename differs.
    {
        let mut cold = super::super::super::super::store::IdentityStore::open(
            &backup,
            f.service.config.namespace.clone(),
            f.service.config.rp.clone(),
        )
        .unwrap();
        cold.attach_machine_restore_guard_from_env(&backup).unwrap();
    }
    f.service.logout(Some(&f.token), now + 4).unwrap();
    let mut restored = super::super::super::super::store::IdentityStore::open(
        &backup,
        f.service.config.namespace.clone(),
        f.service.config.rp.clone(),
    )
    .unwrap();
    // A complete protected options/UV lineage must already have advanced before
    // this revocation. First registration cannot bless the restored old database.
    assert!(
        restored
            .attach_machine_restore_guard_from_env(&backup)
            .is_err()
    );
}
