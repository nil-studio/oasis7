use super::*;
#[test]
fn paid_binding_attempt_recovers_old_receipt_without_reviving_expired_nonce() {
    let (mut s, c, pin, receipt) = binding_ready_with_response_loss(true);
    let old_balance = s.provision().transport_remaining().unwrap();
    claim(
        &mut s,
        &c,
        &pin,
        "binding-recovery",
        "binding-two",
        NativeStageKindV1::Binding,
        40_000,
    );
    assert_eq!(
        s.provision().transport_remaining().unwrap(),
        old_balance - 64
    );
    let p = Policy::new(s.draft());
    let recovered = s
        .recover_native_binding_receipt(
            "binding-recovery",
            receipt.clone(),
            &scope(&pin),
            &p.current(&c),
            40_004,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(recovered, NativeStageResultV1::BindingReceipt(receipt));
    assert_eq!(s.provision().stage(), ProvisionStage::NativeRegistered);
    assert_eq!(
        s.bootstrap_activation().unwrap().binding_operation_id,
        "binding-one"
    );
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
#[test]
fn new_binding_challenge_changes_nonce_but_never_native_account_key_or_registered_facts() {
    let (mut s, c, pin, _) = binding_ready_with_response_loss(true);
    claim(
        &mut s,
        &c,
        &pin,
        "binding-new-challenge",
        "binding-two",
        NativeStageKindV1::Binding,
        40_000,
    );
    let mut challenge = s
        .native_stages()
        .values()
        .find_map(|v| match &v.result {
            Some(NativeStageResultV1::RegistrationChallenge(v)) => Some(v.challenge.clone()),
            _ => None,
        })
        .unwrap();
    challenge.challenge_id = "binding-two".into();
    challenge.nonce = "a1".repeat(32);
    challenge.issued_at = DecimalU64(40_004);
    challenge.expires_at = DecimalU64(70_004);
    let signed = SignedNativeBindingChallengeV1 {
        key_id: "native-key".into(),
        signature: URL_SAFE_NO_PAD
            .encode(key().sign(&challenge.signing_bytes().unwrap()).to_bytes()),
        challenge: challenge.clone(),
    };
    let p = Policy::new(s.draft());
    let mut changed = signed.clone();
    changed.challenge.native_account_id = "another-account".into();
    changed.signature = URL_SAFE_NO_PAD.encode(
        key()
            .sign(&changed.challenge.signing_bytes().unwrap())
            .to_bytes(),
    );
    let before = s.clone();
    assert!(
        s.record_native_binding_challenge(
            "binding-new-challenge",
            changed,
            &scope(&pin),
            &p.current(&c),
            40_004,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(s, before);
    s.record_native_binding_challenge(
        "binding-new-challenge",
        signed,
        &scope(&pin),
        &p.current(&c),
        40_004,
        &mut StrictCrypto,
    )
    .unwrap();
    assert_eq!(
        s.native_stages()["binding-new-challenge"]
            .binding_challenge
            .as_ref()
            .unwrap()
            .challenge,
        challenge
    );
    assert_eq!(s.provision().stage(), ProvisionStage::RoleKeysReady);
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(serde_json::to_value(&s).unwrap())
            .unwrap(),
        s
    );
}
