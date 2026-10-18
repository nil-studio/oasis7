//! CollectData service qualification and original-request feedback regressions.
use super::*;

fn install_collect_data_verified_projection(
    server: &mut ViewerRuntimeLiveServer,
    player: &str,
    public: &str,
    collector: &str,
    claims: usize,
) {
    use oasis7_client_api::world_service::*;
    let identity = WorldIdentity {
        world_id: "collect-world".into(),
        genesis_digest: "fixture".into(),
    };
    let commit = CommitRef {
        world: identity.clone(),
        binding: ExecutionBinding {
            provider_world_id: "collect-world".into(),
            branch_id: "main".into(),
            finality_ref: "fixture".into(),
            reorg_generation: 0,
            governing_manifest_ref: "fixture".into(),
            authority_generation: 1,
            permission_generation: 1,
        },
        position: 2,
        execution_block_hash: "fixture".into(),
        state_root_ref: "fixture".into(),
    };
    let mut projection =
        crate::world_service::projection::WorldServiceProjection::from_world(&server.world, None)
            .unwrap();
    projection.runtime_binding = None;
    projection.state.starter_oc_claims.clear();
    for index in 0..claims {
        projection.state.starter_oc_claims.insert(
            format!("claim-{index}"),
            serde_json::from_value(serde_json::json!({
                "agent_id":collector, "player_id":player, "public_key":public, "amount":0, "claimed_at":0
            })).unwrap(),
        );
    }
    projection
        .state
        .agents
        .get_mut(collector)
        .unwrap()
        .state
        .resources
        .set(
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            30,
        )
        .unwrap();
    let request = ReadWorldViewRequest {
        formal_anchor: None,
        observation_nonce: None,
        contract_version: 1,
        world: identity.clone(),
        scope_id: "public".into(),
        min_commit: None,
        fixed_commit: None,
        deadline_unix_ms: None,
    };
    let response = ReadWorldViewResponse {
        formal_profile: None,
        formal_proof: None,
        execution_evidence_scope: None,
        contract_version: 1,
        version: ProjectionVersion {
            commit: commit.clone(),
            projection_revision: "fixture".into(),
            visibility_scope: "public".into(),
        },
        logical_tick: projection.state.time,
        continuation: EventCursor {
            stream_id: "fixture".into(),
            scope_id: "public".into(),
            era: 0,
            sequence: 0,
            commit,
        },
        view: projection,
    };
    let signed_request = crate::world_service::authority::sign_read_request(
        VIEW_PATH,
        request.clone(),
        &test_signer(241).1,
    )
    .unwrap();
    let digest =
        crate::world_service::authority::request_digest(VIEW_PATH, &signed_request).unwrap();
    let signed_response = crate::world_service::authority::sign_service_response(
        VIEW_PATH,
        digest,
        response,
        &test_signer(240).1,
    )
    .unwrap();
    server.verified_world_view = Some(
        crate::world_service::verified_view::VerifiedWorldView::verify_service_assertion(
            signed_response,
            &signed_request,
            &test_signer(240).0,
            crate::world_service::verified_view::ReadEvidencePolicy::Ordinary,
        )
        .unwrap(),
    );
    server.config.world_service = Some(crate::world_service::client::WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:9".into(),
        trusted_service_public_key: test_signer(240).0,
        expected_world: identity,
        scope_id: "public".into(),
        read_private_key_hex: test_signer(241).1,
        timeout: Duration::from_millis(50),
        formal_read_trust: None,
        max_response_bytes: 4096,
    });
}

