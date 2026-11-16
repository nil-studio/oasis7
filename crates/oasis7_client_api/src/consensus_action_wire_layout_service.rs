//! Extra committed payload owned graphs. DATA layout only; no admission authority.
//! Generated declarations mirror actual named fields; macros enforce exhaustive patterns.
use super::*;

// crates/oasis7_client_api/src/world_service_wire.rs:35
enum_layout!(crate::legacy_world_service::WorldServicePayloadV1, units{}, tuples{GameStarterClaim(value0),GameplayJson(value0),AgentChat(value0),SafeAlternative(value0),Cognition(value0),FeedbackAck(value0),Delegation(value0),Scheduler(value0),GameTaskCreate(value0),GameGoalPublish(value0),GameExecutorControl(value0)}, records{GameParticipantRegister{owner,admission}});

// crates/oasis7_client_api/src/signing.rs:105
record_layout!(crate::signing::MainTokenActionAuthProof, {scheme, account_id, public_key, signature, threshold, participant_signatures});

// crates/oasis7_client_api/src/world_service/task_cleanup_v2.rs:15
record_layout!(crate::world_service::task_cleanup_v2::TaskCleanupSubmitV2, {api_revision, expected_world, original});

// crates/oasis7_client_api/src/world_service.rs:171
record_layout!(crate::world_service::RequestCorrelation, {key, payload_digest});

// crates/oasis7_client_api/src/world_service.rs:146
record_layout!(crate::world_service::RequestKey, {world, verified_subject, operation_domain, nonce_scope, request_id_or_nonce});

// crates/oasis7_client_api/src/world_service.rs:43
record_layout!(crate::world_service::WorldIdentity, {world_id, genesis_digest});

// crates/oasis7_proto/src/viewer.rs:284
record_layout!(oasis7_proto::viewer::AgentChatRequest, {agent_id, message, player_id, public_key, auth, intent_tick, intent_seq, world_id, reorg_epoch, authority_scope, replaces_intent_id, canonical_authority});

// crates/oasis7_agent_api/src/cognition.rs:325
record_layout!(oasis7_agent_api::ContinuousAgentRequestContextV1, {base_decision_request, context_discriminator, context_version, protocol_version, agent_session_id, agent_turn_id, decision_request_id, retry_seq, transport_attempt, agent_subject, runtime_binding, observation_digest, capability_catalog_digest, capability_invocation_context_digest, memory_snapshot_digest, goal_snapshot_digest, continuation_digest, adapter_protocol_version, budget_contract, request_digest});

// crates/oasis7_agent_api/src/legacy_economy.rs:229
record_layout!(oasis7_agent_api::legacy_economy::CognitionLeaseRequestV1, {schema_version, idempotency_key, account_id, agent_id, agent_session_id, agent_turn_id, decision_request_id, request_digest, quote});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:640
record_layout!(oasis7_agent_api::legacy_scheduler::CognitionContinuationProposalV1, {schema_version, continuation_proposal_id, world_id, branch_id, finality_epoch, finality_block_hash, finality_status, reorg_epoch, runtime_manifest_hash, agent_id, agent_session_id, agent_turn_id, decision_request_id, origin_turn_id, origin_request_digest, action_or_plan_kind, proposal_digest, action_or_envelope_digest, baseline_observation_digest, goal_digest, policy_digest, policy_revision, precondition_summary, wake_conditions, next_wake_tick, remaining_budget, valid_until_tick, precondition_digest, source});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:757
record_layout!(oasis7_agent_api::legacy_scheduler::CognitionContinuationResumeRequestV1, {agent_session_id, agent_turn_id, decision_request_id, request_digest, context_digest});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:888
enum_layout!(oasis7_agent_api::legacy_scheduler::CognitionWakeDispositionV1, units{}, tuples{}, records{Terminal{status,reason},Replan{proposal,budget_spent}});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:693
record_layout!(oasis7_agent_api::legacy_scheduler::CognitionContextDigestsV1, {baseline_observation_digest, goal_digest, policy_digest, precondition_digest});

// crates/oasis7_agent_api/src/game/starter.rs:7
record_layout!(oasis7_agent_api::game::GameStarterClaimV1, {api_rev, client_operation_id, world_id, genesis_digest, branch_id, reorg_epoch, agent_id, owner_binding, owner_generation});

