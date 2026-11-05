//! Native protocol fixture only; it signs a real UV assertion with the registered
//! passkey fixture, never forges an Owner signature or installs Account authority.
use super::*;
fn term_request(now: u64) -> Value {
    let mut request: Value = serde_json::from_str(include_str!(
        "../../../../../oasis7_custody_core/tests/fixtures/termination_v3_approval_request.json"
    ))
    .unwrap();
    request["intent"]["plan"]["cleanup_until"] = json!((now + 120_000).to_string());
    request
}
fn browser_headers(f: &model_session_tests::Fixture) -> String {
    let csrf = digest(&f.service.config.session_key, "csrf", &f.token);
    format!(
        "POST / HTTP/1.1\r\nCookie: __Host-oasis7_sid={}\r\nOrigin: https://localhost\r\nContent-Type: application/json\r\nX-Oasis7-CSRF: {csrf}\r\n\r\n",
        f.token
    )
}
fn adapter_headers(f: &model_session_tests::Fixture) -> String {
    let key = f.service.custody.as_ref().unwrap().adapter_secret;
    format!(
        "POST / HTTP/1.1\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\n\r\n",
        hex::encode(key)
    )
}
#[test]
fn termination_v3_production_options_reject_development_profile() {
    let (mut f, _) = fixture();
    let now = now_ms();
    assert!(
        f.service
            .custody_options(Some(&f.token), &term_request(now), "fixture", now)
            .is_err()
    );
}
#[test]
fn termination_v3_fixture_profile_uses_uv_summary_and_native_verification() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let request = term_request(now);
    let headers = browser_headers(&f);
    let start = f
        .service
        .custody_browser_termination_v3_fixture(
            "POST",
            "/custody/approvals/options",
            &headers,
            &request,
            "fixture",
            now,
        )
        .expect("explicit development profile must reach the real approval kernel");
    assert_eq!(
        start["options"]["publicKey"]["userVerification"],
        "required"
    );
    let summary = &start["termination_summary"];
    assert_eq!(summary["kind"], "termination_plan_v3");
    assert_eq!(summary["irreversible"], true);
    assert_eq!(
        summary["owner_key_ref"],
        request["intent"]["plan"]["owner_key_ref"]
    );
    assert_eq!(
        summary["executor_key_ref"],
        request["intent"]["plan"]["executor_key_ref"]
    );
    assert_eq!(
        summary["signer_public_key"],
        request["intent"]["plan"]["original_delegation"]["delegate_public_key"]
    );
    assert_eq!(summary["nonce_policy"], "account_allocated_high_water_v1");
    assert_eq!(summary["requires_all_allocated_unknowns_inventory"], true);
    assert_eq!(
        summary["effects"],
        json!([
            "guarded_stop_v2",
            "revoke_exact_signer",
            "permanent_world_closed_v3_and_detached"
        ])
    );
    assert_eq!(summary["world_effect_limit"], "2");
    assert!(summary.get("request_limit").is_none());
    assert_eq!(
        summary["native_subject"],
        request["intent"]["plan"]["native_subject"]
    );
    assert_eq!(summary["transport_limit"], "24");
    assert_eq!(summary["automatic_lookup_per_original"], 1);
    assert_eq!(summary["fresh_recovery_lookup_per_original"], 1);
    assert_eq!(summary["unknown_can_keep_session_open"], true);
    assert_eq!(summary["development_only"], true);
    assert_eq!(
        summary["owner_signing_recovery_policy"],
        "exact_original_ed25519_recovery_v1"
    );
    assert_eq!(
        summary["may_reopen_owner_seed_for_exact_recomputation"],
        true
    );
    assert_eq!(summary["max_recomputations_per_effect"], "1");
    let payload = complete_payload(&start, &key, 0x05);
    assert!(
        f.service
            .custody_complete(Some(&f.token), &payload, "fixture", now + 1)
            .is_err(),
        "production complete must also reject a dev-profile pending record"
    );
    let completed = f
        .service
        .custody_browser_termination_v3_fixture(
            "POST",
            "/custody/approvals/complete",
            &headers,
            &payload,
            "fixture",
            now + 2,
        )
        .unwrap();
    let verify = verify_request(&completed);
    let adapter = adapter_headers(&f);
    let value = serde_json::to_value(&verify).unwrap();
    assert!(
        f.service
            .custody_verify_route("POST", &adapter, &value, "fixture", now + 3)
            .is_err(),
        "production verification cannot admit a dev-profile proof"
    );
    let verified = f
        .service
        .custody_verify_termination_v3_fixture("POST", &adapter, &value, "fixture", now + 4)
        .unwrap();
    let signed: SignedApprovalVerificationV1 = serde_json::from_value(verified).unwrap();
    assert_eq!(
        signed.verification.intent_digest,
        start["intent_digest"].as_str().unwrap()
    );
    assert_eq!(
        signed.verification.purpose,
        VerificationPurposeV1::ConsumeApproval
    );
    // Actual Account installation/once-consume/subject-current/HWM remain separate consumers.
}
#[test]
fn termination_v3_fixture_keeps_csrf_origin_and_uv_checks() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let request = term_request(now);
    let headers = browser_headers(&f);
    assert!(
        f.service
            .custody_browser_termination_v3_fixture(
                "POST",
                "/custody/approvals/options",
                &headers.replace(
                    "Origin: https://localhost",
                    "Origin: https://outside.invalid"
                ),
                &request,
                "fixture",
                now
            )
            .is_err()
    );
    assert!(
        f.service
            .custody_browser_termination_v3_fixture(
                "POST",
                "/custody/approvals/options",
                &headers.replace("X-Oasis7-CSRF:", "X-Wrong-CSRF:"),
                &request,
                "fixture",
                now + 1
            )
            .is_err()
    );
    let start = f
        .service
        .custody_browser_termination_v3_fixture(
            "POST",
            "/custody/approvals/options",
            &headers,
            &request,
            "fixture",
            now + 2,
        )
        .unwrap();
    assert!(
        f.service
            .custody_browser_termination_v3_fixture(
                "POST",
                "/custody/approvals/complete",
                &headers,
                &complete_payload(&start, &key, 0x01),
                "fixture",
                now + 3
            )
            .is_err(),
        "UP without UV cannot approve irreversible close"
    );
}
#[test]
fn termination_v3_cannot_repurpose_an_old_native_proof() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let start = f
        .service
        .custody_options(Some(&f.token), &options(now), "fixture", now)
        .unwrap();
    let old = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&start, &key, 0x05),
            "fixture",
            now + 1,
        )
        .unwrap();
    let mut verify = verify_request(&old);
    let new: ApprovalOptionsRequestV1 = serde_json::from_value(term_request(now)).unwrap();
    verify.expected_intent_digest = new.intent_digest().unwrap();
    let headers = adapter_headers(&f);
    let payload = serde_json::to_value(verify).unwrap();
    assert!(
        f.service
            .custody_verify_termination_v3_fixture("POST", &headers, &payload, "fixture", now + 2)
            .is_err()
    );
}

