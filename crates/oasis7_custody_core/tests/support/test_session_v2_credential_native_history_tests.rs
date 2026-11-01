use super::*;
use oasis7_custody_core::approval_admission::ApprovalSignatureVerifier;

fn native_history_accepted(f: &SessionFixture) -> bool {
    let ns = scope(&f.pin);
    match f
        .state
        .test_session_v2_native_restore_checks(&f.session_id, &ns)
    {
        Err(_) => false,
        Ok(jobs) => !jobs.is_empty() && jobs.iter().all(|job| StrictCrypto.verify(job).is_ok()),
    }
}

#[test]
fn session_v2_claim_native_history_rejects_actual_wrong_root_signature() {
    // Complete the genuine new V2 Owner/session/activation chain and actual
    // credential mutation first. No credential or Active state is injected.
    let mut f = activation_tests::activated(9000);
    let q = request(&f, "history-native-root-claim", None);
    let outcome = claim(&mut f, &q, &[allocation(51)], 51, 12_000).unwrap();
    assert!(outcome.newly_claimed);
    assert!(
        native_history_accepted(&f),
        "all actual Native history signatures must pass first"
    );
    let original = serde_json::to_value(&f.state).unwrap();
    let path = &original["test_sessions_v2"][&f.session_id]["credential_claims"][&q.request_id]["lifetime"];
    let lifetime: SignedManagedNativeSessionLifetimeV1 =
        serde_json::from_value(path.clone()).unwrap();
    lifetime.validate().unwrap();
    let message = lifetime.lifetime.signing_bytes().unwrap();
    let original_signature: [u8; 64] = URL_SAFE_NO_PAD
        .decode(&lifetime.signature)
        .unwrap()
        .try_into()
        .unwrap();
    assert!(
        key()
            .verifying_key()
            .verify_strict(
                &message,
                &ed25519_dalek::Signature::from_bytes(&original_signature)
            )
            .is_ok()
    );
    let other = SigningKey::from_bytes(&[221; 32]);
    assert!(other.verifying_key() != key().verifying_key());
    let signature = other.sign(&message);
    assert!(
        other
            .verifying_key()
            .verify_strict(&message, &signature)
            .is_ok()
    );
    assert!(
        key()
            .verifying_key()
            .verify_strict(&message, &signature)
            .is_err()
    );
    let mut changed = original;
    // Change only the claim LF signature. Keep known key_id, complete LF
    // payload, all claims/history/challenge/linkage and every paid ledger exact.
    changed["test_sessions_v2"][&f.session_id]["credential_claims"][&q.request_id]["lifetime"]["signature"] =
        serde_json::Value::String(URL_SAFE_NO_PAD.encode(signature.to_bytes()));
    let changed_state: AccountProvisionApprovalState = serde_json::from_value(changed)
        .expect("wrong Root signature is shape-valid; this is not a serde rejection oracle");
    f.state = changed_state;
    // Target assertion: the old iterator omits this claim lifetime and therefore
    // returns only still-valid activation/preflight jobs. The fix must include
    // this exact stored claim and reject under the independently configured pin.
    assert!(
        !native_history_accepted(&f),
        "actual configured Native Root must cover every stored claim lifetime"
    );
}
