//! Bounded waits for the healthy-set process fixture and its owned stop worker.
use super::*;

pub(super) fn wait_for_boundary(runtime: &FormalLiveRuntime, position: u64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let current = runtime.published.position()?;
        if current >= position {
            if current != position {
                return Err(format!(
                    "healthy arm passed exact boundary {position}: current {}",
                    current
                ));
            }
            return Ok(());
        }
        let status = runtime
            .runtime
            .lock()
            .map_err(|_| "healthy boundary runtime lock poisoned")?
            .snapshot();
        if let Some(error) = status.last_error {
            return Err(format!(
                "Node failed before healthy boundary {position}: {error}"
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Node did not qualify healthy boundary {position}; running={} ticks={} slot={} height={} committed={} executed={} pending={:?}",
                status.running,
                status.tick_count,
                status.consensus.slot,
                status.consensus.latest_height,
                status.consensus.committed_height,
                status.consensus.last_execution_height,
                status.consensus.pending_consensus_actions,
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn wait_for_request_result(
    runtime: &FormalLiveRuntime,
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<oasis7::world_service::CanonicalIntentResultV1, String> {
    let key = correlation::key_digest(&request.correlation.key).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let cut = runtime.published.current()?;
        if let Some(result) = optional_retained_result_from_hot_or_cold(&cut.world, &key)? {
            if result.request != *request || result.rejected.is_some() {
                return Err(format!(
                    "healthy-set request has wrong or rejected canonical outcome: {:?}",
                    result.rejected
                ));
            }
            return Ok(result);
        }
        let status = runtime
            .runtime
            .lock()
            .map_err(|_| "healthy request runtime lock poisoned")?
            .snapshot();
        if let Some(error) = status.last_error {
            return Err(format!(
                "Node failed before healthy request result: {error}"
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "healthy-set request result missing at position {}; running={} ticks={} slot={} height={} committed={} executed={} pending={:?}",
                cut.anchor.head().record().position,
                status.running,
                status.tick_count,
                status.consensus.slot,
                status.consensus.latest_height,
                status.consensus.committed_height,
                status.consensus.last_execution_height,
                status.consensus.pending_consensus_actions,
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

/// Keep the same runtime and pending stop worker until graceful completion,
/// bounded by this scenario's original expiry rather than a fresh allowance.
pub(crate) fn stop_healthy_runtime(
    runtime: &mut FormalLiveRuntime,
    scenario: ScenarioDeadline,
) -> Result<(), String> {
    let started = Instant::now();
    eprintln!(
        "formal_healthy_history stage=stop event=begin position={} pending_open={}",
        runtime.published.position()?,
        runtime.durable_set_arm.pending_open()?
    );
    loop {
        let remaining = scenario
            .expires_at
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("healthy runtime stop exceeded the original scenario deadline")?;
        match runtime.stop(remaining.min(Duration::from_secs(10))) {
            Ok(()) => {
                eprintln!(
                    "formal_healthy_history stage=stop event=end stage_ms={} position={}",
                    started.elapsed().as_millis(),
                    runtime.published.position()?
                );
                scenario.report_stage("healthy runtime graceful stop completed")?;
                return Ok(());
            }
            Err(error) if error == "formal Node stop is still pending; runtime remains owned" => {
                scenario.report_stage("healthy runtime stop pending; retaining same owner")?;
            }
            Err(error) => return Err(error),
        }
    }
}
