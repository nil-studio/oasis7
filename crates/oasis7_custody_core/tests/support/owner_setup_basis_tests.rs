use super::*;
fn metadata(c: &ApprovalAdmissionContext) -> CustodySessionStatusV1 {
    CustodySessionStatusV1 {
        api_version: 1,
        identity_namespace: c.identity_namespace.clone(),
        identity_instance_epoch: c.identity_instance_epoch,
        account_id: c.source.manager_account_id.clone(),
        session_ref: c.source.manager_session_ref.clone(),
        credential_ref: c.source.origin_credential_id.clone(),
        credential_version: c.source.origin_credential_version,
        security_version: c.source.manager_security_version,
        policy_version: c.source.manager_policy_version,
        server_time: DecimalU64(1500),
        expires_at: c.source.session_expires_at,
        idle_expires_at: c.source.session_idle_expires_at,
    }
}
#[test]
fn setup_basis_and_prepare_are_actual_core_digest_and_readonly() {
    let (s, c, _) = fixture::participant_bound();
    let policy = Policy::new(s.draft());
    let manager = metadata(&c);
    let view = policy.current(&c);
    let current = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &c.authority,
        policy: view.policy,
        world_identity: view.world_identity,
        native_identity_namespace: view.native_identity_namespace,
        fence: &c.fence,
        observed_at: 1500,
    };
    let before = s.clone();
    let basis = s.owner_setup_basis(&current, 1500).unwrap();
    let plan = plan(&s);
    assert_eq!(
        basis.participant_receipt_digest,
        plan.participant_receipt_digest
    );
    assert_eq!(basis.security_epoch, DecimalU64(1));
    assert_eq!(basis.service_epoch, c.authority.service_epoch);
    assert_eq!(basis.owner_key.key_id, plan.owner_key_ref);
    let request = OwnerSetupPrepareRequestV1 {
        api_version: 1,
        approval_request_id: "prepare-new-uv".into(),
        plan: plan.clone(),
    };
    let options = s
        .owner_setup_prepare_request(&request, &current, 1500)
        .unwrap();
    assert_eq!(options.context, c.authority);
    assert_eq!(options.intent, plan.intent().unwrap());
    assert_eq!(options.approval_request_id, "prepare-new-uv");
    assert_eq!(s, before);
    assert!(s.owner_setups().is_empty());
    let raw = serde_json::to_value(&basis).unwrap().to_string();
    assert!(!raw.contains("goal_text"));
    assert!(!raw.contains("session_ref"));
    assert!(!raw.contains("credential_ref"));
    let mut raw = serde_json::to_value(&request).unwrap();
    raw["verified"] = json!(true);
    assert!(serde_json::from_value::<OwnerSetupPrepareRequestV1>(raw).is_err());
    assert!(
        serde_json::from_value::<OwnerSetupBasisRequestV1>(
            json!({"api_version":1,"custody_account_id":"caller"})
        )
        .is_err()
    );
}
#[test]
fn setup_prepare_wrong_basis_plan_manager_epoch_and_stale_current_reject() {
    let (s, c, _) = fixture::participant_bound();
    let policy = Policy::new(s.draft());
    let manager = metadata(&c);
    let base = plan(&s);
    for case in 0..5 {
        let mut plan = base.clone();
        match case {
            0 => plan.participant_receipt_digest = "00".repeat(32),
            1 => plan.owner_key_ref = "other-owner".into(),
            2 => plan.executor_public_key = "ff".repeat(32),
            3 => plan.world_identity.branch_id = "other".into(),
            _ => plan.provision_id = "other-provision".into(),
        };
        let v = policy.current(&c);
        let current = OwnerSetupManagementCurrent {
            manager: &manager,
            authority: &c.authority,
            policy: v.policy,
            world_identity: v.world_identity,
            native_identity_namespace: v.native_identity_namespace,
            fence: &c.fence,
            observed_at: 1500,
        };
        assert!(
            s.owner_setup_prepare_request(
                &OwnerSetupPrepareRequestV1 {
                    api_version: 1,
                    approval_request_id: "new-uv".into(),
                    plan
                },
                &current,
                1500
            )
            .is_err(),
            "basis case {case}"
        );
    }
    for case in 0..5 {
        let mut manager = metadata(&c);
        let mut authority = c.authority.clone();
        match case {
            0 => manager.account_id = "other-manager".into(),
            1 => manager.identity_instance_epoch = DecimalU64(7),
            2 => authority.security_version = DecimalU64(0),
            3 => authority.custody_account_id = "other-account".into(),
            _ => {}
        };
        let v = policy.current(&c);
        let current = OwnerSetupManagementCurrent {
            manager: &manager,
            authority: &authority,
            policy: v.policy,
            world_identity: v.world_identity,
            native_identity_namespace: v.native_identity_namespace,
            fence: &c.fence,
            observed_at: if case == 4 { 0 } else { 1500 },
        };
        assert!(
            s.owner_setup_basis(&current, if case == 4 { 6000 } else { 1500 })
                .is_err(),
            "current case {case}"
        );
    }
}