// crates/oasis7_agent_api/src/game/participant.rs:20
record_layout!(oasis7_agent_api::game::GameParticipantRegisterV1, {api_rev, client_operation_id, identity_namespace, account_id, world_id, genesis_digest, owner_public_key, owner_key_version});

// crates/oasis7_agent_api/src/game/participant.rs:54
record_layout!(oasis7_agent_api::game::GameParticipantAdmissionV1, {api_rev, business_digest, identity_namespace, account_id, world_id, genesis_digest, owner_public_key, branch_id, reorg_epoch, policy_generation, valid_from_tick, valid_until_tick});

// crates/oasis7_client_api/src/safe_alternative.rs:82
record_layout!(crate::safe_alternative::SafeAlternativeCommandV1, {api_rev, client_operation_id, world_id, genesis_digest, branch_id, reorg_epoch, agent_id, owner_binding, owner_generation, operation});

// crates/oasis7_client_api/src/world_service_wire.rs:130
record_layout!(crate::world_service_wire::CognitionIntentV1, {request, action, response_artifact, delegation_generation, causal_proposal, task_fence});

// crates/oasis7_client_api/src/world_service_wire.rs:175
record_layout!(crate::world_service_wire::FeedbackAckIntentV1, {agent_id, agent_session_id, agent_turn_id, decision_request_id, request_digest, feedback_id, feedback_seq, original_envelope_digest, private_acceptance_digest, runtime_receipt_id, delegation_generation});

// crates/oasis7_client_api/src/world_service_wire.rs:159
record_layout!(crate::world_service_wire::AgentSignerDelegationChangeV1, {world_id, branch_id, agent_id, owner_binding, agent_identity_generation, generation, delegate_public_key, revoked, nonce});

// crates/oasis7_agent_api/src/game/task.rs:9
record_layout!(oasis7_agent_api::game::GameTaskCreateV1, {api_rev, client_operation_id, world_id, genesis_digest, branch_id, reorg_epoch, agent_id, owner_binding, owner_generation});

// crates/oasis7_agent_api/src/game/goal_publish.rs:16
record_layout!(oasis7_agent_api::game::GameGoalPublishV1, {api_rev, world_id, genesis_digest, task_id, agent_id, owner_public_key, owner_binding, owner_generation, branch_id, reorg_epoch, expected_task_revision, expected_latest_goal_revision, goal_revision, client_operation_id, prepare_operation_id, goal_ref});

// crates/oasis7_agent_api/src/game/executor.rs:140
record_layout!(oasis7_agent_api::game::GameExecutorControlV1, {api_rev, client_operation_id, fence, operation});

// crates/oasis7_client_api/src/signing.rs:93
enum_layout!(
    crate::signing::MainTokenActionAuthScheme,
    units {
        Ed25519,
        ThresholdEd25519
    },
    tuples {},
    records {}
);

// crates/oasis7_client_api/src/signing.rs:99
record_layout!(crate::signing::MainTokenActionParticipantSignature, {public_key, signature});

// crates/oasis7_agent_api/src/game/task_cleanup_v2.rs:54
record_layout!(oasis7_agent_api::game::GameTaskRevokeAndCloseV2, {api_revision, client_operation_id, custody_account_id, termination_plan_digest, permitted_branches, signer_public_key, signer_generation, signer_nonce, revoke_generation, revoke_nonce, cleanup_until_ms});

// crates/oasis7_proto/src/viewer.rs:52
record_layout!(oasis7_proto::viewer::PlayerAuthProof, {scheme, player_id, public_key, nonce, signature});

// crates/oasis7_proto/src/canonical_agent_chat.rs:5
record_layout!(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1, {branch_id, agent_identity_generation});

// crates/oasis7_agent_api/src/provider.rs:179
record_layout!(oasis7_agent_api::DecisionRequest, {observation, provider_config_ref, agent_profile, fixture_id, replay_id, capability_catalog, capability_invocation_context, timeout_budget_ms});

// crates/oasis7_agent_api/src/cognition.rs:102
record_layout!(oasis7_agent_api::RuntimeBindingV1, {world_id, branch_id, finality_epoch, finality_block_hash, finality_status, base_tick, base_world_hash, reorg_epoch, runtime_manifest_hash});

