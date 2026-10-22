use super::*;
use oasis7_agent_api::world_action::{Action, GeoPos, MaterialLedgerId, MaterialTransitPriority};
use oasis7_client_api::world_service::{IntentOutcome, IntentResponse};
use oasis7_game_client::{
    device_client::{BuildQuoteOutcome, Client},
    operation_store::ClientStateKind,
};
use serde_json::json;

/// SDK adapter retains the original encrypted device operation namespace.
pub struct SdkNativeTaskHost {
    client: Arc<Client>,
    run_binding: std::sync::Mutex<Option<Value>>,
}
impl SdkNativeTaskHost {
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            run_binding: std::sync::Mutex::new(None),
        }
    }
    pub fn client(&self) -> &Client {
        &self.client
    }
    /// Explicit owner-side control, intentionally absent from NativeTaskTool.
    /// Persist the returned original before submitting through client.remote.
    pub fn prepare_owner_safe_alternative(
        &self,
        proof: oasis7_client_api::world_service::SignedReadRequest<
            oasis7_client_api::safe_alternative::SafeAlternativeCommandV1,
        >,
    ) -> Result<
        oasis7_client_api::world_service::SubmitIntentRequest<
            oasis7_client_api::world_service_wire::WorldServicePayloadV1,
        >,
        String,
    > {
        self.client
            .remote
            .prepare_safe_alternative(proof)
            .map_err(|e| e.to_string())
    }
}
fn state(response: &IntentResponse<Value>) -> NativeTaskRecovery {
    match response.outcome {
        IntentOutcome::Committed { .. } => NativeTaskRecovery::Settled,
        IntentOutcome::Rejected { .. } => NativeTaskRecovery::Rejected,
        IntentOutcome::Expired => NativeTaskRecovery::Expired,
        _ => NativeTaskRecovery::Pending,
    }
}
fn outcome_state(reason: &str, response: &IntentResponse<Value>) -> NativeTaskRecovery {
    recovered_outcome(reason, state(response))
}
fn recovered_outcome(reason: &str, recovered: NativeTaskRecovery) -> NativeTaskRecovery {
    match recovered {
        NativeTaskRecovery::Settled if reason == "context_expired" => {
            NativeTaskRecovery::ContextRetired
        }
        NativeTaskRecovery::Settled => NativeTaskRecovery::Aborted,
        other => other,
    }
}
fn original_outcome_reason(
    request: &oasis7_client_api::world_service::SubmitIntentRequest<
        oasis7_client_api::world_service_wire::WorldServicePayloadV1,
    >,
) -> Result<&str, String> {
    use oasis7_client_api::world_service_wire::{SchedulerOperationV1, WorldServicePayloadV1};
    let WorldServicePayloadV1::Scheduler(signed) = &request.signed_payload else {
        return Err("invalid original context outcome".into());
    };
    let SchedulerOperationV1::TaskProviderFailure { reason, .. } = &signed.request.operation else {
        return Err("invalid original context outcome".into());
    };
    Ok(reason)
}
impl NativeTaskHost for SdkNativeTaskHost {
    fn initialize_run(&self, config: &NativeTaskRunConfig) -> Result<u32, String> {
        let control = self.current_control()?;
        let binding = json!({"config":config.binding(),"world":self.client.world,
            "agent":self.client.agent,"task_fence":control.fence});
        let spent = self.client.state.initialize_native_run(
            &config.run_id,
            &binding,
            config.max_model_calls,
        )?;
        *self
            .run_binding
            .lock()
            .map_err(|_| "native run binding lock poisoned")? = Some(binding);
        Ok(spent)
    }
    fn reserve_model_call(
        &self,
        config: &NativeTaskRunConfig,
        operation: &str,
    ) -> Result<u32, String> {
        let binding = self
            .run_binding
            .lock()
            .map_err(|_| "native run binding lock poisoned")?
            .clone()
            .ok_or("native run was not initialized")?;
        self.client.state.reserve_native_model_call(
            &config.run_id,
            &binding,
            config.max_model_calls,
            operation,
        )
    }
    fn recover(&self, operation: &str) -> Result<NativeTaskRecovery, String> {
        // Never read a new Context or quote before scanning both possible
        // terminal originals and the existing prefix under this operation ID.
        let action = self
            .client
            .cached_intent(ClientStateKind::Action, operation)?;
        let outcome = self
            .client
            .cached_intent(ClientStateKind::Outcome, operation)?;
        let prefix = self
            .client
            .cached_intent(ClientStateKind::Prefix, operation)?;
        if action.is_some() && outcome.is_some() {
            return Err("context has conflicting original action and outcome; retain both".into());
        }
        if let Some(original) = action {
            return Ok(state(&self.client.poll_original(&original)?));
        }
        if let Some(original) = outcome {
            return Ok(outcome_state(
                original_outcome_reason(&original)?,
                &self.client.poll_original(&original)?,
            ));
        }
        if let Some(original) = prefix {
            let result = state(&self.client.poll_original(&original)?);
            return Ok(if result == NativeTaskRecovery::Settled {
                NativeTaskRecovery::Ready
            } else {
                result
            });
        }
        Ok(NativeTaskRecovery::Vacant)
    }
    fn known_rejection_feedback(
        &self,
        operation: &str,
    ) -> Result<Option<oasis7_game_client::AuthorizedFeedbackRecord>, String> {
        self.client.known_stale_rejection(operation)
    }
    fn settle_exhausted_context(&self, operation: &str) -> Result<NativeTaskRecovery, String> {
        // An already signed action keeps its terminal lane; budget cleanup must
        // never create a competing outcome or replace an unknown operation.
        if self
            .client
            .cached_intent(ClientStateKind::Action, operation)?
            .is_some()
        {
            return Err("context has an original action; recover that result instead".into());
        }
        if let Some(original) = self
            .client
            .cached_intent(ClientStateKind::Outcome, operation)?
        {
            return Ok(state(&self.client.poll_original(&original)?));
        }
        let prefix = self
            .client
            .cached_intent(ClientStateKind::Prefix, operation)?
            .ok_or("original context prefix unavailable")?;
        let oasis7_client_api::world_service_wire::WorldServicePayloadV1::Scheduler(signed) =
            prefix.signed_payload
        else {
            return Err("invalid original context prefix".into());
        };
        let oasis7_client_api::world_service_wire::SchedulerOperationV1::TaskProviderPrefix {
            context,
        } = signed.request.operation
        else {
            return Err("invalid original context prefix".into());
        };
        let current = self.current_control()?;
        if current.fence != self.initial_fence()?
            || current.fence != context.fence
            || current.fence.task_status != GameTaskStatusV1::Active
        {
            return Err("task control changed before context termination".into());
        }
        // Client persists this one original failure before its first submission,
        // applies the existing expiry/base checks, and never advances world time.
        Ok(state(
            &self.client.outcome(operation, "transport_exhausted")?,
        ))
    }
    fn retire_expired_context(
        &self,
        operation: &str,
    ) -> Result<Option<NativeTaskRecovery>, String> {
        // Recheck immutable terminal lanes after the complete recovery scan.
        // No competing cleanup may replace an action or another original outcome.
        if self
            .client
            .cached_intent(ClientStateKind::Action, operation)?
            .is_some()
        {
            return Err("context has an original action; recover that result instead".into());
        }
        if let Some(original) = self
            .client
            .cached_intent(ClientStateKind::Outcome, operation)?
        {
            return Ok(Some(outcome_state(
                original_outcome_reason(&original)?,
                &self.client.poll_original(&original)?,
            )));
        }
        let prefix = self
            .client
            .cached_intent(ClientStateKind::Prefix, operation)?
            .ok_or("original context prefix unavailable")?;
        let result = state(&self.client.poll_original(&prefix)?);
        if result != NativeTaskRecovery::Settled {
            return Ok(Some(result));
        }
        use oasis7_client_api::world_service_wire::{SchedulerOperationV1, WorldServicePayloadV1};
        let WorldServicePayloadV1::Scheduler(signed) = prefix.signed_payload else {
            return Err("invalid original context prefix".into());
        };
        let SchedulerOperationV1::TaskProviderPrefix { context } = signed.request.operation else {
            return Err("invalid original context prefix".into());
        };
        let current = self.current_control()?;
        if current.fence != self.initial_fence()?
            || current.fence != context.fence
            || current.fence.task_status != GameTaskStatusV1::Active
        {
            return Err("task control changed before expired context retirement".into());
        }
        if current.logical_tick <= context.valid_until_tick.0 {
            return Ok(None);
        }
        // Client signs from the original public prefix and current public base;
        // this path never decrypts/reads the expired private context body.
        Ok(Some(outcome_state(
            "context_expired",
            &self.client.outcome(operation, "context_expired")?,
        )))
    }
    fn prepare(&self, operation: &str) -> Result<NativeTaskContext, String> {
        if self.current_control()?.fence != self.initial_fence()? {
            return Err("task fence differs from immutable native run binding".into());
        }
        let prefix = self.client.prefix(operation)?;
        let result = self.client.submit_and_poll(&prefix)?;
        if state(&result) != NativeTaskRecovery::Settled {
            return Err(
                "context prefix has not committed; retain original operation and lookup".into(),
            );
        }
        // Native object grants are absent from TaskPublicContextV1. Obtain
        // them only from a currently authorized, signed read at its exact
        // commit; a historical read could retain a revoked reader's scope.
        // Drift retries reread this same committed context, never its prefix.
        let mut matching = None;
        for _ in 0..3 {
            let captured = self.client.captured(operation)?;
            let view = self.client.public_view()?;
            if view.task_fence() != Some(&captured.prepared.identity.fence)
                || captured.prepared.identity.fence != self.initial_fence()?
                || view.logical_tick() > captured.prepared.valid_until_tick.0
            {
                return Err(
                    "native capability discovery authority or context lease changed".into(),
                );
            }
            if view.version().commit == captured.current_public_context.read_commit
                && view.logical_tick()
                    == captured
                        .current_public_context
                        .captured_base_binding
                        .base_tick
            {
                matching = Some((captured, view));
                break;
            }
        }
        let (captured, view) = matching
            .ok_or("native capability discovery drifted; retain original context and refresh")?;
        let grants: Vec<_> = view
            .projection()
            .native_delegation_authorizations
            .iter()
            .filter(|authorization| {
                let grant = &authorization.grant;
                !grant.revoked
                    && grant.valid_from_tick <= view.logical_tick()
                    && view.logical_tick() <= grant.valid_until_tick
                    && authorization.remaining_units > 0
            })
            .collect();
        let prepared = &captured.prepared;
        Ok(NativeTaskContext {
            operation: operation.into(),
            fence: prepared.identity.fence.clone(),
            valid_until_tick: prepared.valid_until_tick.0,
            model_context: json!({
                "goal":{"revision":prepared.snapshot.control.active_goal_revision,
                    "content":prepared.snapshot.active_goal.owner_proof.request.content},
                "actor_observation_schema":prepared.snapshot.actor_observation_schema,
                "actor_observation":prepared.snapshot.actor_observation,
                "current_actor_observation":captured.current_public_context.actor_observation,
                "capability_catalog":captured.current_public_context.provider_authority.as_ref().map(|authority| &authority.capability_catalog),
                "native_authority":captured.current_public_context.native_authority,
                "authority_mode":captured.current_public_context.authority_mode()?,
                "native_delegation_authorizations":grants.iter().take(128).collect::<Vec<_>>(),
                "native_authorization_count":grants.len(),
                "native_authorizations_truncated":grants.len()>128,
                "native_authorization_read_commit":view.version().commit,
                "native_authorization_logical_tick":view.logical_tick(),
                "native_authorization_scope":"current action/object grants only; choose BuildFactory factory ID from grant.object_id; admission still required",
                "native_industry_profile":true,"task_fence":prepared.identity.fence,
                "capture_commit":captured.capture_commit,"current_read_commit":captured.current_public_context.read_commit,
                "context_identity":prepared.identity,
            }),
        })
    }
    fn current_control(&self) -> Result<NativeTaskControl, String> {
        let view = self.client.public_view()?;
        let fence = view
            .task_fence()
            .ok_or("native task control unavailable")?
            .clone();
        Ok(NativeTaskControl {
            fence,
            logical_tick: view.logical_tick(),
        })
    }
    fn initial_fence(&self) -> Result<GameTaskFenceV1, String> {
        let binding = self
            .run_binding
            .lock()
            .map_err(|_| "native run binding lock poisoned")?;
        let binding = binding.as_ref().ok_or("native run was not initialized")?;
        serde_json::from_value(binding["task_fence"].clone())
            .map_err(|_| "invalid native run task binding".into())
    }
    fn invoke(
        &self,
        context: &NativeTaskContext,
        tool: &NativeTaskTool,
    ) -> Result<NativeTaskToolResult, String> {
        let operation = &context.operation;
        let mut effect = None;
        let mut decision_required = false;
        let value = match tool {
            NativeTaskTool::Wait(v) => {
                // Observe independently progressing Runtime time. This never
                // steps the World or extends the original context's validity.
                let initial = self.current_control()?;
                let target = initial
                    .logical_tick
                    .checked_add(u64::from(v.ticks))
                    .filter(|tick| *tick <= context.valid_until_tick)
                    .ok_or("wait would exceed original context validity")?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
                loop {
                    let current = self.current_control()?;
                    if current.fence != context.fence
                        || current.fence.task_status != GameTaskStatusV1::Active
                        || current.logical_tick > context.valid_until_tick
                    {
                        // Let the runner classify the authenticated control
                        // change on its next mandatory check, before inference
                        // or another tool. No effect was attempted.
                        return Ok(NativeTaskToolResult {
                            value: json!({"wait":"interrupted"}),
                            effect: None,
                            decision_required: false,
                        });
                    }
                    if current.logical_tick >= target {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err("world progress wait timed out".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                self.client.industry()?
            }
            NativeTaskTool::Observe(_) => {
                let view = self.client.public_view()?;
                json!({"read_version":view.version(),"logical_tick":view.logical_tick(),"observation":view.raw_projection()})
            }
            NativeTaskTool::Industry(_) => self.client.industry()?,
            NativeTaskTool::Capabilities(_) => self.client.capabilities()?,
            NativeTaskTool::QuoteBuild(v) => match self
                .client
                .quote_build_outcome(operation, &v.site, &v.factory, &v.kind)?
            {
                BuildQuoteOutcome::Quote(quoted) => {
                    decision_required = quoted.quote.rejection.is_some();
                    serde_json::to_value(quoted).map_err(|_| "native quote encoding failed")?
                }
                BuildQuoteOutcome::Denied(denied) => {
                    decision_required = true;
                    json!({"status":"selection_rejected","reason":denied.reason,
                        "no_action_submitted":true,"next":"read_capabilities_and_choose_authorized_factory",
                        "verified_quote_denial":denied})
                }
                BuildQuoteOutcome::Original(original) => original,
            },
            NativeTaskTool::QuoteSchedule(v) => self
                .client
                .quote_schedule(operation, &v.factory, &v.recipe, v.batches)?,
            NativeTaskTool::Build(v) => {
                let result = self.client.build(operation, &v.site, &v.factory, &v.kind)?;
                effect = Some(state(&result));
                serde_json::to_value(result).map_err(|_| "native result encoding failed")?
            }
            NativeTaskTool::Schedule(v) => {
                let result = self
                    .client
                    .schedule(operation, &v.factory, &v.recipe, v.batches)?;
                effect = Some(state(&result));
                serde_json::to_value(result).map_err(|_| "native result encoding failed")?
            }
            NativeTaskTool::Move(v) => {
                let [x, y, z] = v.to;
                let result = self.client.submit_action(
                    operation,
                    Action::MoveAgent {
                        agent_id: self.client.agent.clone(),
                        to: GeoPos::new(x, y, z),
                    },
                )?;
                effect = Some(state(&result));
                serde_json::to_value(result).map_err(|_| "native result encoding failed")?
            }
            NativeTaskTool::Transfer(v) => {
                let priority = match v.priority.as_str() {
                    "standard" => MaterialTransitPriority::Standard,
                    "urgent" => MaterialTransitPriority::Urgent,
                    _ => return Err("invalid native priority".into()),
                };
                let result = self.client.submit_action(
                    operation,
                    Action::TransferMaterial {
                        requester_agent_id: self.client.agent.clone(),
                        from_ledger: MaterialLedgerId::try_from(v.from.clone())
                            .map_err(|_| "invalid source ledger")?,
                        to_ledger: MaterialLedgerId::try_from(v.to.clone())
                            .map_err(|_| "invalid destination ledger")?,
                        kind: v.material.clone(),
                        amount: v.amount,
                        distance_km: v.distance_km,
                        priority: Some(priority),
                        route_id: None,
                        route_ids: v.routes.clone(),
                        auto_reroute: false,
                    },
                )?;
                effect = Some(state(&result));
                serde_json::to_value(result).map_err(|_| "native result encoding failed")?
            }
            NativeTaskTool::Abort(_) => {
                let result = self.client.outcome(operation, "executor_aborted")?;
                effect = Some(state(&result));
                serde_json::to_value(result).map_err(|_| "native result encoding failed")?
            }
        };
        Ok(NativeTaskToolResult {
            value,
            effect,
            decision_required,
        })
    }
}

#[cfg(test)]
mod expiry_tests {
    use super::*;
    #[test]
    fn only_committed_fixed_expiry_outcome_is_skippable_on_restart() {
        assert_eq!(
            recovered_outcome("context_expired", NativeTaskRecovery::Settled),
            NativeTaskRecovery::ContextRetired
        );
        for reason in [
            "executor_aborted",
            "provider_failed",
            "transport_exhausted",
            "unknown",
        ] {
            assert_eq!(
                recovered_outcome(reason, NativeTaskRecovery::Settled),
                NativeTaskRecovery::Aborted
            );
        }
        for state in [
            NativeTaskRecovery::Pending,
            NativeTaskRecovery::Rejected,
            NativeTaskRecovery::Expired,
        ] {
            assert_eq!(recovered_outcome("context_expired", state), state);
        }
    }
}
