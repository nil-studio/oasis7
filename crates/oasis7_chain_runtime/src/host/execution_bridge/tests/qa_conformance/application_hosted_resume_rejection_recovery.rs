//! Real process death after authenticated rejection compensation, before local cleanup.
use super::*;
use std::path::Path;
#[test]
fn real_tcp_hosted_rejected_resume_cleanup_crash_recovers_original_identity() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "resume-rejection-crash",
    );
}
pub(super) fn assert_original_terminal(fixture: &Fixture, root: &Path) {
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join(application_hosted_wait_rejection::STORE)).unwrap(),
    )
    .unwrap();
    let terminal = &checkpoint["provider_terminal_states"]["agent-a"];
    assert_eq!(terminal["status"], "rejected");
    assert_eq!(terminal["reject_reason"], "canonical_resume_rejected");
    assert!(terminal["feedback_id"].is_null());
    assert_eq!(terminal.get("feedback"), Some(&serde_json::Value::Null));
    let original_bytes = fs::read(root.join("resume-rejection-crash-checkpoint.json"))
        .or_else(|_| fs::read(root.join("resume-rejection-original-checkpoint.json")))
        .unwrap();
    let original_checkpoint: serde_json::Value = serde_json::from_slice(&original_bytes).unwrap();
    assert_eq!(
        terminal["request_digest"],
        original_checkpoint["hosted_resume"]["context"]["request_context"]["request_digest"]
    );
    for field in ["agent_session_id", "agent_turn_id", "decision_request_id"] {
        assert_eq!(
            terminal[field],
            original_checkpoint["hosted_resume"]["context"]["request_context"][field]
        );
    }
    assert!(checkpoint["hosted_restored_resume"].is_null());
    assert_eq!(
        checkpoint["provider_memory_store"],
        original_checkpoint["provider_memory_store"]
    );
    assert!(checkpoint["hosted_resume"].is_null());
    let selected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-selected.json")).unwrap()).unwrap();
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let continuations: Vec<oasis7_runtime::AgentContinuation> =
        serde_json::from_value(world.cognition_continuations()).unwrap();
    assert!(continuations.iter().any(|c| c.continuation_id
        == selected["continuation_id"].as_str().unwrap()
        && c.status == oasis7_runtime::ContinuationStatusV1::Rejected));
    assert!(world.active_cognition_continuations().unwrap().is_empty());
    assert!(world.cognition_in_flight_wakes().unwrap().is_empty());
}
pub(super) fn recover_parent(
    fixture: &Fixture,
    root: &Path,
    command: &mut std::process::Command,
    first: &std::process::Output,
    worker: thread::JoinHandle<SubmitIntentRequest<WorldServicePayloadV1>>,
) {
    let original = worker.join().unwrap();
    application_hosted_wait_rejection::secure_artifact(
        fixture,
        root,
        first,
        "resume-rejection-crash",
    );
    assert_eq!(first.status.code(), Some(73));
    let bytes = fs::read(root.join("resume-rejection-crash-checkpoint.json")).unwrap();
    assert_eq!(
        bytes,
        fs::read(root.join(application_hosted_wait_rejection::STORE)).unwrap()
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let pending = checkpoint["provider_scheduler_pending"].as_object().unwrap().values().find(|v| {
        serde_json::from_value::<WorldServicePayloadV1>(v["payload"].clone()).is_ok_and(|payload| matches!(payload, WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation, SchedulerOperationV1::TransitionContinuation { to: oasis7_runtime::ContinuationStatusV1::Rejected, .. })))
    }).expect("actual fixed signed rejection compensation checkpoint");
    let lookup = LookupIntentRequest {
        contract_version: 1,
        key: serde_json::from_value(pending["correlation"]["key"].clone()).unwrap(),
    };
    let key = correlation::key_digest(&lookup.key).unwrap();
    let count = || {
        fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| *entry == &key)
            .count()
    };
    let before = count();
    application_hosted_wait_rejection::private_write(
        &root.join("resume-rejection-release"),
        b"old process exited 73",
    )
    .unwrap();
    command.env_remove("PRE2_RESUME_REJECTION_FS_ROOT");
    command.env("PRE2_APP_ADMISSION", "resume-rejection-recover");
    command.env("PRE2_RESUME_REJECTION_RECOVERY_TRACE", "1");
    let output = command.output().unwrap();
    let after = count();
    assert_eq!(
        fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| **entry == format!("submit:{key}"))
            .count(),
        1,
        "original signed compensation must never be resubmitted"
    );
    application_hosted_wait_rejection::secure_artifact(
        fixture,
        root,
        &output,
        "resume-rejection-recovered",
    );
    if !output.status.success() {
        let http = fixture.finish_http_workers();
        let gate_join = fixture.world_gate.rejected_resume_join_proof();
        println!(
            "resume_rejection_recovery_failed_strict_cleanup http={http:?} gate_join={gate_join}"
        );
        assert!(
            http.is_ok() && gate_join,
            "actual recovery failure cleanup must join every HTTP/gate worker"
        );
    }
    assert!(output.status.success());
    assert!(
        after > before,
        "new process must authenticate Lookup of original compensation before parent final queries"
    );
    assert_original_terminal(fixture, root);
    println!(
        "PRE2_REJECTED_RESUME_PROCESS_RECOVERY_PASSED original_compensation_lookup_before={before} after={after} true_old_exit=73"
    );
    application_hosted_resume_rejection::finish(
        fixture,
        root,
        &output,
        thread::spawn(move || original),
    );
}