// crates/oasis7_agent_api/src/cognition.rs:28
record_layout!(oasis7_agent_api::Digest32, { 0 });

// crates/oasis7_agent_api/src/cognition.rs:156
record_layout!(oasis7_agent_api::BudgetContractV1, {max_latency_ms, max_repair_attempts, max_model_calls, max_tool_calls});

// crates/oasis7_agent_api/src/legacy_economy.rs:110
record_layout!(oasis7_agent_api::legacy_economy::CognitionLeaseQuoteV1, {schema_version, quote_id, payer_id, resource, resource_version, amount, purpose, scope, policy_revision, authority_context, world_binding, valid_until_tick, quote_digest});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:48
record_layout!(oasis7_agent_api::legacy_scheduler::WakeConditionV1, {schema_version, kind, logical_tick, event_digest, receipt_id, subject, path_or_rule, operator, expected_value_bytes});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:628
record_layout!(oasis7_agent_api::legacy_scheduler::ContinuationBudgetV1, {unit, value});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:867
enum_layout!(
    oasis7_agent_api::legacy_scheduler::ContinuationStatusV1,
    units {
        Scheduled,
        Pending,
        Waking,
        Consumed,
        Completed,
        Cancelled,
        Invalidated,
        Expired,
        Rejected
    },
    tuples {},
    records {}
);

// crates/oasis7_agent_api/src/game.rs:96
// Custom Deserialize allocates a temporary String; caller decode phase pays it.
primitive_layout!(oasis7_agent_api::game::DecimalU64);
// crates/oasis7_agent_api/src/game/participant.rs:8
record_layout!(oasis7_agent_api::game::GameIdentityNamespaceV1, {service_id, environment_id, instance_epoch});

// crates/oasis7_client_api/src/safe_alternative.rs:25
enum_layout!(crate::safe_alternative::SafeAlternativeOperationV1, units{}, tuples{}, records{Propose{request_id,original,valid_until_tick,predecessor_request_id},Reevaluate{request_id,valid_until_tick},Confirm{request_id,generation,candidate_id,candidate_digest,confirmation_id,valid_until_tick},Execute{request_id,authorization_id,execution_id},Cancel{request_id},Read{request_id}});

// crates/oasis7_agent_api/src/world_cognition.rs:52
record_layout!(oasis7_agent_api::world_cognition::RuntimeCognitionCommitRequestV1, {agent_id, agent_session_id, agent_turn_id, decision_request_id, retry_seq, transport_attempt, request_digest, observation_digest, context_digest, capability_snapshot_hash, authority_context_hash, captured_base_binding});

// crates/oasis7_agent_api/src/world_cognition.rs:103
record_layout!(oasis7_agent_api::world_cognition::RuntimeCognitionResponseArtifactV1, {schema_version, context_discriminator, context_version, agent_session_id, agent_turn_id, decision_request_id, retry_seq, transport_attempt, request_digest, response_digest, artifact_digest});

// crates/oasis7_client_api/src/world_service_wire.rs:146
record_layout!(crate::world_service_wire::CognitionCausalProposalV1, {expected_consequence, alternative, stakes, reason, evidence_refs, correction_refs, dissent});

// crates/oasis7_agent_api/src/game/executor.rs:44
record_layout!(oasis7_agent_api::game::GameTaskFenceV1, {world_id, genesis_digest, branch_id, reorg_epoch, agent_id, owner_binding, owner_generation, owner_public_key, task_id, task_revision, task_status, latest_goal_revision, active_goal_revision, active_goal_reference, executor_epoch, executor_binding});

// crates/oasis7_agent_api/src/game/executor.rs:130
enum_layout!(oasis7_agent_api::game::GameExecutorOperationV1, units{Stop}, tuples{}, records{Bind{executor_id,executor_public_key,delegation_generation}});

// crates/oasis7_agent_api/src/game/task_cleanup_v2.rs:46
record_layout!(oasis7_agent_api::game::GameTaskCloseConditionsV2, {exact_initial_active, own_stop_intent});

// crates/oasis7_proto/src/viewer/negotiation.rs:12
enum_layout!(
    oasis7_proto::viewer::PlayerAuthScheme,
    units { Ed25519 },
    tuples {},
    records {}
);

