use super::super::{model_session::model_session_tests, now_ms};
use super::*;
use openssl::{
    bn::{BigNum, BigNumContext},
    ec::{EcGroup, EcKey},
    hash::MessageDigest,
    nid::Nid,
    pkey::{PKey, Private},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use webauthn_rs_core::proto::*;

pub(super) fn fixture() -> (model_session_tests::Fixture, PKey<Private>) {
    let mut fixture = model_session_tests::fixture();
    fixture.service.custody = Some(CustodyConfig {
        revocation: Some(revocation::RevocationConfig::http_fixture(
            "account-authority:test".into(),
        )),
        key: SigningKey::from_bytes(&[51; 32]),
        key_id: "approval-test-1".into(),
        audience: "account-authority:test".into(),
        adapter_secret: [52; 32],
    });
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
    let ec = EcKey::generate(&group).unwrap();
    let mut x = BigNum::new().unwrap();
    let mut y = BigNum::new().unwrap();
    ec.public_key()
        .affine_coordinates_gfp(&group, &mut x, &mut y, &mut BigNumContext::new().unwrap())
        .unwrap();
    let credential = Credential::from(CredentialV3 {
        cred_id: vec![1, 2, 3],
        counter: 0,
        verified: true,
        registration_policy: UserVerificationPolicy::Required,
        cred: COSEKey {
            type_: COSEAlgorithm::ES256,
            key: COSEKeyType::EC_EC2(COSEEC2Key {
                curve: ECDSACurve::SECP256R1,
                x: x.to_vec_padded(32).unwrap().into(),
                y: y.to_vec_padded(32).unwrap().into(),
            }),
        },
    });
    fixture
        .service
        .store
        .transaction(|tx| {
            let mut c = tx.get::<CredentialRecord>("credential:p")?.unwrap();
            if let CredentialMaterial::WebAuthn {
                library_record,
                credential_id,
                ..
            } = &mut c.value.public_material
            {
                *library_record = serde_json::to_value(&credential).unwrap();
                *credential_id = vec![1, 2, 3];
            }
            tx.replace(
                "credential:p",
                c.row_version,
                "credential",
                Some("a"),
                None,
                &c.value,
            )?;
            tx.create(
                "lookup:passkey:AQID",
                "lookup",
                Some("a"),
                None,
                &CredentialLookup {
                    scope: "passkey".into(),
                    key_digest: "AQID".into(),
                    canonical_key: vec![1, 2, 3],
                    credential_record_id: "p".into(),
                    account_id: "a".into(),
                    state: "active".into(),
                },
            )
        })
        .unwrap();
    fixture
        .service
        .custody
        .as_ref()
        .unwrap()
        .attach_revocation(
            &mut fixture.service.store,
            fixture.service.config.namespace.key(),
            fixture.service.config.namespace.environment_id.clone(),
            fixture.service.config.rp.identity_instance_epoch,
        )
        .unwrap();
    (fixture, PKey::from_ec_key(ec).unwrap())
}
fn options(now: u64) -> Value {
    serde_json::to_value(ApprovalOptionsRequestV1 {
        approval_request_id: "stable-request".into(),
        context: ApprovalContextV1 {
            environment: "test".into(),
            custody_account_id: "custody-a".into(),
            audience: "account-authority:test".into(),
            policy_version: DecimalU64(1),
            security_version: DecimalU64(1),
            service_epoch: DecimalU64(1),
        },
        intent: CustodyIntentV1::ManagedLogin {
            native_account_id: "native-a".into(),
            login_key_ref: "login-key-1".into(),
            native_identity_namespace: "native-test".into(),
            blocked_operation_ref: "blocked-one".into(),
            deadline: DecimalU64(now + 180_000),
        },
    })
    .unwrap()
}
fn assertion(options: &Value, key: &PKey<Private>, flags: u8) -> Value {
    let client = serde_json::to_vec(
        &json!({"type":"webauthn.get","challenge":options["options"]["publicKey"]["challenge"],
        "origin":"https://localhost","crossOrigin":false}),
    )
    .unwrap();
    let mut auth = Sha256::digest(b"localhost").to_vec();
    auth.push(flags);
    auth.extend_from_slice(&1_u32.to_be_bytes());
    let mut bytes = auth.clone();
    bytes.extend_from_slice(&Sha256::digest(&client));
    let mut signer = openssl::sign::Signer::new(MessageDigest::sha256(), key).unwrap();
    signer.update(&bytes).unwrap();
    json!({"id":"AQID","rawId":"AQID","type":"public-key","response":{
        "clientDataJSON":URL_SAFE_NO_PAD.encode(client),"authenticatorData":URL_SAFE_NO_PAD.encode(auth),
        "signature":URL_SAFE_NO_PAD.encode(signer.sign_to_vec().unwrap()),"userHandle":URL_SAFE_NO_PAD.encode([0;16])},
        "clientExtensionResults":{}})
}
fn complete_payload(start: &Value, key: &PKey<Private>, flags: u8) -> Value {
    json!({"approval_id":start["approval_id"],"ceremony_id":start["ceremony_id"],"operation_id":start["operation_id"],
        "intent_digest":start["intent_digest"],"response":assertion(start,key,flags)})
}
fn verify_request(complete: &Value) -> ApprovalVerifyRequestV1 {
    let approval: SignedCustodyApprovalV1 =
        serde_json::from_value(complete["approval"].clone()).unwrap();
    ApprovalVerifyRequestV1 {
        proof_digest: approval.digest().unwrap(),
        expected_audience: approval.proof.context.audience.clone(),
        expected_intent_digest: approval.proof.intent_digest.clone(),
        approval,
        purpose: VerificationPurposeV1::ConsumeApproval,
        verify_nonce: "owner-generated-nonce".into(),
    }
}

#[test]
fn real_passkey_completion_is_durable_idempotent_and_proof_expires_independently() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let start = f
        .service
        .custody_options(Some(&f.token), &options(now), "source", now)
        .unwrap();
    let retry = f
        .service
        .custody_options(Some(&f.token), &options(now), "source", now + 1)
        .unwrap();
    assert_eq!(start, retry);
    assert_eq!(
        start["options"]["publicKey"]["userVerification"],
        "required"
    );
    let payload = complete_payload(&start, &key, 0x05);
    let result = f
        .service
        .custody_complete(Some(&f.token), &payload, "source", now + 2)
        .unwrap();
    let config = &f.service.config;
    f.service.store = super::super::store::IdentityStore::open(
        &config.sqlite_path,
        config.namespace.clone(),
        config.rp.clone(),
    )
    .unwrap();
    assert_eq!(
        f.service
            .custody_complete(Some(&f.token), &payload, "source", now + 3)
            .unwrap(),
        result
    );
    let request = verify_request(&result);
    let verification = f.service.verify_approval(&request, now + 4).unwrap();
    let signed: SignedApprovalVerificationV1 =
        serde_json::from_value(verification.clone()).unwrap();
    let signature =
        Signature::from_slice(&URL_SAFE_NO_PAD.decode(&signed.signature).unwrap()).unwrap();
    f.service
        .custody
        .as_ref()
        .unwrap()
        .key
        .verifying_key()
        .verify_strict(&signed.verification.signing_bytes().unwrap(), &signature)
        .unwrap();
    let mut changed_purpose = signed.verification.clone();
    changed_purpose.purpose = VerificationPurposeV1::InspectOrigin;
    assert!(
        f.service
            .custody
            .as_ref()
            .unwrap()
            .key
            .verifying_key()
            .verify_strict(&changed_purpose.signing_bytes().unwrap(), &signature)
            .is_err()
    );
    assert_eq!(verification["verification"]["origin_state"], "active");
    assert_eq!(
        verification["verification"]["verify_nonce"],
        "owner-generated-nonce"
    );
    let mut wrong_pin = request.clone();
    wrong_pin.approval.proof.key_id = "unknown-rotation-key".into();
    wrong_pin.proof_digest = wrong_pin.approval.digest().unwrap();
    assert!(f.service.verify_approval(&wrong_pin, now + 4).is_err());
    let mut wrong_body = request.clone();
    wrong_body.expected_intent_digest = "unrelated-intent".into();
    assert!(f.service.verify_approval(&wrong_body, now + 4).is_err());
    assert!(f.service.verify_approval(&request, now + 30_002).is_err());
    let mut inspect = request;
    inspect.purpose = VerificationPurposeV1::InspectOrigin;
    let verification = f.service.verify_approval(&inspect, now + 30_002).unwrap();
    assert_eq!(verification["verification"]["proof_state"], "expired");
    assert_eq!(verification["verification"]["origin_state"], "active");
    assert!(
        f.service
            .custody_complete(Some(&f.token), &payload, "source", now + 30_002)
            .is_err()
    );
    let mut changed = payload;
    changed["response"]["extra"] = true.into();
    assert!(
        f.service
            .custody_complete(Some(&f.token), &changed, "source", now + 4)
            .is_err()
    );
}

