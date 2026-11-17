use super::*;

/// Exercises the real host `open` + `start` path against the signed loopback
/// endpoint fixture. V3Required without an evidence adapter must reject before
/// binding the configured status listener. Then persisted V3 configuration
/// drives ordinary open/start and a signed remote manifest read across two
/// complete owner lifetimes, without qualifying the simulated witness/clock.
#[test]
#[ignore = "requires OASIS7_FORMAL_ENDPOINT_TEST_BIN; loopback endpoints are a test simulation"]
fn protected_endpoint_children_v3_required_runtime_start_rejects_missing_gate_before_listener() {
    std::thread::Builder::new()
        .name("formal-v3-read-gate-startup".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run_protected_runtime_case(false, Some("v3_read_gate_no_adapter"), false))
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