#[test]
fn runtime_collect_data_service_preflight_uses_one_verified_claim_and_balance_projection() {
    let _guard = lock_test_llm_env();
    for (claims, binding, expected_error) in [
        (0, "current", Some("collect_data_owner_claim_missing")),
        (1, "current", None),
        (2, "current", Some("collect_data_owner_claim_ambiguous")),
        (1, "stale", Some("collect_data_binding_mismatch")),
        (1, "missing", Some("player_agent_binding_required")),
    ] {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm),
        )
        .unwrap();
        let collector = server.world.state().agents.keys().next().unwrap().clone();
        let (public, private) = test_signer(239);
        register_runtime_session(
            &mut server,
            "projection-owner",
            Some(&collector),
            1,
            &public,
            &private,
        );
        server
            .world
            .set_agent_resource_balance(
                &collector,
                oasis7_agent_api::world_action::ResourceKind::Electricity,
                2,
            )
            .unwrap();
        install_collect_data_verified_projection(
            &mut server,
            "projection-owner",
            &public,
            &collector,
            claims,
        );
        if binding != "current" {
            server.llm_sidecar.clear_player_binding("projection-owner");
            if binding == "stale" {
                server
                    .llm_sidecar
                    .bind_agent_player("stale-agent", "projection-owner", Some(&public), true)
                    .unwrap();
            }
        }
        let baseline = server.world.state().clone();
        let projection_before =
            serde_json::to_value(server.verified_world_view.as_ref().unwrap().projection())
                .unwrap();
        let version_before = server
            .verified_world_view
            .as_ref()
            .unwrap()
            .version()
            .clone();
        let command = signed_collect_data_command(
            crate::viewer::CollectDataCommand::Preflight {
                request: crate::viewer::CollectDataRequest {
                    electricity_cost: 7,
                    data_amount: 11,
                    player_id: "projection-owner".into(),
                    public_key: None,
                    auth: None,
                },
            },
            2,
            &public,
            &private,
        );
        let result = server.handle_collect_data(command.clone());
        if let Some(expected_error) = expected_error {
            assert_eq!(
                result
                    .err()
                    .unwrap_or_else(|| panic!(
                        "qualification/binding rejected: claims={claims}, binding={binding}"
                    ))
                    .code,
                expected_error,
                "claims={claims}, binding={binding}"
            );
        } else {
            let crate::viewer::runtime_live::player_gameplay::CollectDataResult::Preflight(quote) =
                result.unwrap()
            else {
                panic!("preflight")
            };
            assert_eq!(quote.collector_agent_id, collector);
            assert_eq!(
                quote.electricity_after, 23,
                "balance comes from the same verified view as the claim"
            );
            assert!(quote.can_execute);
        }
        assert_eq!(
            server.world.state(),
            &baseline,
            "quote cannot write world or canonical nonce state"
        );
        assert_eq!(
            serde_json::to_value(server.verified_world_view.as_ref().unwrap().projection())
                .unwrap(),
            projection_before
        );
        assert_eq!(
            server.verified_world_view.as_ref().unwrap().version(),
            &version_before
        );
        assert_eq!(
            server
                .llm_sidecar
                .player_auth_last_nonce
                .get("projection-owner"),
            Some(&2),
            "quote keeps its own existing auth nonce consumption"
        );
        assert_eq!(
            server
                .handle_collect_data(command)
                .err()
                .expect("collect data error")
                .code,
            "auth_nonce_replay"
        );
    }
}

#[test]
fn runtime_collect_data_service_preflight_requires_verified_view_without_local_fallback() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .unwrap();
    let collector = server.world.state().agents.keys().next().unwrap().clone();
    let (public, private) = test_signer(239);
    register_runtime_session(
        &mut server,
        "projection-owner",
        Some(&collector),
        1,
        &public,
        &private,
    );
    install_collect_data_verified_projection(
        &mut server,
        "projection-owner",
        &public,
        &collector,
        1,
    );
    server.verified_world_view = None;
    let baseline = server.world.state().clone();
    let command = signed_collect_data_command(
        crate::viewer::CollectDataCommand::Preflight {
            request: crate::viewer::CollectDataRequest {
                electricity_cost: 1,
                data_amount: 1,
                player_id: "projection-owner".into(),
                public_key: None,
                auth: None,
            },
        },
        2,
        &public,
        &private,
    );
    let error = server
        .handle_collect_data(command)
        .err()
        .expect("collect data error");
    assert_eq!(error.code, "llm_init_failed");
    assert!(
        error
            .message
            .contains("canonical Agent observation unavailable")
    );
    assert_eq!(server.world.state(), &baseline);
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get("projection-owner"),
        Some(&1),
        "the existing readiness gate fails before consuming the request nonce"
    );
}

