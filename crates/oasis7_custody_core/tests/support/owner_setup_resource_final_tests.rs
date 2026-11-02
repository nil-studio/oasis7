//! Resource final pipeline uses a new consumed V2 approval, never an upgraded V1 record.
use super::*;
use oasis7_client_api::world_service::resource_basis as resource;

#[test]
fn owner_resource_v2_final_has_distinct_paid_slot_after_five_canonical_stages() {
    let (mut s, _, _, _) = resource_prefix();
    let before = serde_json::to_value(&s).unwrap();
    assert!(
        s.reserve_owner_setup_stage(
            "setup-one",
            OwnerSetupStageV1::FinalObservation,
            &format!("{:064x}", 2100),
            2100
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&s).unwrap(), before);
    s.reserve_owner_setup_resource_final_v2("setup-one", &format!("{:064x}", 2100), 2100)
        .unwrap();
    assert_ne!(serde_json::to_value(&s).unwrap(), before);
    let restored: AccountProvisionApprovalState =
        serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(&s).unwrap()
    );
}

#[test]
fn owner_resource_v2_sign_claim_is_once_and_replays_without_a_v1_final_original() {
    let (mut s, c, pin, _) = resource_prefix();
    prepare_resource_access(&mut s, &c, &pin, 2100);
    let policy = Policy::new(s.draft());
    let current = policy.current(&c);
    let ns = scope(&pin);
    let trusted = OwnerSetupCurrent {
        provision: &current,
        native_scope: &ns,
    };
    let claim = s
        .claim_owner_setup_resource_read_v2("setup-one", &trusted, 2106, &mut StrictCrypto)
        .unwrap();
    assert_eq!(claim.request().api_version, 2);
    assert_eq!(claim.request().setup_id, "setup-one");
    assert_eq!(claim.valid_until(), 7102);
    let saved = serde_json::to_value(&s).unwrap();
    assert!(
        s.claim_owner_setup_resource_read_v2("setup-one", &trusted, 2106, &mut StrictCrypto)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&s).unwrap(), saved);
    let restored: AccountProvisionApprovalState = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
}

#[test]
fn owner_resource_v2_full_owner_and_node_preflight_cold_replays_in_separate_slot() {
    let (s, _, _, _, candidate) = resource_published();
    resource::verify_resource_goal_request(&candidate.original).unwrap();
    let saved = serde_json::to_value(&s).unwrap();
    let restored: AccountProvisionApprovalState = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    assert_ne!(s.provision().stage(), ProvisionStage::Prepared);
}

