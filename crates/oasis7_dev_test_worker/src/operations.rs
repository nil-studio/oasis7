//! Persist originals before first write; recovery never synthesizes or re-signs one.
use crate::native::{Client, nonce};
use oasis7_agent_api::game::{GameTaskFenceV1, TaskContextAuthorityModeV1};
use oasis7_client_api::world_service::{
    entry_executor::EntryExecutorOperationV1 as Op, executor_codec as codec, task_context::*, *,
};
use oasis7_client_api::world_service_wire::{SchedulerOperationV1, WorldServicePayloadV1};
use oasis7_custody_core::dev_test::{Arguments, Original};
use serde_json::{Value, json};
type Intent = SubmitIntentRequest<WorldServicePayloadV1>;
impl Client<'_> {
    async fn prefix(&self, operation: &str) -> Result<Intent, &'static str> {
        if self
            .cache
            .read::<Value>("prepare", operation)
            .await?
            .is_some()
            || self
                .cache
                .read::<Value>("prefix", operation)
                .await?
                .is_some()
        {
            return Err("native_original_unknown");
        }
        let proof = codec::signed_prepare(
            self.resource.fence.clone(),
            operation.into(),
            nonce()?,
            Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            &self.material.executor_seed,
        )
        .map_err(|_| "native_resource_denied")?;
        self.cache.put("prepare", operation, &proof).await?;
        let prepared: PreparedTaskContextV1 = self
            .signed(
                TASK_CONTEXT_PREPARE_PATH,
                &proof,
                Op::PrepareContext(proof.clone()),
            )
            .await?;
        prepared
            .validate_against(&proof)
            .map_err(|_| "native_context_invalid")?;
        let prefix = codec::prefix(&prepared, &self.material.executor_seed)
            .map_err(|_| "native_context_invalid")?;
        self.cache.put("prefix", operation, &prefix).await?;
        Ok(prefix)
    }
    async fn captured(
        &self,
        operation: &str,
        prefix: &Intent,
    ) -> Result<CapturedTaskContextV1, &'static str> {
        let proof = codec::context_get(prefix, operation, &self.material.executor_seed)
            .map_err(|_| "native_original_invalid")?;
        let captured: CapturedTaskContextV1 = self
            .signed(
                TASK_CONTEXT_READ_PATH,
                &proof,
                Op::ReadContext(proof.clone()),
            )
            .await?;
        captured
            .validate_against(&proof)
            .map_err(|_| "native_context_invalid")?;
        if captured.prepared.identity.fence != self.resource.fence {
            return Err("native_task_fence_changed");
        }
        Ok(captured)
    }
    async fn verify_original(
        &self,
        original: &Original,
        request: &Intent,
    ) -> Result<(), &'static str> {
        codec::lookup(request).map_err(|_| "native_original_invalid")?;
        let prefix: Intent = self
            .cache
            .read("prefix", &original.native_context)
            .await?
            .ok_or("native_original_unknown")?;
        let WorldServicePayloadV1::Scheduler(signed) = &prefix.signed_payload else {
            return Err("native_original_conflict");
        };
        let SchedulerOperationV1::TaskProviderPrefix { context } = &signed.request.operation else {
            return Err("native_original_conflict");
        };
        let prepare: SignedReadRequest<TaskContextPrepareV1> = self
            .cache
            .read("prepare", &original.native_context)
            .await?
            .ok_or("native_original_unknown")?;
        verify_prepare(&prepare).map_err(|_| "native_original_invalid")?;
        let f = &context.fence;
        let digest = blake3::hash(&serde_json::to_vec(f).map_err(|_| "native_original_invalid")?)
            .to_hex()
            .to_string();
        if *f != self.resource.fence
            || digest != original.scope.fence_digest
            || f.world_id != original.scope.world
            || f.genesis_digest != original.scope.genesis_digest
            || f.agent_id != original.scope.agent
            || f.executor_binding
                .as_ref()
                .is_none_or(|b| b.executor_public_key != original.scope.executor)
            || prepare.subject_public_key != original.scope.executor
            || prepare.request.client_operation_id != original.native_context
            || prepare.request.fence != *f
            || original
                .native_context_identity
                .as_ref()
                .is_some_and(|v| v != context)
        {
            return Err("native_original_conflict");
        }
        if original.native_kind == "action" {
            let identity = original
                .native_context_identity
                .as_ref()
                .ok_or("native_original_unknown")?;
            let target = original.native_action.ok_or("native_original_unknown")?;
            codec::validate_cached_move(request, identity, target)
                .map_err(|_| "native_original_conflict")?;
        } else if request != &prefix {
            return Err("native_original_conflict");
        }
        Ok(())
    }
    async fn context_reply(
        &self,
        original: &Original,
        response: IntentResponse<Value>,
        prefix: &Intent,
    ) -> Value {
        let operation = &original.native_context;
        let mut reply = json!({"native_context":operation,"operation":response,"usable_context":false,"context_state":"unavailable"});
        if matches!(response.outcome, IntentOutcome::Committed { .. })
            && let Ok(captured) = self.captured(operation, prefix).await
        {
            let snapshot = &captured.prepared.snapshot;
            if snapshot.validate().is_ok()
                && GameTaskFenceV1::from_control(&snapshot.control) == self.resource.fence
            {
                reply = json!({"native_context":operation,"operation":response,"usable_context":true,"context_state":"verified","context":captured.prepared.identity,"active_goal":{"content":snapshot.active_goal.owner_proof.request.content,"source":{"task_fence":self.resource.fence,"active_goal_reference":snapshot.control.active_goal_reference,"owner_public_key":snapshot.active_goal.owner_proof.request.owner_public_key,"read_commit":snapshot.read_commit,"activation_commit":snapshot.activation_commit}},"actor_observation":snapshot.actor_observation,"native_authority":captured.current_public_context.native_authority});
            }
        }
        if original.historical_lookup_only && matches!(original.arguments, Arguments::Lookup { .. })
        {
            reply["lookup_only"] = json!(true);
            reply["usable_for_write"] = json!(false);
            reply["usable_context"] = json!(false);
            reply["context_state"] = json!("historical_lookup_only");
        }
        reply
    }
    pub async fn execute(&self, original: &Original, fresh: bool) -> Result<Value, &'static str> {
        let operation = &original.native_context;
        if fresh && matches!(original.arguments, Arguments::Context) {
            let prefix = self.prefix(operation).await?;
            self.verify_original(original, &prefix).await?;
            let response = self.intent(&prefix, true).await?;
            return Ok(self.context_reply(original, response, &prefix).await);
        }
        if fresh
            && let Arguments::Move { to, .. } = original.arguments
            && self
                .cache
                .read::<Intent>("action", operation)
                .await?
                .is_none()
        {
            let prefix: Intent = self
                .cache
                .read("prefix", operation)
                .await?
                .ok_or("native_original_unknown")?;
            let captured = self.captured(operation, &prefix).await?;
            if original.native_context_identity.as_ref() != Some(&captured.prepared.identity) {
                return Err("native_original_conflict");
            }
            let request = codec::move_intent(&captured, to, &self.material.executor_seed)
                .map_err(|_| "native_resource_denied")?;
            self.cache.put("action", operation, &request).await?;
            self.verify_original(original, &request).await?;
            return serde_json::to_value(self.intent(&request, true).await?)
                .map_err(|_| "native_response_unknown");
        }
        let kind = if original.native_kind == "prefix" {
            "prefix"
        } else {
            "action"
        };
        let Some(request) = self.cache.read::<Intent>(kind, operation).await? else {
            return Ok(
                json!({"status":"unknown","native_context":operation,"next":"retain_original_operation"}),
            );
        };
        self.verify_original(original, &request).await?;
        let response = self.intent(&request, false).await?;
        if original.native_kind == "prefix" {
            return Ok(self.context_reply(original, response, &request).await);
        }
        serde_json::to_value(response).map_err(|_| "native_response_unknown")
    }
}
