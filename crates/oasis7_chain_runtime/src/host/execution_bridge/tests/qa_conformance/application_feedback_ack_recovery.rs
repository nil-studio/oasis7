//! Whole-process loss at actual signed ACK boundaries with genuine native memory.
use super::*;
use std::path::Path;
#[path = "application_feedback_ack_evidence.rs"]
mod evidence;
pub(super) use evidence::{
    assert_full_ack_readback, assert_initial_economic_requests, assert_recovery_delta, economy,
};
#[test]
fn real_tcp_private_memory_ack_pre_submit_crash_replays_original_only() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "memory-ack-before",
    );
}
#[test]
fn real_tcp_private_memory_ack_committed_response_loss_restart_and_content_tamper() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "memory-ack-after",
    );
}
pub(super) fn verify(
    fixture: &Fixture,
    command: &mut std::process::Command,
    root: &Path,
    output: &std::process::Output,
    mode: &str,
) {
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert_eq!(
        output.status.code(),
        Some(73),
        "actual ACK crash failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let path = root.join("native-memory-lineage.json");
    let bytes = fs::read(&path).unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let baseline_trace = fixture.world_gate.feedback_ack_trace();
    let baseline_economy = economy(fixture);
    assert_initial_economic_requests(fixture, 1, usize::from(mode == "memory-ack-after"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(root.join("feedback-ack-original-model-count.json")).unwrap()
        )
        .unwrap(),
        1
    );
    let pending = &original["provider_service_pending"]["agent-a"];
    let ack = &pending["feedback_ack"];
    assert_eq!(ack["issued"], true);
    let payload: WorldServicePayloadV1 = serde_json::from_value(ack["payload"].clone()).unwrap();
    let WorldServicePayloadV1::FeedbackAck(signed) = &payload else {
        panic!("actual signed ACK codec required")
    };
    oasis7::world_service::authority::verify_read_request("feedback_ack", signed).unwrap();
    let correlation: oasis7_client_api::world_service::RequestCorrelation =
        serde_json::from_value(ack["correlation"].clone()).unwrap();
    assert_eq!(
        oasis7::world_service::derive_correlation(correlation.key.world.clone(), &payload).unwrap(),
        correlation
    );
    let key = correlation::key_digest(&correlation.key).unwrap();
    let trace = fixture.lookup_digests.lock().unwrap().clone();
    assert_eq!(
        trace
            .iter()
            .filter(|v| **v == format!("submit:{key}"))
            .count(),
        usize::from(mode == "memory-ack-after")
    );
    let canonical = fixture.driver.lock().unwrap().execution_world.clone();
    let record = canonical
        .runtime_feedback_outbox()
        .unwrap()
        .into_iter()
        .find(|r| r.feedback_id == signed.request.feedback_id)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&record.state).unwrap() == "acked",
        mode == "memory-ack-after"
    );
    assert!(
        !original["provider_memory_store"]["entries"]
            .as_array()
            .unwrap()
            .is_empty(),
        "actual native private memory acceptance must precede ACK"
    );
    fs::write(
        root.join("world-feedback-ack-process-exit-confirmed"),
        b"parent wait returned exit73",
    )
    .unwrap();
    fs::write(root.join("world-feedback-ack-release"), b"release").unwrap();
    let marker = if mode == "memory-ack-after" {
        "world-feedback-ack-after-dropped"
    } else {
        "world-feedback-ack-before-abandoned"
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join(marker).exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    assert!(root.join(marker).exists());
    // Change actual persisted content while retaining ids/digests/signature. A new
    // application constructor must refuse it before any network/economic work.
    for kind in ["content", "entry", "signature", "signer"] {
        let mut altered = if kind == "signature" {
            evidence::recompute_tampered_private_acceptance(&original)
        } else if kind == "signer" {
            evidence::replace_ack_signer(&original)
        } else {
            original.clone()
        };
        let entries = altered["provider_memory_store"]["entries"]
            .as_array_mut()
            .unwrap();
        if kind == "content" {
            entries[0]["summary"] = serde_json::json!("tampered accepted private content");
        } else if kind == "entry" {
            entries.clear();
        }
        let negative = root.join(format!("negative-ack-{kind}.json"));
        fs::write(&negative, serde_json::to_vec(&altered).unwrap()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&negative, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let before = fixture.lookup_digests.lock().unwrap().clone();
        let output = command
            .env("PRE2_APP_ADMISSION", format!("memory-reject-ack-{kind}"))
            .env("PRE2_MEMORY_STORE", &negative)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual private-content recovery rejection failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("PRE2_NATIVE_MEMORY_TAMPER_REJECTED")
        );
        assert_eq!(
            *fixture.lookup_digests.lock().unwrap(),
            before,
            "tampered native acceptance cannot issue Submit or Lookup"
        );
        assert_eq!(
            fs::read(&negative).unwrap(),
            serde_json::to_vec(&altered).unwrap()
        );
    }
    let output = command
        .env("PRE2_APP_ADMISSION", "memory-recover")
        .env("PRE2_MEMORY_STORE", &path)
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "original ACK Pause recovery failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let disk: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        disk["provider_memory_store"], original["provider_memory_store"],
        "restart cannot consume private memory twice"
    );
    assert!(
        disk["provider_service_pending"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    assert_full_ack_readback(fixture, ack);
    assert_recovery_delta(
        fixture,
        &baseline_trace,
        ack,
        &original["provider_service_pending"]["agent-a"],
        &baseline_economy,
    );
    let trace = fixture.lookup_digests.lock().unwrap();
    assert_eq!(
        trace
            .iter()
            .filter(|v| **v == format!("submit:{key}"))
            .count(),
        1,
        "ACK must use exactly one dispatched original signed Submit across actual process loss"
    );
    assert!(
        trace.contains(&key),
        "restart must Lookup original signed ACK before deciding replay"
    );
    let record = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .runtime_feedback_outbox()
        .unwrap()
        .into_iter()
        .find(|r| r.feedback_id == signed.request.feedback_id)
        .unwrap();
    assert_eq!(serde_json::to_value(record.state).unwrap(), "acked");
    println!(
        "PRE2_PRIVATE_MEMORY_ACK_PROCESS_RECOVERY_PASSED mode={mode} actual_native_acceptance=true original_signed_submit_count=1 original_lookup=true private_memory_exact=true content_tamper_rejected=true"
    );
}

#[test]
fn real_tcp_private_memory_ack_missing_native_runner_cannot_ack() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "memory-ack-missing-runner",
    );
}
pub(super) fn assert_canonical_ack(fixture: &Fixture, root: &Path) {
    let proof: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("feedback-ack-staged.json")).unwrap()).unwrap();
    let ack = &proof["pending"]["agent-a"]["feedback_ack"];
    let payload: WorldServicePayloadV1 = serde_json::from_value(ack["payload"].clone()).unwrap();
    let WorldServicePayloadV1::FeedbackAck(signed) = payload else {
        panic!("actual signed acceptance required")
    };
    let records = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .runtime_feedback_outbox()
        .unwrap();
    assert_eq!(
        records
            .iter()
            .find(|r| r.feedback_id == signed.request.feedback_id)
            .unwrap()
            .state,
        "acked"
    );
}
