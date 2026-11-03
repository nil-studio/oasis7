//! Real actor -> canonical known Failure/Settle -> durable feedback release.
//! Companion queued tests isolate ACK write failure; this case runs the service.
use super::*;
use oasis7_viewer_runtime::viewer::ViewerRuntimeLiveServer;
#[test]
fn real_tcp_hosted_known_provider_failure_settles_original_once() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "hosted-known-failure",
    );
}
pub(super) fn verify(client: &RemoteWorldServiceClient) {
    let root = std::path::PathBuf::from(std::env::var("PRE2_METADATA_DIR").unwrap());
    application_fresh::preflight(client, &root);
    let server = ViewerRuntimeLiveServer::new(application_hosted::server_config(
        client,
        true,
        Duration::from_millis(200),
        Some(root.join("hosted-wait-private-lineage.json")),
    ))
    .unwrap();
    let shared = Arc::new(Mutex::new(server));
    let mut session = application_stream_boundaries::Session::start(&shared, false);
    assert!(session.warm());
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"play"},"request_id":1291}))
        .unwrap();
    session.send(serde_json::json!({"type":"hello_v2","client":"PRE2 known failure","version":2,"capabilities":[]})).unwrap();
    session
        .send(serde_json::json!({"type":"request_snapshot"}))
        .unwrap();
    assert!(session.snapshot_ordered(Duration::from_secs(3), true));
    let mut watch = application_completion::CompletionWatch::new();
    let mut capture_drain: Option<(u64, serde_json::Value)> = None;
    let mut capture_seen = false;
    let mut capture_proven: Option<serde_json::Value> = None;
    let summary = loop {
        session.snapshot(Duration::from_millis(25));
        let summary = shared
            .lock()
            .unwrap()
            .test_canonical_provider_diagnostic_summary();
        let done = summary["terminal_states"]["agent-a"]["status"] == "failed"
            && summary["publication_pending"] == false
            && summary["publication_capture_failed"] == false
            && summary["hosted_service_phase"].is_null()
            && durable_original_cleanup_complete(&root, &summary);
        let mut watched = summary.clone();
        let exact_prepare = summary["agent_service_pump_error"]
            == "Agent service preparation is blocked; original work remains pending";
        let pending = summary["publication_pending"] == true
            && summary["publication_capture_failed"] == false;
        if !capture_seen && exact_prepare && pending {
            if let Some(ticket) = summary["publication_ticket"].as_u64().filter(|v| *v != 0) {
                capture_seen = true;
                capture_drain = Some((ticket, summary.clone()));
            }
        }
        if let Some((ticket, original_error)) = &capture_drain {
            if exact_prepare && pending && summary["publication_ticket"].as_u64() == Some(*ticket) {
                // Only await this captured write under the existing watch deadline.
                // This is not a classified business outcome or an ACK.
                let _ = expected_capture_transition(&root, original_error);
                watched["agent_service_pump_error"] = serde_json::Value::Null;
            } else if summary["publication_pending"] == false {
                assert!(
                    expected_capture_transition(&root, original_error),
                    "ACKed known-failure capture did not preserve the exact original"
                );
                if exact_prepare {
                    watched["agent_service_pump_error"] = serde_json::Value::Null;
                }
                capture_proven = Some(original_error.clone());
                capture_drain = None;
            }
            // A changed pending ticket or another error is not masked.
        }
        if let Some(original_error) = &capture_proven {
            if exact_prepare && expected_original_recovery(&root, &summary, original_error) {
                // The negative scenario's first capture error remains recorded until
                // a real result is applied. Preserve its proven original across all
                // orphan stages; the positive watch's generic failure rule is inapplicable.
                watched["agent_service_pump_error"] = serde_json::Value::Null;
            } else if !exact_prepare {
                capture_proven = None;
            }
        }
        if expected_failed_publication(&root, &summary)
            || durable_original_cleanup_complete(&root, &summary)
        {
            watched["terminal_states"]["agent-a"]["status"] = serde_json::json!("committed");
        }
        if watch.finished_after_injected_rollback(&watched, done, false) {
            break summary;
        }
    };
    session
        .send(serde_json::json!({"type":"live_control","mode":{"mode":"pause"},"request_id":1292}))
        .unwrap();
    session.snapshot(Duration::from_secs(2));
    session.close().unwrap();
    save_private_summary(&root, &summary);
    assert_eq!(
        summary["terminal_states"]["agent-a"]["reject_reason"],
        "cognition_failed",
        "known failure did not reach canonical terminal: {}",
        application_completion::hosted_failure_diagnostic(&summary)
    );
    println!("PRE2_KNOWN_FAILURE_CANONICAL_PASSED");
}
pub(super) fn report(
    fixture: &Fixture,
    root: &std::path::Path,
    output: &std::process::Output,
    metadata: &provider_metadata::MetadataServer,
) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("PRE2_KNOWN_FAILURE_CANONICAL_PASSED")
    );
    assert_eq!(
        metadata.decision_count.load(Ordering::SeqCst),
        1,
        "original model request must not be re-dispatched"
    );
    let origin: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-origin.json")).unwrap()).unwrap();
    let digest = origin["request_digest"].as_str().unwrap();
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut read = fixture.view(None);
    read.scope_id = "agent:agent-a".into();
    let view = client.read_view(read).unwrap();
    let lease = view
        .projection()
        .cognition_leases
        .iter()
        .find(|lease| {
            lease.request_digest == digest && lease.lease_id == origin["lease_id"].as_str().unwrap()
        })
        .unwrap();
    assert_eq!(
        lease.status,
        oasis7_runtime::CognitionLeaseStatusV1::Settled
    );
    assert_eq!(lease.settled_amount, 1);
    assert_eq!(lease.net_amount, 1);
    assert_eq!(lease.compensated_amount, 0);
    assert_eq!(lease.refunded_amount, 0);
    assert_eq!(lease.released_amount, 0);
    let guard = fixture.driver.lock().unwrap();
    let results = guard
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| {
            serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok()
        })
        .filter(|result| result.rejected.is_none())
        .collect::<Vec<_>>();
    drop(guard);
    let failures = results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation, SchedulerOperationV1::ProviderFailure { request, reason } if request.request_digest.to_string() == digest && reason == "cognition_failed"))).count();
    assert_eq!(
        failures, 1,
        "one actual committed Failure for original request"
    );
    let settles = results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation, SchedulerOperationV1::SettleLease { lease_id, consumed_amount } if lease_id == &lease.lease_id && *consumed_amount == 1))).count();
    assert_eq!(settles, 1);
    let durable: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("hosted-wait-private-lineage.json")).unwrap())
            .unwrap();
    assert!(
        durable["provider_active_turns"].get("agent-a").is_none(),
        "actor owner retires only after terminal publication"
    );
    let feedback = &durable["provider_terminal_states"]["agent-a"]["feedback"];
    assert_eq!(feedback["status"], "failed");
    assert_eq!(feedback["request_digest"], digest);
    assert_eq!(feedback["agent_session_id"], lease.agent_session_id);
    assert_eq!(feedback["agent_turn_id"], lease.agent_turn_id);
    assert_eq!(feedback["decision_request_id"], lease.decision_request_id);
    assert!(!feedback["feedback_id"].as_str().unwrap().is_empty());
    assert!(
        durable["provider_raw_outcomes"]
            .as_array()
            .is_none_or(|raw| raw.is_empty())
    );
}

