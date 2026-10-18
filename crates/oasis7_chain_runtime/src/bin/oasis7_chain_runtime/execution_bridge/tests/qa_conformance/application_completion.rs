//! Functional completion barriers share one bounded isolated-child budget.
//! This budget prevents hangs; it is not an end-to-end latency promise.
use super::*;
use std::io::{Seek, SeekFrom};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const CHILD_BUDGET: Duration = Duration::from_secs(60);
const DEADLINE_ENV: &str = "PRE2_CONFORMANCE_COMPLETION_DEADLINE_MS";

pub(super) fn child_deadline() -> Instant {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let remaining = std::env::var(DEADLINE_ENV)
        .ok()
        .map(|value| value.parse::<u128>().expect("parent completion deadline"))
        .map(|deadline| {
            Duration::from_millis(deadline.saturating_sub(now).min(u64::MAX as u128) as u64)
        })
        .unwrap_or(CHILD_BUDGET);
    Instant::now() + remaining.min(CHILD_BUDGET)
}

pub(super) fn run_child(command: &mut Command, root: &std::path::Path) -> Output {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let inherited = std::env::var(DEADLINE_ENV).ok().map(|value| {
        value
            .parse::<u128>()
            .expect("inherited completion deadline")
    });
    let budget = launcher_budget(now.as_millis(), inherited);
    let deadline = Instant::now() + budget;
    let unix_deadline = now + budget;
    command.env(DEADLINE_ENV, unix_deadline.as_millis().to_string());
    let stdout_path = root.join("completion-child.stdout.log");
    let stderr_path = root.join("completion-child.stderr.log");
    let open = |path: &std::path::Path| {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap()
    };
    let stdout = open(&stdout_path);
    let stderr = open(&stderr_path);
    // Regular files never wait for inherited pipe descriptors to reach EOF.
    // Retain bounded tails; full logs can be preserved by the parent evidence run.
    command
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap());
    let mut child = command.spawn().expect("isolated completion child");
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .expect("kill owned isolated child after hard deadline");
            let status = child.wait().unwrap();
            eprintln!(
                "conformance_parent_hard_deadline_exceeded owned_child={} budget_ms={}",
                child.id(),
                budget.as_millis()
            );
            break status;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: retained_output(stdout),
        stderr: retained_output(stderr),
    };
    if let Ok(evidence_dir) = std::env::var("PRE2_CONFORMANCE_EVIDENCE_DIR") {
        let destination = std::path::PathBuf::from(evidence_dir).join(root.file_name().unwrap());
        fs::create_dir_all(&destination).unwrap();
        // Move the complete regular logs without an unbounded read/copy loop.
        for path in [&stdout_path, &stderr_path] {
            fs::rename(path, destination.join(path.file_name().unwrap())).unwrap();
        }
        eprintln!("conformance_full_child_logs={}", destination.display());
    }
    output
}

fn launcher_budget(now_ms: u128, inherited_deadline_ms: Option<u128>) -> Duration {
    // A nested QA launcher must stop its App child before its own parent can
    // stop it. Keep ten seconds for Node, listeners and fixture-thread cleanup.
    const PARENT_CLEANUP_RESERVE: Duration = Duration::from_secs(10);
    inherited_deadline_ms
        .map(|deadline| {
            Duration::from_millis(deadline.saturating_sub(now_ms).min(u64::MAX as u128) as u64)
                .saturating_sub(PARENT_CLEANUP_RESERVE)
                .min(CHILD_BUDGET)
        })
        .unwrap_or(CHILD_BUDGET)
}

#[test]
fn nested_completion_launcher_preserves_parent_cleanup_budget() {
    assert_eq!(launcher_budget(1000, None), CHILD_BUDGET);
    assert_eq!(launcher_budget(1000, Some(61000)), Duration::from_secs(50));
    assert_eq!(launcher_budget(21000, Some(61000)), Duration::from_secs(30));
    assert_eq!(launcher_budget(60000, Some(61000)), Duration::ZERO);
    assert_eq!(launcher_budget(62000, Some(61000)), Duration::ZERO);
}

