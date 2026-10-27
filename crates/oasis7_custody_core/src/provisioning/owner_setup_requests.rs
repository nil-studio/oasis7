use super::*;
use oasis7_agent_api::game::*;
use ws::participant_discovery::{PARTICIPANT_OWNER_VIEW_PATH, ParticipantOwnerViewProfileV1};
use ws::private_goal::verify_owner_signature_strict;
impl AccountProvisionApprovalState {
    pub(super) fn validate_owner_setup_world_evidence(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        w: &OwnerSetupWorldEvidenceV1,
    ) -> Result<()> {
        let access = self.owner_setup_stage_access(id, stage)?;
        self.validate_owner_setup_world_access(id, w, &access.admission)
    }
    pub(super) fn validate_owner_setup_world_access(
        &self,
        id: &str,
        w: &OwnerSetupWorldEvidenceV1,
        admission: &ws::SignedReadRequest<GameParticipantAdmissionV1>,
    ) -> Result<()> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        w.response
            .payload
            .validate(&w.request)
            .map_err(|_| invalid())?;
        if w.request.response_profile != Some(ParticipantOwnerViewProfileV1::OwnerSetupV1)
            || w.response.request_digest
                != ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &w.request)
                    .map_err(|_| invalid())?
            || w.response.payload.control.world.world_id != v.plan.world_identity.world_id
            || w.response.payload.control.world.genesis_digest
                != v.plan.world_identity.genesis_digest
            || w.response.payload.control.version.commit.binding.branch_id
                != v.plan.world_identity.branch_id
            || w.response
                .payload
                .control
                .version
                .commit
                .binding
                .reorg_generation
                != v.plan.world_identity.reorg_epoch.0
        {
            return Err(invalid());
        }
        verify_owner_signature_strict(
            PARTICIPANT_OWNER_VIEW_PATH,
            &(&w.response.request_digest, &w.response.payload),
            &v.plan.world_identity.service_public_key,
            &w.response.signature_hex,
        )
        .map_err(|_| invalid())?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let owner = &w.response.payload.control.view.canonical_agent_owner;
        let old = &participant
            .result
            .as_ref()
            .ok_or_else(invalid)?
            .owner_response
            .payload
            .view
            .canonical_agent_owner;
        if owner.agent_id != old.agent_id
            || owner.player_id != old.player_id
            || owner.public_key != old.public_key
            || owner.world_id != old.world_id
            || owner.reorg_epoch != old.reorg_epoch
            || owner.canonical_authority.agent_identity_generation
                != old.canonical_authority.agent_identity_generation
            || w.request.owner != participant.bundle.as_ref().ok_or_else(invalid)?.owner
            || w.request.admission != *admission
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn owner_setup_signing_request(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
        current: &OwnerSetupCurrent<'_, '_>,
        now: u64,
    ) -> Result<OwnerSetupSigningBundleV1> {
        self.owner_setup_access_current_checks(id, stage, current, now)?;
        Ok(OwnerSetupSigningBundleV1 {
            action: self.owner_setup_expected(id, stage)?,
            observation: self.owner_setup_read_request(id, stage)?,
            participant_access: self.owner_setup_stage_access(id, stage)?,
        })
    }
    pub fn owner_setup_participant_access(
        &self,
        id: &str,
    ) -> Result<ws::participant_discovery::ParticipantOwnerViewRequestV1> {
        self.owner_setups.get(id).ok_or_else(invalid)?;
        self.participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .and_then(|p| p.owner_view.clone())
            .ok_or_else(invalid)
    }
    pub(super) fn owner_setup_read_request(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<ws::ReadWorldViewRequest> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let o = v.stages.get(&stage).ok_or_else(invalid)?;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let mut read = participant.descriptor.owner_read.clone();
        read.observation_nonce = Some(ledger::preflight_nonce(id, stage, &o.session_nonce)?);
        read.deadline_unix_ms = Some(v.plan.deadline.0);
        read.fixed_commit = None;
        read.formal_anchor = None;
        read.min_commit =
            v.stages
                .range(..stage)
                .rev()
                .find_map(|(_, o)| match o.result.as_ref() {
                    Some(OwnerSetupResultV1::Canonical { response, .. }) => {
                        match &response.payload.outcome {
                            ws::IntentOutcome::Committed { commit, .. } => Some((**commit).clone()),
                            _ => None,
                        }
                    }
                    _ => None,
                });
        Ok(read)
    }
    pub(super) fn owner_setup_expected(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<OwnerSetupSigningRequestV1> {
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let plan = &v.plan;
        let participant = self
            .participant_provisions
            .values()
            .find(|p| p.result.is_some())
            .ok_or_else(invalid)?;
        let owner = &participant
            .result
            .as_ref()
            .ok_or_else(invalid)?
            .owner_response
            .payload
            .view
            .canonical_agent_owner;
        let ownerkey = self
            .provision
            .role_key(Role::Owner)
            .ok_or_else(invalid)?
            .public_key
            .clone();
        if stage == OwnerSetupStageV1::CreateTask {
            return Ok(OwnerSetupSigningRequestV1::CreateTask(GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: plan.create_operation_id.clone(),
                world_id: plan.world_identity.world_id.clone(),
                genesis_digest: plan.world_identity.genesis_digest.clone(),
                branch_id: plan.world_identity.branch_id.clone(),
                reorg_epoch: plan.world_identity.reorg_epoch,
                agent_id: owner.agent_id.clone(),
                owner_binding: owner.player_id.clone(),
                owner_generation: DecimalU64(owner.canonical_authority.agent_identity_generation),
            }));
        }
        let task = self
            .owner_setup_expected_control(id, stage)?
            .ok_or_else(invalid)?;
        match stage {
            OwnerSetupStageV1::PrepareGoal => {
                Ok(OwnerSetupSigningRequestV1::PrepareGoal(GameGoalPrepareV1 {
                    api_rev: 1,
                    world_id: task.world_id.clone(),
                    genesis_digest: task.genesis_digest.clone(),
                    task_id: task.task_id.clone(),
                    agent_id: task.agent_id.clone(),
                    owner_public_key: ownerkey,
                    owner_binding: task.owner_binding.clone(),
                    owner_generation: task.owner_generation,
                    branch_id: task.branch_id.clone(),
                    reorg_epoch: task.reorg_epoch,
                    expected_task_revision: task.task_revision,
                    expected_latest_goal_revision: task.latest_goal_revision,
                    goal_revision: DecimalU64(1),
                    client_operation_id: plan.prepare_operation_id.clone(),
                    client_nonce: plan.prepare_nonce.clone(),
                    goal_digest: game_goal_version_digest(
                        &task.task_id,
                        DecimalU64(1),
                        &plan.goal_content,
                    )
                    .map_err(|_| invalid())?,
                    content: plan.goal_content.clone(),
                }))
            }
            OwnerSetupStageV1::PublishGoal => {
                let prepared = v
                    .stages
                    .get(&OwnerSetupStageV1::PrepareGoal)
                    .ok_or_else(invalid)?;
                let Some(OwnerSetupOriginalV1::PrepareGoal(original)) = &prepared.original else {
                    return Err(invalid());
                };
                let Some(OwnerSetupResultV1::PreparedGoal(receipt)) = &prepared.result else {
                    return Err(invalid());
                };
                original.validate_against(&task).map_err(|_| invalid())?;
                Ok(OwnerSetupSigningRequestV1::PublishGoal(
                    ws::private_goal_read::publication(
                        original,
                        &receipt.payload,
                        plan.publish_operation_id.clone(),
                    )
                    .map_err(|_| invalid())?,
                ))
            }
            OwnerSetupStageV1::Delegation => Ok(OwnerSetupSigningRequestV1::Delegation(
                oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1 {
                    world_id: task.world_id.clone(),
                    branch_id: task.branch_id.clone(),
                    agent_id: task.agent_id.clone(),
                    owner_binding: task.owner_binding.clone(),
                    agent_identity_generation: task.owner_generation.0,
                    generation: plan.delegation_generation.0,
                    delegate_public_key: plan.executor_public_key.clone(),
                    revoked: false,
                    nonce: plan.delegation_nonce.0,
                },
            )),
            OwnerSetupStageV1::Bind => {
                Ok(OwnerSetupSigningRequestV1::Bind(GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: plan.bind_operation_id.clone(),
                    fence: GameTaskFenceV1::from_control(&task),
                    operation: GameExecutorOperationV1::Bind {
                        executor_id: plan.executor_id.clone(),
                        executor_public_key: plan.executor_public_key.clone(),
                        delegation_generation: plan.delegation_generation,
                    },
                }))
            }
            OwnerSetupStageV1::FinalObservation => {
                Ok(OwnerSetupSigningRequestV1::FinalObservation {
                    owner: Box::new(self.owner_setup_read_request(id, stage)?),
                    goal: ws::private_goal_read::CurrentGoalRequest {
                        schema_revision: 1,
                        world: ws::WorldIdentity {
                            world_id: task.world_id.clone(),
                            genesis_digest: task.genesis_digest.clone(),
                        },
                        agent_id: task.agent_id.clone(),
                        task_id: task.task_id.clone(),
                    },
                })
            }
            OwnerSetupStageV1::CreateTask => Err(invalid()),
        }
    }
    pub(super) fn owner_setup_expected_control(
        &self,
        id: &str,
        stage: OwnerSetupStageV1,
    ) -> Result<Option<GameTaskControlV1>> {
        if stage == OwnerSetupStageV1::CreateTask {
            return Ok(None);
        }
        let mut control = self.owner_setup_created_task(id)?;
        if matches!(
            stage,
            OwnerSetupStageV1::PrepareGoal | OwnerSetupStageV1::PublishGoal
        ) {
            return Ok(Some(control));
        }
        let v = self.owner_setups.get(id).ok_or_else(invalid)?;
        let Some(OwnerSetupResultV1::Canonical { response, .. }) = v
            .stages
            .get(&OwnerSetupStageV1::PublishGoal)
            .and_then(|o| o.result.as_ref())
        else {
            return Err(invalid());
        };
        let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome else {
            return Err(invalid());
        };
        let published: oasis7_client_api::legacy_task_results::PublishReceipt =
            serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
        apply_published_control(&mut control, published);
        if stage == OwnerSetupStageV1::FinalObservation {
            let Some(OwnerSetupResultV1::Canonical { response, .. }) = v
                .stages
                .get(&OwnerSetupStageV1::Bind)
                .and_then(|o| o.result.as_ref())
            else {
                return Err(invalid());
            };
            let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome else {
                return Err(invalid());
            };
            let bound: oasis7_client_api::legacy_task_results::ExecutorReceipt =
                serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
            control.task_revision = bound.task_revision;
            control.executor_epoch = bound.executor_epoch;
            control.executor_binding = bound.executor_binding;
            control.executor_id = control
                .executor_binding
                .as_ref()
                .map(|b| b.executor_id.clone());
            control.task_status = bound.task_status;
        }
        Ok(Some(control))
    }
    pub(super) fn owner_setup_created_task(&self, id: &str) -> Result<GameTaskControlV1> {
        let v = self
            .owner_setups
            .get(id)
            .and_then(|v| v.stages.get(&OwnerSetupStageV1::CreateTask))
            .ok_or_else(invalid)?;
        let Some(OwnerSetupResultV1::Canonical { response, .. }) = &v.result else {
            return Err(invalid());
        };
        let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome else {
            return Err(invalid());
        };
        serde_json::from_value(receipt.clone()).map_err(|_| invalid())
    }
    pub(super) fn owner_setup_active_goal(&self, id: &str, task: &GameTaskControlV1) -> Result<()> {
        let plan = &self.owner_setups.get(id).ok_or_else(invalid)?.plan;
        let p = self
            .owner_setups
            .get(id)
            .and_then(|v| v.stages.get(&OwnerSetupStageV1::PublishGoal))
            .and_then(|v| v.result.as_ref())
            .ok_or_else(invalid)?;
        let OwnerSetupResultV1::Canonical { response, .. } = p else {
            return Err(invalid());
        };
        let ws::IntentOutcome::Committed { receipt, .. } = &response.payload.outcome else {
            return Err(invalid());
        };
        let r: oasis7_client_api::legacy_task_results::PublishReceipt =
            serde_json::from_value(receipt.clone()).map_err(|_| invalid())?;
        if task.task_status != GameTaskStatusV1::Active
            || task.task_revision.0 < r.task_revision.0
            || task.active_goal_reference.as_ref() != Some(&r.goal_reference)
            || task.active_goal_revision.0 != 1
            || task.latest_goal_revision.0 != 1
            || !plan.resource_refs.is_empty()
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn owner_setup_delegation(
        &self,
        id: &str,
        current: Option<&oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1>,
    ) -> Result<()> {
        let o = self
            .owner_setups
            .get(id)
            .and_then(|v| v.stages.get(&OwnerSetupStageV1::Delegation))
            .ok_or_else(invalid)?;
        let Some(OwnerSetupOriginalV1::Canonical { submit, .. }) = &o.original else {
            return Err(invalid());
        };
        let WorldServicePayloadV1::Delegation(signed) = &submit.signed_payload else {
            return Err(invalid());
        };
        if o.result.as_ref().is_none_or(|r| !r.succeeded()) || current != Some(&signed.request) {
            return Err(invalid());
        }
        Ok(())
    }
}

