use super::super::protocol::{CollectDataCommand, GameplayActionError};
use super::chain_link::PreparedChainLinkedRuntimeUpdate;
use super::*;
use crate::world_service::client::{
    RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort,
};
use crate::world_service::verified_view::VerifiedWorldView;
use oasis7_client_api::world_service::{
    CommitRef, ContractError, EventCursor, ReadWorldChangesRequest, ReadWorldViewRequest,
    WORLD_SERVICE_CONTRACT_VERSION,
};

#[cfg(test)]
mod coherence_tests;
mod formal_refresh;
#[cfg(test)]
mod formal_tests;

pub(super) fn coherence_trace_request_kind(request: &ViewerRequest) -> &'static str {
    match request {
        ViewerRequest::HelloV2 { .. } => "hello_v2",
        ViewerRequest::RequestSnapshot => "request_snapshot",
        ViewerRequest::AuthoritativeRecovery { .. } => "authoritative_recovery",
        ViewerRequest::PlaybackControl { .. } => "playback_control",
        ViewerRequest::LiveControl { .. } => "live_control",
        ViewerRequest::Control { .. } => "control",
        _ => "other",
    }
}

fn same_cursor_history(left: &EventCursor, right: &EventCursor) -> bool {
    left.stream_id == right.stream_id && left.scope_id == right.scope_id && left.era == right.era
}

fn coherence_refresh_eligible(
    baseline: &EventCursor,
    requested: &EventCursor,
    target: &EventCursor,
    changes: &EventCursor,
    refresh_used: bool,
) -> Result<bool, ContractError> {
    if refresh_used
        || !same_cursor_history(baseline, requested)
        || !same_cursor_history(baseline, target)
        || !same_cursor_history(baseline, changes)
        || target.sequence < baseline.sequence
        || requested.sequence < baseline.sequence
        || requested.sequence > target.sequence
        || changes.sequence < requested.sequence
    {
        return Ok(false);
    }
    if !target.commit.satisfies_minimum(&baseline.commit)?
        || !requested.commit.satisfies_minimum(&baseline.commit)?
        || !target.commit.satisfies_minimum(&requested.commit)?
        || !changes.commit.satisfies_minimum(&requested.commit)?
    {
        return Ok(false);
    }
    Ok(!target.commit.satisfies_minimum(&changes.commit)?)
}

fn coherence_refresh_target_valid(
    baseline: &EventCursor,
    requested: &EventCursor,
    sampled_target: &EventCursor,
    changes: &EventCursor,
    refreshed: &EventCursor,
    projection_commit: &CommitRef,
) -> Result<bool, ContractError> {
    if !same_cursor_history(baseline, requested)
        || !same_cursor_history(baseline, sampled_target)
        || !same_cursor_history(baseline, changes)
        || !same_cursor_history(baseline, refreshed)
        || refreshed.sequence < baseline.sequence
        || refreshed.sequence < requested.sequence
        || refreshed.sequence < sampled_target.sequence
        || refreshed.sequence < changes.sequence
        || refreshed.commit != *projection_commit
    {
        return Ok(false);
    }
    Ok(refreshed.commit.satisfies_minimum(&baseline.commit)?
        && refreshed.commit.satisfies_minimum(&requested.commit)?
        && refreshed.commit.satisfies_minimum(&sampled_target.commit)?
        && refreshed.commit.satisfies_minimum(&changes.commit)?)
}

#[derive(Clone, Copy)]
pub(super) struct WorldServiceCoherenceTraceContext {
    pub(super) caller_phase: &'static str,
    pub(super) request_kind: &'static str,
    pub(super) session_fence: &'static str,
}

#[cfg(any(test, feature = "test_tier_required"))]
struct CursorRelationFlags {
    stream_equal: bool,
    scope_equal: bool,
    era_equal: bool,
    sequence_order: &'static str,
}

