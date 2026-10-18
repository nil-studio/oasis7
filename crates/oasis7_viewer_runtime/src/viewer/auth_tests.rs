use ed25519_dalek::SigningKey;
const HOSTED_REGISTRATION_GRANT_TTL_MS: u64 = 30_000;

use super::*;

fn test_signer_with_seed(seed: u8) -> (String, String) {
    let private_key = [seed; 32];
    let signing_key = SigningKey::from_bytes(&private_key);
    (
        hex::encode(signing_key.verifying_key().to_bytes()),
        hex::encode(private_key),
    )
}

#[test]
fn hosted_session_register_grant_is_bound_and_single_use_across_ledger_reload() {
    let (issuer_public_key, issuer_private_key) = test_signer_with_seed(51);
    let (public_key, private_key) = test_signer_with_seed(52);
    let ledger_path = std::env::temp_dir().join(format!(
        "oasis7-registration-replay-{}-{}.json",
        std::process::id(),
        now_unix_ms()
    ));
    unsafe {
        std::env::set_var(
            HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV,
            &issuer_public_key,
        );
        std::env::set_var(HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV, &ledger_path);
    }
    let grant = issue_hosted_registration_grant(
        "hosted-player-test-login",
        public_key.as_str(),
        "device-1",
        "nonce-1",
        now_unix_ms(),
        issuer_private_key.as_str(),
    )
    .expect("issue registration grant");
    let request = AuthoritativeSessionRegisterRequest {
        player_id: "hosted-player-test-login".to_string(),
        public_key: Some(public_key.clone()),
        registration_grant: Some(grant),
        auth: None,
        requested_agent_id: None,
        force_rebind: false,
    };
    let proof =
        sign_session_register_auth_proof(&request, 91, public_key.as_str(), private_key.as_str())
            .expect("sign session request");

    let mut mismatched_request = request.clone();
    mismatched_request.player_id = "hosted-player-other".to_string();
    let mismatched_proof = sign_session_register_auth_proof(
        &mismatched_request,
        92,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign mismatched session request");
    let mismatch = verify_session_register_auth_proof(&mismatched_request, &mismatched_proof)
        .expect_err("grant binding mismatch rejected");
    assert!(mismatch.contains("binding"), "unexpected error: {mismatch}");

    let expired_grant = issue_hosted_registration_grant(
        "hosted-player-test-login",
        public_key.as_str(),
        "device-1",
        "nonce-expired",
        now_unix_ms()
            .saturating_sub(HOSTED_REGISTRATION_GRANT_TTL_MS)
            .saturating_sub(1),
        issuer_private_key.as_str(),
    )
    .expect("issue expired registration grant");
    let mut expired_request = request.clone();
    expired_request.registration_grant = Some(expired_grant);
    let expired_proof = sign_session_register_auth_proof(
        &expired_request,
        93,
        public_key.as_str(),
        private_key.as_str(),
    )
    .expect("sign expired session request");
    let expired = verify_session_register_auth_proof(&expired_request, &expired_proof)
        .expect_err("expired grant rejected");
    assert!(expired.contains("expired"), "unexpected error: {expired}");

    let verified =
        verify_session_register_auth_proof(&request, &proof).expect("first use accepted");
    consume_registration_grant_nonce(
        verified
            .hosted_registration_nonce
            .as_deref()
            .expect("hosted nonce"),
    )
    .expect("consume grant after registration commit");
    let replay = verify_session_register_auth_proof(&request, &proof).expect_err("replay rejected");
    assert!(replay.contains("replay"), "unexpected error: {replay}");

    std::fs::write(&ledger_path, b"not-json").expect("write corrupt replay ledger");
    let corrupt = preflight_hosted_registration_replay_ledger()
        .expect_err("corrupt replay ledger must fail preflight");
    assert!(corrupt.contains("decode"), "unexpected error: {corrupt}");
    std::fs::remove_file(&ledger_path).expect("remove corrupt replay ledger");
    preflight_hosted_registration_replay_ledger().expect("writable replay ledger preflight");
    assert!(ledger_path.is_file());
    let _ = std::fs::remove_file(ledger_path);
    unsafe {
        std::env::remove_var(HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV);
        std::env::remove_var(HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV);
    }
}
