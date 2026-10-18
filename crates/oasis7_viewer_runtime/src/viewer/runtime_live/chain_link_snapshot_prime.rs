//! Snapshot priming keeps authenticated reads outside the shared Viewer lock.
use super::super::world_service_link::{
    WorldServiceCoherenceTraceContext, prepare_world_service_update_for_shared_request,
};
use super::*;

impl ViewerRuntimeLiveServer {
    pub(in super::super) fn prime_chain_linked_runtime_for_snapshot(
        &mut self,
    ) -> Result<bool, ViewerRuntimeLiveServerError> {
        if let Some(config) = &self.config.world_service {
            let prepared = prepare_world_service_update(
                config.clone(),
                self.verified_world_view.clone(),
                self.pending_world_service_gameplay.clone(),
                self.world_service_query_state.clone(),
            )?;
            let mut silent_session = RuntimeLiveSession::new_with_playing(false);
            return Ok(self
                .apply_chain_linked_runtime_update(prepared, &mut silent_session)?
                .advanced);
        }
        let Some(_chain_status_bind) = self
            .config
            .chain_status_bind
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(false);
        };

        let Some(prepared) = request_observer_update(&self.chain_observer_loader, &self.config)?
        else {
            return Ok(false);
        };
        self.clear_chain_sync_failure_feedback();
        let mut silent_session = RuntimeLiveSession::new_with_playing(false);
        let dispatch = self.apply_chain_linked_runtime_update(prepared, &mut silent_session)?;
        Ok(dispatch.advanced)
    }

    /// Prime the first chain-linked projection without holding the shared
    /// Viewer mutex across the status/world read. HelloV2 and the initial
    /// snapshot request are served by separate client threads; keeping the
    /// network read outside the mutex prevents a slow chain-status response
    /// from starving the launcher presence probe's HelloAck. The prepared
    /// world is still applied under the mutex and passes the same authority
    /// checks before it can become visible.
    pub(in super::super) fn prime_chain_linked_runtime_for_snapshot_minimized_lock(
        shared: &Arc<Mutex<Self>>,
        request_kind: &'static str,
    ) -> Result<bool, ViewerRuntimeLiveServerError> {
        // Snapshots report an authenticated version already known by this
        // shared server. New observers need not refresh that immutable version
        // synchronously. Controls, recovery and periodic Changes still refresh.
        if request_kind == "request_snapshot" {
            let mut server = lock_shared_server(shared)?;
            if let (Some(config), Some(view)) =
                (&server.config.world_service, &server.verified_world_view)
                && server.pending_world_service_gameplay.is_empty()
                && view.version().commit.world == config.expected_world
                && view.version().visibility_scope == config.scope_id
                && view.belongs_to(
                    &config
                        .read_authority()
                        .map_err(ViewerRuntimeLiveServerError::Init)?,
                )
            {
                let view = view.clone();
                let prepared = PreparedChainLinkedRuntimeUpdate {
                    committed_height: view.version().commit.position,
                    source: (None, None),
                    source_epoch: 0,
                    world: crate::runtime_host::world::new_with_state(
                        view.projection().state.clone(),
                    ),
                    verified_view: Some(view),
                    // No new cursor, event or intent disposition is observed.
                    service_events: Some(Vec::new()),
                    intent_results: Vec::new(),
                };
                let mut silent_session = RuntimeLiveSession::new_with_playing(false);
                return Ok(server
                    .apply_chain_linked_runtime_update(prepared, &mut silent_session)?
                    .advanced);
            }
        }
        let remote = {
            let server = lock_shared_server(shared)?;
            server.config.world_service.clone().map(|config| {
                (
                    config,
                    server.verified_world_view.clone(),
                    server.pending_world_service_gameplay.clone(),
                    server.world_service_query_state.clone(),
                )
            })
        };
        if let Some((config, previous, pending, query_state)) = remote {
            #[cfg(any(test, feature = "test_tier_required"))]
            let prime_started = std::time::Instant::now();
            #[cfg(any(test, feature = "test_tier_required"))]
            if std::env::var("PRE2_WORLD_COHERENCE_TRACE").is_ok_and(|value| value == "1") {
                eprintln!(
                    "PRE2_SNAPSHOT_PRIME request_kind={request_kind} phase=authenticated_read_started"
                );
            }
            let prepared = prepare_world_service_update_for_shared_request(
                config,
                previous,
                pending,
                query_state,
                WorldServiceCoherenceTraceContext {
                    caller_phase: "shared_request_prime",
                    request_kind,
                    session_fence: "not_applicable_sync_prime",
                },
            )?;
            #[cfg(any(test, feature = "test_tier_required"))]
            if std::env::var("PRE2_WORLD_COHERENCE_TRACE").is_ok_and(|value| value == "1") {
                eprintln!(
                    "PRE2_SNAPSHOT_PRIME request_kind={request_kind} phase=authenticated_read_complete elapsed_us={}",
                    prime_started.elapsed().as_micros()
                );
            }
            let mut server = lock_shared_server(shared)?;
            let mut silent_session = RuntimeLiveSession::new_with_playing(false);
            let advanced = server
                .apply_chain_linked_runtime_update(prepared, &mut silent_session)?
                .advanced;
            #[cfg(any(test, feature = "test_tier_required"))]
            if std::env::var("PRE2_WORLD_COHERENCE_TRACE").is_ok_and(|value| value == "1") {
                eprintln!(
                    "PRE2_SNAPSHOT_PRIME request_kind={request_kind} phase=projection_applied elapsed_us={}",
                    prime_started.elapsed().as_micros()
                );
            }
            return Ok(advanced);
        }
        let (loader, config) = {
            let server = lock_shared_server(shared)?;
            (server.chain_observer_loader.clone(), server.config.clone())
        };
        let chain_status_bind = {
            let server = lock_shared_server(shared)?;
            server
                .config
                .chain_status_bind
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(_chain_status_bind) = chain_status_bind else {
            return Ok(false);
        };

        let Some(prepared) = request_observer_update(&loader, &config)? else {
            return Ok(false);
        };
        let mut server = lock_shared_server(shared)?;
        server.clear_chain_sync_failure_feedback();
        let mut silent_session = RuntimeLiveSession::new_with_playing(false);
        let dispatch = server.apply_chain_linked_runtime_update(prepared, &mut silent_session)?;
        Ok(dispatch.advanced)
    }
}