#[test]
fn owner_resource_v2_dispatch_is_prepaid_once_and_preserves_the_original_on_unknown() {
    let (mut s, c, pin, _, candidate) = resource_published();
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
    let sdk = ws::request_digest(resource::RESOURCE_GOAL_PATH_V2, &candidate.original).unwrap();
    let raw = sdk.strip_prefix("blake3:").unwrap();
    assert_eq!(raw.len(), 64);
    assert!(
        raw.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(claim.payload().world_request_digest, raw);
    assert_eq!(claim.payload().native_http_units, DecimalU64(1));
    assert_eq!(claim.payload().world_http_units, DecimalU64(2));
    let saved = serde_json::to_value(&s).unwrap();
    assert!(
        s.claim_owner_setup_resource_dispatch_v2(
            "setup-one",
            &configured,
            &trusted,
            2111,
            &mut StrictCrypto
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&s).unwrap(), saved);
    let restored: AccountProvisionApprovalState = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    assert_ne!(s.provision().stage(), ProvisionStage::Prepared);
}

#[path = "owner_setup_resource_final_fact_fixture.rs"]
mod fact_fixture;

#[test]
fn owner_resource_v2_strong_final_result_produces_only_historical_ready_basis() {
    for ready in [false, true] {
        let (mut s, c, pin, canonical, candidate) = resource_published();
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
        let dispatched = s
            .claim_owner_setup_resource_dispatch_v2(
                "setup-one",
                &configured,
                &trusted,
                2111,
                &mut StrictCrypto,
            )
            .unwrap();
        assert_eq!(dispatched.payload().world_http_units, DecimalU64(2));
        let response = fact_fixture::response(&s, &canonical, &candidate.original, ready);
        resource::verify_resource_goal_response(
            &candidate.original,
            &response,
            &s.owner_setup_v2_plan()
                .unwrap()
                .world_identity
                .service_public_key,
        )
        .unwrap();
        let session = fixture::root_session(
            &s,
            s.owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
                .unwrap()
                .session_nonce
                .clone(),
            2112,
        );
        let unreserved = serde_json::to_value(&s).unwrap();
        assert!(
            actual_resource_paid_selector::material(
                &s,
                "setup-one",
                &s.draft().world_identity,
                2112
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&s).unwrap(), unreserved);
        s.reserve_owner_setup_resource_result_callback_v2("setup-one", 2112)
            .unwrap();
        let material = actual_resource_paid_selector::material(
            &s,
            "setup-one",
            &s.draft().world_identity,
            2112,
        )
        .unwrap();
        assert!(matches!(
            &material.input.request.intent,
            CustodyIntentV1::OwnerSetupV2 { .. }
        ));
        assert!(material.signed.verification.observed_at.0 <= 2112);
        let wrong_world = {
            let mut w = s.draft().world_identity.clone();
            w.world_id.push_str("-wrong");
            w
        };
        assert!(
            actual_resource_paid_selector::material(&s, "setup-one", &wrong_world, 2112).is_err()
        );
        let callback_state = serde_json::to_value(&s).unwrap();
        assert!(
            s.reserve_owner_setup_resource_result_callback_v2("setup-one", 2112)
                .is_err()
        );
        assert_eq!(serde_json::to_value(&s).unwrap(), callback_state);
        let cold: AccountProvisionApprovalState =
            serde_json::from_value(callback_state.clone()).unwrap();
        assert_eq!(serde_json::to_value(cold).unwrap(), callback_state);
        let (input, source) = s
            .owner_setup_resource_result_callback_source_v2("setup-one", 2112)
            .unwrap();
        assert!(matches!(
            &input.request.intent,
            CustodyIntentV1::OwnerSetupV2 { .. }
        ));
        assert!(source.verification.observed_at.0 <= 2112);
        let common = s
            .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
            .unwrap();
        assert!(common.claimed_at.is_none() && common.callback_claimed_at.is_none());
        // Restore must verify the full local once record, not its presence.
        for field in [
            "dispatch_record_digest",
            "original_digest",
            "source_record_digest",
        ] {
            let mut wrong = callback_state.clone();
            wrong["owner_setup_v2"]["resource_final"]["result_callback"][field] =
                serde_json::json!("00".repeat(32));
            assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
        }
        for (field, value) in [("claimed_at", "2110"), ("expires_at", "2112")] {
            let mut wrong = callback_state.clone();
            wrong["owner_setup_v2"]["resource_final"]["result_callback"][field] =
                serde_json::json!(value);
            assert!(serde_json::from_value::<AccountProvisionApprovalState>(wrong).is_err());
        }
        let before = serde_json::to_value(&s).unwrap();
        let mut bad = response.clone();
        bad.signature_hex = "00".repeat(64);
        assert!(
            s.complete_owner_setup_resource_final_v2(
                "setup-one",
                CompleteOwnerSetupResourceFinalV2 {
                    response: bad,
                    session: session.clone()
                },
                &trusted,
                2112,
                &mut StrictCrypto
            )
            .is_err()
        );
        assert_eq!(serde_json::to_value(&s).unwrap(), before);
        s.complete_owner_setup_resource_final_v2(
            "setup-one",
            CompleteOwnerSetupResourceFinalV2 { response, session },
            &trusted,
            2112,
            &mut StrictCrypto,
        )
        .unwrap();
        let saved = serde_json::to_value(&s).unwrap();
        let restored: AccountProvisionApprovalState =
            serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(serde_json::to_value(&restored).unwrap(), saved);
        let mut missing = saved.clone();
        missing["owner_setup_v2"]["resource_final"]
            .as_object_mut()
            .unwrap()
            .remove("result_callback")
            .unwrap();
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
        let mut unclaimed = saved.clone();
        unclaimed["owner_setup_v2"]["resource_final"]["result_callback"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(unclaimed).is_err());
        if ready {
            assert_eq!(s.provision().stage(), ProvisionStage::Prepared);
            let basis = s
                .owner_setup_prepared_resource_v2("setup-one", &c, &ns, &mut StrictCrypto)
                .unwrap();
            let wrong_root: String = key
                .verifying_key()
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_ne!(wrong_root, ns.native_root_public_key);
            let wrong_scope = NativeStageScope {
                http_origin: ns.http_origin,
                native_root_key_id: ns.native_root_key_id,
                native_root_public_key: &wrong_root,
                owner_binding_pin: ns.owner_binding_pin,
            };
            let before_wrong_root = serde_json::to_value(&s).unwrap();
            assert!(
                s.owner_setup_prepared_resource_v2(
                    "setup-one",
                    &c,
                    &wrong_scope,
                    &mut StrictCrypto
                )
                .is_err()
            );
            assert_eq!(serde_json::to_value(&s).unwrap(), before_wrong_root);
            basis
                .validate_provenance(s.owner_setup_v2_plan().unwrap())
                .unwrap();
            assert_eq!(
                basis.digest(s.owner_setup_v2_plan().unwrap()).unwrap(),
                restored
                    .owner_setup_prepared_resource_v2("setup-one", &c, &ns, &mut StrictCrypto)
                    .unwrap()
                    .digest(restored.owner_setup_v2_plan().unwrap())
                    .unwrap()
            );
        } else {
            assert_ne!(s.provision().stage(), ProvisionStage::Prepared);
            assert!(
                s.owner_setup_prepared_resource_v2("setup-one", &c, &ns, &mut StrictCrypto)
                    .is_err()
            );
        }
    }
}

#[path = "owner_setup_resource_final_flow_fixture.rs"]
mod flow_fixture;
use flow_fixture::{prepare_resource_access, resource_prefix, resource_published};

#[path = "owner_setup_resource_final_window_tests.rs"]
mod window_tests;

#[path = "owner_setup_resource_final_http_tests.rs"]
mod http_tests;

#[test]
fn owner_resource_v2_result_callback_requires_real_dispatch_and_required_nullable_storage() {
    let (mut s, _, _, _, _) = resource_published();
    let saved = serde_json::to_value(&s).unwrap();
    assert!(
        s.reserve_owner_setup_resource_result_callback_v2("setup-one", 2111)
            .is_err()
    );
    assert_eq!(serde_json::to_value(&s).unwrap(), saved);
    assert!(
        s.owner_setup_resource_result_callback_source_v2("setup-one", 2111)
            .is_err()
    );
    let cold: AccountProvisionApprovalState = serde_json::from_value(saved.clone()).unwrap();
    assert!(cold == s);
    assert!(saved["owner_setup_v2"]["resource_final"]["result_callback"].is_null());
    let mut missing = saved;
    missing["owner_setup_v2"]["resource_final"]
        .as_object_mut()
        .unwrap()
        .remove("result_callback")
        .unwrap();
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
}

#[path = "owner_setup_resource_result_callback_tests.rs"]
mod result_callback_tests;

// This is the actual pure selector called immediately before the Worker's
// existing paid Current I/O, not a copied approximation or Worker crate stub.
#[path = "../../../oasis7_custody_worker/src/owner_setup_resource_paid_result_material_v2.rs"]
mod actual_resource_paid_selector;