// crates/oasis7_agent_api/src/provider.rs:153
record_layout!(oasis7_agent_api::ObservationEnvelope, {agent_id, world_time, mode, observation_schema_version, action_schema_version, environment_class, fallback_reason, observation, recent_event_summary, memory_summary, action_catalog, module_command_catalog, timeout_budget_ms});

// crates/oasis7_wasm_abi/src/capability_v2.rs:433
record_layout!(oasis7_wasm_abi::CapabilityCatalogSnapshot, {snapshot_id, world_id, world_head, branch_id, finality_epoch, logical_tick, module_registry_hash, policy_hash, revocation_epoch, subject, presenter, audience, entries, valid_until_tick});

// crates/oasis7_agent_api/src/provider.rs:287
record_layout!(oasis7_agent_api::CapabilityInvocationContext, {grant_id, subject, presenter, audience, catalog_snapshot_id, module_id, module_version, response_nonce});

// crates/oasis7_agent_api/src/legacy_scheduler.rs:901
record_layout!(oasis7_agent_api::legacy_scheduler::PreconditionSubjectV1, {kind, id});

// crates/oasis7_client_api/src/safe_alternative.rs:12
record_layout!(crate::safe_alternative::FineGrainedMoveRequestV1, {goal, waypoints, arrival_tick});

// crates/oasis7_agent_api/src/world_cognition.rs:11
record_layout!(oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1, {world_id, branch_id, finality_epoch, finality_block_hash, finality_status, base_tick, base_world_hash, reorg_epoch, runtime_manifest_hash});

// crates/oasis7_agent_api/src/game/task.rs:44
enum_layout!(
    oasis7_agent_api::game::GameTaskStatusV1,
    units { Suspended, Active },
    tuples {},
    records {}
);

// crates/oasis7_agent_api/src/game/goal_publish.rs:65
record_layout!(oasis7_agent_api::game::GameGoalReferenceV1, {goal_revision, goal_ref, prepare_operation_id, publish_operation_id, activation_committed_height});

// crates/oasis7_agent_api/src/game/executor.rs:22
record_layout!(oasis7_agent_api::game::GameExecutorBindingV1, {executor_id, executor_public_key, delegation_generation, executor_epoch});

// crates/oasis7_agent_api/src/game/task_cleanup_v2.rs:32
record_layout!(oasis7_agent_api::game::GameOwnStopIntentV2, {operation_id, intent_digest});

// crates/oasis7_agent_api/src/provider.rs:14
enum_layout!(
    oasis7_agent_api::ProviderExecutionMode,
    units {
        PlayerParity,
        HeadlessAgent
    },
    tuples {},
    records {}
);

// crates/oasis7_agent_api/src/provider.rs:115
record_layout!(oasis7_agent_api::ProviderObservation, {self_state, mission_context, nearby_entities, recent_events, local_navigation_graph, hazard_summary, interaction_targets});

// crates/oasis7_agent_api/src/provider.rs:50
record_layout!(oasis7_agent_api::ActionCatalogEntry, {action_ref, summary});

// crates/oasis7_wasm_abi/src/lib.rs:430
record_layout!(oasis7_wasm_abi::ModuleCommandCatalogEntry, {module_id, module_version, namespace, name, schema_version, schema_hash, max_payload_bytes});

// crates/oasis7_wasm_abi/src/capability_v2.rs:15
enum_layout!(oasis7_wasm_abi::CapabilitySubject, units{}, tuples{}, records{Agent{agent_id,owner_binding,generation},Module{module_id,module_version,instance_id},System{system_id,epoch}});

// crates/oasis7_wasm_abi/src/capability_v2.rs:59
record_layout!(oasis7_wasm_abi::CapabilityPresenter, {presenter_id, presenter_kind, session_id, attestation_ref});

// crates/oasis7_wasm_abi/src/capability_v2.rs:87
record_layout!(oasis7_wasm_abi::CapabilityAudience, {world_id, branch_id, finality_epoch, target_kind, target_id});

// crates/oasis7_wasm_abi/src/capability_v2.rs:396
record_layout!(oasis7_wasm_abi::CapabilityCatalogEntry, {module_id, module_version, namespace, command, schema_version, schema_hash, max_payload_bytes, eligible_grant_ids});