fn retained_output(mut file: fs::File) -> Vec<u8> {
    const MAX_RETAINED: u64 = 2 * 1024 * 1024;
    let captured_len = file.metadata().unwrap().len();
    file.seek(SeekFrom::Start(captured_len.saturating_sub(MAX_RETAINED)))
        .unwrap();
    let mut bytes = Vec::new();
    file.take(captured_len.min(MAX_RETAINED))
        .read_to_end(&mut bytes)
        .unwrap();
    eprintln!(
        "conformance_child_output captured_bytes={captured_len} retained_bytes={} retained_blake3={} truncated={}",
        bytes.len(),
        blake3::hash(&bytes),
        captured_len > MAX_RETAINED
    );
    bytes
}

pub(super) fn has_failure(summary: &serde_json::Value) -> bool {
    ["agent_service_pump_error", "hosted_service_memory_failure"]
        .iter()
        .any(|key| !summary[*key].is_null())
        || summary["terminal_states"]["agent-a"]["status"]
            .as_str()
            .is_some_and(|status| status != "committed")
}

pub(super) struct CompletionWatch {
    deadline: Instant,
    started: Instant,
    previous: serde_json::Value,
}
impl CompletionWatch {
    pub(super) fn new() -> Self {
        Self {
            deadline: child_deadline(),
            started: Instant::now(),
            previous: serde_json::Value::Null,
        }
    }
    pub(super) fn finished(&mut self, summary: &serde_json::Value, complete: bool) -> bool {
        self.finished_after_injected_rollback(summary, complete, false)
    }
    pub(super) fn finished_after_injected_rollback(
        &mut self,
        summary: &serde_json::Value,
        complete: bool,
        rollback_observed: bool,
    ) -> bool {
        let progress = serde_json::json!({
            "phase": summary["hosted_service_phase"], "inflight": summary["hosted_service_inflight"],
            "terminal": summary["terminal_states"], "pending_intents": summary["pending_intent_count"],
            "pending_actions": summary["pending_action_count"]
        });
        if progress != self.previous {
            println!(
                "conformance_completion_progress elapsed_ms={} original_phase={progress}",
                self.started.elapsed().as_millis()
            );
            self.previous = progress;
        }
        complete
            || unrecoverable_failure(summary, rollback_observed)
            || Instant::now() >= self.deadline
    }
}

fn unrecoverable_failure(summary: &serde_json::Value, rollback_observed: bool) -> bool {
    if rollback_observed
        && summary["agent_service_pump_error"]
            == "Agent service preparation is blocked; original work remains pending"
    {
        // Only the QA-injected, observed atomic-write rollback may retry this
        // specific preparation error. Preserve terminal and memory failures.
        let mut failure = summary.clone();
        failure["agent_service_pump_error"] = serde_json::Value::Null;
        has_failure(&failure)
    } else {
        has_failure(summary)
    }
}

#[test]
fn observed_injected_rollback_only_allows_its_preparation_error() {
    let mut summary = serde_json::json!({
        "agent_service_pump_error": "Agent service preparation is blocked; original work remains pending"
    });
    assert!(unrecoverable_failure(&summary, false));
    assert!(!unrecoverable_failure(&summary, true));
    summary["terminal_states"] = serde_json::json!({"agent-a": {"status": "rejected"}});
    assert!(unrecoverable_failure(&summary, true));
    summary["terminal_states"] = serde_json::Value::Null;
    summary["hosted_service_memory_failure"] = serde_json::json!("memory failure");
    assert!(unrecoverable_failure(&summary, true));
    summary["hosted_service_memory_failure"] = serde_json::Value::Null;
    summary["agent_service_pump_error"] = serde_json::json!("unexpected error");
    assert!(unrecoverable_failure(&summary, true));
}

