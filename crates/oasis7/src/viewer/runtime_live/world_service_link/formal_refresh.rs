//! Formal Viewer refresh is a proved snapshot read. Event-stream and write
//! reconciliation require their own public proof contracts.
use super::*;

pub(super) fn prepare_formal_refresh(
    mut config: WorldServiceClientConfig,
    previous: Option<VerifiedWorldView>,
    has_pending_intents: bool,
    query_state: crate::world_service::client::WorldServiceQueryState,
    deadline: Option<Instant>,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    let error = |message: String| ViewerRuntimeLiveServerError::Init(message);
    if has_pending_intents {
        return Err(error("Unsupported: formal Viewer write reconciliation has no committed proof contract; original intents retained".into()));
    }
    if let Some(deadline) = deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(error("periodic formal read deadline exceeded".into()));
        }
        config.timeout = config.timeout.min(remaining);
    }
    let client = RemoteWorldServiceClient::new(config.clone())
        .map_err(|e| error(e.to_string()))?
        .with_query_state(query_state);
    if let Some(view) = &previous {
        client
            .resume_formal_view(view)
            .map_err(|e| error(e.to_string()))?;
    }
    let view = client
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: config.expected_world,
            scope_id: config.scope_id,
            min_commit: previous.as_ref().map(|view| view.version().commit.clone()),
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .map_err(|e| error(e.to_string()))?;
    if view.formal_commit().is_none() {
        return Err(error(
            "formal Viewer read lacks independently qualified commit".into(),
        ));
    }
    Ok(PreparedChainLinkedRuntimeUpdate {
        committed_height: view.version().commit.position,
        source: (None, None),
        source_epoch: 0,
        world: crate::runtime_host::world::new_with_state(view.projection().state.clone()),
        verified_view: Some(view),
        // Snapshot refresh does not assert a lossless Changes stream.
        service_events: Some(Vec::new()),
        intent_results: Vec::new(),
    })
}