// crates/oasis7_agent_api/src/world_action/geometry.rs:5
record_layout!(oasis7_agent_api::world_action::GeoPos, {x_cm, y_cm, z_cm});

// crates/oasis7_agent_api/src/provider.rs:65
record_layout!(oasis7_agent_api::ProviderSelfState, {location_ref, pose_hint, status_flags, resource_summary});

// crates/oasis7_agent_api/src/provider.rs:75
record_layout!(oasis7_agent_api::ProviderMissionContext, {goal_summary, blocked_reason});

// crates/oasis7_agent_api/src/provider.rs:82
record_layout!(oasis7_agent_api::ProviderNearbyEntity, {entity_ref, kind, relation, relative_hint, interaction_hint});

// crates/oasis7_agent_api/src/provider.rs:92
record_layout!(oasis7_agent_api::ProviderRecentEvent, {event_ref, kind, summary, age_ticks});

// crates/oasis7_agent_api/src/provider.rs:100
record_layout!(oasis7_agent_api::ProviderNavigationNode, {node_ref, relation, relative_hint, traversable});

// crates/oasis7_agent_api/src/provider.rs:108
record_layout!(oasis7_agent_api::ProviderInteractionTarget, {target_ref, target_kind, interaction_hint});

// The concrete legacy aliases fill all seven operation parameters. The native
// TaskScheduler default profile is not the committed service payload profile.
record_layout!(crate::legacy_world_service::SchedulerIntentV1, {agent_id, request_id, delegation_generation, task_prefix_binding_revision, captured_base_binding, operation});
enum_layout!(crate::legacy_world_service::SchedulerOperationV1,
 units{}, tuples{ReserveLease(value),AdmitContinuation(value)},
 records{TaskProviderPrefix{context},TaskProviderFailure{context,reason},ProviderPrefix{request,context_digest},ProviderFailure{request,reason},SettleLease{lease_id,consumed_amount},ReleaseLease{lease_id},TransitionContinuation{continuation_id,to,logical_tick},ConsumeContinuationBudget{continuation_id,budget_spent,current_context},ResumeWake{wake_id,proposal,budget_spent,resume,current_context},HandoffWake{wake_id,disposition,current_context}});
record_layout!(oasis7_agent_api::game::TaskContextIdentityV1, {authority_mode, native_authorization_root, schema_revision, fence, agent_session_id, agent_turn_id, decision_request_id, retry_seq, transport_attempt, request_commitment, context_commitment, provider_invocation_commitment, valid_until_tick});
enum_layout!(
    oasis7_agent_api::game::TaskContextAuthorityModeV1,
    units { NativeExecutorV1 },
    tuples {},
    records {}
);
// GeoPos has an untagged numeric temporary (Integer/Float); DecimalU64 has a
// temporary String. Neither transient parser graph is an owned output field;
// the caller's bounded decode phase must pay them without changing grammar.
// GameTaskCloseConditionsV2.own_stop_intent has required_stop_intent custom
// deserialization: absent is distinct from an explicitly encoded null.
// Dynamic leaves in this extra graph: GameplayJson/expected_value_bytes use
// Vec<u8>; ProviderSelfState.resource_summary is BTreeMap<String,i64>;
// CognitionCausalProposalV1 has three arbitrary serde_json::Value trees.
// No extra Range/Duration/array/Box output is introduced by these service roots.
// Nonempty derived-default Strings also belong to caller decode admission:
// lease quote resource_version/purpose/scope/policy_revision/authority_context/
// world_binding, and provider observation/action schema version defaults.

// Exact permanent terminal DTO graph; source validation and signatures remain
// with the fixed task consumer, never with this allocation inventory.
record_layout!(oasis7_agent_api::game::GameOwnStopReferenceV2, {
    operation_id, original_digest, canonical_result_digest, post_fence
});
enum_layout!(oasis7_agent_api::game::GameTaskClosePredicateV2,
    units{InitialActive}, tuples{OwnStopSuspended(reference)}, records{});
record_layout!(oasis7_agent_api::game::GameTaskClosedV2, {
    schema_revision, close_operation_id, close_original_digest,
    termination_plan_digest, initial_fence, actual_before_fence,
    selected_predicate, revoked_signer_public_key, revoked_generation,
    permanent_nonce_high_water
});