#[test]
fn native_keys_bad_uv_and_revocation_cannot_approve_or_recover_authority() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let start = f
        .service
        .custody_options(Some(&f.token), &options(now), "source", now)
        .unwrap();
    let bad = complete_payload(&start, &key, 0x01);
    assert!(
        f.service
            .custody_complete(Some(&f.token), &bad, "source", now + 1)
            .is_err()
    );
    assert!(
        f.service
            .custody_complete(
                Some(&f.token),
                &complete_payload(&start, &key, 0x05),
                "source",
                now + 2
            )
            .is_err()
    );
    let mut fresh = options(now);
    fresh["approval_request_id"] = "explicit-fresh-approval".into();
    let start = f
        .service
        .custody_options(Some(&f.token), &fresh, "source", now + 2)
        .unwrap();
    let result = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&start, &key, 0x05),
            "source",
            now + 2,
        )
        .unwrap();
    let mut request = verify_request(&result);
    f.service
        .store
        .transaction(|tx| {
            let mut c = tx.get::<CredentialRecord>("credential:p")?.unwrap();
            c.value.version += 1;
            c.value.state = "revoked".into();
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
    assert!(f.service.verify_approval(&request, now + 3).is_err());
    request.purpose = VerificationPurposeV1::InspectOrigin;
    assert_eq!(
        f.service.verify_approval(&request, now + 3).unwrap()["verification"]["origin_state"],
        "revoked"
    );

    let (mut f, _) = fixture();
    let session = session_key(&f.service.config.session_key, &f.token);
    f.service
        .store
        .transaction(|tx| {
            let mut s = tx.get::<AccountSession>(&session)?.unwrap();
            s.value.auth_method = "native_key".into();
            tx.replace(
                &session,
                s.row_version,
                "session",
                Some("a"),
                Some(s.value.expires_at),
                &s.value,
            )
        })
        .unwrap();
    assert!(
        f.service
            .custody_options(Some(&f.token), &options(now), "source", now)
            .is_err()
    );
}

#[test]
fn service_credential_is_exclusive_and_session_metadata_uses_current_authority() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let start = f
        .service
        .custody_options(Some(&f.token), &options(now), "source", now)
        .unwrap();
    let complete = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&start, &key, 0x05),
            "source",
            now + 1,
        )
        .unwrap();
    let payload = serde_json::to_value(verify_request(&complete)).unwrap();
    let headers = format!(
        "POST / HTTP/1.1\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\n",
        hex::encode([52; 32])
    );
    assert!(
        f.service
            .custody_verify_route("POST", &headers, &payload, "source", now + 2)
            .is_ok()
    );
    for invalid in [
        headers.replace(&hex::encode([52; 32]), &hex::encode([53; 32])),
        format!("{headers}Origin: https://localhost\r\n"),
        format!("{headers}Cookie: __Host-oasis7_sid={}\r\n", f.token),
    ] {
        assert!(
            f.service
                .custody_verify_route("POST", &invalid, &payload, "source", now + 2)
                .is_err()
        );
    }
    let status = f
        .service
        .custody_browser_route(
            "GET",
            "/custody/session-status",
            Some(&f.token),
            &json!({}),
            "source",
            now + 2,
        )
        .unwrap();
    assert_eq!(status["account_id"], "a");
    assert!(status["expires_at"].is_string());
    let session = session_key(&f.service.config.session_key, &f.token);
    f.service
        .store
        .transaction(|tx| {
            let mut saved = tx.get::<AccountSession>(&session)?.unwrap();
            saved.value.expires_at = now + 10_000;
            saved.value.idle_expires_at = now + 8_000;
            tx.replace(
                &session,
                saved.row_version,
                "session",
                Some("a"),
                Some(saved.value.expires_at),
                &saved.value,
            )
        })
        .unwrap();
    let status = f
        .service
        .custody_browser_route(
            "GET",
            "/custody/session-status",
            Some(&f.token),
            &json!({}),
            "source",
            now + 3,
        )
        .unwrap();
    assert_eq!(status["expires_at"], (now + 10_000).to_string());
    assert_eq!(status["idle_expires_at"], (now + 8_000).to_string());
    assert!(f.service.validate_json_write(&headers, "csrf").is_err());
    f.service.custody = None;
    assert!(
        f.service
            .custody_browser_route(
                "GET",
                "/custody/session-status",
                Some(&f.token),
                &json!({}),
                "source",
                now + 3
            )
            .is_err()
    );
}

