//! Strong-Ed25519 inventory fixture; not CF, payer or full Read Current proof.
use super::*;
use oasis7_custody_core::read_fence_admission_v3::*;
fn proof(attempt: &str) -> (FenceAdmissionProofV3, FenceAdmissionConfigV3, String) {
    let (mut b, mut pin, floor) = material();
    b.fence_attempt_id = attempt.into();
    let d = dispatch(b.clone());
    let claim = ReadFenceReadV3 {
        binding: b.clone(),
        dispatch_digest: d.digest().unwrap(),
        claim_ordinal: DecimalU64(14),
        claimed_at: DecimalU64(14_002),
    };
    let signature_hex = hx(SigningKey::from_bytes(&[113; 32])
        .sign(&claim.signing_bytes().unwrap())
        .to_bytes());
    pin.allowed_purpose = AccountMachinePurposeV2::ReadFenceReadV3;
    let verifier = FenceAdmissionVerifierV3 { pin, floor };
    (
        FenceAdmissionProofV3::ReconcileReadV3(SignedReadFenceReadV3 {
            claim,
            signature_hex,
        }),
        FenceAdmissionConfigV3 {
            profile: FenceAdmissionProfileV3::ReconcileReadV3,
            capacity: DecimalU64(2),
            active: Some(verifier.clone()),
            history: vec![verifier],
        },
        b.fence_object_id,
    )
}
fn execution_profile() -> (FenceAdmissionProofV3, FenceAdmissionConfigV3, String) {
    use oasis7_custody_core::execution_fence_dispatch_v2::*;
    let (b, mut pin, floor) = material();
    let query = ExecutionFenceQueryV2 {
        api_version: 2,
        expected_service_epoch: b.expected_service_epoch,
    };
    let binding = ExecutionFenceBindingV2 {
        api_version: 2,
        issuer: b.issuer,
        scope: b.scope,
        session_id: b.session_id,
        approved_input_digest: b.approved_input_digest,
        master_digest: b.read_graph_digest,
        source_operation_id: b.source_operation_id,
        accepted_ordinal: b.reservation_ordinal,
        dispatcher_claim_ordinal: b.dispatcher_claim_ordinal,
        phase: ExecutionFencePhaseV2::PendingPreInspect,
        dispatcher_attempt_id: b.dispatcher_attempt_id,
        fence_attempt_id: "execution-storage-fixture".into(),
        dispatcher_object_id: b.dispatcher_object_id,
        fence_object_id: b.fence_object_id.clone(),
        expected_service_epoch: b.expected_service_epoch,
        business_body_digest: oasis7_custody_core::approval::bytes_digest(
            &serde_json::to_vec(&query).unwrap(),
        ),
        issued_at: b.issued_at,
        expires_at: b.expires_at,
    };
    let claim = ExecutionFenceReadV2 {
        binding,
        dispatch_digest: "aa".repeat(32),
        claim_ordinal: DecimalU64(14),
        claimed_at: DecimalU64(14_002),
    };
    let signature_hex = hx(SigningKey::from_bytes(&[113; 32])
        .sign(&claim.signing_bytes().unwrap())
        .to_bytes());
    pin.allowed_purpose = AccountMachinePurposeV2::ExecutionFenceReadV2;
    let verifier = FenceAdmissionVerifierV3 { pin, floor };
    (
        FenceAdmissionProofV3::ExecutionV2(SignedExecutionFenceReadV2 {
            claim,
            signature_hex,
        }),
        FenceAdmissionConfigV3 {
            profile: FenceAdmissionProfileV3::ExecutionV2,
            capacity: DecimalU64(2),
            active: Some(verifier.clone()),
            history: vec![verifier],
        },
        b.fence_object_id,
    )
}
fn business(object: &str, environment: &str) -> Vec<FenceBusinessRecordV3> {
    use oasis7_custody_core::recovery::*;
    let policy = RecoveryAuthorityPolicyV1 {
        api_version: 1,
        environment: environment.into(),
        authority_id: "offline-fixture".into(),
        quorum: 2,
        keys: [81, 82]
            .into_iter()
            .map(|v| RecoveryAuthorityKeyV1 {
                key_id: format!("offline-{v}"),
                public_key: hx(SigningKey::from_bytes(&[v; 32]).verifying_key().to_bytes()),
            })
            .collect(),
    };
    let fence = RecoveryFence::new(1, true);
    let ledger = RecoveryFenceLedgerV1::new(policy, object.into(), fence).unwrap();
    vec![
        FenceBusinessRecordV3 {
            id: "recovery-fence-v1".into(),
            payload: serde_json::to_string(&fence).unwrap(),
        },
        FenceBusinessRecordV3 {
            id: "recovery-fence-ledger-v1".into(),
            payload: serde_json::to_string(&ledger).unwrap(),
        },
    ]
}
fn birth(rows: &FenceAdmissionRowsV3) -> FenceBusinessRecordV3 {
    FenceBusinessRecordV3 {
        id: rows.birth.profile.birth_key().into(),
        payload: serde_json::to_string(&rows.birth).unwrap(),
    }
}
#[test]
fn read_fences_admission_once_cold_history_disabled_and_capacity_remain_closed() {
    let (p, c, object) = proof("read-first");
    let rows = FenceAdmissionRowsV3::consume(None, &c, &object, p.clone(), 14_003).unwrap();
    rows.validate(&c, &object).unwrap();
    let raw = serde_json::to_value(&rows).unwrap();
    let cold: FenceAdmissionRowsV3 = serde_json::from_value(raw.clone()).unwrap();
    assert!(rows == cold);
    assert!(FenceAdmissionRowsV3::consume(Some(&cold), &c, &object, p, 14_004).is_err());
    let mut disabled = c.clone();
    disabled.active = None;
    cold.validate(&disabled, &object).unwrap();
    let (second, _, _) = proof("read-second");
    assert!(
        FenceAdmissionRowsV3::consume(Some(&cold), &disabled, &object, second.clone(), 14_004)
            .is_err()
    );
    let two = FenceAdmissionRowsV3::consume(Some(&cold), &c, &object, second, 14_004).unwrap();
    let (third, _, _) = proof("read-third");
    assert!(FenceAdmissionRowsV3::consume(Some(&two), &c, &object, third, 14_005).is_err());
    let (expired, _, _) = proof("read-expired");
    assert!(FenceAdmissionRowsV3::consume(Some(&cold), &c, &object, expired, 19_000).is_err());
    let mut bad = raw.clone();
    bad.as_object_mut().unwrap().remove("header");
    assert!(serde_json::from_value::<FenceAdmissionRowsV3>(bad).is_err());
    let mut bad = raw;
    bad["once"].as_array_mut().unwrap().clear();
    let bad: FenceAdmissionRowsV3 = serde_json::from_value(bad).unwrap();
    assert!(bad.validate(&c, &object).is_err());
    let mut changed = c.clone();
    changed.capacity = DecimalU64(3);
    assert!(cold.validate(&changed, &object).is_err());
}
#[test]
fn read_fences_admission_projection_preserves_real_business_and_rejects_unknown_partial() {
    let (p, c, object) = proof("read-first");
    let rows = FenceAdmissionRowsV3::consume(None, &c, &object, p, 14_003).unwrap();
    let old = business(&object, &c.history[0].pin.environment);
    assert!(project_fence_business_records_v3(&old, None, None, &object).unwrap() == old);
    let mut records = old.clone();
    records.push(birth(&rows));
    assert!(
        project_fence_business_records_v3(&records, None, Some((&rows, &c)), &object).unwrap()
            == old
    );
    let (exec, ec, eo) = execution_profile();
    assert_eq!(eo, object);
    let er = FenceAdmissionRowsV3::consume(None, &ec, &object, exec, 14_003).unwrap();
    let mut both = records.clone();
    both.push(birth(&er));
    assert!(
        project_fence_business_records_v3(&both, Some((&er, &ec)), Some((&rows, &c)), &object)
            .unwrap()
            == old
    );
    assert!(project_fence_business_records_v3(&records, None, None, &object).is_err());
    assert!(project_fence_business_records_v3(&old, None, Some((&rows, &c)), &object).is_err());
    records.push(FenceBusinessRecordV3 {
        id: "unknown-profile-birth".into(),
        payload: "{}".into(),
    });
    assert!(project_fence_business_records_v3(&records, None, Some((&rows, &c)), &object).is_err());
    records.push(FenceBusinessRecordV3 {
        id: "unknown-overflow".into(),
        payload: "{}".into(),
    });
    assert_eq!(fence_record_probe_limit_v3().unwrap(), 5);
    assert!(project_fence_business_records_v3(&records, None, Some((&rows, &c)), &object).is_err());
    assert!(fence_admission_row_limit_v3().unwrap() > MAX_FENCE_READ_REQUEST_BYTES_V3);
}
#[test]
fn read_fences_admission_rejects_strong_tamper_wrong_purpose_and_current_rotation() {
    let (proof, c, object) = proof("read-first");
    let mut tamper = proof.clone();
    if let FenceAdmissionProofV3::ReconcileReadV3(p) = &mut tamper {
        p.claim.binding.original_digest = "ff".repeat(32)
    }
    assert!(FenceAdmissionRowsV3::consume(None, &c, &object, tamper, 14_003).is_err());
    let mut wrong = c.clone();
    wrong.history[0].pin.allowed_purpose = AccountMachinePurposeV2::ExecutionFenceReadV2;
    assert!(wrong.validate().is_err());
    let rows = FenceAdmissionRowsV3::consume(None, &c, &object, proof.clone(), 14_003).unwrap();
    let mut rotated = c.clone();
    let mut active = c.active.clone().unwrap();
    active.pin.key_id = "rotated".into();
    active.pin.issuer_epoch = DecimalU64(2);
    active.floor.issuer_epoch = DecimalU64(2);
    active.pin.public_key = hx(SigningKey::from_bytes(&[114; 32])
        .verifying_key()
        .to_bytes());
    rotated.history.push(active.clone());
    rotated.active = Some(active);
    rows.validate(&rotated, &object).unwrap();
    let mut resign = proof.clone();
    if let FenceAdmissionProofV3::ReconcileReadV3(p) = &mut resign {
        p.claim.binding.issuer.key_id = "rotated".into();
        p.claim.binding.issuer.issuer_epoch = DecimalU64(2);
        p.signature_hex = hx(SigningKey::from_bytes(&[114; 32])
            .sign(&p.claim.signing_bytes().unwrap())
            .to_bytes());
    }
    rotated.verify_current(&resign, &object, 14_004).unwrap();
    assert!(
        FenceAdmissionRowsV3::consume(Some(&rows), &rotated, &object, resign, 14_004).is_err(),
        "rotation cannot reset the same physical attempt once"
    );

    assert!(FenceAdmissionRowsV3::consume(Some(&rows), &rotated, &object, proof, 14_004).is_err());
}
