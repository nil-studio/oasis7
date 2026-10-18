use super::super::execution_bridge;
use super::{build_minimal_status_payload_with_world_dir, fs, temp_dir};

#[test]
fn persisted_module_tick_status_is_isolated_from_parallel_test_live_metrics() {
    let dir = temp_dir("module-routing-parallel-test-isolation");
    oasis7::runtime_host::world::new()
        .save_to_dir(&dir)
        .expect("save healthy world");
    let published = std::sync::Barrier::new(2);
    let reader_finished = std::sync::Barrier::new(2);

    // These threads model independent Rust test contexts, not production drivers:
    // live publication must remain visible to its owning test but not its peers.
    let (persisted_payload, live_payload) = std::thread::scope(|scope| {
        let publisher = scope.spawn(|| {
            let mut metrics = oasis7_runtime::ModuleTickRoutingMetricsSnapshot {
                routing_count: 4,
                ..Default::default()
            };
            metrics.duration_buckets.ge_100ms = 4;
            execution_bridge::record_execution_bridge_module_tick_routing_metrics(metrics);
            let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);
            published.wait();
            reader_finished.wait();
            execution_bridge::reset_execution_bridge_commit_timing_for_tests();
            payload
        });
        published.wait();
        let payload = build_minimal_status_payload_with_world_dir(dir.as_path(), None);
        reader_finished.wait();
        (
            payload,
            publisher.join().expect("join live metric publisher"),
        )
    });
    let _ = fs::remove_dir_all(&dir);

    assert!(
        live_payload.observability.alerts.iter().any(|alert| {
            alert.code == "module_tick_routing_degraded"
                && alert.summary.contains("sustained_slow_routes=4")
        }),
        "the publishing test must retain its own sustained slow live-route alert"
    );
    assert!(
        !persisted_payload
            .observability
            .alerts
            .iter()
            .any(|alert| alert.code == "module_tick_routing_degraded"),
        "healthy persisted status must not consume another test's live routing metrics: {:?}",
        persisted_payload.observability.alerts
    );
}
