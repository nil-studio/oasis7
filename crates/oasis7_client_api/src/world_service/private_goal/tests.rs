use super::*;
use oasis7_agent_api::game::{
    DecimalU64, GameTaskStatusV1, GoalContentV1, game_goal_version_digest,
};
fn key() -> String {
    hex::encode([7; 32])
}
fn request() -> GameGoalPrepareV1 {
    let owner = sign_read_request("fixture", (), &key())
        .unwrap()
        .subject_public_key;
    let content = GoalContentV1 {
        goal_text: "owner private goal complete正文".into(),
        soft_constraints: vec!["first".into(), "second".into()],
    };
    GameGoalPrepareV1 {
        api_rev: 1,
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        task_id: "task".into(),
        agent_id: "agent".into(),
        owner_public_key: owner,
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        expected_task_revision: DecimalU64(1),
        expected_latest_goal_revision: DecimalU64(0),
        goal_revision: DecimalU64(1),
        client_operation_id: "op".into(),
        client_nonce: "ab".repeat(32),
        goal_digest: game_goal_version_digest("task", DecimalU64(1), &content).unwrap(),
        content,
    }
}
fn control(r: &GameGoalPrepareV1) -> GameTaskControlV1 {
    GameTaskControlV1 {
        schema_revision: 1,
        task_id: r.task_id.clone(),
        world_id: r.world_id.clone(),
        genesis_digest: r.genesis_digest.clone(),
        branch_id: r.branch_id.clone(),
        reorg_epoch: r.reorg_epoch,
        agent_id: r.agent_id.clone(),
        owner_binding: r.owner_binding.clone(),
        owner_generation: r.owner_generation,
        owner_public_key: r.owner_public_key.clone(),
        task_revision: r.expected_task_revision,
        task_status: GameTaskStatusV1::Suspended,
        latest_goal_revision: r.expected_latest_goal_revision,
        active_goal_revision: DecimalU64(0),
        active_goal_reference: None,
        executor_epoch: DecimalU64(0),
        executor_id: None,
        executor_binding: None,
        created_operation_id: "create".into(),
    }
}
#[test]
fn private_goal_prepare_owner_signature_exact_body_and_domain() {
    let r = request();
    let c = control(&r);
    let signed = sign_goal_prepare(r.clone(), &key()).unwrap();
    signed.validate_against(&c).unwrap();
    assert!(sign_goal_prepare(r.clone(), &hex::encode([8; 32])).is_err());
    for field in 0..6 {
        let mut bad = signed.clone();
        match field {
            0 => {
                bad.request.content.goal_text.push('x');
                bad.request.goal_digest = game_goal_version_digest(
                    &bad.request.task_id,
                    bad.request.goal_revision,
                    &bad.request.content,
                )
                .unwrap();
            }
            1 => {
                bad.request.content.soft_constraints.reverse();
                bad.request.goal_digest = game_goal_version_digest(
                    &bad.request.task_id,
                    bad.request.goal_revision,
                    &bad.request.content,
                )
                .unwrap();
            }
            2 => bad.request.client_nonce = "cd".repeat(32),
            3 => bad.request.client_operation_id.push('x'),
            4 => bad.request.goal_digest.0 = "blake3:".to_owned() + &"00".repeat(32),
            _ => bad.signature_hex = "00".repeat(64),
        }
        assert!(bad.verify().is_err());
        assert!(bad.validate_against(&c).is_err());
    }
    for domain in ["game_task_create", "game_goal_publish/v1", "/v1/world/view"] {
        let other = sign_read_request(domain, r.clone(), &key()).unwrap();
        let forged = SignedGoalPrepareV1 {
            request: other.request,
            subject_public_key: other.subject_public_key,
            signature_hex: other.signature_hex,
        };
        assert!(forged.verify().is_err());
    }
    let mut bad = signed.clone();
    bad.subject_public_key = sign_read_request("fixture", (), &hex::encode([8; 32]))
        .unwrap()
        .subject_public_key;
    assert!(bad.verify().is_err());
    let mut uppercase = signed.clone();
    uppercase.signature_hex = uppercase.signature_hex.to_uppercase();
    assert!(uppercase.verify().is_err());
    let mut value = serde_json::to_value(&signed).unwrap();
    value["extra"] = serde_json::json!(true);
    assert!(
        oasis7_agent_api::game::parse_request::<SignedGoalPrepareV1>(
            &serde_json::to_vec(&value).unwrap()
        )
        .is_err()
    );
}
#[test]
fn private_goal_prepare_valid_signature_does_not_grant_foreign_or_stale_control() {
    let signed = sign_goal_prepare(request(), &key()).unwrap();
    let trusted = control(&signed.request);
    for field in 0..12 {
        let mut c = trusted.clone();
        match field {
            0 => c.world_id.push('x'),
            1 => c.genesis_digest.push('x'),
            2 => c.task_id.push('x'),
            3 => c.agent_id.push('x'),
            4 => c.owner_binding.push('x'),
            5 => c.owner_public_key = "cd".repeat(32),
            6 => c.owner_generation.0 += 1,
            7 => c.branch_id.push('x'),
            8 => c.reorg_epoch.0 += 1,
            9 => c.task_revision.0 += 1,
            10 => c.latest_goal_revision.0 += 1,
            _ => c.schema_revision += 1,
        }
        assert!(signed.verify().is_ok());
        assert!(signed.validate_against(&c).is_err());
    }
    let bytes = serde_json::to_vec(&signed).unwrap();
    let decoded: SignedGoalPrepareV1 = oasis7_agent_api::game::parse_request(&bytes).unwrap();
    decoded.validate_against(&trusted).unwrap();
    assert!(decoded == signed);
    // A structurally valid edit with a recomputed digest remains unsigned.
    let mut bad = signed.clone();
    bad.request.content.goal_text.push_str("tamper");
    bad.request.goal_digest = game_goal_version_digest(
        &bad.request.task_id,
        bad.request.goal_revision,
        &bad.request.content,
    )
    .unwrap();
    let error = bad.validate_against(&trusted).unwrap_err();
    assert!(!error.contains(&bad.request.content.goal_text));
    assert!(!error.contains(&bad.signature_hex));
}

