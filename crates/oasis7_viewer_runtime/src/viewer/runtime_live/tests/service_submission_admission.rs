use super::*;

#[test]
fn signed_unregistered_mismatched_and_revoked_sessions_never_submit_to_service() {
    for state in ["unregistered", "mismatched", "revoked"] {
        for collect in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
                WorldScenario::Minimal,
            ))
            .unwrap();
            let service = ed25519_dalek::SigningKey::from_bytes(&[9; 32]);
            server.config.world_service =
                Some(crate::world_service::client::WorldServiceClientConfig {
                    formal_read_trust: None,
                    endpoint: format!("http://{}", listener.local_addr().unwrap()),
                    trusted_service_public_key: hex::encode(service.verifying_key().to_bytes()),
                    expected_world: oasis7_client_api::world_service::WorldIdentity {
                        world_id: "test-world".into(),
                        genesis_digest: "genesis".into(),
                    },
                    scope_id: "public".into(),
                    read_private_key_hex: hex::encode([7; 32]),
                    timeout: Duration::from_millis(100),
                    max_response_bytes: 4096,
                });
            let key = ed25519_dalek::SigningKey::from_bytes(&[5; 32]);
            let public = hex::encode(key.verifying_key().to_bytes());
            match state {
                "mismatched" => {
                    server
                        .session_policy
                        .register_session("player-service", &hex::encode([6; 32]))
                        .unwrap();
                }
                "revoked" => {
                    server
                        .session_policy
                        .register_session("player-service", &public)
                        .unwrap();
                    server
                        .session_policy
                        .revoke_session("player-service", Some(&public))
                        .unwrap();
                }
                _ => {}
            }
            let request = if collect {
                let mut command = crate::viewer::CollectDataCommand::Submit {
                    request: crate::viewer::CollectDataRequest {
                        electricity_cost: 7,
                        data_amount: 11,
                        player_id: "player-service".into(),
                        public_key: Some(public.clone()),
                        auth: None,
                    },
                };
                let proof = crate::viewer::sign_collect_data_auth_proof(
                    &command,
                    19,
                    &public,
                    &hex::encode([5; 32]),
                )
                .unwrap();
                let crate::viewer::CollectDataCommand::Submit { request } = &mut command else {
                    unreachable!()
                };
                request.auth = Some(proof);
                ViewerRequest::CollectData { command }
            } else {
                ViewerRequest::GameplayAction {
                    request: signed_gameplay_action_request(
                        crate::viewer::GameplayActionRequest {
                            action_id: crate::viewer::ACTION_CLAIM_FIRST_AGENT.into(),
                            target_agent_id: crate::viewer::FIRST_AGENT_CLAIM_TARGET_AGENT_ID
                                .into(),
                            actor_agent_id: None,
                            player_id: "player-service".into(),
                            public_key: None,
                            auth: None,
                        },
                        19,
                        &public,
                        &hex::encode([5; 32]),
                    ),
                }
            };
            let baseline = serde_json::to_value(server.world.state()).unwrap();
            let shared = Arc::new(Mutex::new(server));
            let prepared =
                ViewerRuntimeLiveServer::prepare_shared_world_service_submission(&shared, &request)
                    .unwrap()
                    .unwrap();
            let expected_code = match state {
                "mismatched" => "session_key_mismatch",
                "revoked" => "session_revoked",
                _ => "session_not_found",
            };
            assert_eq!(
                prepared.admission_error.as_ref().unwrap().code,
                expected_code
            );
            let mut server = shared.lock().unwrap();
            if state == "unregistered" {
                // Later session admission cannot turn a rejected preparation into Submit.
                server
                    .session_policy
                    .register_session("player-service", &public)
                    .unwrap();
            }
            server.prepared_world_service_submission = Some(prepared);
            let error = match &request {
                ViewerRequest::GameplayAction { request } => {
                    server.submit_world_service_gameplay(request).unwrap_err()
                }
                ViewerRequest::CollectData { command } => {
                    server.submit_world_service_gameplay(command).unwrap_err()
                }
                _ => unreachable!(),
            };
            assert_eq!(error.code, expected_code);
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock,
                "unauthorized request crossed Submit boundary"
            );
            assert!(server.pending_world_service_gameplay.is_empty());
            assert!(server.runtime_action_players.is_empty());
            assert_eq!(
                serde_json::to_value(server.world.state()).unwrap(),
                baseline
            );
        }
    }
}
