use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, atomic::AtomicBool};

struct OfflineProviderFixture {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl OfflineProviderFixture {
    fn start() -> Self {
        let keys = [
            VIEWER_AGENT_DECISION_SOURCE_ENV,
            VIEWER_AGENT_PROVIDER_BACKEND_ENV,
            VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
            VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
            VIEWER_AGENT_PROVIDER_URL_ENV,
            VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
            VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
            VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
            VIEWER_AGENT_PROVIDER_PROFILE_ENV,
            VIEWER_AGENT_EXECUTION_LANE_ENV,
            VIEWER_AGENT_PROVIDER_MODE_ENV,
        ];
        let previous = keys.iter().map(|k| (*k, std::env::var_os(k))).collect();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let worker = std::thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                let mut socket = match listener.accept() {
                    Ok((socket, _)) => socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("owner fixture listener: {e}"),
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 1024];
                while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    assert!(bytes.len() < 65536);
                }
                let first = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap();
                let body = if first.starts_with("GET /v1/provider/info ") {
                    r#"{"provider_id":"provider_local_bridge","name":"Provider Local Bridge","version":"0.1.0","protocol_version":"world-simulator-provider-loopback-http-v1","chain_resource_manifest_schema_version":"oasis7.world_resource_manifest.v1","chain_resource_delta_schema_version":"oasis7.world_resource_delta.v1","capabilities":["decision","feedback"],"supported_action_sets":["wait","wait_ticks","move_agent","speak_to_nearby","inspect_target","simple_interact"]}"#
                } else if first.starts_with("GET /v1/provider/health ") {
                    r#"{"ok":true,"status":"ready","uptime_ms":1,"last_error":null,"queue_depth":0}"#
                } else {
                    panic!("owner authorization fixture must not invoke model/economy: {first}")
                };
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        // SAFETY: Caller holds the canonical provider environment lock; RAII restores every changed key.
        unsafe {
            for key in keys {
                oasis7::env_mut::remove_var(key);
            }
            for (key, value) in [
                (VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed"),
                (VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock"),
                (VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1"),
                (VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http"),
                (VIEWER_AGENT_PROVIDER_URL_ENV, endpoint.as_str()),
                (VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc"),
                (VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity"),
                (VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http"),
            ] {
                oasis7::env_mut::set_var(key, value);
            }
        }
        Self {
            previous,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for OfflineProviderFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let joined = self.worker.take().map(|worker| worker.join());
        for (key, value) in self.previous.drain(..) {
            // SAFETY: The canonical provider lock outlives this fixture.
            unsafe {
                match value {
                    Some(v) => oasis7::env_mut::set_var(key, v),
                    None => oasis7::env_mut::remove_var(key),
                }
            }
        }
        if let Some(result) = joined {
            result.unwrap();
        }
    }
}

fn runtime_commands(
    server: &ViewerRuntimeLiveServer,
    agent_id: &str,
) -> Vec<AgencyControlOperation> {
    let (intent_id, request_digest) = current_intent(server, agent_id);
    vec![
        AgencyControlOperation::InstallDelegation {
            grant_id: "new-owner-grant".into(),
            object_id: agent_id.into(),
            action_kinds: vec!["TransferMaterial".into()],
            period_id: "new-owner-period".into(),
            valid_from_tick: server.world.state().time,
            valid_until_tick: server.world.state().time.saturating_add(100),
            limit_units: 4,
        },
        AgencyControlOperation::RevokeDelegation {
            grant_id: "owner-grant".into(),
            expected_revision: 1,
        },
        AgencyControlOperation::OverridePendingIntent {
            control_id: "service-override".into(),
            intent_id: intent_id.clone(),
            request_digest: request_digest.clone(),
            grant_id: "owner-grant".into(),
            expected_grant_revision: 1,
        },
        AgencyControlOperation::InterruptPendingIntent {
            control_id: "service-interrupt".into(),
            intent_id,
            request_digest,
            grant_id: None,
            expected_grant_revision: None,
        },
    ]
}

#[test]
fn signed_runtime_agency_controls_fail_closed_in_service_mode_and_preserve_offline_success() {
    let _lock = runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let _provider = OfflineProviderFixture::start();
    // Both service configuration and the legacy chain-linked configuration
    // make the App World a projection rather than mutation authority.
    for service_configured in [false, true] {
        for operation_index in 0..4 {
            let (mut server, agent_id, public_key, private_key) = owner_server_with_config(
                117,
                ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                    .with_decision_mode(ViewerLiveDecisionMode::Llm),
            );
            let command = runtime_commands(&server, &agent_id).remove(operation_index);
            if service_configured {
                server.config.world_service =
                    Some(crate::world_service::client::WorldServiceClientConfig {
                        formal_read_trust: None,
                        endpoint: "http://127.0.0.1:1".into(),
                        trusted_service_public_key: public_key.clone(),
                        expected_world: oasis7_client_api::world_service::WorldIdentity {
                            world_id: server.config.world_id.clone(),
                            genesis_digest: "agency-gate-test-genesis".into(),
                        },
                        scope_id: "agency-gate-test-scope".into(),
                        read_private_key_hex: private_key.clone(),
                        timeout: Duration::from_millis(100),
                        max_response_bytes: 65536,
                    });
            } else {
                server.config.chain_status_bind = Some("127.0.0.1:1".into());
            }
            let request = signed_agency_request(
                &server,
                &agent_id,
                "service-owner-control",
                command,
                1_002,
                PLAYER_ID,
                &public_key,
                &private_key,
            );
            crate::viewer::auth::verify_agency_control_auth_proof(
                &request.request_id,
                &request.player_id,
                &request.public_key,
                &request.agent_id,
                &request.world_id,
                request.reorg_epoch,
                &serde_json::to_value(&request.command).unwrap(),
                request.auth.as_ref().unwrap(),
            )
            .expect("actual owner signature verifies before testing the service boundary");
            let before_snapshot = serde_json::to_value(server.world.snapshot()).unwrap();
            let before_journal = serde_json::to_value(server.world.journal()).unwrap();
            let before_cognition = server.world.cognition().clone();
            let before_root = server
                .world
                .execution_state_identity()
                .unwrap()
                .state_root_hash;
            let denied = server.handle_agency_control_request(request.clone());
            assert_eq!(denied.status, "error", "{denied:?}");
            assert_eq!(
                denied.error.as_ref().unwrap().code,
                "canonical_agency_control_unavailable"
            );
            assert!(
                denied.data.is_none(),
                "no local authorization can masquerade as a canonical receipt"
            );
            assert_eq!(
                serde_json::to_value(server.world.snapshot()).unwrap(),
                before_snapshot
            );
            assert_eq!(
                serde_json::to_value(server.world.journal()).unwrap(),
                before_journal
            );
            assert_eq!(
                server
                    .world
                    .execution_state_identity()
                    .unwrap()
                    .state_root_hash,
                before_root
            );
            assert_eq!(server.world.cognition(), &before_cognition);
            assert!(
                server
                    .llm_sidecar
                    .validate_player_auth_nonce(PLAYER_ID, 1_002)
                    .is_ok()
            );

            // Reuse the exact signed request, including its nonce. Offline
            // success proves denial neither consumed authentication nor
            // changed the pending intent/delegation preconditions.
            server.config.world_service = None;
            server.config.chain_status_bind = None;
            let offline_command = request.command.clone();
            let offline = server.handle_agency_control_request(request.clone());
            assert_eq!(
                offline.status, "ok",
                "operation={operation_index}: {offline:?}"
            );
            assert!(
                server
                    .llm_sidecar
                    .validate_player_auth_nonce(PLAYER_ID, 1_002)
                    .is_err()
            );
            assert_ne!(
                server.world.cognition(),
                &before_cognition,
                "offline authorization must alter the actual delegation/control ledger"
            );
            match offline_command {
                AgencyControlOperation::InstallDelegation {
                    grant_id,
                    action_kinds,
                    limit_units,
                    ..
                } => {
                    let rows = server
                        .world
                        .agent_delegation_authorizations(&agent_id)
                        .unwrap();
                    let grant = &rows
                        .iter()
                        .find(|row| row.grant.grant_id == grant_id)
                        .unwrap()
                        .grant;
                    assert_eq!(grant.agent_id, agent_id);
                    assert_eq!(grant.owner_id, PLAYER_ID);
                    assert_eq!(grant.action_kinds, action_kinds);
                    assert_eq!(grant.limit_units, limit_units);
                    assert_eq!(grant.revision, 1);
                    assert!(!grant.revoked);
                }
                AgencyControlOperation::RevokeDelegation {
                    grant_id,
                    expected_revision,
                } => {
                    let rows = server
                        .world
                        .agent_delegation_authorizations(&agent_id)
                        .unwrap();
                    let grant = &rows
                        .iter()
                        .find(|row| row.grant.grant_id == grant_id)
                        .unwrap()
                        .grant;
                    assert!(grant.revoked);
                    assert_eq!(grant.revision, expected_revision + 1);
                }
                AgencyControlOperation::OverridePendingIntent {
                    control_id,
                    intent_id,
                    request_digest,
                    grant_id,
                    expected_grant_revision,
                } => {
                    let (actual, _) = server
                        .world
                        .signed_owner_control_replay(&request, &server.current_owner_authority())
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        actual,
                        crate::runtime::AgentOwnerControlV1 {
                            control_id,
                            agent_id: agent_id.clone(),
                            intent_id,
                            request_digest,
                            grant_id: Some(grant_id),
                            expected_grant_revision: Some(expected_grant_revision),
                            kind: crate::runtime::AgentOwnerControlKindV1::Override,
                        }
                    );
                }
                AgencyControlOperation::InterruptPendingIntent {
                    control_id,
                    intent_id,
                    request_digest,
                    grant_id,
                    expected_grant_revision,
                } => {
                    let (actual, disposition) = server
                        .world
                        .signed_owner_control_replay(&request, &server.current_owner_authority())
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        actual,
                        crate::runtime::AgentOwnerControlV1 {
                            control_id,
                            agent_id: agent_id.clone(),
                            intent_id,
                            request_digest,
                            grant_id,
                            expected_grant_revision,
                            kind: crate::runtime::AgentOwnerControlKindV1::Interrupt,
                        }
                    );
                    assert_eq!(disposition.status, "cancelled");
                    assert_eq!(
                        server.world.state().agents[&agent_id]
                            .intent
                            .as_ref()
                            .unwrap()
                            .status,
                        "cancelled"
                    );
                }
                _ => unreachable!(),
            }
        }
    }
}