// Append inside custody_termination_v3_tests.rs (which is custody::tests child).
#[test]
fn termination_actual_managed_subject_and_uv_approval() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let sid = super::super::managed_binding::tests::termination_managed_machine_fixture(&mut f, now);
    let status: oasis7_custody_core::native_binding::SignedManagedNativeSessionStatusV1 = serde_json::from_value(
        f.service.managed_session_status(Some(&sid), &json!({"nonce":"44".repeat(32)}), now + 7).unwrap()
    ).unwrap();
    let subject = oasis7_custody_core::provisioning::NativeSubjectRefV3::from_status(&status.status).unwrap();
    let mut value = term_request(now + 8);
    value["intent"]["plan"]["native_subject"] = serde_json::to_value(&subject).unwrap();
    value["intent"]["plan"]["custody_account_id"] = json!(status.status.receipt.custody_account_id);
    value["intent"]["plan"]["provision_id"] = json!(status.status.receipt.provision_id);
    value["context"]["custody_account_id"] = json!(status.status.receipt.custody_account_id);
    value["context"]["environment"] = json!(status.status.receipt.environment);
    let headers = browser_headers(&f);
    let start = f.service.custody_browser_termination_v3_fixture("POST", "/custody/approvals/options", &headers, &value, "fixture", now + 8).unwrap();
    let completed = f.service.custody_browser_termination_v3_fixture("POST", "/custody/approvals/complete", &headers, &complete_payload(&start, &key, 0x05), "fixture", now + 9).unwrap();
    let verify = verify_request(&completed);
    let request: ApprovalOptionsRequestV1 = serde_json::from_value(value).unwrap();
    let approval_id = &verify.approval.proof.approval_id;
    use super::super::termination_v3::TerminationApprovalProfile::DevelopmentOnlyFixture;
    let _observation = f.service.require_managed_termination_subject_v3(Some(&sid), approval_id, DevelopmentOnlyFixture, &request, now + 10).unwrap();
    // Use real wall-clock expiry: ClockGuard deliberately rejects fabricated
    // jumps beyond 5 seconds from startup UTC + monotonic elapsed.
    let expires_at = verify.approval.proof.expires_at.0;
    let remaining = expires_at.saturating_add(1).saturating_sub(now_ms());
    std::thread::sleep(std::time::Duration::from_millis(remaining));
    let err = f.service.require_managed_termination_subject_v3(Some(&sid), approval_id, DevelopmentOnlyFixture, &request, now_ms()).err().unwrap();
    assert!(format!("{err:?}").contains("approval_expired_or_revoked"));
    // Destroy the service/store and reopen the same SQLite DB. No observation
    // witness is passed to the new service; durable completed history is required.
    f = f.cold_reopen();
    assert!(f.service.inspect_completed_managed_subject_v3(Some(&sid), approval_id, DevelopmentOnlyFixture, &request, now_ms()).is_ok());
    assert!(f.service.require_managed_termination_subject_v3(Some(&f.token), approval_id, DevelopmentOnlyFixture, &request, now_ms()).is_err());
    // Calls the validator directly past HTTP CSRF: this must fail actual Cookie session lookup.
    assert_ne!(sid, subject.native_session_ref);
    let err = f.service.require_managed_termination_subject_v3(Some(&subject.native_session_ref), approval_id, DevelopmentOnlyFixture, &request, now_ms()).err().unwrap();
    assert!(format!("{err:?}").contains("session_expired"));
    // Last check: time outside the supported 5-second clock skew returns no witness.
    let err = f.service.inspect_completed_managed_subject_v3(Some(&sid), approval_id, DevelopmentOnlyFixture, &request, now_ms().saturating_sub(5_001)).err().unwrap();
    assert!(format!("{err:?}").contains("clock_unhealthy"));

}