pub(super) fn canonical_complete(summary: &serde_json::Value) -> bool {
    let terminal = &summary["terminal_states"]["agent-a"];
    terminal["status"] == "committed"
        && terminal["request_digest"]
            .as_str()
            .is_some_and(|digest| !digest.is_empty())
        && summary["pending_intent_count"] == 0
        && summary["pending_action_count"] == 0
}

pub(super) fn save_completion(summary: &serde_json::Value) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    fs::write(
        root.join("canonical-completion.json"),
        serde_json::to_vec(summary).unwrap(),
    )
    .unwrap();
}

pub(super) fn verify_canonical_completion(fixture: &Fixture, root: &std::path::Path) {
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("canonical-completion.json")).unwrap()).unwrap();
    assert!(
        canonical_complete(&summary),
        "child canonical completion barrier missing"
    );
    let digest = summary["terminal_states"]["agent-a"]["request_digest"]
        .as_str()
        .unwrap();
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let results = world.capability_revocation_state().world_service_results.values()
        .filter_map(|value| serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok())
        .filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Cognition(signed) if signed.request.request.request_digest == digest)).collect::<Vec<_>>();
    assert_eq!(
        results.len(),
        1,
        "one canonical Cognition result for original request"
    );
    let result = &results[0];
    assert!(result.rejected.is_none());
    let response = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: result.request.correlation.key.clone(),
            },
            result.request.signed_payload.clone(),
        )
        .unwrap();
    let IntentOutcome::Committed { receipt, .. } = response.outcome else {
        panic!("original canonical Lookup receipt missing");
    };
    assert_eq!(receipt["commit_record"]["request_digest"], digest);
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut request = fixture.view(None);
    request.scope_id = "agent:agent-a".into();
    let view = client.read_view(request).unwrap();
    assert_eq!(
        view.projection()
            .cognition_leases
            .iter()
            .filter(|lease| lease.request_digest == digest
                && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled)
            .count(),
        1,
        "one settled original lease"
    );
    println!(
        "conformance_original_completion request_digest={digest} canonical_result_count=1 settled_lease_count=1 original_correlation={}",
        serde_json::to_string(&result.request.correlation).unwrap()
    );
}

/// Failure-only diagnostics deliberately exclude identities, credentials and memory.
/// Unknown strings stay classified; the complete private summary is never logged.
pub(super) fn hosted_failure_diagnostic(summary: &serde_json::Value) -> serde_json::Value {
    let phase = match summary["hosted_service_phase"].as_str() {
        Some(
            value @ ("act" | "act_view" | "act_settled_view" | "act_settle" | "handoff_wake"
            | "handoff_view" | "feedback" | "feedback_ack" | "feedback_ack_view"
            | "finalize"),
        ) => value,
        None => "absent",
        Some(_) => "unknown",
    };
    let pump_error = match summary["agent_service_pump_error"].as_str() {
        Some("Agent service result could not be applied; original work remains pending") => {
            "apply_blocked"
        }
        Some("Agent service preparation is blocked; original work remains pending") => {
            "prepare_blocked"
        }
        None => "absent",
        Some(_) => "other",
    };
    let terminal = &summary["terminal_states"]["agent-a"];
    let status = match terminal["status"].as_str() {
        Some(
            value @ ("committed" | "completed" | "rejected" | "failed" | "check_pending"
            | "check_failed"),
        ) => value,
        None => "absent",
        Some(_) => "unknown",
    };
    let io = &summary["io_lifecycle"];
    let mut lifecycle = serde_json::Map::new();
    for field in [
        "queued_count",
        "executing_count",
        "returned_count",
        "consumed_count",
        "applied_count",
        "execute_error_count",
        "apply_error_count",
        "same_phase_launch_count",
        "current_age_us",
        "last_execute_us",
        "max_execute_us",
        "total_execute_us",
        "total_return_to_consume_us",
        "max_return_to_consume_us",
        "max_same_phase_launch_count",
    ] {
        lifecycle.insert(field.into(), serde_json::json!(io[field].as_u64()));
    }
    lifecycle.insert(
        "state".into(),
        serde_json::json!(
            io["state"]
                .as_str()
                .filter(|value| matches!(
                    *value,
                    "queued" | "executing" | "returned" | "consumed" | "applied" | "absent"
                ))
                .unwrap_or("unknown")
        ),
    );
    lifecycle.insert(
        "operation".into(),
        serde_json::json!(
            io["operation"]
                .as_str()
                .filter(|value| matches!(
                    *value,
                    "submit" | "lookup" | "view" | "feedback" | "metadata" | "absent"
                ))
                .unwrap_or("unknown")
        ),
    );
    serde_json::json!({
        "io_lifecycle": lifecycle,
        "phase": phase,
        "pump_error_class": pump_error,
        "terminal_status": status,
        "terminal_rejection_present": !terminal["reject_reason"].is_null(),
        "memory_failure_present": !summary["hosted_service_memory_failure"].is_null(),
        "io_inflight": !summary["hosted_service_inflight"].is_null(),
        "native_runner_present": summary["native_runner_present"].as_bool(),
        "native_model_call_count": summary["native_model_call_count"].as_u64(),
        "pending_intent_count": summary["pending_intent_count"].as_u64(),
        "pending_action_count": summary["pending_action_count"].as_u64(),
        "pending_wake_count": summary["pending_wake_ids"].as_array().map(Vec::len),
        "mirrored_lease_count": summary["mirrored_lease_count"].as_u64(),
    })
}

