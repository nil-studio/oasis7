//! Compute an owned completion's phase identity without changing its original token.
use super::ViewerRuntimeLiveServer;
use super::agent_service_io::{AgentServiceIoResult, AgentServiceIoToken, AgentServiceProgress};
use super::agent_service_phase::HostedServicePhase;
use std::sync::{Mutex, MutexGuard};

pub(super) struct ReadyResultPhase {
    token: AgentServiceIoToken,
    phase: HostedServicePhase,
    identity: Result<String, String>,
}

pub(super) fn prepare_result_phase<'a>(
    shared: &'a Mutex<ViewerRuntimeLiveServer>,
    server: MutexGuard<'a, ViewerRuntimeLiveServer>,
    result: &AgentServiceIoResult,
) -> Option<(
    MutexGuard<'a, ViewerRuntimeLiveServer>,
    Option<ReadyResultPhase>,
)> {
    let token = &result.token;
    if server.llm_sidecar.hosted_service_inflight.as_ref() != Some(token)
        || token.phase_digest == "metadata"
        || token.phase_digest.starts_with("admission:")
        || token.phase_digest.starts_with("wait:")
        || token.phase_digest.starts_with("resume:")
    {
        return Some((server, None));
    }
    let Some(phase) = server.llm_sidecar.hosted_service_phase.clone() else {
        return Some((server, None));
    };
    let token = token.clone();
    drop(server);
    let identity = compute_identity(&phase);
    let server = shared.lock().ok()?;
    Some((
        server,
        Some(ReadyResultPhase {
            token,
            phase,
            identity,
        }),
    ))
}

fn compute_identity(phase: &HostedServicePhase) -> Result<String, String> {
    #[cfg(any(test, feature = "test_tier_required"))]
    let _identity_phase = super::stream_budget_diagnostics::phase(74);
    #[cfg(any(test, feature = "test_tier_required"))]
    super::stream_budget_diagnostics::identity_barrier();
    phase.identity()
}

pub(super) enum RetryIdentity {
    Current(Option<Result<String, String>>),
    Changed,
}

pub(super) fn prepare_retry_identity<'a>(
    shared: &'a Mutex<ViewerRuntimeLiveServer>,
    server: MutexGuard<'a, ViewerRuntimeLiveServer>,
) -> Option<(MutexGuard<'a, ViewerRuntimeLiveServer>, RetryIdentity)> {
    let Some(phase) = server.llm_sidecar.hosted_service_phase.clone() else {
        return Some((server, RetryIdentity::Current(None)));
    };
    let generation = server.llm_sidecar.hosted_service_generation;
    drop(server);
    let identity = compute_identity(&phase);
    let server = shared.lock().ok()?;
    let current = if server.llm_sidecar.hosted_service_generation == generation
        && server.llm_sidecar.hosted_service_phase.as_ref() == Some(&phase)
    {
        RetryIdentity::Current(Some(identity))
    } else {
        RetryIdentity::Changed
    };
    Some((server, current))
}

impl ViewerRuntimeLiveServer {
    pub(super) fn apply_agent_service_completion(
        &mut self,
        result: AgentServiceIoResult,
        prepared: Option<ReadyResultPhase>,
    ) -> Result<AgentServiceProgress, String> {
        #[cfg(any(test, feature = "test_tier_required"))]
        let diagnostic = self.agent_service_io_diagnostics.prepare_apply(&result);
        let applied = self.apply_prepared_agent_result(result, prepared);
        #[cfg(any(test, feature = "test_tier_required"))]
        self.agent_service_io_diagnostics
            .finish_apply(diagnostic, &applied);
        // Publish successful progress and diagnostics in the same critical section.
        // Retry identity computation releases the lock, so no stale pump error may
        // remain paired with the new successful application classification.
        if let Ok(value) = &applied {
            self.observe_agent_pump_progress(value);
        }
        applied
    }
    pub(super) fn apply_prepared_agent_result(
        &mut self,
        result: AgentServiceIoResult,
        prepared: Option<ReadyResultPhase>,
    ) -> Result<AgentServiceProgress, String> {
        let Some(prepared) = prepared else {
            return self.apply_agent_service_io(result);
        };
        if result.token != prepared.token
            || self.llm_sidecar.hosted_service_inflight.as_ref() != Some(&prepared.token)
        {
            return Err(
                "hosted completion token changed during identity computation; original retained"
                    .into(),
            );
        }
        if self.llm_sidecar.hosted_service_generation != prepared.token.generation
            || self.llm_sidecar.hosted_service_phase.as_ref() != Some(&prepared.phase)
        {
            // This result has consumed exactly the old in-flight marker. Retire
            // only that marker so the current phase can prepare again; keep all
            // durable originals, newer reservations and submit bits untouched.
            self.llm_sidecar.hosted_service_inflight = None;
            return Err(
                "hosted completion phase changed during identity computation; original retained"
                    .into(),
            );
        }
        self.apply_hosted_service_io_with_identity(
            result,
            Some((prepared.phase, prepared.identity)),
        )
    }
}
