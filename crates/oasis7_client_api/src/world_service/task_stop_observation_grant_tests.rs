//! Genuine Owner signatures and a distinct legacy field-set codec golden.
use super::*;
#[derive(serde::Serialize)]
struct LegacyStopBytes<'a> {
    api_revision: u32,
    client_operation_id: &'a str,
    custody_account_id: &'a str,
    termination_plan_digest: &'a str,
    initial_fence: &'a GameTaskFenceV1,
    target_executor_public_key: &'a str,
    target_executor_generation: DecimalU64,
    target_executor_nonce: DecimalU64,
    execute_until_ms: DecimalU64,
}
#[test]
fn guarded_stop_v2_none_legacy_bytes_and_same_owner_signature_grant_binding() {
    let (q, _, _) = fixture();
    let p = &q.submitted.original.request;
    assert!(p.observation_grant.is_none());
    let legacy = LegacyStopBytes {
        api_revision: p.api_revision,
        client_operation_id: &p.client_operation_id,
        custody_account_id: &p.custody_account_id,
        termination_plan_digest: &p.termination_plan_digest,
        initial_fence: &p.initial_fence,
        target_executor_public_key: &p.target_executor_public_key,
        target_executor_generation: p.target_executor_generation,
        target_executor_nonce: p.target_executor_nonce,
        execute_until_ms: p.execute_until_ms,
    };
    assert_eq!(
        crate::world_service::signing_bytes(GAME_TASK_STOP_DOMAIN_V2, p).unwrap(),
        crate::world_service::signing_bytes(GAME_TASK_STOP_DOMAIN_V2, &legacy).unwrap()
    );
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        serde_json::to_value(&legacy).unwrap()
    );
    q.submitted.verify_owner_signature().unwrap();
    let mut with = p.clone();
    with.observation_grant = Some(
        StopObservationGrantV2::known_control_for_exact_target(&with, 8000, 7500, 7000).unwrap(),
    );
    assert!(
        crate::world_service::private_goal::verify_owner_signature_strict(
            GAME_TASK_STOP_DOMAIN_V2,
            &with,
            &q.submitted.original.subject_public_key,
            &q.submitted.original.signature_hex
        )
        .is_err()
    );
    let signed =
        sign_read_request(GAME_TASK_STOP_DOMAIN_V2, with.clone(), &"11".repeat(32)).unwrap();
    crate::world_service::private_goal::verify_owner_signature_strict(
        GAME_TASK_STOP_DOMAIN_V2,
        &with,
        &signed.subject_public_key,
        &signed.signature_hex,
    )
    .unwrap();
    assert_ne!(signed.signature_hex, q.submitted.original.signature_hex);
    with.observation_grant = None;
    assert!(
        crate::world_service::private_goal::verify_owner_signature_strict(
            GAME_TASK_STOP_DOMAIN_V2,
            &with,
            &signed.subject_public_key,
            &signed.signature_hex
        )
        .is_err()
    );
}
#[test]
fn guarded_stop_v2_observation_grant_closed_target_mode_deadline_validation() {
    let (q, _, _) = fixture();
    let mut p = q.submitted.original.request.clone();
    p.observation_grant =
        Some(StopObservationGrantV2::known_control_for_exact_target(&p, 8000, 7500, 7000).unwrap());
    p.validate().unwrap();
    let raw = serde_json::to_value(&p).unwrap();
    for (field, value) in [
        ("scope", serde_json::json!("owner_all")),
        ("target_digest", serde_json::json!("00".repeat(32))),
        ("historical_read_until_ms", serde_json::json!("8001")),
        ("control_read_until_ms", serde_json::json!("7501")),
        ("approved_cleanup_until_ms", serde_json::json!("4999")),
        ("control_read_until_ms", serde_json::json!("0")),
    ] {
        let mut changed = raw.clone();
        changed["observation_grant"][field] = value;
        if let Ok(changed) = serde_json::from_value::<GameTaskStopV2>(changed) {
            assert!(changed.validate().is_err());
        }
    }
    let mut changed = p.clone();
    changed.initial_fence.task_id = "other-task".into();
    assert!(changed.validate().is_err());
    let mut added = raw.clone();
    added["observation_grant"]["fields"] = serde_json::json!(["context"]);
    assert!(serde_json::from_value::<GameTaskStopV2>(added).is_err());
}
