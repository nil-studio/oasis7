//! Real Lookup disconnects remain pending until the original receipt is reconciled.
use super::*;

const FAULT_ENV: &str = "PRE2_CONFORMANCE_INTERRUPT_ACT_LOOKUP";

#[test]
fn real_tcp_hosted_wait_reconciles_original_after_lookup_disconnect() {
    run(false);
}

#[test]
fn real_tcp_hosted_delayed_wait_reconciles_original_after_lookup_disconnect() {
    run(true);
}

fn run(delayed: bool) {
    let _permit = scenario_resource::ScenarioResourcePermit::acquire();
    let root = temp_dir("qa-hosted-lookup-recovery");
    fs::create_dir_all(&root).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command.args([
        "--exact",
        "execution_bridge_real_tests::real_execution_bridge::tests::qa_conformance::application_hosted_wait::real_tcp_hosted_ordinary_wait_resume_act_preserves_canonical_identity",
        "--nocapture",
    ]);
    command.env(FAULT_ENV, "1");
    if delayed {
        command.env("PRE2_CONFORMANCE_VIEW_DELAY_MS", "1000");
    }
    let output = application_completion::run_child(&mut command, &root);
    fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "original completion must survive a real Lookup disconnect: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("PRE2_HOSTED_LOOKUP_RECOVERY_PASSED"));
}

pub(super) fn interrupt_lookup(
    path: &str,
    bytes: &[u8],
    stream: &TcpStream,
    gate: &http_fixture::WorldGate,
) -> bool {
    if path != LOOKUP_PATH || std::env::var(FAULT_ENV).as_deref() != Ok("1") {
        return false;
    }
    let body = crate::feedback_submit_api::extract_http_json_body(bytes).unwrap();
    let request: wire::AuthenticatedLookup = serde_json::from_slice(body).unwrap();
    if !matches!(request.original, WorldServicePayloadV1::Cognition(_)) {
        return false;
    }
    let root = gate.root.lock().unwrap().clone().unwrap();
    // The child releases this fault only after observing the actual pump error.
    // This makes the transient failure deterministic without timing sleeps.
    if root.join("hosted-lookup-retry-release").exists() {
        return false;
    }
    application_hosted_wait::publish_marker(&root.join("hosted-lookup-interrupted"), b"reset")
        .unwrap();
    stream.shutdown(std::net::Shutdown::Both).unwrap();
    true
}

pub(super) fn observe_error(root: &std::path::Path, summary: &serde_json::Value) -> bool {
    if std::env::var(FAULT_ENV).as_deref() != Ok("1")
        || !root.join("hosted-lookup-interrupted").exists()
        || summary["agent_service_pump_error"]
            != "Agent service result could not be applied; original work remains pending"
    {
        return false;
    }
    application_hosted_wait::publish_marker(
        &root.join("hosted-lookup-retry-release"),
        b"original failure observed; reconcile original Lookup",
    )
    .unwrap();
    true
}

pub(super) fn verify_observed(observed: bool) {
    if std::env::var(FAULT_ENV).as_deref() == Ok("1") {
        assert!(
            observed,
            "real transport failure must have reached completion watch"
        );
        println!("PRE2_HOSTED_LOOKUP_RECOVERY_PASSED");
    }
}