#[test]
fn pending_retry_cannot_change_body_or_refresh_expired_ceremony() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let original = options(now);
    let start = f
        .service
        .custody_options(Some(&f.token), &original, "source", now)
        .unwrap();
    let mut changed = original.clone();
    changed["intent"]["login_key_ref"] = "replacement-key".into();
    assert!(
        f.service
            .custody_options(Some(&f.token), &changed, "source", now + 1)
            .is_err()
    );
    let expired = f
        .service
        .custody_options(Some(&f.token), &original, "source", now + 120_000)
        .unwrap();
    assert_eq!(expired["phase"], "expired");
    assert_eq!(expired["ceremony_id"], start["ceremony_id"]);
    assert_eq!(expired["ceremony_expires_at"], start["ceremony_expires_at"]);
    assert!(
        f.service
            .custody_complete(
                Some(&f.token),
                &complete_payload(&start, &key, 0x05),
                "source",
                now + 120_000
            )
            .is_err()
    );
}

#[path = "custody_native_http_fixture.rs"]
mod native_http_fixture;

#[test]
fn management_status_requires_passkey_and_post_keeps_native_origin_csrf_contract() {
    let (mut f, _) = fixture();
    let now = now_ms();
    let path = format!(
        "{}/custody/management-session-status",
        crate::hosted_identity::service::PREFIX
    );
    let mut issuer = crate::hosted_player_session::HostedPlayerSessionIssuer::default();
    let cookie = format!("Cookie: __Host-oasis7_sid={}\r\n", f.token);
    let csrf = digest(&f.service.config.session_key, "csrf", &f.token);
    let good = format!(
        "POST / HTTP/1.1\r\n{cookie}Origin: https://localhost\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\n\r\n"
    );
    assert_eq!(
        f.service
            .handle("POST", &path, &good, json!({}), "fixture", &mut issuer)
            .status,
        200
    );
    for bad in [
        good.replace("https://localhost", "https://other.test"),
        good.replace(&csrf, "wrong"),
        format!("POST / HTTP/1.1\r\n{cookie}Content-Type: application/json\r\n\r\n"),
    ] {
        assert_ne!(
            f.service
                .handle("POST", &path, &bad, json!({}), "fixture", &mut issuer)
                .status,
            200
        );
    }
    assert!(
        f.service
            .custody_browser_route(
                "GET",
                "/custody/management-session-status",
                Some(&f.token),
                &json!({"account_id":"other"}),
                "fixture",
                now
            )
            .is_err()
    );
    let session = session_key(&f.service.config.session_key, &f.token);
    f.service
        .store
        .transaction(|tx| {
            let mut saved = tx.get::<AccountSession>(&session)?.unwrap();
            saved.value.auth_method = "native_key".into();
            tx.replace(
                &session,
                saved.row_version,
                "session",
                Some("a"),
                Some(saved.value.expires_at),
                &saved.value,
            )
        })
        .unwrap();
    assert!(
        f.service
            .custody_browser_route(
                "GET",
                "/custody/management-session-status",
                Some(&f.token),
                &json!({}),
                "fixture",
                now
            )
            .is_err()
    );
    assert!(
        f.service
            .custody_browser_route(
                "GET",
                "/custody/session-status",
                Some(&f.token),
                &json!({}),
                "fixture",
                now
            )
            .is_ok()
    );
}