#[test]
fn private_goal_prepare_batch_still_rechecks_every_current_control_field() {
    #[cfg(feature = "signature-metrics")]
    signature_metrics::begin_scope();
    with_owner_signature_verification_batch(
        private_goal_prepare_valid_signature_does_not_grant_foreign_or_stale_control,
    );
    #[cfg(feature = "signature-metrics")]
    {
        let metrics = signature_metrics::take_summary();
        assert!(metrics.memo_hits > 0);
        assert!(metrics.strict_attempts > 0);
        assert!(
            metrics.failures > 0,
            "changed private content must still fail"
        );
    }
}

#[test]
fn private_goal_prepare_all_metadata_fields_are_signed_and_envelope_has_no_debug() {
    let signed = sign_goal_prepare(request(), &key()).unwrap();
    for field in 0..13 {
        let mut changed = signed.clone();
        let r = &mut changed.request;
        match field {
            0 => r.world_id.push('x'),
            1 => r.genesis_digest.push('x'),
            2 => r.task_id.push('x'),
            3 => r.agent_id.push('x'),
            4 => {
                r.owner_public_key = sign_read_request("fixture", (), &hex::encode([8; 32]))
                    .unwrap()
                    .subject_public_key
            }
            5 => r.owner_binding.push('x'),
            6 => r.owner_generation.0 += 1,
            7 => r.branch_id.push('x'),
            8 => r.reorg_epoch.0 += 1,
            9 => r.expected_task_revision.0 += 1,
            10 => {
                r.expected_latest_goal_revision.0 += 1;
                r.goal_revision.0 += 1;
            }
            11 => {
                r.goal_revision.0 += 1;
                r.expected_latest_goal_revision.0 += 1;
            }
            _ => r.api_rev = 2,
        }
        r.goal_digest = game_goal_version_digest(&r.task_id, r.goal_revision, &r.content).unwrap();
        if field != 12 {
            r.validate().unwrap();
        }
        assert!(changed.verify().is_err());
    }
    // Shape and the old signature still match this new declared signer; only
    // cryptographic verification rejects the substitution.
    let mut substituted = signed.clone();
    substituted.subject_public_key = sign_read_request("fixture", (), &hex::encode([8; 32]))
        .unwrap()
        .subject_public_key;
    substituted.request.owner_public_key = substituted.subject_public_key.clone();
    substituted.request.validate().unwrap();
    assert!(substituted.verify().is_err());
    trait AmbiguousIfDebug<A> {
        fn marker() {}
    }
    impl<T: ?Sized> AmbiguousIfDebug<()> for T {}
    impl<T: ?Sized + std::fmt::Debug> AmbiguousIfDebug<u8> for T {}
    let _ = <SignedGoalPrepareV1 as AmbiguousIfDebug<_>>::marker;
}

