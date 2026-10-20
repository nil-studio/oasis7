//! Local owner authority, never a model-facing executor interface.
use super::*;
use oasis7_client_api::world_service::{
    entry_control::EntryOwnerOperationV1,
    private_goal::{self, PreparedGoalReceiptV1, SignedGoalPrepareV1},
    private_goal_read::*,
};
use oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1;

#[cfg(test)]
#[path = "entry_owner_client_tests.rs"]
mod tests;

/// Non-secret progress only. A committed receipt still requires a current
/// canonical read before executing against a task.
#[derive(Clone, Serialize, Deserialize)]
pub struct EntryOwnerProgress {
    pub stage: String,
    pub operation_id: String,
    pub state: String,
    pub next_action: String,
    pub resource_scope: String,
    pub payload_digest: String,
    pub canonical_commit: Option<CommitRef>,
    pub receipt_ref: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct OwnerIntentOriginal {
    participant_operation: String,
    resource_scope: String,
    payload: WorldServicePayloadV1,
}

fn resource_scope(
    base: &str,
    world: &WorldIdentity,
    agent: &str,
    task: Option<&str>,
    generation: u64,
) -> Result<String, String> {
    ws::request_digest(
        "entry-owner-resource/v1",
        &(base, world, agent, task, generation),
    )
}

fn verify_intent_response(
    request: &impl Serialize,
    path: &str,
    value: Value,
    correlation: &RequestCorrelation,
    key: &str,
) -> Result<IntentResponse<Value>, String> {
    let signed: SignedServiceResponse<IntentResponse<Value>> =
        serde_json::from_value(value).map_err(|_| "owner result unknown; retain original")?;
    ws::verify_service_response(path, &ws::request_digest(path, request)?, &signed, key)?;
    signed
        .payload
        .validate(correlation)
        .map_err(|e| e.to_string())?;
    if signed.payload.execution_evidence_scope.is_some() || signed.payload.formal_profile.is_some()
    {
        return Err("owner result requires configured ordinary service profile; formal proof downgrade forbidden".into());
    }
    if let IntentOutcome::Committed { commit, .. } = &signed.payload.outcome
        && commit.world != correlation.key.world
    {
        return Err("owner result world mismatch".into());
    }
    Ok(signed.payload)
}

impl ParticipantClient {
    fn reserve_owner_operation(
        &self,
        participant: &str,
        operation: &str,
        purpose: &str,
    ) -> Result<(), String> {
        validate_operation(operation)?;
        let expected = (participant.to_string(), purpose.to_string());
        let stored = self.owner.put_original(
            &self.scope,
            operation,
            "owner-operation-purpose",
            &expected,
        )?;
        if stored != expected {
            return Err("owner operation purpose or resource conflict".into());
        }
        Ok(())
    }
    /// Explicit creation of a separately encrypted executor vault. Never uses
    /// the owner or login directory and never returns a seed to the caller.
    pub fn initialize_executor(
        &self,
        directory: &std::path::Path,
        passphrase: &[u8],
    ) -> Result<String, String> {
        if !directory.is_absolute()
            || directory.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )
            })
            || directory == self.config.owner_state_directory
            || directory == self.config.identity.state_directory
            || directory.canonicalize().ok().is_some_and(|canonical| {
                self.config
                    .owner_state_directory
                    .canonicalize()
                    .ok()
                    .as_ref()
                    == Some(&canonical)
                    || self
                        .config
                        .identity
                        .state_directory
                        .canonicalize()
                        .ok()
                        .as_ref()
                        == Some(&canonical)
            })
        {
            return Err("executor vault must use an independent directory".into());
        }
        let store = oasis7_client_storage::identity_store::ExecutorControlStore::create(
            directory, passphrase,
        )?;
        let public = hex::encode(
            SigningKey::from_bytes(store.executor_seed())
                .verifying_key()
                .to_bytes(),
        );
        self.validate_executor_key(&public)?;
        Ok(public)
    }
    /// Historical evidence only. Does not revalidate ownership, task grant,
    /// branch, goal activation, session, or current executor permission.
    pub fn owner_operation_status(
        &self,
        participant: &str,
        operation: &str,
    ) -> Result<EntryOwnerProgress, String> {
        let Some(original) = self.stored_intent(participant, operation)? else {
            if let Some((bound, scope, _)) = self
                .owner
                .read_original::<(String, String, SignedGoalPrepareV1)>(
                    &self.scope,
                    operation,
                    "owner-goal-original",
                )?
            {
                if bound != participant {
                    return Err("goal participant resource conflict".into());
                }
                return Ok(EntryOwnerProgress {
                    stage: "goal_prepare".into(),
                    operation_id: operation.into(),
                    state: "unknown".into(),
                    next_action: "reconcile_original_private_prepare".into(),
                    resource_scope: scope,
                    payload_digest: String::new(),
                    canonical_commit: None,
                    receipt_ref: None,
                });
            }
            return Ok(EntryOwnerProgress {
                stage: "owner_control".into(),
                operation_id: operation.into(),
                state: "not_started".into(),
                next_action: "explicit_owner_action_required".into(),
                resource_scope: String::new(),
                payload_digest: String::new(),
                canonical_commit: None,
                receipt_ref: None,
            });
        };
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            self.config.world.clone(),
            &original.payload,
        )?;
        let settled = self.owner.read_original::<IntentResponse<Value>>(
            &original.resource_scope,
            operation,
            "owner-settlement",
        )?;
        let mut canonical_commit = None;
        let mut receipt_ref = None;
        if let Some(settled) = settled {
            settled.validate(&correlation).map_err(|e| e.to_string())?;
            if let IntentOutcome::Committed { commit, receipt } = &settled.outcome {
                let submit = SubmitIntentRequest {
                    contract_version: 1,
                    correlation: correlation.clone(),
                    deadline_unix_ms: None,
                    signed_payload: original.payload.clone(),
                };
                validate_owner_receipt(&submit, commit.position, receipt)?;
                canonical_commit = Some((**commit).clone());
                receipt_ref = Some(ws::request_digest("owner-public-receipt/v1", receipt)?);
            } else {
                return Err("invalid local settlement".into());
            }
        }
        Ok(EntryOwnerProgress {
            stage: correlation.key.operation_domain,
            operation_id: operation.into(),
            state: "unknown".into(),
            next_action: "query_original_and_read_current_canonical_control".into(),
            resource_scope: original.resource_scope,
            payload_digest: correlation.payload_digest,
            canonical_commit,
            receipt_ref,
        })
    }
    fn owner_private(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(hex::encode(self.key.to_bytes()))
    }
    fn stored_intent(
        &self,
        participant: &str,
        operation: &str,
    ) -> Result<Option<OwnerIntentOriginal>, String> {
        validate_operation(operation)?;
        let stored = self.owner.read_original::<OwnerIntentOriginal>(
            &self.scope,
            operation,
            "owner-intent-index",
        )?;
        if stored
            .as_ref()
            .is_some_and(|s| s.participant_operation != participant)
        {
            return Err("owner operation resource conflict".into());
        }
        Ok(stored)
    }
    fn save_intent(
        &self,
        participant: &str,
        operation: &str,
        agent: &str,
        task: Option<&str>,
        generation: u64,
        payload: WorldServicePayloadV1,
    ) -> Result<OwnerIntentOriginal, String> {
        let resource_scope =
            resource_scope(&self.scope, &self.config.world, agent, task, generation)?;
        let original = OwnerIntentOriginal {
            participant_operation: participant.into(),
            resource_scope,
            payload,
        };
        let saved: OwnerIntentOriginal =
            self.owner
                .put_original(&self.scope, operation, "owner-intent-index", &original)?;
        if serde_json::to_value(&saved).map_err(|_| "owner original encoding failed")?
            != serde_json::to_value(&original).map_err(|_| "owner original encoding failed")?
        {
            return Err("owner operation content conflict".into());
        }
        Ok(saved)
    }
    fn settle_owner_intent(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        original: &OwnerIntentOriginal,
    ) -> Result<EntryOwnerProgress, String> {
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            self.config.world.clone(),
            &original.payload,
        )?;
        if correlation.key.verified_subject != hex::encode(self.key.verifying_key().to_bytes()) {
            return Err("owner intent subject mismatch".into());
        }
        let lookup = AuthenticatedLookup {
            request: LookupIntentRequest {
                contract_version: 1,
                key: correlation.key.clone(),
            },
            original: original.payload.clone(),
        };
        let submit = SubmitIntentRequest {
            contract_version: 1,
            correlation: correlation.clone(),
            deadline_unix_ms: None,
            signed_payload: original.payload.clone(),
        };
        // Store all fixed wire originals before even the first lookup. Fresh
        // identity admission is transport authorization, never a new business op.
        let (lookup, submit): (
            AuthenticatedLookup,
            SubmitIntentRequest<WorldServicePayloadV1>,
        ) = self.owner.put_original(
            &original.resource_scope,
            operation,
            "owner-wire-original",
            &(lookup, submit),
        )?;
        let reply = self.owner_control_call(
            participant,
            login,
            EntryOwnerOperationV1::Lookup(lookup.clone()),
        )?;
        let mut response = verify_intent_response(
            &lookup,
            LOOKUP_PATH,
            reply,
            &correlation,
            &self.config.trusted_world_service_key,
        )?;
        if matches!(response.outcome, IntentOutcome::Unknown) {
            let reply = self.owner_control_call(
                participant,
                login,
                EntryOwnerOperationV1::Submit(submit.clone()),
            )?;
            response = verify_intent_response(
                &submit,
                SUBMIT_PATH,
                reply,
                &correlation,
                &self.config.trusted_world_service_key,
            )?;
        }
        let (state, next) = match &response.outcome {
            IntentOutcome::Committed { commit, receipt } => {
                validate_owner_receipt(&submit, commit.position, receipt)?;
                self.owner.put_original(
                    &original.resource_scope,
                    operation,
                    "owner-settlement",
                    &response,
                )?;
                ("confirmed", "read_current_canonical_control")
            }
            IntentOutcome::Rejected { .. } | IntentOutcome::Expired => {
                ("blocked", "inspect_original_rejection")
            }
            IntentOutcome::Received { .. } => ("pending", "query_original"),
            _ => ("unknown", "query_original"),
        };
        let (canonical_commit, receipt_ref) = match &response.outcome {
            IntentOutcome::Committed { commit, receipt } => (
                Some((**commit).clone()),
                Some(ws::request_digest("owner-public-receipt/v1", receipt)?),
            ),
            _ => (None, None),
        };
        Ok(EntryOwnerProgress {
            stage: correlation.key.operation_domain,
            operation_id: operation.into(),
            state: state.into(),
            next_action: next.into(),
            resource_scope: original.resource_scope.clone(),
            payload_digest: correlation.payload_digest,
            canonical_commit,
            receipt_ref,
        })
    }
    /// Creates a suspended task. Retries reconcile its exact original, including
    /// the captured branch/owner fence, before consulting a newer fence.
    pub fn create_task(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
    ) -> Result<EntryOwnerProgress, String> {
        self.reserve_owner_operation(participant, operation, "task_create")?;
        if let Some(original) = self.stored_intent(participant, operation)? {
            if !matches!(original.payload, WorldServicePayloadV1::GameTaskCreate(_)) {
                return Err("owner operation purpose conflict".into());
            }
            return self.settle_owner_intent(participant, login, operation, &original);
        }
        let current = self.current_owner(participant, login)?;
        if current.task.is_some() {
            return Err("task already exists; read current task instead".into());
        }
        let owner = &current.owner;
        let request = GameTaskCreateV1 {
            api_rev: 1,
            client_operation_id: operation.into(),
            world_id: self.config.world.world_id.clone(),
            genesis_digest: self.config.world.genesis_digest.clone(),
            branch_id: owner.canonical_authority.branch_id.clone(),
            reorg_epoch: DecimalU64(owner.reorg_epoch),
            agent_id: owner.agent_id.clone(),
            owner_binding: owner.player_id.clone(),
            owner_generation: DecimalU64(owner.canonical_authority.agent_identity_generation),
        };
        request.validate()?;
        let payload = WorldServicePayloadV1::GameTaskCreate(ws::sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            request,
            &self.owner_private(),
        )?);
        let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
            self.config.world.clone(),
            &payload,
        )?;
        let task_id = format!(
            "game-task-{}",
            ws::request_digest("game-task-id", &correlation.key)?
        );
        let original = self.save_intent(
            participant,
            operation,
            &owner.agent_id,
            Some(&task_id),
            owner.canonical_authority.agent_identity_generation,
            payload,
        )?;
        self.settle_owner_intent(participant, login, operation, &original)
    }
    /// Independent, explicit owner action. The client never chooses funding,
    /// amount, quotas or eligibility, and never treats registration as a gift.
    pub fn claim_starter(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
    ) -> Result<EntryOwnerProgress, String> {
        self.reserve_owner_operation(participant, operation, "starter_claim")?;
        if let Some(original) = self.stored_intent(participant, operation)? {
            if !matches!(original.payload, WorldServicePayloadV1::GameStarterClaim(_)) {
                return Err("starter operation purpose conflict".into());
            }
            return self.settle_owner_intent(participant, login, operation, &original);
        }
        let current = self.current_owner(participant, login)?;
        let owner = &current.owner;
        let request = GameStarterClaimV1 {
            api_rev: 1,
            client_operation_id: operation.into(),
            world_id: self.config.world.world_id.clone(),
            genesis_digest: self.config.world.genesis_digest.clone(),
            branch_id: owner.canonical_authority.branch_id.clone(),
            reorg_epoch: DecimalU64(owner.reorg_epoch),
            agent_id: owner.agent_id.clone(),
            owner_binding: owner.player_id.clone(),
            owner_generation: DecimalU64(owner.canonical_authority.agent_identity_generation),
        };
        request.validate()?;
        let payload = WorldServicePayloadV1::GameStarterClaim(ws::sign_read_request(
            GAME_STARTER_CLAIM_DOMAIN,
            request,
            &self.owner_private(),
        )?);
        let original = self.save_intent(
            participant,
            operation,
            &owner.agent_id,
            None,
            owner.canonical_authority.agent_identity_generation,
            payload,
        )?;
        self.settle_owner_intent(participant, login, operation, &original)
    }
    /// Full private content stays in the protected owner outbox and private
    /// gateway. This method never publishes it or activates an executor.
    pub fn prepare_goal(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        content: GoalContentV1,
    ) -> Result<PreparedGoalReceiptV1, String> {
        validate_operation(operation)?;
        content.validate().map_err(str::to_owned)?;
        let expected_content = content.clone();
        self.reserve_owner_operation(participant, operation, "goal_prepare")?;
        let prior = self
            .owner
            .read_original::<(String, String, SignedGoalPrepareV1)>(
                &self.scope,
                operation,
                "owner-goal-original",
            )?;
        let (bound, scope, original) = if let Some(prior) = prior {
            if prior.0 != participant || prior.2.request.content != content {
                return Err("private goal operation content conflict".into());
            }
            prior
        } else {
            let current = self.current_owner(participant, login)?;
            let task = current
                .task
                .ok_or("create and confirm task before preparing goal")?;
            let revision = DecimalU64(
                task.latest_goal_revision
                    .0
                    .checked_add(1)
                    .ok_or("goal revision overflow")?,
            );
            let mut nonce = [0u8; 32];
            getrandom::fill(&mut nonce).map_err(|_| "goal nonce unavailable")?;
            let request = GameGoalPrepareV1 {
                api_rev: 1,
                world_id: task.world_id.clone(),
                genesis_digest: task.genesis_digest.clone(),
                task_id: task.task_id.clone(),
                agent_id: task.agent_id.clone(),
                owner_public_key: task.owner_public_key.clone(),
                owner_binding: task.owner_binding.clone(),
                owner_generation: task.owner_generation,
                branch_id: task.branch_id.clone(),
                reorg_epoch: task.reorg_epoch,
                expected_task_revision: task.task_revision,
                expected_latest_goal_revision: task.latest_goal_revision,
                goal_revision: revision,
                client_operation_id: operation.into(),
                client_nonce: hex::encode(nonce),
                goal_digest: game_goal_version_digest(&task.task_id, revision, &content)
                    .map_err(str::to_owned)?,
                content,
            };
            let original = private_goal::sign_goal_prepare(request, &self.owner_private())?;
            let scope = resource_scope(
                &self.scope,
                &self.config.world,
                &task.agent_id,
                Some(&task.task_id),
                task.owner_generation.0,
            )?;
            self.owner.put_original(
                &self.scope,
                operation,
                "owner-goal-original",
                &(participant.to_string(), scope, original),
            )?
        };
        if bound != participant || original.request.content != expected_content {
            return Err("goal resource conflict".into());
        }
        original.verify()?;
        let reply = self.owner_control_call(
            participant,
            login,
            EntryOwnerOperationV1::PrepareGoal(original.clone()),
        )?;
        let response: SignedServiceResponse<PreparedGoalReceiptV1> = serde_json::from_value(reply)
            .map_err(|_| "private goal result unknown; retain original")?;
        verify_prepare_response(&original, &response, &self.config.trusted_world_service_key)?;
        self.owner
            .put_original(&scope, operation, "owner-prepared-goal", &response)?;
        Ok(response.payload)
    }
    pub fn publish_prepared_goal(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        prepare_operation: &str,
    ) -> Result<EntryOwnerProgress, String> {
        self.reserve_owner_operation(participant, operation, "goal_publish")?;
        let (bound, scope, proof) = self
            .owner
            .read_original::<(String, String, SignedGoalPrepareV1)>(
                &self.scope,
                prepare_operation,
                "owner-goal-original",
            )?
            .ok_or("private goal original missing")?;
        if bound != participant {
            return Err("goal participant resource conflict".into());
        }
        let response = self
            .owner
            .read_original::<SignedServiceResponse<PreparedGoalReceiptV1>>(
                &scope,
                prepare_operation,
                "owner-prepared-goal",
            )?
            .ok_or("goal prepare unknown; reconcile original prepare")?;
        verify_prepare_response(&proof, &response, &self.config.trusted_world_service_key)?;
        let publish = publication(&proof, &response.payload, operation.into())?;
        let expected = WorldServicePayloadV1::GameGoalPublish(ws::sign_read_request(
            GAME_GOAL_PUBLISH_DOMAIN,
            publish,
            &self.owner_private(),
        )?);
        let original = if let Some(original) = self.stored_intent(participant, operation)? {
            if serde_json::to_value(&original.payload).map_err(|_| "goal encoding failed")?
                != serde_json::to_value(&expected).map_err(|_| "goal encoding failed")?
            {
                return Err("goal publication operation conflict".into());
            }
            original
        } else {
            self.save_intent(
                participant,
                operation,
                &proof.request.agent_id,
                Some(&proof.request.task_id),
                proof.request.owner_generation.0,
                expected,
            )?
        };
        self.settle_owner_intent(participant, login, operation, &original)
    }
    /// Owner-only full private goal subset; never return this to a model-facing
    /// executor as proof of a task grant or context authorization.
    pub fn read_current_goal(
        &self,
        participant: &str,
        login: &str,
    ) -> Result<CurrentGoalResponse, String> {
        let current = self.current_owner(participant, login)?;
        let task = current.task.ok_or("current task unavailable")?;
        let request = ws::sign_read_request(
            GOAL_CURRENT_PATH,
            CurrentGoalRequest {
                schema_revision: 1,
                world: self.config.world.clone(),
                agent_id: task.agent_id.clone(),
                task_id: task.task_id.clone(),
            },
            &self.owner_private(),
        )?;
        let reply = self.owner_control_call(
            participant,
            login,
            EntryOwnerOperationV1::ReadGoal(request.clone()),
        )?;
        let response: SignedServiceResponse<CurrentGoalResponse> =
            serde_json::from_value(reply).map_err(|_| "current private goal malformed")?;
        verify_current_response(&request, &response, &self.config.trusted_world_service_key)?;
        if response.payload.control != task
            || response.payload.read_commit.binding != current.version.commit.binding
            || response.payload.read_commit.position < current.version.commit.position
        {
            return Err(
                "current private goal differs from fresh canonical task observation".into(),
            );
        }
        Ok(response.payload)
    }
    /// Explicit owner authorization. The executor key must come from its own
    /// private vault; neither login nor owner keys are valid delegates.
    pub fn authorize_executor(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        executor_key: &str,
        generation: u64,
        nonce: u64,
    ) -> Result<EntryOwnerProgress, String> {
        self.change_executor_authorization(
            participant,
            login,
            operation,
            executor_key,
            (generation, nonce),
            false,
        )
    }
    pub fn revoke_executor(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        executor_key: &str,
        generation: u64,
        nonce: u64,
    ) -> Result<EntryOwnerProgress, String> {
        self.change_executor_authorization(
            participant,
            login,
            operation,
            executor_key,
            (generation, nonce),
            true,
        )
    }
    fn change_executor_authorization(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        executor_key: &str,
        revision: (u64, u64),
        revoked: bool,
    ) -> Result<EntryOwnerProgress, String> {
        let (generation, nonce) = revision;
        self.validate_executor_key(executor_key)?;
        self.reserve_owner_operation(participant, operation, "executor_delegation")?;
        if let Some(original) = self.stored_intent(participant, operation)? {
            match &original.payload {
                WorldServicePayloadV1::Delegation(s)
                    if s.request.delegate_public_key == executor_key
                        && s.request.generation == generation
                        && s.request.nonce == nonce
                        && s.request.revoked == revoked => {}
                _ => return Err("delegation operation content conflict".into()),
            };
            return self.settle_owner_intent(participant, login, operation, &original);
        }
        if generation == 0 {
            return Err("delegation generation must be positive".into());
        }
        let current = self.current_owner(participant, login)?;
        let o = &current.owner;
        let request = AgentSignerDelegationChangeV1 {
            world_id: self.config.world.world_id.clone(),
            branch_id: o.canonical_authority.branch_id.clone(),
            agent_id: o.agent_id.clone(),
            owner_binding: o.player_id.clone(),
            agent_identity_generation: o.canonical_authority.agent_identity_generation,
            generation,
            delegate_public_key: executor_key.into(),
            revoked,
            nonce,
        };
        let payload = WorldServicePayloadV1::Delegation(ws::sign_read_request(
            "delegation",
            request,
            &self.owner_private(),
        )?);
        let original = self.save_intent(
            participant,
            operation,
            &o.agent_id,
            current.task.as_ref().map(|t| t.task_id.as_str()),
            o.canonical_authority.agent_identity_generation,
            payload,
        )?;
        self.settle_owner_intent(participant, login, operation, &original)
    }
    pub fn bind_executor(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
        executor_id: &str,
        executor_key: &str,
        generation: u64,
    ) -> Result<EntryOwnerProgress, String> {
        self.validate_executor_key(executor_key)?;
        self.reserve_owner_operation(participant, operation, "executor_bind")?;
        let requested = GameExecutorOperationV1::Bind {
            executor_id: executor_id.into(),
            executor_public_key: executor_key.into(),
            delegation_generation: DecimalU64(generation),
        };
        if let Some(original) = self.stored_intent(participant, operation)? {
            match &original.payload {
                WorldServicePayloadV1::GameExecutorControl(s)
                    if s.request.operation == requested => {}
                _ => return Err("executor bind operation content conflict".into()),
            };
            return self.settle_owner_intent(participant, login, operation, &original);
        }
        let current = self.current_owner(participant, login)?;
        let task = current.task.ok_or("current task unavailable")?;
        let request = GameExecutorControlV1 {
            api_rev: 1,
            client_operation_id: operation.into(),
            fence: GameTaskFenceV1::from_control(&task),
            operation: requested,
        };
        request.validate()?;
        let payload = WorldServicePayloadV1::GameExecutorControl(ws::sign_read_request(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            request,
            &self.owner_private(),
        )?);
        let original = self.save_intent(
            participant,
            operation,
            &task.agent_id,
            Some(&task.task_id),
            task.owner_generation.0,
            payload,
        )?;
        self.settle_owner_intent(participant, login, operation, &original)
    }
    pub fn stop_executor(
        &self,
        participant: &str,
        login: &str,
        operation: &str,
    ) -> Result<EntryOwnerProgress, String> {
        self.reserve_owner_operation(participant, operation, "executor_stop")?;
        if let Some(original) = self.stored_intent(participant, operation)? {
            match &original.payload {
                WorldServicePayloadV1::GameExecutorControl(s)
                    if s.request.operation == GameExecutorOperationV1::Stop => {}
                _ => return Err("executor stop operation conflict".into()),
            };
            return self.settle_owner_intent(participant, login, operation, &original);
        }
        let current = self.current_owner(participant, login)?;
        let task = current.task.ok_or("current task unavailable")?;
        let request = GameExecutorControlV1 {
            api_rev: 1,
            client_operation_id: operation.into(),
            fence: GameTaskFenceV1::from_control(&task),
            operation: GameExecutorOperationV1::Stop,
        };
        request.validate()?;
        let payload = WorldServicePayloadV1::GameExecutorControl(ws::sign_read_request(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            request,
            &self.owner_private(),
        )?);
        let original = self.save_intent(
            participant,
            operation,
            &task.agent_id,
            Some(&task.task_id),
            task.owner_generation.0,
            payload,
        )?;
        self.settle_owner_intent(participant, login, operation, &original)
    }
    fn validate_executor_key(&self, key: &str) -> Result<(), String> {
        validate_independent_executor_key(
            key,
            &hex::encode(self.key.verifying_key().to_bytes()),
            &hex::encode(self.identity.signing_key.verifying_key().to_bytes()),
        )
    }
}

