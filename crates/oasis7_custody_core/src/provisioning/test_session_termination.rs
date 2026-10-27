use super::*;
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationTriggerV1 {
    ClosingOnlyV1,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationNoncePolicyV1 {
    AccountAllocatedHighWaterV1,
}
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationPlanV1 {
    pub api_version: u8,
    pub session_id: String,
    pub custody_account_id: String,
    pub provision_id: String,
    pub setup_id: String,
    pub prepared_result_digest: String,
    pub world: crate::approval::ApprovedWorldV1,
    pub owner_key_ref: String,
    pub executor_key_ref: String,
    pub initial_fence: GameTaskFenceV1,
    #[serde(with = "delegation_metadata")]
    #[cfg_attr(test, schemars(with = "delegation_metadata::DelegationMetadataV1"))]
    pub original_delegation: AgentSignerDelegationChangeV1,
    pub stop_operation_id: String,
    pub revoke_operation_id: String,
    pub cleanup_until: UnixMillis,
    pub request_limit: DecimalU64,
    pub transport_limit: DecimalU64,
    pub trigger: TerminationTriggerV1,
    pub nonce_policy: TerminationNoncePolicyV1,
}
impl TerminationPlanV1 {
    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        digest(&("oasis7.custody.termination-plan.v1", self))
    }
    pub fn validate(&self) -> Result<()> {
        self.initial_fence.validate().map_err(|_| invalid())?;
        let f = &self.initial_fence;
        let d = &self.original_delegation;
        let binding = f.executor_binding.as_ref().ok_or_else(invalid)?;
        if self.api_version != 1
            || [
                &self.session_id,
                &self.custody_account_id,
                &self.provision_id,
                &self.setup_id,
                &self.owner_key_ref,
                &self.executor_key_ref,
            ]
            .iter()
            .any(|v| !id(v))
            || !hex64(&self.prepared_result_digest)
            || !self.world.valid()
            || f.world_id != self.world.world_id
            || f.genesis_digest != self.world.genesis_digest
            || f.branch_id != self.world.branch_id
            || f.reorg_epoch != self.world.reorg_epoch
            || f.task_status != oasis7_agent_api::game::GameTaskStatusV1::Active
            || f.active_goal_reference.is_none()
            || d.world_id != f.world_id
            || d.branch_id != f.branch_id
            || d.agent_id != f.agent_id
            || d.owner_binding != f.owner_binding
            || d.agent_identity_generation != f.owner_generation.0
            || d.revoked
            || d.generation == 0
            || d.nonce == 0
            || d.delegate_public_key != binding.executor_public_key
            || d.generation != binding.delegation_generation.0
            || self.cleanup_until.get() == 0
            || self.request_limit.0 != 2
            || self.transport_limit.0 != 24
            || self.stop_operation_id
                != termination_id("stop", &self.session_id, &self.prepared_result_digest)?
            || self.revoke_operation_id
                != termination_id("revoke", &self.session_id, &self.prepared_result_digest)?
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn termination_id(kind: &str, session: &str, prepared: &str) -> Result<String> {
    digest(&(
        match kind {
            "stop" => "oasis7.custody.session-stop.v1",
            _ => "oasis7.custody.session-revoke.v1",
        },
        session,
        prepared,
    ))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminationOperationV1 {
    Stop,
    Revoke,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TerminationPayloadV1 {
    Stop(Box<GameExecutorControlV1>),
    Revoke(AgentSignerDelegationChangeV1),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationOriginalV1 {
    pub operation_id: String,
    pub payload: TerminationPayloadV1,
    pub allocated_at: UnixMillis,
    pub claimed_at: Option<UnixMillis>,
    pub signed: Option<Box<OwnerSetupOriginalV1>>,
    pub result: Option<Box<OwnerSetupResultV1>>,
    pub observations: BTreeMap<OwnerSetupCanonicalSourceV1, OwnerSetupResultV1>,
    pub transport_attempts: Vec<String>,
    pub lookup_claimed_at: Option<UnixMillis>,
}
impl AccountProvisionApprovalState {
    pub fn test_session_termination_basis(
        &self,
        session_id: &str,
        cleanup_until: UnixMillis,
    ) -> Result<TerminationPlanV1> {
        if !id(session_id) || self.provision.stage() != ProvisionStage::Prepared {
            return Err(invalid());
        }
        let (setup_id, s) = self
            .owner_setups
            .iter()
            .find(|(_, s)| {
                s.stages
                    .get(&OwnerSetupStageV1::FinalObservation)
                    .and_then(|o| o.result.as_ref())
                    .is_some_and(OwnerSetupResultV1::succeeded)
            })
            .ok_or_else(invalid)?;
        let result = s
            .stages
            .get(&OwnerSetupStageV1::FinalObservation)
            .and_then(|o| o.result.as_ref())
            .ok_or_else(invalid)?;
        let OwnerSetupResultV1::FinalObservation { owner, .. } = result else {
            return Err(invalid());
        };
        let control = owner
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .ok_or_else(invalid)?;
        let prepared = digest(&("oasis7.custody.prepared-result.v1", setup_id, result))?;
        let plan = TerminationPlanV1 {
            api_version: 1,
            session_id: session_id.into(),
            custody_account_id: self.draft.entry.custody_account_id.clone(),
            provision_id: self.draft.entry.provision_id.clone(),
            setup_id: setup_id.clone(),
            prepared_result_digest: prepared.clone(),
            world: s.plan.world_identity.clone(),
            owner_key_ref: s.plan.owner_key_ref.clone(),
            executor_key_ref: s.plan.executor_key_ref.clone(),
            initial_fence: GameTaskFenceV1::from_control(control),
            original_delegation: owner
                .payload
                .signer_delegation
                .clone()
                .ok_or_else(invalid)?,
            stop_operation_id: termination_id("stop", session_id, &prepared)?,
            revoke_operation_id: termination_id("revoke", session_id, &prepared)?,
            cleanup_until,
            request_limit: DecimalU64(2),
            transport_limit: DecimalU64(24),
            trigger: TerminationTriggerV1::ClosingOnlyV1,
            nonce_policy: TerminationNoncePolicyV1::AccountAllocatedHighWaterV1,
        };
        plan.validate()?;
        Ok(plan)
    }
}

pub struct TestSessionTerminationCurrent<'a, 'b, 'c> {
    pub live: &'a TestSessionLiveCurrent<'b, 'c>,
    pub world: &'a OwnerSetupWorldEvidenceV1,
}
impl AccountProvisionApprovalState {
    pub fn test_session_termination_read(
        &self,
        id: &str,
        kind: TerminationOperationV1,
        source_id: &str,
    ) -> Result<ws::ReadWorldViewRequest> {
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        let p = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let mut r = p.descriptor.owner_read.clone();
        r.observation_nonce = Some(digest(&(
            "oasis7.custody.test-session-termination-read.v1",
            id,
            kind,
            source_id,
        ))?);
        r.deadline_unix_ms = Some(s.termination.cleanup_until.get());
        r.fixed_commit = None;
        r.formal_anchor = None;
        Ok(r)
    }
    fn termination_current_checks(
        &self,
        id: &str,
        kind: TerminationOperationV1,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
    ) -> Result<ExactSignatureChecks> {
        self.require_session_source_funding(
            id,
            current.live.source_operation_id,
            SessionSourceFundingV1::TerminationPlan,
        )?;
        let s = self.test_session_local(id, SessionPurposeV1::TerminationPlan, now)?;
        if s.closing.is_none() {
            return Err(invalid());
        }
        let checks = self.test_session_live_checks(
            id,
            SessionPurposeV1::TerminationPlan,
            current.live,
            now,
        )?;
        let w = current.world;
        w.response
            .payload
            .validate(&w.request)
            .map_err(|_| invalid())?;
        if w.request.read.request
            != self.test_session_termination_read(id, kind, current.live.source_operation_id)?
            || w.request.read.subject_public_key != s.termination.initial_fence.owner_public_key
            || w.response.request_digest
                != ws::request_digest(
                    ws::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH,
                    &w.request,
                )
                .map_err(|_| invalid())?
        {
            return Err(invalid());
        }
        let task = w
            .response
            .payload
            .control
            .view
            .game_task_control
            .as_ref()
            .ok_or_else(invalid)?;
        let observed = GameTaskFenceV1::from_control(task);
        let mut expected = s.termination.initial_fence.clone();
        if let Some(stop) = s
            .termination_slots
            .get(&TerminationOperationV1::Stop)
            .and_then(|o| o.result.as_ref())
        {
            let OwnerSetupResultV1::Canonical { response, .. } = stop.as_ref() else {
                return Err(invalid());
            };
            if let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome {
                let receipt: oasis7_client_api::legacy_task_results::ExecutorReceipt =
                    serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
                expected.task_revision = receipt.task_revision;
                expected.executor_epoch = receipt.executor_epoch;
                expected.executor_binding = receipt.executor_binding;
                expected.task_status = receipt.task_status;
            }
        }
        if observed != expected {
            return Err(invalid());
        }
        let d = w
            .response
            .payload
            .signer_delegation
            .as_ref()
            .ok_or_else(invalid)?;
        let original = &s.termination.original_delegation;
        if d.world_id != original.world_id
            || d.branch_id != original.branch_id
            || d.agent_id != original.agent_id
            || d.owner_binding != original.owner_binding
            || d.agent_identity_generation != original.agent_identity_generation
            || d.delegate_public_key != original.delegate_public_key
            || d.generation < original.generation
            || d.nonce < original.nonce
        {
            return Err(invalid());
        }
        Ok(append_owner_setup_checks(
            checks,
            OwnerSetupSignatureFact::World(w, &s.termination.world.service_public_key),
        )?)
    }
    pub fn allocate_test_session_termination(
        &mut self,
        id: &str,
        kind: TerminationOperationV1,
        current: &TestSessionTerminationCurrent<'_, '_, '_>,
        now: UnixMillis,
        verifier: &mut impl ApprovalSignatureVerifier,
    ) -> Result<TerminationOriginalV1> {
        verifier.verify(&self.termination_current_checks(id, kind, current, now)?)?;
        let s = self.test_sessions.get(id).ok_or_else(invalid)?;
        if let Some(old) = s.termination_slots.get(&kind) {
            return Ok(old.clone());
        }
        let plan = &s.termination;
        let (operation_id, payload) = match kind {
            TerminationOperationV1::Stop => (
                plan.stop_operation_id.clone(),
                TerminationPayloadV1::Stop(Box::new(GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: plan.stop_operation_id.clone(),
                    fence: GameTaskFenceV1::from_control(
                        current
                            .world
                            .response
                            .payload
                            .control
                            .view
                            .game_task_control
                            .as_ref()
                            .ok_or_else(invalid)?,
                    ),
                    operation: GameExecutorOperationV1::Stop,
                })),
            ),
            TerminationOperationV1::Revoke => {
                let observed = current
                    .world
                    .response
                    .payload
                    .signer_delegation
                    .as_ref()
                    .ok_or_else(invalid)?;
                let mut generation = observed.generation;
                let mut nonce = observed.nonce;
                // Known inventory includes every allocated original, even those never sent or unknown.
                for setup in self.owner_setups.values() {
                    generation = generation.max(setup.plan.delegation_generation.0);
                    nonce = nonce.max(setup.plan.delegation_nonce.0)
                }
                for session in self.test_sessions.values() {
                    for original in session.termination_slots.values() {
                        if let TerminationPayloadV1::Revoke(d) = &original.payload {
                            generation = generation.max(d.generation);
                            nonce = nonce.max(d.nonce)
                        }
                    }
                }
                let mut d = plan.original_delegation.clone();
                d.generation = generation.checked_add(1).ok_or_else(invalid)?;
                d.nonce = nonce.checked_add(1).ok_or_else(invalid)?;
                d.revoked = true;
                (
                    plan.revoke_operation_id.clone(),
                    TerminationPayloadV1::Revoke(d),
                )
            }
        };
        let original = TerminationOriginalV1 {
            operation_id,
            payload,
            allocated_at: now,
            claimed_at: None,
            signed: None,
            result: None,
            observations: BTreeMap::new(),
            transport_attempts: Vec::new(),
            lookup_claimed_at: None,
        };
        let mut next = self.clone();
        let s = next.test_sessions.get_mut(id).ok_or_else(invalid)?;
        s.roots
            .get_mut(&SessionPurposeV1::TerminationPlan)
            .ok_or_else(invalid)?
            .budgets
            .requests
            .charge(1)
            .map_err(|_| invalid())?;
        s.termination_slots.insert(kind, original.clone());
        next.validate_persistence()?;
        *self = next;
        Ok(original)
    }
}
#[path = "test_session_termination_io.rs"]
mod io;

#[path = "test_session_delegation_metadata.rs"]
mod delegation_metadata;