// Match Runtime game_task::reduce: publishing changes the goal/revision only.
// Executor binding is the separate canonical operation that activates the task.
pub(super) fn apply_published_control(
    control: &mut GameTaskControlV1,
    published: oasis7_client_api::legacy_task_results::PublishReceipt,
) {
    control.task_revision = published.task_revision;
    control.latest_goal_revision = published.goal_reference.goal_revision;
    control.active_goal_revision = published.goal_reference.goal_revision;
    control.active_goal_reference = Some(published.goal_reference);
}

#[cfg(test)]
mod publication_state_tests {
    use super::*;
    fn published_control() -> GameTaskControlV1 {
        let mut control: GameTaskControlV1 = serde_json::from_value(serde_json::json!({
            "schema_revision":1,"task_id":"task-one","world_id":"world-one",
            "genesis_digest":"genesis-one","branch_id":"main","reorg_epoch":"0",
            "agent_id":"agent-one","owner_binding":"owner-one","owner_generation":"1",
            "owner_public_key":"aa".repeat(32),"task_revision":"1","task_status":"suspended",
            "latest_goal_revision":"0","active_goal_revision":"0","executor_epoch":"0",
            "executor_id":null,"created_operation_id":"create-one"
        }))
        .unwrap();
        apply_published_control(
            &mut control,
            oasis7_client_api::legacy_task_results::PublishReceipt {
                task_id: "task-one".into(),
                task_revision: DecimalU64(2),
                goal_reference: GameGoalReferenceV1 {
                    goal_revision: DecimalU64(1),
                    goal_ref: "blake3:goal-one".into(),
                    prepare_operation_id: "prepare-one".into(),
                    publish_operation_id: "publish-one".into(),
                    activation_committed_height: DecimalU64(44),
                },
            },
        );
        control
    }
    #[test]
    fn published_task_retains_suspended_and_bind_fence_until_executor_receipt() {
        // Runtime's canonical_task publication regression also asserts Suspended
        // after goal activation. An active goal is not an active executor.
        let control = published_control();
        assert_eq!(control.task_status, GameTaskStatusV1::Suspended);
        assert_eq!(control.task_revision, DecimalU64(2));
        assert_eq!(control.active_goal_revision, DecimalU64(1));
        assert_eq!(
            control
                .active_goal_reference
                .as_ref()
                .unwrap()
                .activation_committed_height,
            DecimalU64(44)
        );
        let fence = GameTaskFenceV1::from_control(&control);
        let bind = GameExecutorControlV1 {
            api_rev: 1,
            client_operation_id: "bind-one".into(),
            fence,
            operation: GameExecutorOperationV1::Bind {
                executor_id: "executor-one".into(),
                executor_public_key: "bb".repeat(32),
                delegation_generation: DecimalU64(1),
            },
        };
        assert_eq!(bind.fence.task_status, GameTaskStatusV1::Suspended);
        assert_eq!(bind.fence.executor_epoch, DecimalU64(0));
    }
    #[test]
    fn signed_world_suspended_publication_matches_but_signed_active_does_not() {
        let expected = published_control();
        for active in [false, true] {
            let mut actual = expected.clone();
            if active {
                actual.task_status = GameTaskStatusV1::Active;
            }
            // Both observations are genuinely signed by the same World key;
            // rejection is semantic, not a malformed/tampered signature.
            let signed = ws::sign_service_response(
                PARTICIPANT_OWNER_VIEW_PATH,
                "ab".repeat(32),
                actual,
                &"3f".repeat(32),
            )
            .unwrap();
            let public = ws::sign_read_request("world-key", (), &"3f".repeat(32))
                .unwrap()
                .subject_public_key;
            verify_owner_signature_strict(
                PARTICIPANT_OWNER_VIEW_PATH,
                &(&signed.request_digest, &signed.payload),
                &public,
                &signed.signature_hex,
            )
            .unwrap();
            assert_eq!(signed.payload == expected, !active);
        }
    }
}
