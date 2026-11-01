//! Actual signed Resource prefix, narrower trusted Current only; no changed proof/fact.
use super::*;
fn read_window(kind: u8) {
    let (mut s, mut c, pin, _) = resource_prefix();
    prepare_resource_access(&mut s, &c, &pin, 2100);
    let bound = DecimalU64(2107);
    match kind {
        0 => c.handoff_until = bound,
        1 => c.source.session_expires_at = bound,
        2 => c.source.session_idle_expires_at = bound,
        _ => panic!("closed fixture case"),
    }
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let before = serde_json::to_value(&s).unwrap();
    let result =
        s.claim_owner_setup_resource_read_v2("setup-one", &trusted, 2106, &mut StrictCrypto);
    if kind != 0 {
        assert!(result.is_err());
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
        return;
    }
    let claim = result.unwrap();
    assert!(
        claim.valid_until() <= bound.0,
        "resource read cannot outlive actual trusted Current window"
    );
    let before_view = serde_json::to_value(&s).unwrap();
    let view = s
        .authorize_owner_setup_read(
            "setup-one",
            OwnerSetupReadTicket {
                stage: OwnerSetupStageV1::FinalObservation,
                ticket: 4,
                purpose: OwnerSetupReadPurposeV1::Sign,
            },
            Role::Owner,
            &trusted,
            2106,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(serde_json::to_value(&s).unwrap(), before_view);
    assert!(
        view.expires_at.0 <= bound.0,
        "actual Resource signer view must not outlive the same trusted handoff as its opaque original"
    );
    let saved = serde_json::to_value(&s).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    let mut missing = saved;
    missing["owner_setup_v2"]["resource_final"]
        .as_object_mut()
        .unwrap()
        .remove("signing_window");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
}
#[test]
fn owner_resource_v2_opaque_read_respects_tighter_actual_handoff() {
    read_window(0);
}
#[test]
fn owner_resource_v2_opaque_read_respects_tighter_actual_manager_expiry() {
    read_window(1);
}
#[test]
fn owner_resource_v2_opaque_read_respects_tighter_actual_manager_idle() {
    read_window(2);
}
#[test]
fn owner_resource_v2_opaque_dispatch_respects_tighter_actual_handoff() {
    let (mut s, mut c, pin, _, _) = resource_published();
    let bound = DecimalU64(2113);
    c.handoff_until = bound;
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let key = SigningKey::from_bytes(&[43; 32]);
    let issuer = PaidResourceReadIssuerPinV1 {
        namespace: "custody:test".into(),
        service_id: "account".into(),
        environment: s.draft().entry.environment.clone(),
        key_id: "resource-issuer".into(),
        public_key: key
            .verifying_key()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        issuer_epoch: DecimalU64(2),
        issuer_config_generation: DecimalU64(3),
        not_before: DecimalU64(1),
        not_after: DecimalU64(10000),
        native_audience_origin: ns.http_origin.into(),
        world_audience: s.owner_setup_v2_plan().unwrap().world_identity.clone(),
        allowed_stage: PaidResourceReadStageV1::OwnerSetupFinal,
    };
    let configured = ResourceFinalIssueContextV2 {
        issuer: &issuer,
        resource_feature_generation: DecimalU64(1),
    };
    let claim = s
        .claim_owner_setup_resource_dispatch_v2(
            "setup-one",
            &configured,
            &trusted,
            2111,
            &mut StrictCrypto,
        )
        .unwrap();
    assert!(
        claim.payload().expires_at.0 <= bound.0,
        "paid dispatch cannot outlive actual trusted Current handoff"
    );
    let saved = serde_json::to_value(&s).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    let mut widened = saved;
    widened["owner_setup_v2"]["resource_final"]["dispatch"]["expires_at"] = json!("2114");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(widened).is_err());
}
