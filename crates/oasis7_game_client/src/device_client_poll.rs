//! Bounded read recovery of one immutable submitted intent. This module never
//! signs a request, allocates a replacement operation, or replays a submission.
use super::*;
use crate::WorldServiceClientError;
use std::time::Instant;
pub(super) const POLL_ATTEMPTS: usize = 40;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
pub(super) const POLL_WINDOW: Duration = Duration::from_secs(8);
/// Emit only a bounded public category/status, never response text or proofs.
pub(super) fn failure_tag(error: &crate::WorldServiceClientError) -> String {
    use crate::WorldServiceClientError;
    match error {
        WorldServiceClientError::Configuration(_) => "configuration rejected".into(),
        WorldServiceClientError::Transport(_) => "transport unavailable".into(),
        WorldServiceClientError::Assurance(_) => "response not verified".into(),
        WorldServiceClientError::Rejected { status, .. } => format!("rejected HTTP {status}"),
        WorldServiceClientError::Cooldown { status, .. } => format!("cooldown HTTP {status}"),
    }
}

fn query_error(error: &WorldServiceClientError) -> String {
    format!(
        "original result query {}; retain original operation",
        failure_tag(error)
    )
}
pub(super) fn retry_delay(error: &WorldServiceClientError) -> Option<Duration> {
    match error {
        WorldServiceClientError::Transport(_)
        | WorldServiceClientError::Rejected { status: 503, .. } => Some(POLL_INTERVAL),
        WorldServiceClientError::Cooldown { retry_after, .. } => {
            Some((*retry_after).max(POLL_INTERVAL))
        }
        _ => None,
    }
}
impl Client {
    /// Preserve transport versus assurance/configuration failures for bounded
    /// retry decisions. Callers must not log private response/error bodies.
    pub fn lookup_typed(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<Value>, WorldServiceClientError> {
        self.remote.lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: request.correlation.key.clone(),
            },
            request.signed_payload.clone(),
        )
    }
    pub fn lookup(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<Value>, String> {
        self.lookup_typed(request)
            .map_err(|error| query_error(&error))
    }
    pub fn submit_and_poll(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<Value>, String> {
        #[cfg(unix)]
        if self.gateway.is_some() {
            let response = self
                .lookup_typed(request)
                .map_err(|error| query_error(&error))?;
            match response.outcome {
                IntentOutcome::Unknown => {}
                IntentOutcome::Received { .. } | IntentOutcome::Pending => {
                    return self.poll_original(request);
                }
                _ => return Ok(response),
            }
        }
        // The original signed payload is durable before this single send. Lost
        // delivery can only lead to queries of this exact correlation/payload.
        let submission = self.remote.submit(request.clone());
        if let Ok(SubmitObservation::Response(response)) = submission
            && matches!(
                response.outcome,
                IntentOutcome::Committed { .. }
                    | IntentOutcome::Rejected { .. }
                    | IntentOutcome::Expired
            )
        {
            return Ok(*response);
        }
        self.poll_original(request)
    }
    /// Query only the exact original signed intent within the existing bounded
    /// recovery window. Never submits, signs, or creates an operation identity.
    pub fn poll_original(
        &self,
        request: &SubmitIntentRequest<WorldServicePayloadV1>,
    ) -> Result<IntentResponse<Value>, String> {
        // Keep the former forty 200ms polls within their existing eight-second
        // window. A single bounded five-second HTTP read already in flight may
        // finish after the deadline; no further HTTP call then starts.
        let deadline = Instant::now() + POLL_WINDOW;
        let mut last_response = None;
        let mut last_error = None;
        for _ in 0..POLL_ATTEMPTS {
            if Instant::now() >= deadline {
                break;
            }
            let delay = match self.lookup_typed(request) {
                Ok(response) => {
                    if !matches!(
                        response.outcome,
                        IntentOutcome::Received { .. }
                            | IntentOutcome::Pending
                            | IntentOutcome::Unknown
                    ) {
                        return Ok(response);
                    }
                    last_response = Some(response);
                    last_error = None;
                    POLL_INTERVAL
                }
                Err(error) => {
                    let Some(delay) = retry_delay(&error) else {
                        return Err(query_error(&error));
                    };
                    last_error = Some(error);
                    delay
                }
            };
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            // If Retry-After exceeds the remaining window, stop at the deadline
            // without querying early or extending the original context lifetime.
            std::thread::sleep(delay.min(remaining));
        }
        if let Some(error) = last_error {
            return Err(query_error(&error));
        }
        last_response.ok_or_else(|| {
            "original result query window exhausted; retain original operation".into()
        })
    }
}
#[cfg(all(test, unix))]
#[path = "device_client_poll_tests.rs"]
mod tests;
