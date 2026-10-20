//! Real old-process exit after canonical final Consume and minimum View, before local persist.
use super::*;
use std::path::{Path, PathBuf};
const STORE: &str = "hosted-final-budget-private-lineage.json";
#[test]
fn real_tcp_hosted_final_budget_cleanup_crash_recovers_original_consume() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "hosted-final-budget-crash",
    );
}
pub(super) fn start_crash_watcher(root: PathBuf) {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !root.join("final-budget-before-persist").exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            root.join("final-budget-before-persist").exists(),
            "actual final Consume persistence boundary absent"
        );
        let bytes = fs::read(root.join(STORE)).unwrap();
        let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(checkpoint["hosted_resume"]["stage"], "budget_finalize");
        assert_eq!(checkpoint["hosted_resume"]["final_budget"], true);
        assert!(checkpoint["hosted_resume"]["commit"].is_object());
        application_hosted_wait_rejection::private_write(
            &root.join("final-budget-crash-checkpoint.json"),
            &bytes,
        )
        .unwrap();
        println!(
            "PRE2_FINAL_BUDGET_CRASH_BOUNDARY true_process_exit=73 checkpoint_blake3={}",
            blake3::hash(&bytes)
        );
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        std::io::stderr().flush().unwrap();
        std::process::exit(73);
    });
}
pub(super) fn assert_original_terminal(fixture: &Fixture, root: &Path) {
    let checkpoint: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(STORE)).unwrap()).unwrap();
    let original = fs::read(root.join("final-budget-crash-checkpoint.json"))
        .or_else(|_| fs::read(root.join("final-budget-original-checkpoint.json")))
        .unwrap();
    let original: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let terminal = &checkpoint["provider_terminal_states"]["agent-a"];
    assert_eq!(terminal["status"], "completed");
    assert!(terminal["feedback_id"].is_null());
    assert_eq!(terminal.get("feedback"), Some(&serde_json::Value::Null));
    assert!(terminal["reject_reason"].is_null());
    for field in [
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "request_digest",
    ] {
        assert_eq!(
            terminal[field],
            original["hosted_resume"]["context"]["request_context"][field]
        );
    }
    assert!(checkpoint["hosted_resume"].is_null());
    assert_eq!(
        checkpoint["provider_memory_store"],
        original["provider_memory_store"]
    );
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let all: Vec<oasis7_runtime::AgentContinuation> =
        serde_json::from_value(world.cognition_continuations()).unwrap();
    let selected: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-final-budget-selected.json")).unwrap())
            .unwrap();
    assert!(all.iter().any(
        |c| c.continuation_id == selected["continuation_id"].as_str().unwrap()
            && c.status == oasis7_runtime::ContinuationStatusV1::Completed
            && c.remaining_budget.value == 0
    ));
    assert!(world.cognition_in_flight_wakes().unwrap().is_empty());
}
fn preserve_private_checkpoint(root: &Path, phase: &str) {
    let evidence = temp_dir("pre2-final-budget-private");
    fs::create_dir_all(&evidence).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&evidence, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for name in [
        STORE,
        "final-budget-crash-checkpoint.json",
        "hosted-final-budget-selected.json",
        "hosted-final-budget-summary.json",
    ] {
        if let Ok(bytes) = fs::read(root.join(name)) {
            let file = evidence.join(name);
            application_hosted_wait_rejection::private_write(&file, &bytes).unwrap();
            println!(
                "final_budget_private_artifact phase={phase} path={} bytes={} blake3={}",
                file.display(),
                bytes.len(),
                blake3::hash(&bytes)
            );
        }
    }
}
pub(super) fn recover_parent(
    fixture: &Fixture,
    root: &Path,
    command: &mut std::process::Command,
    first: &std::process::Output,
) -> std::process::Output {
    application_hosted_wait_rejection::secure_artifact(fixture, root, first, "final-budget-crash");
    preserve_private_checkpoint(root, "crash");
    assert_eq!(first.status.code(), Some(73));
    let bytes = fs::read(root.join("final-budget-crash-checkpoint.json")).unwrap();
    assert_eq!(bytes, fs::read(root.join(STORE)).unwrap());
    let checkpoint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let pending=checkpoint["provider_scheduler_pending"].as_object().unwrap().values().find(|v|serde_json::from_value::<WorldServicePayloadV1>(v["payload"].clone()).is_ok_and(|payload|matches!(payload,WorldServicePayloadV1::Scheduler(s) if matches!(s.request.operation,SchedulerOperationV1::ConsumeContinuationBudget {budget_spent:1,..})))).expect("actual fixed signed final Consume checkpoint");
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
    command.env_remove("PRE2_FINAL_BUDGET_FS_ROOT");
    command.env("PRE2_APP_ADMISSION", "hosted-final-budget-recover");
    let output = command.output().unwrap();
    let after = count();
    application_hosted_wait_rejection::secure_artifact(
        fixture,
        root,
        &output,
        "final-budget-recovered",
    );
    preserve_private_checkpoint(root, "recovered");
    if !output.status.success() {
        fixture.finish_http_workers().unwrap();
    }
    assert!(output.status.success(), "actual recovery child failed");
    assert!(
        after > before,
        "recovery child must Lookup original Consume before parent queries"
    );
    assert_eq!(
        fixture
            .lookup_digests
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| **entry == format!("submit:{key}"))
            .count(),
        1
    );
    assert_original_terminal(fixture, root);
    println!(
        "PRE2_FINAL_BUDGET_PROCESS_RECOVERY_PASSED actual_old_exit=73 consume_lookup_before={before} after={after} original_submit=1"
    );
    output
}
