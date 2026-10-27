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