#[test]
fn hosted_failure_diagnostic_does_not_echo_private_summary() {
    let secret = "PRIVATE_KEY_NONCE_GOAL_MEMORY_PROVIDER_PAYLOAD";
    let mut summary = serde_json::json!({
        "hosted_service_phase": secret, "agent_service_pump_error": secret,
        "terminal_states": {"agent-a": {"status": secret, "reject_reason": secret}},
        "hosted_service_memory_failure": secret, "hosted_service_inflight": secret,
        "native_runner_present": secret, "native_model_call_count": secret,
        "pending_intent_count": secret, "pending_action_count": 3,
        "pending_wake_ids": [secret], "mirrored_lease_count": secret,
        "memory_store": secret, "private_key": secret,
        "io_lifecycle": {"state": secret, "operation": secret, "queued_count": secret, "private_payload": secret},
    });
    let diagnostic = hosted_failure_diagnostic(&summary);
    assert_eq!(diagnostic["phase"], "unknown");
    assert_eq!(diagnostic["pump_error_class"], "other");
    assert_eq!(diagnostic["terminal_status"], "unknown");
    assert_eq!(diagnostic["pending_action_count"], 3);
    assert_eq!(diagnostic["pending_wake_count"], 1);
    assert!(diagnostic["pending_intent_count"].is_null());
    let panic = std::panic::catch_unwind(|| panic!("Hosted completion failed: {diagnostic}"))
        .expect_err("exercise the actual assertion diagnostic format");
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(!message.contains(secret));
    assert!(!message.contains("memory_store"));
    assert!(!message.contains("private_key"));
    summary["terminal_states"]["agent-a"]["status"] = serde_json::json!("failed");
    let failed_diagnostic = hosted_failure_diagnostic(&summary);
    assert_eq!(failed_diagnostic["terminal_status"], "failed");
    let failed_message = failed_diagnostic.to_string();
    assert!(!failed_message.contains(secret));
    assert!(!failed_message.contains("memory_store"));
    assert!(!failed_message.contains("private_key"));

    summary["hosted_service_phase"] = serde_json::json!("handoff_wake");
    assert_eq!(hosted_failure_diagnostic(&summary)["phase"], "handoff_wake");
    summary["hosted_service_phase"] = serde_json::json!("handoff_view");
    assert_eq!(hosted_failure_diagnostic(&summary)["phase"], "handoff_view");
}