#[cfg(any(test, feature = "test_tier_required"))]
impl CursorRelationFlags {
    fn between(left: &EventCursor, right: &EventCursor) -> Self {
        Self {
            stream_equal: left.stream_id == right.stream_id,
            scope_equal: left.scope_id == right.scope_id,
            era_equal: left.era == right.era,
            sequence_order: order(left.sequence, right.sequence),
        }
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
struct CommitRelationFlags {
    world_id_equal: bool,
    genesis_digest_equal: bool,
    provider_world_id_equal: bool,
    branch_id_equal: bool,
    finality_ref_equal: bool,
    reorg_generation_equal: bool,
    governing_manifest_ref_equal: bool,
    authority_generation_equal: bool,
    permission_generation_equal: bool,
    position_order: &'static str,
    execution_block_hash_equal: bool,
    state_root_ref_equal: bool,
}

#[cfg(any(test, feature = "test_tier_required"))]
impl CommitRelationFlags {
    fn between(left: &CommitRef, right: &CommitRef) -> Self {
        Self {
            world_id_equal: left.world.world_id == right.world.world_id,
            genesis_digest_equal: left.world.genesis_digest == right.world.genesis_digest,
            provider_world_id_equal: left.binding.provider_world_id
                == right.binding.provider_world_id,
            branch_id_equal: left.binding.branch_id == right.binding.branch_id,
            finality_ref_equal: left.binding.finality_ref == right.binding.finality_ref,
            reorg_generation_equal: left.binding.reorg_generation == right.binding.reorg_generation,
            governing_manifest_ref_equal: left.binding.governing_manifest_ref
                == right.binding.governing_manifest_ref,
            authority_generation_equal: left.binding.authority_generation
                == right.binding.authority_generation,
            permission_generation_equal: left.binding.permission_generation
                == right.binding.permission_generation,
            position_order: order(left.position, right.position),
            execution_block_hash_equal: left.execution_block_hash == right.execution_block_hash,
            state_root_ref_equal: left.state_root_ref == right.state_root_ref,
        }
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
fn order(left: u64, right: u64) -> &'static str {
    match left.cmp(&right) {
        std::cmp::Ordering::Less => "less",
        std::cmp::Ordering::Equal => "equal",
        std::cmp::Ordering::Greater => "greater",
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
fn minimum_comparison_name(satisfied: bool) -> &'static str {
    if satisfied {
        "satisfied"
    } else {
        "not_satisfied"
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
fn minimum_error_name(error: &oasis7_client_api::world_service::ContractError) -> &'static str {
    match error.0 {
        "incompatible world or execution binding" => "incompatible_world_or_binding",
        "conflicting commit references" => "conflicting_commit_references",
        _ => "invalid_commit_reference",
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
fn coherence_trace_enabled() -> bool {
    std::env::var("PRE2_WORLD_COHERENCE_TRACE").is_ok_and(|value| value == "1")
}

#[cfg(any(test, feature = "test_tier_required"))]
#[expect(
    clippy::too_many_arguments,
    reason = "The diagnostic compares four distinct cursor boundaries and the authenticated view without copying their payloads"
)]
fn trace_world_service_coherence_failure(
    context: WorldServiceCoherenceTraceContext,
    config: &WorldServiceClientConfig,
    baseline: &EventCursor,
    requested: &EventCursor,
    changes: &EventCursor,
    view: &EventCursor,
    view_commit: &CommitRef,
    view_visibility_scope: &str,
    minimum_comparison: &'static str,
) {
    if !coherence_trace_enabled() {
        return;
    }
    let config_scope_world_matches = config.expected_world.world_id == view_commit.world.world_id;
    let config_scope_genesis_matches =
        config.expected_world.genesis_digest == view_commit.world.genesis_digest;
    let baseline_changes_commit = CommitRelationFlags::between(&baseline.commit, &changes.commit);
    let request_changes_commit = CommitRelationFlags::between(&requested.commit, &changes.commit);
    let view_changes_commit = CommitRelationFlags::between(view_commit, &changes.commit);
    let view_version_cursor_commit = CommitRelationFlags::between(view_commit, &view.commit);
    eprintln!(
        "PRE2_WORLD_COHERENCE_TRACE schema=v1 caller_phase={} request_kind={} session_fence={} minimum_comparison={} config_world_id_equal={} config_genesis_digest_equal={} config_scope_id_equal={} view_visibility_scope_equal={} baseline_to_changes_cursor={:?} request_to_changes_cursor={:?} changes_to_view_cursor={:?} baseline_to_changes_commit={:?} request_to_changes_commit={:?} view_version_to_changes_commit={:?} view_version_to_cursor_commit={:?}",
        context.caller_phase,
        context.request_kind,
        context.session_fence,
        minimum_comparison,
        config_scope_world_matches,
        config_scope_genesis_matches,
        config.scope_id == view.scope_id,
        config.scope_id == view_visibility_scope,
        CursorRelationFlags::between(baseline, changes),
        CursorRelationFlags::between(requested, changes),
        CursorRelationFlags::between(changes, view),
        baseline_changes_commit,
        request_changes_commit,
        view_changes_commit,
        view_version_cursor_commit,
    );
}

pub(super) struct PreparedWorldServiceSubmission {
    pub(super) correlation: oasis7_client_api::world_service::RequestCorrelation,
    pub(super) outcome:
        Result<oasis7_client_api::world_service::SubmitObservation<serde_json::Value>, String>,
    pub(super) admission_error: Option<GameplayActionError>,
    pub(super) feedback_origin: Option<super::gameplay_feedback_scope::GameplayFeedbackOrigin>,
}

impl ViewerRuntimeLiveServer {
    pub(super) fn prepare_shared_world_service_submission(
        shared: &Arc<Mutex<Self>>,
        request: &ViewerRequest,
    ) -> Result<Option<PreparedWorldServiceSubmission>, ViewerRuntimeLiveServerError> {
        let (config, query_state) = {
            let server = lock_shared_server(shared)?;
            if server.authoritative_recovery_write_fence.is_some() {
                return Ok(None);
            }
            (
                server.config.world_service.clone(),
                server.world_service_query_state.clone(),
            )
        };
        let Some(config) = config else {
            return Ok(None);
        };
        let payload = match request {
            ViewerRequest::AgentChat { request } => {
                crate::world_service::WorldServicePayloadV1::AgentChat(request.clone())
            }
            ViewerRequest::GameplayAction { request } => {
                crate::world_service::WorldServicePayloadV1::GameplayJson(
                    serde_json::to_vec(request)
                        .map_err(|e| ViewerRuntimeLiveServerError::Serde(e.to_string()))?,
                )
            }
            ViewerRequest::CollectData { command }
                if matches!(command, CollectDataCommand::Submit { .. }) =>
            {
                crate::world_service::WorldServicePayloadV1::GameplayJson(
                    serde_json::to_vec(command)
                        .map_err(|e| ViewerRuntimeLiveServerError::Serde(e.to_string()))?,
                )
            }
            _ => return Ok(None),
        };
        let correlation =
            match crate::world_service::derive_correlation(config.expected_world.clone(), &payload)
            {
                Ok(correlation) => correlation,
                Err(_) => return Ok(None), // local admission returns the auth error
            };
        // Authenticate the Viewer session before crossing the canonical Submit
        // boundary. This lock protects admission only, never HTTP I/O.
        let admission = {
            use super::super::auth::{
                verify_collect_data_auth_proof, verify_gameplay_action_auth_proof,
            };
            let mut server = lock_shared_server(shared)?;
            let verified = match request {
                ViewerRequest::AgentChat { request } => request
                    .auth
                    .as_ref()
                    .ok_or_else(|| "auth proof required".to_string())
                    .and_then(|auth| crate::viewer::verify_agent_chat_auth_proof(request, auth)),
                ViewerRequest::GameplayAction { request } => request
                    .auth
                    .as_ref()
                    .ok_or_else(|| "auth proof required".to_string())
                    .and_then(|auth| verify_gameplay_action_auth_proof(request, auth)),
                ViewerRequest::CollectData { command } => {
                    let CollectDataCommand::Submit { request } = command else {
                        unreachable!()
                    };
                    request
                        .auth
                        .as_ref()
                        .ok_or_else(|| "auth proof required".to_string())
                        .and_then(|auth| verify_collect_data_auth_proof(command, auth))
                }
                _ => unreachable!("only signed mutation requests were prepared"),
            };
            verified.and_then(|verified| {
                server
                    .session_policy
                    .validate_known_session_key(&verified.player_id, &verified.public_key)
                    .map(|_| ())?;
                Ok(server.new_gameplay_feedback_origin(&verified))
            })
        };
        let feedback_origin = match admission {
            Ok(origin) => origin,
            Err(message) => {
                return Ok(Some(PreparedWorldServiceSubmission {
                    correlation,
                    outcome: Err("Viewer admission rejected before service submission".into()),
                    admission_error: Some(GameplayActionError {
                        code: super::session_policy::map_session_policy_error_code(&message).into(),
                        message,
                        action_id: None,
                        target_agent_id: None,
                    }),
                    feedback_origin: None,
                }));
            }
        };
        let client = RemoteWorldServiceClient::new(config)
            .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?
            .with_query_state(query_state);
        let outcome = client
            .submit(oasis7_client_api::world_service::SubmitIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                correlation: correlation.clone(),
                deadline_unix_ms: None,
                signed_payload: payload,
            })
            .map_err(|e| e.to_string());
        Ok(Some(PreparedWorldServiceSubmission {
            correlation,
            outcome,
            admission_error: None,
            feedback_origin,
        }))
    }
    pub(super) fn submit_world_service_gameplay<T: serde::Serialize>(
        &mut self,
        original: &T,
    ) -> Result<(u64, Option<String>), GameplayActionError> {
        use oasis7_client_api::world_service::{
            IntentOutcome, SubmitIntentRequest, SubmitObservation,
        };
        let error = |code: &str, message: String| GameplayActionError {
            code: code.into(),
            message,
            action_id: None,
            target_agent_id: None,
        };
        let client = self
            .world_service_client()
            .map_err(|e| error("world_service_unavailable", format!("{e:?}")))?
            .ok_or_else(|| {
                error(
                    "world_service_unavailable",
                    "service is not configured".into(),
                )
            })?;
        let bytes = serde_json::to_vec(original)
            .map_err(|e| error("world_service_invalid_request", e.to_string()))?;
        let payload = crate::world_service::WorldServicePayloadV1::GameplayJson(bytes);
        let correlation = crate::world_service::derive_correlation(
            client.config().expected_world.clone(),
            &payload,
        )
        .map_err(|e| error("world_service_invalid_request", e))?;
        let origin = self.reuse_pending_gameplay_feedback_origin(
            &correlation,
            self.active_gameplay_feedback_origin.clone(),
        );
        self.active_gameplay_feedback_origin = origin.clone();
        let outcome = if let Some(prepared) = self.prepared_world_service_submission.take() {
            if prepared.correlation != correlation {
                return Err(error(
                    "world_service_invalid_request",
                    "prepared submission identity differs".into(),
                ));
            }
            if let Some(error) = prepared.admission_error {
                return Err(error);
            }
            prepared
                .outcome
                .map_err(|e| error("world_service_unavailable", e))?
        } else {
            client
                .submit(SubmitIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation: correlation.clone(),
                    deadline_unix_ms: None,
                    signed_payload: payload.clone(),
                })
                .map_err(|e| error("world_service_unavailable", e.to_string()))?
        };
        match outcome {
            SubmitObservation::OutcomeUnknown(_) => {
                self.pending_gameplay_feedback_origin(&correlation, origin);
                self.remember_pending_world_service_gameplay(correlation, payload);
                Err(error("world_service_outcome_unknown",
                    "submission outcome unknown; query the original signed request, do not replace its nonce".into()))
            }
            SubmitObservation::Response(response) => match response.outcome {
                IntentOutcome::Committed { receipt, .. } => {
                    let action_id = receipt
                        .get("action_id")
                        .and_then(serde_json::Value::as_u64)
                        .ok_or_else(|| {
                            error(
                                "world_service_invalid_receipt",
                                "receipt has no action reference".into(),
                            )
                        })?;
                    let payload_hash = receipt
                        .get("consensus_action_payload_hash")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned);
                    if let Some(origin) = origin {
                        self.set_request_gameplay_feedback(origin, Self::make_player_gameplay_feedback(
                            "world_service_intent", "committed",
                            "original signed request has an authenticated canonical disposition",
                            None, None, None,
                            Some("inspect the committed view and receipt for actual effects".into()),
                            0, 0,
                        ));
                    }
                    Ok((action_id, payload_hash))
                }
                IntentOutcome::Received { .. } | IntentOutcome::Pending => {
                    if let Some(origin) = origin.clone() {
                        self.set_request_gameplay_feedback(
                            origin,
                            Self::make_player_gameplay_feedback(
                                "world_service_intent",
                                "pending",
                                "intent received; no committed world effect yet",
                                None,
                                None,
                                None,
                                Some("wait for the original request outcome".into()),
                                0,
                                0,
                            ),
                        );
                    }
                    self.pending_gameplay_feedback_origin(&correlation, origin);
                    self.remember_pending_world_service_gameplay(correlation, payload);
                    // Legacy ACK has a scalar action ID; zero denotes no
                    // committed action reference, never execution success.
                    Ok((0, None))
                }
                other => Err(error(
                    "world_service_intent_unresolved",
                    format!("{other:?}"),
                )),
            },
        }
    }
}

pub(super) fn prepare_world_service_update(
    config: WorldServiceClientConfig,
    previous: Option<VerifiedWorldView>,
    pending: Vec<(
        oasis7_client_api::world_service::RequestCorrelation,
        crate::world_service::WorldServicePayloadV1,
    )>,
    query_state: crate::world_service::client::WorldServiceQueryState,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    prepare_world_service_update_until(config, previous, pending, query_state, None)
}

pub(super) fn prepare_world_service_update_for_shared_request(
    config: WorldServiceClientConfig,
    previous: Option<VerifiedWorldView>,
    pending: Vec<(
        oasis7_client_api::world_service::RequestCorrelation,
        crate::world_service::WorldServicePayloadV1,
    )>,
    query_state: crate::world_service::client::WorldServiceQueryState,
    context: WorldServiceCoherenceTraceContext,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    prepare_world_service_update_with_trace_context(
        config,
        previous,
        pending,
        query_state,
        None,
        context,
    )
}

pub(super) fn prepare_world_service_update_until(
    config: WorldServiceClientConfig,
    previous: Option<VerifiedWorldView>,
    pending: Vec<(
        oasis7_client_api::world_service::RequestCorrelation,
        crate::world_service::WorldServicePayloadV1,
    )>,
    query_state: crate::world_service::client::WorldServiceQueryState,
    deadline: Option<Instant>,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    let context = WorldServiceCoherenceTraceContext {
        caller_phase: if deadline.is_some() {
            "periodic_service_worker"
        } else {
            "synchronous_chain_sync"
        },
        request_kind: if deadline.is_some() {
            "periodic_read"
        } else {
            "chain_sync"
        },
        session_fence: if deadline.is_some() {
            "not_evaluated_worker_pre_apply"
        } else {
            "not_applicable_no_session"
        },
    };
    prepare_world_service_update_with_trace_context(
        config,
        previous,
        pending,
        query_state,
        deadline,
        context,
    )
}

fn prepare_world_service_update_with_trace_context(
    config: WorldServiceClientConfig,
    previous: Option<VerifiedWorldView>,
    pending: Vec<(
        oasis7_client_api::world_service::RequestCorrelation,
        crate::world_service::WorldServicePayloadV1,
    )>,
    query_state: crate::world_service::client::WorldServiceQueryState,
    deadline: Option<Instant>,
    trace_context: WorldServiceCoherenceTraceContext,
) -> Result<PreparedChainLinkedRuntimeUpdate, ViewerRuntimeLiveServerError> {
    let _ = trace_context;
    if config.formal_read_trust.is_some() {
        return formal_refresh::prepare_formal_refresh(
            config,
            previous,
            !pending.is_empty(),
            query_state,
            deadline,
        );
    }
    let client = || {
        let mut bounded = config.clone();
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(ViewerRuntimeLiveServerError::Init(
                    "periodic read deadline exceeded; original intents retained".into(),
                ));
            }
            bounded.timeout = bounded.timeout.min(remaining);
        }
        RemoteWorldServiceClient::new(bounded)
            .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))
            .map(|client| client.with_query_state(query_state.clone()))
    };
    let mut minimum = previous.as_ref().map(|view| view.version().commit.clone());
    let mut intent_results = Vec::new();
    for (correlation, payload) in pending.into_iter().take(32) {
        let result = client()?
            .lookup(
                oasis7_client_api::world_service::LookupIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    key: correlation.key,
                },
                payload,
            )
            .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?;
        if let oasis7_client_api::world_service::IntentOutcome::Committed { commit, .. } =
            &result.outcome
        {
            if let Some(current) = &minimum {
                if commit
                    .satisfies_minimum(current)
                    .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?
                {
                    minimum = Some(commit.as_ref().clone());
                }
            } else {
                minimum = Some(commit.as_ref().clone());
            }
        }
        intent_results.push(result);
    }
    let mut view = client()?
        .read_view(ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            world: config.expected_world.clone(),
            scope_id: config.scope_id.clone(),
            min_commit: minimum,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?;
    let events = if let Some(previous) = previous {
        let mut cursor = previous.continuation().clone();
        let baseline_cursor = cursor.clone();
        let mut events = Vec::new();
        let mut coherence_refresh_used = false;
        for _ in 0..8 {
            let requested_cursor = cursor.clone();
            let changes = client()?
                .read_changes(ReadWorldChangesRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    cursor: requested_cursor.clone(),
                    max_items: 256,
                    max_bytes: 1_048_576,
                })
                .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?;
            let target_cursor = view.continuation().clone();
            let cursor_mismatch = !same_cursor_history(&baseline_cursor, &requested_cursor)
                || !same_cursor_history(&baseline_cursor, &target_cursor)
                || !same_cursor_history(&baseline_cursor, &changes.next_cursor)
                || requested_cursor.sequence < baseline_cursor.sequence
                || requested_cursor.sequence > target_cursor.sequence
                || target_cursor.sequence < baseline_cursor.sequence
                || changes.next_cursor.sequence < requested_cursor.sequence;
            if cursor_mismatch {
                #[cfg(any(test, feature = "test_tier_required"))]
                if coherence_trace_enabled() {
                    let minimum_comparison = view
                        .version()
                        .commit
                        .satisfies_minimum(&changes.next_cursor.commit)
                        .map(minimum_comparison_name)
                        .unwrap_or_else(|error| minimum_error_name(&error));
                    trace_world_service_coherence_failure(
                        trace_context,
                        &config,
                        &baseline_cursor,
                        &requested_cursor,
                        &changes.next_cursor,
                        &target_cursor,
                        &view.version().commit,
                        &view.version().visibility_scope,
                        minimum_comparison,
                    );
                }
                return Err(ViewerRuntimeLiveServerError::Init(
                    "view and changes are not coherent; resynchronize".into(),
                ));
            }
            let minimum = view
                .version()
                .commit
                .satisfies_minimum(&changes.next_cursor.commit);
            let minimum_satisfied = match minimum {
                Ok(satisfied) => satisfied,
                Err(error) => {
                    #[cfg(any(test, feature = "test_tier_required"))]
                    trace_world_service_coherence_failure(
                        trace_context,
                        &config,
                        &baseline_cursor,
                        &requested_cursor,
                        &changes.next_cursor,
                        &target_cursor,
                        &view.version().commit,
                        &view.version().visibility_scope,
                        minimum_error_name(&error),
                    );
                    return Err(ViewerRuntimeLiveServerError::Init(error.to_string()));
                }
            };
            if !minimum_satisfied {
                let refresh_eligible = coherence_refresh_eligible(
                    &baseline_cursor,
                    &requested_cursor,
                    &target_cursor,
                    &changes.next_cursor,
                    coherence_refresh_used,
                )
                .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?;
                if !refresh_eligible {
                    #[cfg(any(test, feature = "test_tier_required"))]
                    trace_world_service_coherence_failure(
                        trace_context,
                        &config,
                        &baseline_cursor,
                        &requested_cursor,
                        &changes.next_cursor,
                        &target_cursor,
                        &view.version().commit,
                        &view.version().visibility_scope,
                        "not_satisfied",
                    );
                    return Err(ViewerRuntimeLiveServerError::Init(
                        "view and changes are not coherent; resynchronize".into(),
                    ));
                }
                let refreshed_view = client()?
                    .read_view(ReadWorldViewRequest {
                        observation_nonce: None,
                        formal_anchor: None,
                        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                        world: config.expected_world.clone(),
                        scope_id: config.scope_id.clone(),
                        min_commit: Some(changes.next_cursor.commit.clone()),
                        fixed_commit: None,
                        deadline_unix_ms: None,
                    })
                    .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?;
                let refreshed_target = refreshed_view.continuation();
                if !coherence_refresh_target_valid(
                    &baseline_cursor,
                    &requested_cursor,
                    &target_cursor,
                    &changes.next_cursor,
                    refreshed_target,
                    &refreshed_view.version().commit,
                )
                .map_err(|e| ViewerRuntimeLiveServerError::Init(e.to_string()))?
                {
                    return Err(ViewerRuntimeLiveServerError::Init(
                        "view and changes are not coherent; resynchronize".into(),
                    ));
                }
                view = refreshed_view;
                coherence_refresh_used = true;
            } else if changes.next_cursor.sequence > target_cursor.sequence {
                #[cfg(any(test, feature = "test_tier_required"))]
                trace_world_service_coherence_failure(
                    trace_context,
                    &config,
                    &baseline_cursor,
                    &requested_cursor,
                    &changes.next_cursor,
                    &target_cursor,
                    &view.version().commit,
                    &view.version().visibility_scope,
                    "satisfied",
                );
                return Err(ViewerRuntimeLiveServerError::Init(
                    "view and changes are not coherent; resynchronize".into(),
                ));
            }
            let stopped = changes.next_cursor == cursor;
            cursor = changes.next_cursor.clone();
            for event in changes.changes {
                events.push(
                    serde_json::from_value(event.change)
                        .map_err(|e| ViewerRuntimeLiveServerError::Serde(e.to_string()))?,
                );
            }
            if cursor.sequence == view.continuation().sequence {
                break;
            }
            if stopped {
                return Err(ViewerRuntimeLiveServerError::Init(
                    "changes stopped before view cursor".into(),
                ));
            }
        }
        if cursor.sequence != view.continuation().sequence {
            return Err(ViewerRuntimeLiveServerError::Init(
                "changes catch-up exceeds bounded sync budget".into(),
            ));
        }
        events
    } else {
        view.projection().events.clone()
    };
    let world = crate::runtime_host::world::new_with_state(view.projection().state.clone());
    Ok(PreparedChainLinkedRuntimeUpdate {
        committed_height: view.version().commit.position,
        // Legacy observer source fencing does not apply to authenticated
        // service candidates, which retain their captured transport/CAS token.
        source: (None, None),
        source_epoch: 0,
        world,
        verified_view: Some(view),
        service_events: Some(events),
        intent_results,
    })
}

