//! Recreate real application processes from intentionally altered copies of a genuine private checkpoint.
use super::*;

#[test]
fn real_tcp_native_memory_process_rejects_tampered_original_checkpoint() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "memory-crash-tamper",
    );
}

pub(super) fn verify_processes(
    fixture: &Fixture,
    bytes: &[u8],
    command: &mut std::process::Command,
    root: &std::path::Path,
) {
    let original: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let scheduler = original["provider_scheduler_pending"].as_object().unwrap();
    let settlements: Vec<SubmitIntentRequest<WorldServicePayloadV1>> = scheduler.values().filter_map(|entry| {
        let request: SubmitIntentRequest<WorldServicePayloadV1> = serde_json::from_value(serde_json::json!({"contract_version":1,"correlation":entry["correlation"],"signed_payload":entry["payload"],"deadline_unix_ms":null})).unwrap();
        matches!(&request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation, SchedulerOperationV1::SettleLease {..})).then_some(request)
    }).collect();
    assert_eq!(
        settlements.len(),
        1,
        "one genuine original Settle checkpoint"
    );
    let request = &settlements[0];
    let pending = original["provider_service_pending"].as_object().unwrap();
    assert_eq!(pending.len(), 1);
    let lease = &pending.values().next().unwrap()["cognition"]["cognition_lease"];
    match &request.signed_payload {
        WorldServicePayloadV1::Scheduler(signed) => match &signed.request.operation {
            SchedulerOperationV1::SettleLease {
                lease_id,
                consumed_amount,
            } => {
                assert_eq!(Some(lease_id.as_str()), lease["lease_id"].as_str());
                assert_eq!(Some(*consumed_amount), lease["reserved_amount"].as_u64());
            }
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
    fixture.committed(request);
    application_fairness::validate_canonical_identity(fixture);
    for kind in ["context", "response", "intents"] {
        let mut altered = original.clone();
        {
            let map = "provider_service_pending";
            let entries = altered[map].as_object_mut().unwrap();
            assert_eq!(entries.len(), 1);
            let cognition = &mut entries.values_mut().next().unwrap()["cognition"];
            match kind {
                "context" => {
                    cognition["request"]["request_context"]["goal_snapshot_digest"] =
                        serde_json::json!(format!("blake3:{}", "f".repeat(64)))
                }
                "response" => {
                    cognition["response"]["base_decision_response"]["memory_write_intents"][0]["summary"] =
                        serde_json::json!("tampered original response")
                }
                "intents" => {
                    cognition["memory_write_intents"][0]["summary"] =
                        serde_json::json!("tampered captured intent")
                }
                _ => unreachable!(),
            }
        }
        let path = root.join(format!("negative-original-{kind}.json"));
        fs::write(&path, serde_json::to_vec(&altered).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let before = fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.starts_with("submit:"))
            .count();
        let output = command
            .env("PRE2_APP_ADMISSION", format!("memory-reject-{kind}"))
            .env("PRE2_MEMORY_STORE", &path)
            .output()
            .unwrap();
        let after = fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.starts_with("submit:"))
            .count();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "actual {kind} recovery rejection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("PRE2_NATIVE_MEMORY_TAMPER_REJECTED")
        );
        assert_eq!(
            after, before,
            "invalid original {kind} cannot submit replacement work"
        );
        println!(
            "native_memory_process_negative kind={kind} actual_rejected=true new_submit_count=0"
        );
    }
    application_fairness::validate_canonical_identity(fixture);
    println!("PRE2_NATIVE_MEMORY_THREE_PROCESS_TAMPERS_REJECTED");
}