#[test]
fn setup_v2_prepare_is_mounted_core_producer_and_does_not_upgrade_v1_authority() {
    let (s, c, _) = fixture::participant_bound();
    let policy = Policy::new(s.draft());
    let manager = metadata(&c);
    let view = policy.current(&c);
    let current = OwnerSetupManagementCurrent {
        manager: &manager,
        authority: &c.authority,
        policy: view.policy,
        world_identity: view.world_identity,
        native_identity_namespace: view.native_identity_namespace,
        fence: &c.fence,
        observed_at: 1500,
    };
    // A test input is built from the exact genuine participant basis; production
    // never coerces V1 plans into V2 or supplies the model budget by default.
    let mut value = serde_json::to_value(plan(&s)).unwrap();
    value.as_object_mut().unwrap().remove("resource_refs");
    value["api_version"] = json!(2);
    value["model_budget"] = serde_json::Value::Null;
    value["transport_limit"] = json!("584");
    value["selection"] =
        json!({"kind":"build","site":"site","factory":"factory","factory_kind":"smelter"});
    let plan: OwnerSetupPlanV2 = serde_json::from_value(value).unwrap();
    plan.validate().unwrap();
    let request = OwnerSetupPrepareRequestV2 {
        api_version: 2,
        approval_request_id: "prepare-v2-uv".into(),
        plan: plan.clone(),
    };
    let before = s.clone();
    let options = s
        .owner_setup_prepare_request_v2(&request, &current, 1500)
        .unwrap();
    assert_eq!(options.context, c.authority);
    assert_eq!(options.intent, plan.intent().unwrap());
    assert_eq!(s, before);
    assert!(s.owner_setups().is_empty());
    let mut changed = request;
    changed.plan.executor_public_key = "99".repeat(32);
    assert!(
        s.owner_setup_prepare_request_v2(&changed, &current, 1500)
            .is_err()
    );
    assert_eq!(s, before);
}

#[test]
fn setup_v2_real_fresh_mfa_book_consumes_once_and_roundtrips_without_v1_dispatch() {
    let (mut state, mut c, _) = fixture::participant_bound();
    let mut value = serde_json::to_value(plan(&state)).unwrap();
    value.as_object_mut().unwrap().remove("resource_refs");
    value["api_version"] = json!(2);
    value["model_budget"] = serde_json::Value::Null;
    value["selection"] =
        json!({"kind":"build","site":"site","factory":"factory","factory_kind":"smelter"});
    let plan: OwnerSetupPlanV2 = serde_json::from_value(value).unwrap();
    plan.validate().unwrap();
    c.handoff_until = plan.deadline;
    let mut input = state.accepted_approval().unwrap().input().clone();
    input.operation_id = plan.setup_id.clone();
    input.request.context = c.authority.clone();
    input.request.approval_request_id = "v2-setup-request".into();
    input.request.intent = plan.intent().unwrap();
    input.approval.proof.context = input.request.context.clone();
    input.approval.proof.intent_digest = input.request.intent_digest().unwrap();
    input.approval.proof.nonce = "v2-new-mfa-nonce".into();
    input.approval.proof.approval_id = "v2-proof".into();
    input.approval.proof.approval_request_id = "v2-setup-request".into();
    input.approval.proof.issued_at = DecimalU64(1500);
    input.approval.proof.expires_at = DecimalU64(31500);
    input.approval = sign_proof(input.approval.proof);
    let policy = Policy::new(state.draft());
    let record = state
        .begin_owner_setup_v2(
            input.clone(),
            plan.clone(),
            "b3".repeat(32),
            &policy.current(&c),
            1500,
            &mut StrictCrypto,
        )
        .unwrap();
    let mut fact = verification(&record, &c).verification;
    fact.observed_at = DecimalU64(1501);
    let signed = sign_verification(fact);
    state
        .commit_owner_setup_v2(
            &plan.setup_id,
            signed.clone(),
            &policy.current(&c),
            1502,
            &mut StrictCrypto,
        )
        .unwrap();
    let consumed = state.clone();
    state
        .commit_owner_setup_v2(
            &plan.setup_id,
            signed,
            &policy.current(&c),
            1503,
            &mut StrictCrypto,
        )
        .unwrap();
    assert_eq!(state, consumed);
    assert_eq!(
        state.owner_setup_v2_progress(&plan.setup_id).unwrap().phase,
        ApprovalPhaseV1::Completed
    );
    assert!(state.owner_setups().is_empty());
    assert!(state.owner_setup_plan(&plan.setup_id).is_none());
    let value = serde_json::to_value(&state).unwrap();
    assert_eq!(
        serde_json::from_value::<AccountProvisionApprovalState>(value.clone()).unwrap(),
        state
    );
    let mut malformed = value;
    malformed["owner_setup_v2"]["plan"]
        .as_object_mut()
        .unwrap()
        .remove("model_budget");
    assert!(serde_json::from_value::<AccountProvisionApprovalState>(malformed).is_err());
    let mut different = input;
    different.request.approval_request_id = "different".into();
    assert!(
        state
            .begin_owner_setup_v2(
                different,
                plan,
                "b3".repeat(32),
                &policy.current(&c),
                1503,
                &mut StrictCrypto
            )
            .is_err()
    );
    assert_eq!(state, consumed);
}