fn validate_independent_executor_key(key: &str, owner: &str, login: &str) -> Result<(), String> {
    let bytes: [u8; 32] = hex::decode(key)
        .map_err(|_| "invalid executor key")?
        .try_into()
        .map_err(|_| "invalid executor key")?;
    let public =
        ed25519_dalek::VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid executor key")?;
    if key != hex::encode(bytes) || public.is_weak() || key == owner || key == login {
        return Err("executor requires independent strong canonical public key".into());
    }
    Ok(())
}

fn validate_owner_receipt(
    request: &SubmitIntentRequest<WorldServicePayloadV1>,
    height: u64,
    receipt: &Value,
) -> Result<(), String> {
    if let WorldServicePayloadV1::GameStarterClaim(signed) = &request.signed_payload {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StarterClaim {
            agent_id: String,
            player_id: String,
            public_key: Option<String>,
            amount: u64,
            claimed_at: u64,
            #[serde(default)]
            source_treasury_bucket_id: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct StarterReceipt {
            claim: StarterClaim,
            #[serde(default)]
            events: Vec<Value>,
        }
        let result: StarterReceipt = serde_json::from_value(receipt.clone())
            .map_err(|_| "starter claim receipt malformed")?;
        let claim = result.claim;
        // Canonical finalizer events may accompany the first claim. They are
        // not a substitute for the exact claim/owner checks and are never
        // interpreted here as an independently proved authority or balance.
        let _ = result.events;
        if claim.agent_id != signed.request.agent_id
            || claim.player_id != signed.request.owner_binding
            || claim.public_key.as_deref() != Some(signed.subject_public_key.as_str())
            || claim.amount == 0
            || claim
                .source_treasury_bucket_id
                .as_ref()
                .is_some_and(|b| b.trim().is_empty() || b.len() > 256)
        {
            return Err("starter claim receipt resource mismatch".into());
        }
        let _ = claim.claimed_at;
        return Ok(());
    }
    use oasis7_client_api::legacy_task_results::{
        create_control, executor_receipt, publish_receipt,
    };
    let legacy: oasis7_client_api::world_service::SubmitIntentRequest<
        oasis7_client_api::legacy_world_service::WorldServicePayloadV1,
    > = serde_json::from_value(
        serde_json::to_value(request).map_err(|_| "owner original encoding failed")?,
    )
    .map_err(|_| "unsupported owner original")?;
    let expected = match &request.signed_payload {
        WorldServicePayloadV1::GameTaskCreate(_) => serde_json::to_value(create_control(&legacy)?),
        WorldServicePayloadV1::GameGoalPublish(_) => {
            serde_json::to_value(publish_receipt(&legacy, height)?)
        }
        WorldServicePayloadV1::GameExecutorControl(_) => {
            serde_json::to_value(executor_receipt(&legacy)?)
        }
        WorldServicePayloadV1::Delegation(s) => Ok(serde_json::json!({"delegation":s.request})),
        _ => return Err("unsupported owner settlement".into()),
    }
    .map_err(|_| "owner receipt encoding failed")?;
    if receipt != &expected {
        return Err("owner receipt resource mismatch".into());
    }
    Ok(())
}
