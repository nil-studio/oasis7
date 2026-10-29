use super::*;
use crate::protected_bft_v3::state::{BftSignerCheckpointV3, test_state_digest};
fn signed(state: BftSignerStateV3) -> BftSignerCheckpointV3 {
    let mut cp = BftSignerCheckpointV3 {
        state,
        lease_generation: 7,
        challenge: "fresh-control".into(),
        request_digest: "ab".repeat(32),
        provider_public_key: public(33),
        provider_signature_hex: String::new(),
    };
    cp.provider_signature_hex = crate::protected_bft_v3::crypto::sign_preimage(
        &Handle {
            key: key(33),
            disabled: None,
        },
        crate::protected_bft_v3::service::BFT_SIGNER_CHECKPOINT_DOMAIN_V3,
        &cp.signable_body(),
    )
    .unwrap();
    cp
}
#[test]
fn checkpoint_authenticated_envelope_does_not_validate_history_or_grant_verified_state() {
    let (binding, snapshot) = fixture_kind(true);
    let original = snapshot.lock().unwrap().state.clone();
    let cp = signed(original.clone());
    cp.validate(&binding, "fresh-control", &"ab".repeat(32), 7, &public(33))
        .unwrap();
    let mut invalid = original;
    assert!(!invalid.completed_journals.is_empty());
    invalid.completed_journals.clear();
    invalid.state_digest = test_state_digest(&invalid);
    let cp = signed(invalid);
    assert!(
        cp.authenticate(&binding, "fresh-control", &"ab".repeat(32), 7, &public(33))
            .is_ok()
    );
    assert!(
        cp.validate(&binding, "fresh-control", &"ab".repeat(32), 7, &public(33))
            .is_err()
    );
    assert!(
        cp.authenticate(&binding, "replayed", &"ab".repeat(32), 7, &public(33))
            .is_err()
    );
    assert!(
        cp.authenticate(&binding, "fresh-control", &"ab".repeat(32), 8, &public(33))
            .is_err()
    );
    let mut tampered = cp;
    tampered.state.revision += 1;
    assert!(
        tampered
            .authenticate(&binding, "fresh-control", &"ab".repeat(32), 7, &public(33))
            .is_err()
    );
}
