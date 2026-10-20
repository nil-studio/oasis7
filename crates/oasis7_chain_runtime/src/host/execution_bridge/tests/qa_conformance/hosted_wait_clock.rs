//! Parent clock advances only the exact ordinary Wait after canonical settlement.
use super::*;
pub(super) struct Clock {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Clock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
pub(super) fn start(fixture: &Fixture, root: std::path::PathBuf) -> Clock {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let driver = fixture.driver.clone();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !stopping.load(Ordering::SeqCst) && Instant::now() < deadline {
            if let Ok(bytes) = fs::read(root.join("hosted-wait-origin.json")) {
                let origin: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let digest = origin["request_digest"].as_str().unwrap();
                let lease_id = origin["lease_id"].as_str().unwrap();
                let mut guard = driver.lock().unwrap();
                let admitted = guard.execution_world.capability_revocation_state().world_service_results.values()
                    .filter_map(|value| serde_json::from_value::<wire::CanonicalIntentResultV1>(value.clone()).ok())
                    .any(|result| result.rejected.is_none() && matches!(&result.request.signed_payload,
                        WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation,
                            SchedulerOperationV1::AdmitContinuation(proposal) if proposal.origin_request_digest == digest)));
                let settled = guard
                    .execution_world
                    .cognition_economy()
                    .unwrap()
                    .leases
                    .values()
                    .any(|lease| {
                        lease.lease_id == lease_id
                            && lease.request_digest == digest
                            && lease.status == oasis7_runtime::CognitionLeaseStatusV1::Settled
                    });
                let continuation = guard
                    .execution_world
                    .active_cognition_continuations()
                    .unwrap()
                    .into_iter()
                    .find(|entry| entry.origin_request_digest == digest);
                if admitted
                    && settled
                    && let Some(continuation) = continuation
                {
                    let selected = guard
                        .execution_world
                        .cognition_in_flight_wakes()
                        .unwrap()
                        .into_iter()
                        .find(|wake| wake.continuation_id == continuation.continuation_id);
                    if let Some(wake) = selected {
                        fs::write(root.join("hosted-wait-selected.json"), serde_json::to_vec(&serde_json::json!({
                            "continuation_id":continuation.continuation_id,"wake_id":wake.wake_id,
                            "origin_request_digest":digest,"exact_admit":true,"exact_wait_lease_settled":true})).unwrap()).unwrap();
                        println!(
                            "hosted_wait_real_clock_selected exact_admit=true exact_wait_lease_settled=true"
                        );
                        return;
                    }
                    let mut before = serde_json::to_value(guard.execution_world.state()).unwrap();
                    before["time"] = serde_json::json!(0);
                    commit_request(&mut guard, 0, None);
                    let mut after = serde_json::to_value(guard.execution_world.state()).unwrap();
                    after["time"] = serde_json::json!(0);
                    assert_eq!(
                        before, after,
                        "genuine canonical clock changed protected non-time WorldState"
                    );
                    println!(
                        "hosted_wait_real_empty_commit exact_admit=true exact_wait_lease_settled=true non_time_unchanged=true"
                    );
                    if let Some(wake) = guard
                        .execution_world
                        .cognition_in_flight_wakes()
                        .unwrap()
                        .into_iter()
                        .find(|wake| wake.continuation_id == continuation.continuation_id)
                    {
                        fs::write(root.join("hosted-wait-selected.json"), serde_json::to_vec(&serde_json::json!({
                            "continuation_id":continuation.continuation_id,"wake_id":wake.wake_id,
                            "origin_request_digest":digest,"exact_admit":true,"exact_wait_lease_settled":true})).unwrap()).unwrap();
                        println!(
                            "hosted_wait_real_clock_selected exact_admit=true exact_wait_lease_settled=true"
                        );
                        return;
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    Clock {
        stop,
        worker: Some(worker),
    }
}
