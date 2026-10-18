use super::*;

impl ViewerRuntimeLiveServer {
    pub(super) fn prime_shared_request_if_needed(
        shared: &Arc<Mutex<Self>>,
        request: &ViewerRequest,
        session: &RuntimeLiveSession,
    ) -> Result<Option<Result<(), ViewerRuntimeLiveServerError>>, ViewerRuntimeLiveServerError>
    {
        let should_prime = {
            let server = lock_shared_server(shared)?;
            match request {
                ViewerRequest::HelloV2 { version, .. } => {
                    *version >= 2
                        && server.hosted_local_mock_test_lane_active
                        && server.chain_link_enabled()
                        && server.world.state().agents.is_empty()
                        && !session.chain_runtime_authoritatively_primed
                }
                ViewerRequest::RequestSnapshot => {
                    server.chain_link_enabled()
                        && !session.initial_snapshot_sent
                        && !session.chain_runtime_authoritatively_primed
                }
                ViewerRequest::AuthoritativeRecovery { .. } => {
                    server.config.world_service.is_some()
                }
                ViewerRequest::PlaybackControl { .. }
                | ViewerRequest::LiveControl { .. }
                | ViewerRequest::Control { .. } => server.config.world_service.is_some(),
                _ => false,
            }
        };
        if !should_prime {
            return Ok(None);
        }
        Ok(Some(
            Self::prime_chain_linked_runtime_for_snapshot_minimized_lock(
                shared,
                world_service_link::coherence_trace_request_kind(request),
            )
            .map(|_| ()),
        ))
    }
}