#[test]
fn runtime_collect_data_service_submit_keeps_own_pending_result_and_canonical_ack_target() {
    use oasis7_client_api::world_service::{IntentOutcome, IntentResponse, SubmitObservation};
    let _guard = lock_test_llm_env();
    for initial_projection in [true, false] {
        let mut server = ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm),
        )
        .unwrap();
        let collector = server.world.state().agents.keys().next().unwrap().clone();
        let (public, private) = test_signer(239);
        register_runtime_session(
            &mut server,
            "projection-owner",
            Some(&collector),
            1,
            &public,
            &private,
        );
        install_collect_data_verified_projection(
            &mut server,
            "projection-owner",
            &public,
            &collector,
            1,
        );
        if !initial_projection {
            server.verified_world_view = None;
        }
        server.llm_sidecar.clear_player_binding("projection-owner");
        server
            .llm_sidecar
            .bind_agent_player("stale-local-agent", "projection-owner", Some(&public), true)
            .unwrap();
        let command = signed_collect_data_command(
            crate::viewer::CollectDataCommand::Submit {
                request: crate::viewer::CollectDataRequest {
                    electricity_cost: 7,
                    data_amount: 11,
                    player_id: "projection-owner".into(),
                    public_key: None,
                    auth: None,
                },
            },
            2,
            &public,
            &private,
        );
        let request = ViewerRequest::CollectData {
            command: command.clone(),
        };
        let correlation = crate::world_service::derive_correlation(
            server
                .config
                .world_service
                .as_ref()
                .unwrap()
                .expected_world
                .clone(),
            &crate::world_service::WorldServicePayloadV1::GameplayJson(
                serde_json::to_vec(&command).unwrap(),
            ),
        )
        .unwrap();
        let feedback_origin = server.gameplay_request_feedback_origin(&request);
        server.prepared_world_service_submission = Some(
            crate::viewer::runtime_live::world_service_link::PreparedWorldServiceSubmission {
                correlation: correlation.clone(),
                admission_error: None,
                feedback_origin,
                outcome: Ok(SubmitObservation::Response(Box::new(IntentResponse {
                    formal_profile: None,
                    execution_evidence_scope: None,
                    contract_version: 1,
                    correlation: correlation.clone(),
                    outcome: IntentOutcome::Pending,
                }))),
            },
        );
        let mut session = RuntimeLiveSession::new();
        let mut output = Vec::new();
        server
            .handle_request(request.clone(), &mut session, &mut output)
            .unwrap();
        let response: ViewerResponse =
            serde_json::from_slice(output.split(|byte| *byte == b'\n').next().unwrap()).unwrap();
        let ViewerResponse::GameplayActionAck { ack } = response else {
            panic!("expected ack")
        };
        assert_eq!(
            ack.target_agent_id,
            if initial_projection {
                collector.clone()
            } else {
                String::new()
            }
        );
        assert_ne!(ack.target_agent_id, "stale-local-agent");
        assert!(
            session.gameplay_feedback_owner.is_none(),
            "replayable submit alone cannot authorize historical reads"
        );
        assert!(server.scoped_gameplay_feedback.is_empty());
        assert_eq!(
            session
                .local_gameplay_feedback
                .lock()
                .unwrap()
                .feedback
                .as_ref()
                .unwrap()
                .stage,
            "pending"
        );
        if !initial_projection {
            install_collect_data_verified_projection(
                &mut server,
                "projection-owner",
                &public,
                &collector,
                1,
            );
            server
                .llm_sidecar
                .bind_agent_player(&collector, "projection-owner", Some(&public), true)
                .unwrap();
            let preflight = signed_collect_data_command(
                crate::viewer::CollectDataCommand::Preflight {
                    request: crate::viewer::CollectDataRequest {
                        electricity_cost: 7,
                        data_amount: 11,
                        player_id: "projection-owner".into(),
                        public_key: None,
                        auth: None,
                    },
                },
                3,
                &public,
                &private,
            );
            output.clear();
            server
                .handle_request(
                    ViewerRequest::CollectData { command: preflight },
                    &mut session,
                    &mut output,
                )
                .unwrap();
            let response: ViewerResponse =
                serde_json::from_slice(output.split(|byte| *byte == b'\n').next().unwrap())
                    .unwrap();
            assert!(
                matches!(response, ViewerResponse::CollectDataPreflight { .. }),
                "fresh authentication: {response:?}"
            );
            assert!(session.gameplay_feedback_owner.is_some());
            // Retrying the older canonical request must retain its first
            // unknown-context fence and this newly authenticated connection's
            // local result sink, since it cannot write that fence to cache.
            let feedback_origin = server.gameplay_request_feedback_origin(&request);
            server.prepared_world_service_submission = Some(
                crate::viewer::runtime_live::world_service_link::PreparedWorldServiceSubmission {
                    correlation: correlation.clone(),
                    admission_error: None,
                    feedback_origin,
                    outcome: Ok(SubmitObservation::Response(Box::new(IntentResponse {
                        formal_profile: None,
                        execution_evidence_scope: None,
                        contract_version: 1,
                        correlation: correlation.clone(),
                        outcome: IntentOutcome::Pending,
                    }))),
                },
            );
            output.clear();
            server
                .handle_request(request, &mut session, &mut output)
                .unwrap();
            assert_eq!(
                session
                    .local_gameplay_feedback
                    .lock()
                    .unwrap()
                    .feedback
                    .as_ref()
                    .unwrap()
                    .stage,
                "pending"
            );
        }
        server.complete_pending_gameplay_feedback(
            &correlation,
            ViewerRuntimeLiveServer::make_player_gameplay_feedback(
                "world_service_intent",
                "committed",
                "authenticated original disposition",
                None,
                None,
                None,
                None,
                0,
                0,
            ),
        );
        let current = server
            .compat_snapshot_for_session(&mut session)
            .player_gameplay
            .unwrap()
            .recent_feedback
            .unwrap();
        assert_eq!(current.stage, "committed");
        assert!(server.latest_player_gameplay_feedback.is_none());
        assert_eq!(
            server
                .llm_sidecar
                .player_auth_last_nonce
                .get("projection-owner"),
            Some(if initial_projection { &1 } else { &3 }),
            "service submit/retry keeps the nonce consumed only by fresh registration/preflight"
        );
    }
}
