use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use oasis7_custody_core::{
    DecimalU64, approval::*, approval_admission::*, recovery::RecoveryFence,
};
use serde_json::{Value, json};

struct StrictCrypto;
impl ApprovalSignatureVerifier for StrictCrypto {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        for check in checks.checks() {
            let key = check
                .public_key_hex()
                .as_bytes()
                .chunks_exact(2)
                .map(|v| {
                    std::str::from_utf8(v)
                        .ok()
                        .and_then(|v| u8::from_str_radix(v, 16).ok())
                        .ok_or(ApprovalAdmissionError::SignatureRejected)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let key: [u8; 32] = key
                .try_into()
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            let key = VerifyingKey::from_bytes(&key)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            let bytes = URL_SAFE_NO_PAD
                .decode(check.signature_base64url())
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            let signature = Signature::from_slice(&bytes)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
            key.verify_strict(check.canonical_bytes(), &signature)
                .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
        }
        Ok(())
    }
}
struct ExactCache(ExactSignatureChecks);
impl ExactCache {
    fn verified(checks: ExactSignatureChecks) -> Self {
        StrictCrypto.verify(&checks).unwrap();
        Self(checks)
    }
}
impl ApprovalSignatureVerifier for ExactCache {
    fn verify(&mut self, checks: &ExactSignatureChecks) -> Result<(), ApprovalAdmissionError> {
        if checks == &self.0 {
            Ok(())
        } else {
            Err(ApprovalAdmissionError::SignatureRejected)
        }
    }
}
fn key() -> SigningKey {
    SigningKey::from_bytes(&[41; 32])
}
fn sign_proof(proof: CustodyApprovalProofV1) -> SignedCustodyApprovalV1 {
    SignedCustodyApprovalV1 {
        signature: URL_SAFE_NO_PAD.encode(key().sign(&proof.signing_bytes().unwrap()).to_bytes()),
        proof,
    }
}
fn sign_verification(verification: ApprovalVerificationV1) -> SignedApprovalVerificationV1 {
    SignedApprovalVerificationV1 {
        signature: URL_SAFE_NO_PAD.encode(
            key()
                .sign(&verification.signing_bytes().unwrap())
                .to_bytes(),
        ),
        verification,
    }
}
fn fixture() -> (ApprovalAdmissionInput, ApprovalAdmissionContext) {
    let authority = ApprovalContextV1 {
        environment: "test".into(),
        custody_account_id: "custody-one".into(),
        audience: "account-authority:test".into(),
        policy_version: DecimalU64(1),
        security_version: DecimalU64(2),
        service_epoch: DecimalU64(3),
    };
    let source = ApprovalSourceVersions {
        manager_account_id: "manager-one".into(),
        manager_session_ref: "session-one".into(),
        origin_credential_id: "origin-one".into(),
        origin_credential_version: DecimalU64(9_007_199_254_740_993),
        passkey_credential_id: "passkey-one".into(),
        passkey_credential_version: DecimalU64(u64::MAX),
        manager_policy_version: DecimalU64(4),
        manager_security_version: DecimalU64(5),
        session_expires_at: DecimalU64(100_000),
        session_idle_expires_at: DecimalU64(90_000),
    };
    let request = ApprovalOptionsRequestV1 {
        approval_request_id: "approval-request-one".into(),
        context: authority.clone(),
        intent: CustodyIntentV1::ManagedLogin {
            native_account_id: "native-one".into(),
            login_key_ref: "login-one".into(),
            native_identity_namespace: "identity:test".into(),
            blocked_operation_ref: "blocked-one".into(),
            deadline: DecimalU64(100_000),
        },
    };
    let proof = CustodyApprovalProofV1 {
        version: 1,
        key_id: "native-one".into(),
        identity_namespace: "identity:test".into(),
        approval_id: "approval-one".into(),
        approval_request_id: request.approval_request_id.clone(),
        context: authority.clone(),
        intent_digest: request.intent_digest().unwrap(),
        manager_account_id: source.manager_account_id.clone(),
        manager_session_ref: source.manager_session_ref.clone(),
        origin_credential_id: source.origin_credential_id.clone(),
        origin_credential_version: source.origin_credential_version,
        passkey_credential_id: source.passkey_credential_id.clone(),
        passkey_credential_version: source.passkey_credential_version,
        manager_policy_version: source.manager_policy_version,
        manager_security_version: source.manager_security_version,
        identity_instance_epoch: DecimalU64(6),
        nonce: "native-proof-nonce".into(),
        issued_at: DecimalU64(1000),
        expires_at: DecimalU64(31000),
    };
    let context = ApprovalAdmissionContext {
        identity_namespace: "identity:test".into(),
        identity_instance_epoch: DecimalU64(6),
        authority,
        allowed_managers: [source.manager_account_id.clone()].into(),
        source,
        native_key_pins: [(
            "native-one".into(),
            key()
                .verifying_key()
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        )]
        .into(),
        fence: RecoveryFence::new(3, false),
        frozen: false,
        handoff_until: DecimalU64(40_000),
    };
    (
        ApprovalAdmissionInput {
            operation_id: "operation-one".into(),
            request,
            approval: sign_proof(proof),
        },
        context,
    )
}
fn verification(
    record: &ApprovalAdmissionRecord,
    context: &ApprovalAdmissionContext,
) -> SignedApprovalVerificationV1 {
    let request = record.native_verify_request().unwrap();
    let proof = &request.approval.proof;
    let source = &context.source;
    sign_verification(ApprovalVerificationV1 {
        version: 1,
        key_id: "native-one".into(),
        identity_namespace: context.identity_namespace.clone(),
        purpose: request.purpose,
        proof_digest: request.proof_digest,
        approval_request_id: proof.approval_request_id.clone(),
        audience: request.expected_audience,
        intent_digest: request.expected_intent_digest,
        verify_nonce: request.verify_nonce,
        manager_security_version: source.manager_security_version,
        manager_policy_version: source.manager_policy_version,
        origin_credential_version: source.origin_credential_version,
        identity_instance_epoch: context.identity_instance_epoch,
        credential_version: source.passkey_credential_version,
        observed_at: DecimalU64(1100),
        proof_expires_at: proof.expires_at,
        source_session_expires_at: source.session_expires_at,
        source_session_idle_expires_at: source.session_idle_expires_at,
        proof_state: ProofStateV1::Active,
        origin_state: OriginStateV1::Active,
    })
}
fn pending() -> (
    ApprovalAdmissionBook,
    ApprovalAdmissionContext,
    ApprovalAdmissionRecord,
) {
    let (input, context) = fixture();
    let mut book = ApprovalAdmissionBook::default();
    let checks = book
        .begin_checks(&input, "owner-nonce", &context, 1000)
        .unwrap();
    assert_eq!(checks.checks().len(), 1);
    let record = book
        .begin(
            input,
            "owner-nonce".into(),
            &context,
            1000,
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    (book, context, record)
}
fn rehash_input(value: &mut Value) {
    let input: ApprovalAdmissionInput = serde_json::from_value(value["input"].clone()).unwrap();
    value["body_digest"] = bytes_digest(
        &oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.admission-body.v1", input))
            .unwrap(),
    )
    .into();
}

/// Developer-only interoperable vector. This seed is public test data, never authority configuration.
#[test]
fn export_developer_admission_fixture() {
    let Ok(path) = std::env::var("OASIS7_CUSTODY_ADMISSION_FIXTURE_OUTPUT") else {
        return;
    };
    let (book, context, record) = pending();
    let signed_verification = verification(&record, &context);
    let checks = book
        .commit_checks("operation-one", &signed_verification, &context, 1100)
        .unwrap();
    StrictCrypto.verify(&checks).unwrap();
    let vector = json!({
        "fixture_label": "PUBLIC TEST ONLY - not a production identity or manager authority",
        "public_test_signing_seed_hex": "29".repeat(32),
        "now": DecimalU64(1000),
        "verification_observed_at": DecimalU64(1100),
        "context": context,
        "input": record.input(),
        "source_fields": context.source,
        "sample_verify_request": record.native_verify_request().unwrap(),
        "sample_signed_verification": signed_verification,
        "sample_signature_checks": checks,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&vector).unwrap()).unwrap();
}

#[test]
fn durable_original_handoff_verifies_both_domains_and_never_refreshes() {
    let (book, context, record) = pending();
    assert_eq!(record.stage(), ApprovalAdmissionStage::Pending);
    assert_eq!(record.handoff_until(), 31000);
    let mut restored: ApprovalAdmissionBook =
        serde_json::from_str(&serde_json::to_string(&book).unwrap()).unwrap();
    let signed = verification(&record, &context);
    let checks = restored
        .commit_checks("operation-one", &signed, &context, 1200)
        .unwrap();
    assert_eq!(checks.checks().len(), 2);
    assert_ne!(
        checks.checks()[0].canonical_bytes(),
        checks.checks()[1].canonical_bytes()
    );
    let mut cache = ExactCache::verified(checks);
    let consumed = restored
        .commit("operation-one", signed.clone(), &context, 1200, &mut cache)
        .unwrap();
    assert_eq!(consumed.stage(), ApprovalAdmissionStage::Consumed);
    assert_eq!(consumed.handoff_until(), 31000);
    let repeat = restored
        .commit("operation-one", signed, &context, 100_000, &mut cache)
        .unwrap();
    assert_eq!(repeat, consumed); // Historical receipt, not renewed authority.
    let replay = restored
        .begin(
            record.input().clone(),
            "discard-new-random-nonce".into(),
            &context,
            100_000,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(replay, consumed);
    assert_eq!(replay.verify_nonce(), "owner-nonce");
    let json = serde_json::to_string(&restored).unwrap();
    assert!(json.contains("\"9007199254740993\""));
    assert!(json.contains("\"18446744073709551615\""));
    assert_eq!(
        serde_json::from_str::<ApprovalAdmissionBook>(&json).unwrap(),
        restored
    );
}

#[test]
fn cached_crypto_is_bound_to_full_body_context_nonce_and_stage() {
    let (input, context) = fixture();
    let mut book = ApprovalAdmissionBook::default();
    let proof_checks = book
        .begin_checks(&input, "losing-concurrent-nonce", &context, 1000)
        .unwrap();
    let mut stale = ExactCache::verified(proof_checks);
    let record = book
        .begin(
            input.clone(),
            "winner-nonce".into(),
            &context,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(
        book.begin(
            input.clone(),
            "losing-concurrent-nonce".into(),
            &context,
            1001,
            &mut stale
        ),
        Err(ApprovalAdmissionError::SignatureRejected)
    );
    assert_eq!(book.original("operation-one").unwrap(), &record);
    let signed = verification(&record, &context);
    assert_eq!(
        book.commit("operation-one", signed.clone(), &context, 1200, &mut stale),
        Err(ApprovalAdmissionError::SignatureRejected)
    );
    let mut cache = ExactCache::verified(
        book.commit_checks("operation-one", &signed, &context, 1200)
            .unwrap(),
    );
    let mut tightened = context.clone();
    tightened.handoff_until = DecimalU64(25_000);
    let before = book.clone();
    assert_eq!(
        book.commit(
            "operation-one",
            signed.clone(),
            &tightened,
            1200,
            &mut cache
        ),
        Err(ApprovalAdmissionError::SignatureRejected)
    );
    assert_eq!(before, book);
    let checks = book
        .commit_checks("operation-one", &signed, &tightened, 1200)
        .unwrap();
    let consumed = book
        .commit(
            "operation-one",
            signed,
            &tightened,
            1200,
            &mut ExactCache::verified(checks),
        )
        .unwrap();
    assert_eq!(consumed.handoff_until(), 25_000);
    let mut extended = tightened;
    extended.handoff_until = DecimalU64(30_000);
    assert!(
        book.begin_checks(&input, "ignored", &extended, 1300)
            .is_err()
    );
}

#[test]
fn signed_verification_must_bind_every_current_source_and_request_field() {
    let (book, context, record) = pending();
    let valid = verification(&record, &context);
    let fields = [
        ("purpose", json!("inspect_origin")),
        ("proof_digest", json!("other")),
        ("approval_request_id", json!("other")),
        ("audience", json!("other")),
        ("intent_digest", json!("other")),
        ("verify_nonce", json!("other")),
        ("identity_namespace", json!("other")),
        ("identity_instance_epoch", json!("7")),
        ("manager_security_version", json!("6")),
        ("manager_policy_version", json!("5")),
        ("origin_credential_version", json!("1")),
        ("credential_version", json!("1")),
        ("proof_expires_at", json!("32000")),
        ("source_session_expires_at", json!("100001")),
        ("source_session_idle_expires_at", json!("90001")),
        ("proof_state", json!("expired")),
        ("origin_state", json!("revoked")),
        ("origin_state", json!("unavailable")),
        ("observed_at", json!("999")),
        ("observed_at", json!("1201")),
    ];
    for (field, value) in fields {
        let mut value_json = serde_json::to_value(&valid.verification).unwrap();
        value_json[field] = value;
        let invalid = sign_verification(serde_json::from_value(value_json).unwrap()); // Valid crypto, wrong correlation.
        let mut candidate = book.clone();
        assert!(
            candidate
                .commit("operation-one", invalid, &context, 1200, &mut StrictCrypto)
                .is_err(),
            "{field}"
        );
        assert_eq!(candidate, book, "{field}");
    }
    assert_eq!(
        record.native_verify_request().unwrap().purpose,
        VerificationPurposeV1::ConsumeApproval
    );
    assert!(
        book.commit_checks("missing", &valid, &context, 1200)
            .is_err()
    );
}

#[test]
fn cross_account_manager_versions_epoch_frozen_and_unknown_pins_reject() {
    let (input, context) = fixture();
    let mut contexts = Vec::new();
    let mut changed = context.clone();
    changed.authority.custody_account_id = "other".into();
    contexts.push(changed);
    let mut changed = context.clone();
    changed.authority.environment = "other".into();
    contexts.push(changed);
    let mut changed = context.clone();
    changed.authority.audience = "other".into();
    contexts.push(changed);
    let mut changed = context.clone();
    changed.authority.policy_version = DecimalU64(2);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.authority.security_version = DecimalU64(3);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.authority.service_epoch = DecimalU64(4);
    changed.fence = RecoveryFence::new(4, false);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.allowed_managers.clear();
    contexts.push(changed);
    let mut changed = context.clone();
    changed.identity_namespace = "other".into();
    contexts.push(changed);
    let mut changed = context.clone();
    changed.identity_instance_epoch = DecimalU64(7);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.source.manager_session_ref = "other".into();
    contexts.push(changed);
    let mut changed = context.clone();
    changed.source.passkey_credential_version = DecimalU64(1);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.source.origin_credential_version = DecimalU64(1);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.source.manager_security_version = DecimalU64(6);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.source.session_idle_expires_at = DecimalU64(1000);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.frozen = true;
    contexts.push(changed);
    let mut changed = context.clone();
    changed.fence = RecoveryFence::new(3, true);
    contexts.push(changed);
    let mut changed = context.clone();
    changed.native_key_pins.clear();
    contexts.push(changed);
    for context in contexts {
        let mut book = ApprovalAdmissionBook::default();
        assert!(
            book.begin(
                input.clone(),
                "nonce".into(),
                &context,
                1000,
                &mut StrictCrypto
            )
            .is_err()
        );
        assert_eq!(book, ApprovalAdmissionBook::default());
    }
    let mut forged = input;
    forged.approval.signature = "invalid".into();
    assert!(
        ApprovalAdmissionBook::default()
            .begin(forged, "nonce".into(), &context, 1000, &mut StrictCrypto)
            .is_err()
    );
}

#[test]
fn proof_ttl_and_final_clock_rechecks_are_independent_of_alarm_or_crypto_cache() {
    let (input, context) = fixture();
    for (issued, expires, now) in [
        (1000, 31001, 1000),
        (1001, 31000, 1000),
        (1000, 31000, 31000),
        (1000, 1000, 1000),
    ] {
        let mut invalid = input.clone();
        invalid.approval.proof.issued_at = DecimalU64(issued);
        invalid.approval.proof.expires_at = DecimalU64(expires);
        invalid.approval = sign_proof(invalid.approval.proof);
        assert!(
            ApprovalAdmissionBook::default()
                .begin(invalid, "nonce".into(), &context, now, &mut StrictCrypto)
                .is_err()
        );
    }
    let (mut book, context, record) = pending();
    let signed = verification(&record, &context);
    let mut cache = ExactCache::verified(
        book.commit_checks("operation-one", &signed, &context, 1200)
            .unwrap(),
    );
    let before = book.clone();
    assert_eq!(
        book.commit("operation-one", signed.clone(), &context, 31000, &mut cache),
        Err(ApprovalAdmissionError::Expired)
    );
    assert_eq!(book, before);
    let mut frozen = context.clone();
    frozen.frozen = true;
    assert_eq!(
        book.commit("operation-one", signed, &frozen, 1200, &mut cache),
        Err(ApprovalAdmissionError::Frozen)
    );
    assert_eq!(book, before);
}

#[test]
fn nonce_tombstones_and_operation_body_survive_restart() {
    let (mut book, context, record) = pending();
    let before = book.clone();
    let mut other = record.input().clone();
    other.operation_id = "operation-two".into();
    assert_eq!(
        book.begin(
            other,
            "other-nonce".into(),
            &context,
            1100,
            &mut StrictCrypto
        ),
        Err(ApprovalAdmissionError::NonceReused)
    );
    assert_eq!(book, before);
    let mut changed = record.input().clone();
    changed.request.approval_request_id = "different".into();
    assert_eq!(
        book.begin(
            changed,
            "other-nonce".into(),
            &context,
            1100,
            &mut StrictCrypto
        ),
        Err(ApprovalAdmissionError::Conflict)
    );
    let mut raw = serde_json::to_value(&book).unwrap();
    let mut duplicate = raw["records"]["operation-one"].clone();
    duplicate["input"]["operation_id"] = "operation-two".into();
    rehash_input(&mut duplicate);
    raw["records"]["operation-two"] = duplicate;
    assert!(serde_json::from_value::<ApprovalAdmissionBook>(raw).is_err());
}

#[test]
fn restore_never_substitutes_for_crypto_and_rejects_inconsistent_state() {
    let (book, context, record) = pending();
    let original = serde_json::to_value(&book).unwrap();
    for (field, value) in [
        ("handoff_until", json!("31001")),
        ("body_digest", json!("forged")),
        ("consumed_at", json!("1200")),
    ] {
        let mut raw = original.clone();
        raw["records"]["operation-one"][field] = value;
        assert!(
            serde_json::from_value::<ApprovalAdmissionBook>(raw).is_err(),
            "{field}"
        );
    }
    let mut raw = original;
    let persisted = &mut raw["records"]["operation-one"];
    persisted["input"]["approval"]["signature"] = "invalid-but-not-validated-by-serde".into();
    rehash_input(persisted);
    let mut restored: ApprovalAdmissionBook = serde_json::from_value(raw).unwrap();
    let forged = restored.original("operation-one").unwrap();
    let signed = verification(forged, &context);
    assert!(
        restored
            .commit("operation-one", signed, &context, 1200, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(
        restored.original("operation-one").unwrap().stage(),
        ApprovalAdmissionStage::Pending
    );
    assert_eq!(record.stage(), ApprovalAdmissionStage::Pending);
}

#[test]
fn capacity_never_deletes_nonce_evidence_and_original_retries_still_work() {
    let (input, context) = fixture();
    let mut book = ApprovalAdmissionBook::default();
    for index in 0..MAX_APPROVAL_ADMISSION_RECORDS {
        let mut next = input.clone();
        next.operation_id = format!("operation-{index}");
        next.approval.proof.nonce = format!("proof-{index}");
        next.approval = sign_proof(next.approval.proof);
        book.begin(
            next,
            format!("verify-{index}"),
            &context,
            1000,
            &mut StrictCrypto,
        )
        .unwrap();
    }
    let mut next = input.clone();
    next.operation_id = "overflow".into();
    assert_eq!(
        book.begin_checks(&next, "nonce", &context, 1000),
        Err(ApprovalAdmissionError::Capacity)
    );
    let original = book.original("operation-0").unwrap().clone();
    assert_eq!(
        book.begin(
            original.input().clone(),
            "discard".into(),
            &context,
            100_000,
            &mut StrictCrypto
        )
        .unwrap(),
        original
    );
    let json = serde_json::to_string(&book).unwrap();
    assert_eq!(
        serde_json::from_str::<ApprovalAdmissionBook>(&json).unwrap(),
        book
    );
}