// A failed watch must leave its complete private evidence before assertions.
// Serialization is bounded; stdout receives only the existing allowlisted diagnostic.
fn save_private_summary(root: &std::path::Path, summary: &serde_json::Value) {
    save_private_named(root, "known-failure-terminal.json", summary);
}
fn save_private_named(root: &std::path::Path, name: &str, summary: &serde_json::Value) {
    use std::io::Write;
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > (1024 * 1024usize).saturating_sub(self.0.len()) {
                return Err(std::io::Error::other(
                    "private known failure summary exceeds cap",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut encoded = Bounded(Vec::new());
    serde_json::to_writer(&mut encoded, summary).expect("bounded private summary encode failed");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(root.join(name))
        .expect("private known failure summary create failed");
    file.write_all(&encoded.0)
        .expect("private known failure summary write failed");
    file.sync_all()
        .expect("private known failure summary sync failed");
}

fn expected_capture_transition(root: &std::path::Path, summary: &serde_json::Value) -> bool {
    fn read_bounded(path: std::path::PathBuf) -> Option<serde_json::Value> {
        use std::io::Read;
        let file = fs::File::open(path).ok()?;
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 1024 * 1024 {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }
    let Some(durable) = read_bounded(root.join("hosted-wait-private-lineage.json")) else {
        return false;
    };
    let Some(origin) = read_bounded(root.join("hosted-wait-origin.json")) else {
        return false;
    };
    let matches = matches_capture_transition(summary, &durable, &origin);
    if !root.join("known-failure-capture-probe.json").exists() {
        let raw = &durable["provider_raw_outcomes"][0];
        let active = &durable["provider_recovery_pending"]["agent-a"]["active"];
        save_private_named(
            root,
            "known-failure-capture-probe.json",
            &serde_json::json!({
                "matches":matches,"summary":summary,"durable":durable,"origin":origin,
                "checks":{
                    "schema3":durable["schema_version"] == 3,
                    "raw_count_one":durable["provider_raw_outcomes"].as_array().is_some_and(|v|v.len()==1),
                    "same_turn_context":raw["prepared_context"] == active["turn_context"],
                    "same_request_context":raw["prepared_request_context"] == active["request_context"],
                    "same_active":durable["provider_active_turns"]["agent-a"] == *active,
                    "same_lease":raw["cognition_lease"] == durable["provider_cognition_leases"]["agent-a"],
                    "same_lease_id":raw["cognition_lease"]["lease_id"] == origin["lease_id"],
                    "same_session":raw["prepared_request_context"]["agent_session_id"] == origin["agent_session_id"],
                    "same_turn":raw["prepared_request_context"]["agent_turn_id"] == origin["agent_turn_id"],
                    "same_request_id":raw["prepared_request_context"]["decision_request_id"] == origin["decision_request_id"],
                    "same_digest":raw["prepared_request_context"]["request_digest"] == origin["request_digest"]
                }
            }),
        );
    }

    if matches {
        // These actual-original negative probes prevent widening this local exception.
        let mut wrong = origin.clone();
        wrong["request_digest"] = serde_json::json!("wrong-original");
        assert!(!matches_capture_transition(summary, &durable, &wrong));
        let mut wrong = durable.clone();
        wrong["provider_raw_outcomes"][0]["feedback"] =
            serde_json::json!({"ProviderError":{"code":"other"}});
        assert!(!matches_capture_transition(summary, &wrong, &origin));
        let mut wrong = summary.clone();
        wrong["agent_service_pump_error"] = serde_json::json!("different failure");
        assert!(!matches_capture_transition(&wrong, &durable, &origin));
    }
    matches
}
fn matches_capture_transition(
    summary: &serde_json::Value,
    durable: &serde_json::Value,
    origin: &serde_json::Value,
) -> bool {
    if summary["agent_service_pump_error"]
        != "Agent service preparation is blocked; original work remains pending"
        || summary["publication_pending"] != true
        || summary["publication_capture_failed"] != false
        || durable["schema_version"] != 3
        || origin["reserve_committed"] != true
        || origin["signed_reserved_lease"] != true
        || origin["prefix_exact_request"] != true
    {
        return false;
    }
    let recovery = &durable["provider_recovery_pending"]["agent-a"];
    if recovery["reason"]
        != "executor_failed_after_dispatch:known_provider_failure:hosted provider outcome missing response artifact"
        || durable["provider_active_turns"]["agent-a"] != recovery["active"]
    {
        return false;
    }
    let Some(raw) = durable["provider_raw_outcomes"].as_array() else {
        return false;
    };
    if raw.len() != 1 {
        return false;
    }
    let raw = &raw[0];
    if raw["agent_id"] != "agent-a"
        || raw["cognition_lease"] != durable["provider_cognition_leases"]["agent-a"]
        || raw["feedback"] != serde_json::json!({"ProviderError":{"code":"qa_known_nonretryable"}})
        || !raw["prepared_response_context"].is_null()
        || raw["prepared_context"] != recovery["active"]["turn_context"]
        || raw["prepared_request_context"] != recovery["active"]["request_context"]
        || raw["cognition_lease"]["agent_id"] != "agent-a"
        || raw["cognition_lease"]["lease_id"] != origin["lease_id"]
        || !origin["lease_id"].as_str().is_some_and(|v| !v.is_empty())
    {
        return false;
    }
    [
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "request_digest",
    ]
    .iter()
    .all(|key| {
        origin[*key].as_str().is_some_and(|v| !v.is_empty())
            && raw["prepared_request_context"][*key] == origin[*key]
            && raw["cognition_lease"][*key] == origin[*key]
    })
}

fn expected_failed_publication(root: &std::path::Path, summary: &serde_json::Value) -> bool {
    let terminal = &summary["terminal_states"]["agent-a"];
    if summary["publication_capture_failed"] != false
        || terminal["status"] != "failed"
        || terminal["reject_reason"] != "cognition_failed"
    {
        return false;
    }
    fn read(path: std::path::PathBuf) -> Option<serde_json::Value> {
        use std::io::Read;
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 1024 * 1024 {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }
    let Some(origin) = read(root.join("hosted-wait-origin.json")) else {
        return false;
    };
    let Some(durable) = read(root.join("hosted-wait-private-lineage.json")) else {
        return false;
    };
    let lease = &durable["provider_cognition_leases"]["agent-a"];
    let feedback = &terminal["feedback"];
    if lease["lease_id"] != origin["lease_id"]
        || durable["provider_recovery_pending"]["agent-a"]["reason"]
            != "executor_failed_after_dispatch:known_provider_failure:hosted provider outcome missing response artifact"
        || durable["provider_raw_outcomes"][0]["feedback"]
            != serde_json::json!({"ProviderError":{"code":"qa_known_nonretryable"}})
        || feedback["status"] != "failed"
        || feedback["reject_reason"] != "cognition_failed"
        || !feedback["feedback_id"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
        || terminal["feedback_id"] != feedback["feedback_id"]
    {
        return false;
    }
    [
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "request_digest",
    ]
    .iter()
    .all(|key| {
        origin[*key].as_str().is_some_and(|v| !v.is_empty())
            && terminal[*key] == origin[*key]
            && feedback[*key] == origin[*key]
            && lease[*key] == origin[*key]
            && durable["provider_raw_outcomes"][0]["prepared_request_context"][*key] == origin[*key]
    })
}

fn expected_original_recovery(
    root: &std::path::Path,
    summary: &serde_json::Value,
    original_error: &serde_json::Value,
) -> bool {
    use std::io::Read;
    if summary["publication_capture_failed"] != false
        || !(summary["hosted_service_phase"].is_null()
            || summary["hosted_service_phase"] == "orphan")
    {
        return false;
    }
    let mut bytes = Vec::new();
    let Ok(file) = fs::File::open(root.join("hosted-wait-private-lineage.json")) else {
        return false;
    };
    if file.take(1024 * 1024 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > 1024 * 1024 {
        return false;
    }
    let Ok(mut durable) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    // begin_hosted_orphan_recovery selects the sole recovery Agent; no other
    // active/recovery identity may substitute for this scenario's original.
    if !durable["provider_recovery_pending"]
        .as_object()
        .is_some_and(|v| v.len() == 1 && v.contains_key("agent-a"))
        || !durable["provider_active_turns"]
            .as_object()
            .is_some_and(|v| v.len() == 1 && v.contains_key("agent-a"))
    {
        return false;
    }
    let original_lease = durable["provider_raw_outcomes"][0]["cognition_lease"].clone();
    let current_lease = &durable["provider_cognition_leases"]["agent-a"];
    for key in [
        "agent_id",
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "request_digest",
        "lease_id",
        "idempotency_key",
        "account_id",
        "quote",
        "reserved_amount",
        "reserved_at_tick",
    ] {
        if original_lease[key].is_null() || current_lease[key] != original_lease[key] {
            return false;
        }
    }
    // Settlement changes only lifecycle/accounting fields, not original admission.
    // Reuse the complete original capture matcher after checking these immutable fields.
    durable["provider_cognition_leases"]["agent-a"] = original_lease;
    let origin_path = root.join("hosted-wait-origin.json");
    let Ok(file) = fs::File::open(origin_path) else {
        return false;
    };
    bytes.clear();
    if file.take(16 * 1024 + 1).read_to_end(&mut bytes).is_err() || bytes.len() > 16 * 1024 {
        return false;
    }
    let Ok(origin) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    matches_capture_transition(original_error, &durable, &origin)
}

fn durable_original_cleanup_complete(root: &std::path::Path, summary: &serde_json::Value) -> bool {
    use std::io::Read;
    fn read(path: std::path::PathBuf) -> Option<serde_json::Value> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        if bytes.len() > 1024 * 1024 {
            return None;
        }
        serde_json::from_slice(&bytes).ok()
    }
    let Some(durable) = read(root.join("hosted-wait-private-lineage.json")) else {
        return false;
    };
    let Some(origin) = read(root.join("hosted-wait-origin.json")) else {
        return false;
    };
    let terminal = &durable["provider_terminal_states"]["agent-a"];
    if durable["provider_active_turns"].get("agent-a").is_some()
        || durable["provider_raw_outcomes"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
        || terminal != &summary["terminal_states"]["agent-a"]
        || terminal["status"] != "failed"
        || terminal["reject_reason"] != "cognition_failed"
        || terminal["feedback"]["status"] != "failed"
        || terminal["feedback"]["reject_reason"] != "cognition_failed"
        || !terminal["feedback_id"]
            .as_str()
            .is_some_and(|v| !v.is_empty())
        || terminal["feedback_id"] != terminal["feedback"]["feedback_id"]
    {
        return false;
    }
    [
        "agent_session_id",
        "agent_turn_id",
        "decision_request_id",
        "request_digest",
    ]
    .iter()
    .all(|key| {
        origin[*key].as_str().is_some_and(|v| !v.is_empty())
            && terminal[*key] == origin[*key]
            && terminal["feedback"][*key] == origin[*key]
    })
}