#[test]
fn private_goal_prepare_rejects_low_order_owner_and_identity_signature() {
    // Edwards identity encoding and R=identity/S=zero require no owner secret.
    // Ordinary dalek verify accepts this for arbitrary messages; private proofs
    // must reject it even when the trusted control names that owner key.
    let identity = "01".to_owned() + &"00".repeat(31);
    let signature = identity.clone() + &"00".repeat(32);
    for goal in [
        "original private goal",
        "arbitrary substituted private goal",
    ] {
        let mut r = request();
        r.owner_public_key = identity.clone();
        r.content.goal_text = goal.into();
        r.goal_digest = game_goal_version_digest(&r.task_id, r.goal_revision, &r.content).unwrap();
        r.validate().unwrap();
        let trusted = control(&r);
        let forged = SignedGoalPrepareV1 {
            request: r,
            subject_public_key: identity.clone(),
            signature_hex: signature.clone(),
        };
        assert!(
            forged.verify().is_err(),
            "low-order owner must not authenticate arbitrary goal bodies"
        );
        assert!(forged.validate_against(&trusted).is_err());
    }
}

#[test]
fn private_goal_prepare_rejects_small_order_r_for_nonweak_owner() {
    let r = request();
    let public_key: [u8; 32] = hex::decode(&r.owner_public_key)
        .unwrap()
        .try_into()
        .unwrap();
    assert!(
        !ed25519_dalek::VerifyingKey::from_bytes(&public_key)
            .unwrap()
            .is_weak()
    );
    let forged = SignedGoalPrepareV1 {
        subject_public_key: r.owner_public_key.clone(),
        request: r,
        signature_hex: "01".to_owned() + &"00".repeat(63),
    };
    // This negative vector also fails ordinary verification. The separate
    // weak-owner regression demonstrates the original acceptance bug; the
    // production verify_strict call additionally rejects all small-order R.
    assert!(forged.verify().is_err());
    assert!(forged.validate_against(&control(&forged.request)).is_err());
}

#[test]
fn scoped_goal_chain_nested_repeats_crypto_but_never_skips_receipt_semantics() {
    use crate::world_service::{private_goal_read::*, request_digest, sign_service_response};
    let original = sign_goal_prepare(request(), &key()).unwrap();
    let salt = [19u8; 32];
    let receipt = PreparedGoalReceiptV1 {
        schema_revision: 1,
        goal_ref: prepared_goal_commitment(&salt, &original).unwrap(),
        reference_salt: hex::encode(salt),
        owner_proof: original.clone(),
    };
    let service = hex::encode([31; 32]);
    let pin = sign_read_request("pin", (), &service)
        .unwrap()
        .subject_public_key;
    let response = sign_service_response(
        GOAL_PREPARE_PATH,
        request_digest(GOAL_PREPARE_PATH, &original).unwrap(),
        receipt.clone(),
        &service,
    )
    .unwrap();
    let mut scope = StrictOwnerSignatureScope::default();
    #[cfg(feature = "signature-metrics")]
    signature_metrics::begin_scope();
    verify_prepare_response_with_signature_scope(&original, &response, &pin, &mut scope).unwrap();
    verify_prepare_response_with_signature_scope(&original, &response, &pin, &mut scope).unwrap();
    let observed = scope.summary();
    assert_eq!(
        (
            observed.nested_calls,
            observed.nested_repeats,
            observed.strict_attempts,
            observed.tracked_tuples
        ),
        (4, 2, 2, 2)
    );
    #[cfg(feature = "signature-metrics")]
    assert_eq!(signature_metrics::take_summary().strict_attempts, 2);
    let mut bad = receipt;
    bad.reference_salt = "ab".repeat(32);
    let semantic = sign_service_response(
        GOAL_PREPARE_PATH,
        response.request_digest.clone(),
        bad,
        &service,
    )
    .unwrap();
    assert!(
        verify_prepare_response_with_signature_scope(&original, &semantic, &pin, &mut scope)
            .is_err()
    );
    // The newly signed packet and its owner proof are cryptographically valid;
    // the repeated semantic commitment check must nevertheless reject it.
    assert_eq!(scope.summary().failures, 0);
    verify_prepare_response(&original, &response, &pin).unwrap();
}

#[path = "current_scoped_tests.rs"]
mod current_scoped_tests;