#[cfg(any(test, feature = "test_tier_required"))]
impl std::fmt::Debug for CursorRelationFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorRelationFlags")
            .field("stream_equal", &self.stream_equal)
            .field("scope_equal", &self.scope_equal)
            .field("era_equal", &self.era_equal)
            .field("sequence_order", &self.sequence_order)
            .finish()
    }
}

#[cfg(any(test, feature = "test_tier_required"))]
impl std::fmt::Debug for CommitRelationFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommitRelationFlags")
            .field("world_id_equal", &self.world_id_equal)
            .field("genesis_digest_equal", &self.genesis_digest_equal)
            .field("provider_world_id_equal", &self.provider_world_id_equal)
            .field("branch_id_equal", &self.branch_id_equal)
            .field("finality_ref_equal", &self.finality_ref_equal)
            .field("reorg_generation_equal", &self.reorg_generation_equal)
            .field(
                "governing_manifest_ref_equal",
                &self.governing_manifest_ref_equal,
            )
            .field(
                "authority_generation_equal",
                &self.authority_generation_equal,
            )
            .field(
                "permission_generation_equal",
                &self.permission_generation_equal,
            )
            .field("position_order", &self.position_order)
            .field(
                "execution_block_hash_equal",
                &self.execution_block_hash_equal,
            )
            .field("state_root_ref_equal", &self.state_root_ref_equal)
            .finish()
    }
}
