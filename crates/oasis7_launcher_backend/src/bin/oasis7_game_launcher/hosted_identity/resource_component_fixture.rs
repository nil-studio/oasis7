//! Signed synthetic World transport fixture, never MFA/prepayment or real World evidence.
use oasis7_agent_api::game::{
    DecimalU64, GAME_PARTICIPANT_ADMISSION_DOMAIN, GAME_PARTICIPANT_REGISTER_DOMAIN,
    GameExecutorBindingV1, GameGoalPrepareV1, GameGoalReferenceV1, GameIdentityNamespaceV1,
    GameParticipantAdmissionV1, GameParticipantRegisterV1, GameTaskControlV1, GameTaskFenceV1,
    GameTaskStatusV1, GoalContentV1, game_goal_version_digest,
};
use oasis7_client_api::world_service::private_goal::{
    PreparedGoalReceiptV1, prepared_goal_commitment, sign_goal_prepare,
};
use oasis7_client_api::world_service::resource_basis as wire;
use oasis7_client_api::world_service::*;
use oasis7_client_api::world_service::{
    entry_control::{EntryOwnerControlRequestV1, EntryOwnerOperationV1},
    participant_discovery::{
        OwnerSetupObservationV1, PARTICIPANT_OWNER_VIEW_PATH, ParticipantOwnerControlResponseV1,
        ParticipantOwnerControlViewV1, ParticipantOwnerViewProfileV1,
        ParticipantOwnerViewRequestV1,
    },
};
pub(in super::super) struct Data {
    pub owner: String,
    pub issuer: String,
    pub world_key: String,
    pub world: WorldIdentity,
    pub state: wire::ResourceOwnerStateV1,
    pub prepared: PreparedGoalReceiptV1,
}
impl Data {
    pub fn new() -> Self {
        let owner = hex::encode([62; 32]);
        let issuer = hex::encode([61; 32]);
        let world_key = hex::encode([60; 32]);
        let public = sign_read_request("key", (), &owner)
            .unwrap()
            .subject_public_key;
        let delegate = sign_read_request("key", (), &hex::encode([63; 32]))
            .unwrap()
            .subject_public_key;
        let world = WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        };
        let content = GoalContentV1 {
            goal_text: "private component goal".into(),
            soft_constraints: vec!["no production approval".into()],
        };
        let proof = sign_goal_prepare(
            GameGoalPrepareV1 {
                api_rev: 1,
                world_id: world.world_id.clone(),
                genesis_digest: world.genesis_digest.clone(),
                task_id: "task".into(),
                agent_id: "agent-a".into(),
                owner_public_key: public.clone(),
                owner_binding: "owner-binding".into(),
                owner_generation: DecimalU64(1),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(3),
                expected_task_revision: DecimalU64(1),
                expected_latest_goal_revision: DecimalU64(0),
                goal_revision: DecimalU64(1),
                client_operation_id: "prepare".into(),
                client_nonce: "ab".repeat(32),
                goal_digest: game_goal_version_digest("task", DecimalU64(1), &content).unwrap(),
                content,
            },
            &owner,
        )
        .unwrap();
        let prepared = PreparedGoalReceiptV1 {
            schema_revision: 1,
            goal_ref: prepared_goal_commitment(&[1; 32], &proof).unwrap(),
            reference_salt: hex::encode([1; 32]),
            owner_proof: proof,
        };
        let control = GameTaskControlV1 {
            schema_revision: 1,
            task_id: "task".into(),
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            branch_id: "main".into(),
            reorg_epoch: DecimalU64(3),
            agent_id: "agent-a".into(),
            owner_binding: "owner-binding".into(),
            owner_generation: DecimalU64(1),
            owner_public_key: public.clone(),
            task_revision: DecimalU64(3),
            task_status: GameTaskStatusV1::Active,
            latest_goal_revision: DecimalU64(1),
            active_goal_revision: DecimalU64(1),
            active_goal_reference: Some(GameGoalReferenceV1 {
                goal_revision: DecimalU64(1),
                goal_ref: prepared.goal_ref.clone(),
                prepare_operation_id: "prepare".into(),
                publish_operation_id: "publish".into(),
                activation_committed_height: DecimalU64(2),
            }),
            executor_epoch: DecimalU64(1),
            executor_id: Some("executor".into()),
            executor_binding: Some(GameExecutorBindingV1 {
                executor_id: "executor".into(),
                executor_public_key: delegate.clone(),
                delegation_generation: DecimalU64(1),
                executor_epoch: DecimalU64(1),
            }),
            created_operation_id: "create".into(),
        };
        let commit = CommitRef {
            world: world.clone(),
            binding: ExecutionBinding {
                provider_world_id: world.world_id.clone(),
                branch_id: "main".into(),
                finality_ref: "fixture-finality".into(),
                reorg_generation: 3,
                governing_manifest_ref: "fixture-manifest".into(),
                authority_generation: 1,
                permission_generation: 1,
            },
            position: 3,
            execution_block_hash: "fixture-block".into(),
            state_root_ref: "fixture-root".into(),
        };
        let state = wire::ResourceOwnerStateV1 {
            schema_revision: 1,
            world: (&world).into(),
            read_commit: (&commit).into(),
            logical_tick: DecimalU64(3),
            owner: wire::ResourceOwnerFenceV1 {
                agent_id: "agent-a".into(),
                player_id: "owner-binding".into(),
                public_key: public,
                world_id: world.world_id.clone(),
                reorg_epoch: DecimalU64(3),
                branch_id: "main".into(),
                agent_identity_generation: DecimalU64(1),
                current_intent_id: None,
            },
            control,
            signer_delegation: Some(wire::ResourceSignerRecordV1 {
                world_id: world.world_id.clone(),
                branch_id: "main".into(),
                agent_id: "agent-a".into(),
                owner_binding: "owner-binding".into(),
                agent_identity_generation: DecimalU64(1),
                generation: DecimalU64(1),
                delegate_public_key: delegate,
                revoked: false,
                nonce: DecimalU64(1),
            }),
        };
        Self {
            owner,
            issuer,
            world_key,
            world,
            state,
            prepared,
        }
    }
    pub fn original(&self) -> SignedReadRequest<GameParticipantRegisterV1> {
        sign_read_request(
            GAME_PARTICIPANT_REGISTER_DOMAIN,
            GameParticipantRegisterV1 {
                api_rev: 1,
                client_operation_id: "register".into(),
                identity_namespace: GameIdentityNamespaceV1 {
                    service_id: "model-contract".into(),
                    environment_id: "test".into(),
                    instance_epoch: DecimalU64(4),
                },
                account_id: "a".into(),
                world_id: self.world.world_id.clone(),
                genesis_digest: self.world.genesis_digest.clone(),
                owner_public_key: self.state.owner.public_key.clone(),
                owner_key_version: DecimalU64(1),
            },
            &self.owner,
        )
        .unwrap()
    }
    pub fn admission(&self) -> SignedReadRequest<GameParticipantAdmissionV1> {
        sign_read_request(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            GameParticipantAdmissionV1 {
                api_rev: 1,
                business_digest:
                    oasis7_client_api::world_service_correlation::participant_business_digest(
                        &self.original().request,
                    )
                    .unwrap(),
                identity_namespace: self.original().request.identity_namespace,
                account_id: "a".into(),
                world_id: self.world.world_id.clone(),
                genesis_digest: self.world.genesis_digest.clone(),
                owner_public_key: self.state.owner.public_key.clone(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(3),
                policy_generation: DecimalU64(1),
                valid_from_tick: DecimalU64(0),
                valid_until_tick: DecimalU64(100),
            },
            &self.issuer,
        )
        .unwrap()
    }
    pub fn selection(&self) -> wire::ResourceBasisSelectionV1 {
        wire::ResourceBasisSelectionV1::Build {
            site: "site".into(),
            factory: "factory".into(),
            factory_kind: "factory.smelter.mk1".into(),
        }
    }
    pub fn owner_request(&self) -> wire::ParticipantOwnerResourceViewRequestV1 {
        wire::ParticipantOwnerResourceViewRequestV1 {
            owner: self.original().into(),
            admission: self.admission().into(),
            read: sign_read_request(
                wire::OWNER_RESOURCE_VIEW_PATH_V1,
                wire::OwnerResourceReadV1 {
                    api_version: 1,
                    world: (&self.world).into(),
                    expected_fence: GameTaskFenceV1::from_control(&self.state.control),
                    expected_signer_delegation: self.state.signer_delegation.clone().unwrap(),
                    session_id: "component-not-a-custody-session".into(),
                    approval_operation_id: "component-not-approved".into(),
                    prepared_resource_basis_digest: "ab".repeat(32),
                    selection: self.selection(),
                    observation_nonce: "cd".repeat(32),
                },
                &self.owner,
            )
            .unwrap()
            .into(),
        }
    }
    pub fn goal_request(&self) -> EntryOwnerControlRequestV1 {
        EntryOwnerControlRequestV1 {
            access: ParticipantOwnerViewRequestV1 {
                owner: self.original(),
                admission: self.admission(),
                read: sign_read_request(
                    VIEW_JSON_PATH,
                    ReadWorldViewRequest {
                        contract_version: 1,
                        world: self.world.clone(),
                        scope_id: "agent:agent-a".into(),
                        min_commit: None,
                        fixed_commit: None,
                        formal_anchor: None,
                        observation_nonce: Some("de".repeat(32)),
                        deadline_unix_ms: None,
                    },
                    &self.owner,
                )
                .unwrap(),
                response_profile: Some(ParticipantOwnerViewProfileV1::OwnerSetupV1),
            },
            operation: EntryOwnerOperationV1::ReadResourceGoal(
                sign_read_request(
                    wire::RESOURCE_GOAL_PATH_V2,
                    wire::OwnerSetupFinalResourceReadV2 {
                        api_version: 2,
                        setup_id: "component-not-approved".into(),
                        provision_id: "component-provision".into(),
                        plan_digest: "ab".repeat(32),
                        world: (&self.world).into(),
                        expected_fence: GameTaskFenceV1::from_control(&self.state.control),
                        expected_signer_delegation: self.state.signer_delegation.clone().unwrap(),
                        goal_reference: self.prepared.goal_ref.clone(),
                        selection: self.selection(),
                        observation_nonce: "ef".repeat(32),
                    },
                    &self.owner,
                )
                .unwrap()
                .into(),
            ),
        }
    }
    pub fn body(&self, path: &str, body: &[u8]) -> Vec<u8> {
        match path {
            wire::OWNER_RESOURCE_VIEW_PATH_V1 => {
                let q: wire::ParticipantOwnerResourceViewRequestV1 =
                    serde_json::from_slice(body).unwrap();
                let p = wire::ParticipantOwnerResourceObservationV1 {
                    api_version: 1,
                    owner: self.state.clone(),
                    selection: q.read.request.selection.clone(),
                    resource: wire::ResourceBasisResolutionV1::Blocked {
                        reason: wire::ResourceBasisBlockerV1::AdmissionUnavailable,
                    },
                };
                serde_json::to_vec(&wire::SignedResourceResponseV1::from(
                    sign_service_response(
                        path,
                        request_digest(path, &q).unwrap(),
                        p,
                        &self.world_key,
                    )
                    .unwrap(),
                ))
                .unwrap()
            }
            wire::RESOURCE_GOAL_PATH_V2 => {
                let q: wire::SignedResourceReadV1<wire::OwnerSetupFinalResourceReadV2> =
                    serde_json::from_slice(body).unwrap();
                let mut activation = self.state.read_commit.clone();
                activation.position = DecimalU64(2);
                activation.execution_block_hash = "activation-block".into();
                activation.state_root_ref = "activation-root".into();
                let p = wire::OwnerSetupFinalResourceObservationV2 {
                    api_version: 2,
                    owner: self.state.clone(),
                    current_goal: wire::ResourceCurrentGoalV2 {
                        schema_revision: 1,
                        control: self.state.control.clone(),
                        read_commit: self.state.read_commit.clone(),
                        activation_commit: Some(activation),
                        active_goal: Some(self.prepared.clone()),
                    },
                    selection: q.request.selection.clone(),
                    resource: wire::ResourceBasisResolutionV1::Blocked {
                        reason: wire::ResourceBasisBlockerV1::AdmissionUnavailable,
                    },
                };
                serde_json::to_vec(&wire::SignedResourceResponseV1::from(
                    sign_service_response(
                        path,
                        request_digest(path, &q).unwrap(),
                        p,
                        &self.world_key,
                    )
                    .unwrap(),
                ))
                .unwrap()
            }
            PARTICIPANT_OWNER_VIEW_PATH => {
                let q: ParticipantOwnerViewRequestV1 = serde_json::from_slice(body).unwrap();
                let view = ParticipantOwnerControlViewV1 {
                    canonical_agent_owner: oasis7_proto::viewer::CanonicalAgentOwnerFenceV1 {
                        agent_id: self.state.owner.agent_id.clone(),
                        player_id: self.state.owner.player_id.clone(),
                        public_key: self.state.owner.public_key.clone(),
                        world_id: self.world.world_id.clone(),
                        reorg_epoch: 3,
                        canonical_authority: oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
                            branch_id: "main".into(),
                            agent_identity_generation: 1,
                        },
                        current_intent_id: None,
                    },
                    game_task_control: Some(self.state.control.clone()),
                };
                let control = ParticipantOwnerControlResponseV1 {
                    schema_revision: 1,
                    world: self.world.clone(),
                    version: ProjectionVersion {
                        commit: self.state.read_commit.clone().into(),
                        projection_revision: request_digest("projection", &view).unwrap(),
                        visibility_scope: "agent:agent-a".into(),
                    },
                    logical_tick: 3,
                    view,
                };
                let r = self.state.signer_delegation.as_ref().unwrap();
                let signer_delegation = Some(
                    oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1 {
                        world_id: r.world_id.clone(),
                        branch_id: r.branch_id.clone(),
                        agent_id: r.agent_id.clone(),
                        owner_binding: r.owner_binding.clone(),
                        agent_identity_generation: r.agent_identity_generation.0,
                        generation: r.generation.0,
                        delegate_public_key: r.delegate_public_key.clone(),
                        revoked: r.revoked,
                        nonce: r.nonce.0,
                    },
                );
                let p = OwnerSetupObservationV1 {
                    api_version: 1,
                    control,
                    signer_delegation,
                };
                serde_json::to_vec(
                    &sign_service_response(
                        path,
                        request_digest(path, &q).unwrap(),
                        p,
                        &self.world_key,
                    )
                    .unwrap(),
                )
                .unwrap()
            }
            _ => panic!("unexpected resource component path"),
        }
    }
}
