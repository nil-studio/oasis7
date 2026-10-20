//! Manual HTTP admission follows the Node's idempotent enqueue contract.
use super::*;
use fixture_stage_diagnostics::AdmissionObservation;
use std::sync::Condvar;

#[derive(Default)]
pub(super) struct Completion {
    count: Mutex<u64>,
    changed: Condvar,
}
impl Completion {
    pub(super) fn count(&self) -> u64 {
        *self.count.lock().unwrap()
    }
    fn finish(&self) {
        *self.count.lock().unwrap() += 1;
        self.changed.notify_all();
    }
    pub(super) fn wait_after(&self, previous: u64) {
        let until = Instant::now() + Duration::from_secs(5);
        let mut count = self.count.lock().unwrap();
        while *count <= previous {
            let (next, timed) = self
                .changed
                .wait_timeout(count, until.saturating_duration_since(Instant::now()))
                .unwrap();
            count = next;
            assert!(
                !timed.timed_out() || *count > previous,
                "original controlled HTTP hook did not complete"
            );
        }
    }
}

pub(super) fn execute(
    driver: &Arc<Mutex<NodeRuntimeExecutionDriver>>,
    storage: &std::path::Path,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
    diagnostics: &fixture_stage_diagnostics::Diagnostics,
    completion: &Completion,
) -> Result<(), String> {
    let mut driver =
        diagnostics.measure(Stage::ControlledDriverLockWait, || driver.lock().unwrap());
    let key = correlation::key_digest(&request.correlation.key)?;
    let results = &driver
        .execution_world
        .capability_revocation_state()
        .world_service_results;
    if let Some(value) = results.get(&key) {
        let verified = diagnostics.measure(Stage::ReplayVerification, || -> Result<(), String> {
            correlation::validate_result(&key, value)?;
            let result: wire::CanonicalIntentResultV1 =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            if result.request != request
                || result.committed_height > driver.state.last_applied_committed_height
            {
                return Err("controlled replay differs from original committed request".into());
            }
            let pinned = super::super::super::world_service_read::pin_at_height(
                &driver.records_dir,
                storage,
                &request.correlation.key.world,
                result.committed_height,
            )?;
            if pinned.commit.position != result.committed_height
                || pinned
                    .world
                    .capability_revocation_state()
                    .world_service_results
                    .get(&key)
                    != Some(value)
            {
                return Err("controlled replay lacks the exact published result".into());
            }
            Ok(())
        });
        if let Err(error) = verified {
            diagnostics.observe_admission(AdmissionObservation::RejectedExisting);
            return Err(error);
        }
        diagnostics.observe_admission(AdmissionObservation::ExactReplay);
    } else {
        diagnostics.observe_admission(AdmissionObservation::Fresh);
        let height = driver.state.last_applied_committed_height + 1;
        diagnostics.measure(Stage::ControlledCommit, || {
            commit_request(&mut driver, height, Some(request))
        });
    }
    completion.finish();
    Ok(())
}

#[test]
fn real_tcp_committed_http_replay_does_not_create_another_execution_block() {
    let fixture = Fixture::with_controlled_commits(true);
    fixture
        .controlled_submit_commit
        .store(true, Ordering::SeqCst);
    let request = fixture.delegation();
    let before = fixture.controlled_completion.count();
    let SubmitObservation::Response(response) = fixture.client.submit(request.clone()).unwrap()
    else {
        panic!("actual fresh HTTP admission response missing");
    };
    assert!(matches!(response.outcome, IntentOutcome::Received { .. }));
    fixture.controlled_completion.wait_after(before);
    let committed = fixture.committed(&request);
    let view = fixture
        .client
        .read_view(fixture.view(None))
        .unwrap()
        .version()
        .clone();
    let (height, world, journal) = {
        let driver = fixture.driver.lock().unwrap();
        (
            driver.state.last_applied_committed_height,
            serde_json::to_value(driver.execution_world.snapshot()).unwrap(),
            driver.execution_world.journal().events.clone(),
        )
    };
    let before = fixture.controlled_completion.count();
    let SubmitObservation::Response(response) = fixture.client.submit(request.clone()).unwrap()
    else {
        panic!("actual committed HTTP replay response missing");
    };
    assert!(matches!(response.outcome, IntentOutcome::Committed { .. }));
    fixture.controlled_completion.wait_after(before);
    let driver = fixture.driver.lock().unwrap();
    assert_eq!(
        driver.state.last_applied_committed_height, height,
        "completed original HTTP replay must not manufacture a new block"
    );
    assert!(
        serde_json::to_value(driver.execution_world.snapshot()).unwrap() == world,
        "original World snapshot must remain unchanged"
    );
    assert!(
        driver.execution_world.journal().events == journal,
        "original Journal must remain unchanged"
    );
    drop(driver);
    assert!(
        fixture.committed(&request) == committed,
        "original canonical result identity must remain unchanged"
    );
    assert!(
        *fixture
            .client
            .read_view(fixture.view(None))
            .unwrap()
            .version()
            == view,
        "original published record head must remain unchanged"
    );
    let completion = fixture.controlled_completion.count();
    let mut correlation_conflict = request.clone();
    correlation_conflict
        .correlation
        .payload_digest
        .push_str("-conflict");
    assert!(
        execute(
            &fixture.driver,
            &fixture.root.join("store"),
            correlation_conflict,
            &fixture.stage_diagnostics,
            &fixture.controlled_completion
        )
        .is_err()
    );
    let mut payload_conflict = request.clone();
    let WorldServicePayloadV1::Delegation(signed) = &mut payload_conflict.signed_payload else {
        panic!("fixture delegation payload required");
    };
    signed.request.agent_id.push_str("-conflict");
    assert!(
        execute(
            &fixture.driver,
            &fixture.root.join("store"),
            payload_conflict,
            &fixture.stage_diagnostics,
            &fixture.controlled_completion
        )
        .is_err()
    );
    let record = super::super::super::checkpoint::execution_bridge_record_path(
        &fixture.root.join("records"),
        committed.position,
    );
    let original = fs::read(&record).unwrap();
    fs::remove_file(&record).unwrap();
    let missing_publication = execute(
        &fixture.driver,
        &fixture.root.join("store"),
        request,
        &fixture.stage_diagnostics,
        &fixture.controlled_completion,
    );
    fs::write(record, original).unwrap();
    assert!(
        missing_publication.is_err(),
        "in-memory result is not a published commit"
    );
    assert_eq!(fixture.controlled_completion.count(), completion);
    let driver = fixture.driver.lock().unwrap();
    assert_eq!(driver.state.last_applied_committed_height, height);
    assert!(
        serde_json::to_value(driver.execution_world.snapshot()).unwrap() == world,
        "original World snapshot must remain unchanged"
    );
    assert!(
        driver.execution_world.journal().events == journal,
        "original Journal must remain unchanged"
    );
}
