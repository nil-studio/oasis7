use super::*;
use sha2::{Digest, Sha256};

fn install_viewer_recovery_precommit_failure(dir: &std::path::Path) {
    let generation_root = dir.join(".distfs-state/sidecar-generations");
    std::fs::create_dir_all(&generation_root).expect("create generation root");
    std::fs::write(
        generation_root.join(".test-fail-before-index-commit"),
        b"fail",
    )
    .expect("install precommit failpoint");
}

fn persist_session_side_effect_projection(
    recovery_dir: &std::path::Path,
) -> (ViewerRuntimeLiveServer, String, String, Vec<u8>) {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.to_path_buf()));
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("agent");
    let player_id = "player-session-projection".to_string();
    let cached_ack = crate::viewer::AgentChatAck {
        auth_nonce: None,
        agent_id: agent_id.clone(),
        accepted_at_tick: server.world.state().time,
        message_len: "persisted chat".len(),
        player_id: Some("cached-player".to_string()),
        intent_tick: Some(server.world.state().time),
        intent_seq: Some(77),
        idempotent_replay: false,
        intent_id: None,
        accepted_event_seq: None,
        status: None,
        receipt_ref: None,
        replaced_by: None,
    };
    server.llm_sidecar.record_chat_intent_ack(
        "cached-player",
        agent_id.as_str(),
        77,
        Some(server.world.state().time),
        "persisted chat",
        None,
        &cached_ack,
    );
    let (public_key, private_key) = test_signer(96);
    register_runtime_session(
        &mut server,
        player_id.as_str(),
        Some(agent_id.as_str()),
        9,
        public_key.as_str(),
        private_key.as_str(),
    );
    let metadata = RuntimeWorld::load_authoritative_recovery_metadata(recovery_dir)
        .expect("load recovery metadata")
        .expect("committed recovery metadata");
    (server, agent_id, player_id, metadata)
}

fn assert_session_side_effect_projection(
    server: &ViewerRuntimeLiveServer,
    agent_id: &str,
    player_id: &str,
) {
    assert_eq!(
        server.llm_sidecar.agent_player_bindings.get(agent_id),
        Some(&player_id.to_string())
    );
    assert_eq!(
        server.llm_sidecar.player_agent_bindings.get(player_id),
        Some(&agent_id.to_string())
    );
    assert!(
        server
            .llm_sidecar
            .agent_public_key_bindings
            .contains_key(agent_id)
    );
    assert_eq!(
        server.llm_sidecar.player_auth_last_nonce.get(player_id),
        Some(&9)
    );
    assert!(
        server
            .llm_sidecar
            .find_chat_intent_replay(
                "cached-player",
                agent_id,
                77,
                Some(server.world.state().time),
                "persisted chat",
                None,
            )
            .expect("lookup cached chat acknowledgement")
            .is_some()
    );
    assert!(!server.pending_virtual_events.is_empty());
}

#[test]
fn committed_fence_readback_restores_complete_session_side_effect_projection() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-session-projection-readback-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let (mut server, agent_id, player_id, metadata) =
        persist_session_side_effect_projection(&recovery_dir);
    let encoded: serde_json::Value =
        serde_json::from_slice(&metadata).expect("decode recovery envelope");
    assert!(encoded.get("session_side_effects").is_some());

    server.llm_sidecar.agent_player_bindings.clear();
    server.llm_sidecar.player_agent_bindings.clear();
    server.llm_sidecar.agent_public_key_bindings.clear();
    server.llm_sidecar.player_auth_last_nonce.clear();
    server.llm_sidecar.player_chat_intent_acks.clear();
    server.pending_virtual_events.clear();
    server.authoritative_recovery_write_fence =
        Some(hex::encode(Sha256::digest(metadata.as_slice())));
    server
        .resolve_authoritative_recovery_write_fence()
        .expect("resolve committed generation");

    assert_session_side_effect_projection(&server, agent_id.as_str(), player_id.as_str());
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn recovery_restart_restores_complete_session_side_effect_projection() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-session-projection-restart-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let (_source, agent_id, player_id, metadata) =
        persist_session_side_effect_projection(&recovery_dir);
    let mut restarted =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("restarted server");
    restarted.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    restarted.authoritative_recovery_write_fence =
        Some(hex::encode(Sha256::digest(metadata.as_slice())));
    restarted
        .resolve_authoritative_recovery_write_fence()
        .expect("restore committed generation after restart");

    assert_session_side_effect_projection(&restarted, agent_id.as_str(), player_id.as_str());
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn session_metadata_status_unknown_installs_write_fence() {
    let recovery_dir = std::env::temp_dir().join(format!(
        "oasis7-session-unknown-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("agent");
    let (old_public, old_private) = test_signer(94);
    register_runtime_session(
        &mut server,
        "player-unknown",
        Some(agent_id.as_str()),
        1,
        old_public.as_str(),
        old_private.as_str(),
    );
    let root = recovery_dir.join(".distfs-state/sidecar-generations");
    std::fs::write(root.join(".test-fail-before-index-commit"), b"fail").expect("failpoint");
    std::fs::write(root.join("index.json"), b"not-json").expect("corrupt index");
    let (new_public, _) = test_signer(95);

    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RotateSession {
            request: AuthoritativeSessionRotateRequest {
                player_id: "player-unknown".to_string(),
                old_session_pubkey: old_public.clone(),
                new_session_pubkey: new_public.clone(),
                rotate_reason: "unknown".to_string(),
                rotated_by: Some("security".to_string()),
            },
        })
        .expect_err("status unknown");

    assert_eq!(error.code, "recovery_persistence_status_unknown");
    assert!(
        server.authoritative_recovery_write_fence.is_some(),
        "indeterminate session metadata commit must fence writes"
    );
    assert!(
        server
            .session_policy
            .validate_known_session_key("player-unknown", old_public.as_str())
            .is_ok()
    );
    assert!(
        server
            .session_policy
            .validate_known_session_key("player-unknown", new_public.as_str())
            .is_err()
    );
    server
        .resolve_authoritative_recovery_write_fence()
        .expect("resolve attempt");
    assert!(
        server.authoritative_recovery_write_fence.is_some(),
        "unknown readback must remain fenced"
    );
    let _ = std::fs::remove_dir_all(recovery_dir);
}

#[test]
fn session_mutations_roll_back_in_memory_when_recovery_persistence_fails() {
    let register_dir = std::env::temp_dir().join(format!(
        "oasis7-session-register-atomic-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    install_viewer_recovery_precommit_failure(&register_dir);
    let mut register_server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    register_server.set_authoritative_recovery_dir_override(Some(register_dir.clone()));
    let agent_id = register_server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (register_public, register_private) = test_signer(91);
    let register_request = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: "player-register-atomic".to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: Some(agent_id.clone()),
            force_rebind: false,
        },
        1,
        register_public.as_str(),
        register_private.as_str(),
    );
    register_server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: register_request,
        })
        .expect_err("register persistence failure");
    assert!(
        register_server
            .session_policy
            .validate_known_session_key("player-register-atomic", register_public.as_str())
            .is_err()
    );
    assert!(
        !register_server
            .llm_sidecar
            .agent_player_bindings
            .contains_key(agent_id.as_str())
    );
    assert!(
        !register_server
            .llm_sidecar
            .player_auth_last_nonce
            .contains_key("player-register-atomic")
    );
    assert_eq!(
        register_server
            .prompt_control_authority
            .binding_epoch(agent_id.as_str()),
        0,
        "failed registration must roll back binding epoch"
    );

    let mutation_dir = std::env::temp_dir().join(format!(
        "oasis7-session-mutation-atomic-{}-{}",
        std::process::id(),
        crate::viewer::runtime_live::recovery_receipt::current_unix_time_ms()
    ));
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(mutation_dir.clone()));
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (old_public, old_private) = test_signer(92);
    register_runtime_session(
        &mut server,
        "player-mutation-atomic",
        Some(agent_id.as_str()),
        1,
        old_public.as_str(),
        old_private.as_str(),
    );
    let cached_ack = crate::viewer::AgentChatAck {
        auth_nonce: None,
        agent_id: agent_id.clone(),
        accepted_at_tick: server.world.state().time,
        message_len: "persist me".len(),
        player_id: Some("player-mutation-atomic".to_string()),
        intent_tick: Some(server.world.state().time),
        intent_seq: Some(44),
        idempotent_replay: false,
        intent_id: None,
        accepted_event_seq: None,
        status: None,
        receipt_ref: None,
        replaced_by: None,
    };
    server.llm_sidecar.record_chat_intent_ack(
        "player-mutation-atomic",
        agent_id.as_str(),
        44,
        Some(server.world.state().time),
        "persist me",
        Some(old_public.as_str()),
        &cached_ack,
    );
    install_viewer_recovery_precommit_failure(&mutation_dir);
    let (new_public, _) = test_signer(93);
    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RotateSession {
            request: AuthoritativeSessionRotateRequest {
                player_id: "player-mutation-atomic".to_string(),
                old_session_pubkey: old_public.clone(),
                new_session_pubkey: new_public.clone(),
                rotate_reason: "atomic-rotate".to_string(),
                rotated_by: Some("security".to_string()),
            },
        })
        .expect_err("rotate persistence failure");
    assert!(
        server
            .session_policy
            .validate_known_session_key("player-mutation-atomic", old_public.as_str())
            .is_ok()
    );
    assert!(
        server
            .llm_sidecar
            .find_chat_intent_replay(
                "player-mutation-atomic",
                agent_id.as_str(),
                44,
                Some(server.world.state().time),
                "persist me",
                Some(old_public.as_str()),
            )
            .expect("lookup cached acknowledgement")
            .is_some(),
        "failed rotation must restore chat acknowledgement idempotency state"
    );
    assert!(
        server
            .session_policy
            .validate_known_session_key("player-mutation-atomic", new_public.as_str())
            .is_err()
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_public_key_bindings
            .get(agent_id.as_str())
            .map(String::as_str),
        Some(old_public.as_str())
    );
    assert_eq!(
        server
            .prompt_control_authority
            .binding_epoch(agent_id.as_str()),
        1,
        "failed rotation must restore binding epoch"
    );
    assert!(
        !server
            .session_revoke_metadata
            .contains_key(&session_revoke_metadata_key(
                "player-mutation-atomic",
                old_public.as_str()
            ))
    );

    server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-mutation-atomic".to_string(),
                session_pubkey: Some(old_public.clone()),
                revoke_reason: "atomic-revoke".to_string(),
                revoked_by: Some("security".to_string()),
            },
        })
        .expect_err("revoke persistence failure");
    assert!(
        server
            .session_policy
            .validate_known_session_key("player-mutation-atomic", old_public.as_str())
            .is_ok()
    );
    assert!(
        server
            .llm_sidecar
            .find_chat_intent_replay(
                "player-mutation-atomic",
                agent_id.as_str(),
                44,
                Some(server.world.state().time),
                "persist me",
                Some(old_public.as_str()),
            )
            .expect("lookup cached acknowledgement after revoke failure")
            .is_some(),
        "failed revoke must restore chat acknowledgement idempotency state"
    );
    assert_eq!(
        server
            .prompt_control_authority
            .binding_epoch(agent_id.as_str()),
        1,
        "failed revoke must restore binding epoch"
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get(agent_id.as_str())
            .map(String::as_str),
        Some("player-mutation-atomic")
    );
    assert!(
        !server
            .session_revoke_metadata
            .contains_key(&session_revoke_metadata_key(
                "player-mutation-atomic",
                old_public.as_str()
            ))
    );
    let _ = std::fs::remove_dir_all(register_dir);
    let _ = std::fs::remove_dir_all(mutation_dir);
}

#[cfg(unix)]
#[test]
fn hosted_registration_persist_failure_rolls_back_binding_and_keeps_grant_retryable() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = lock_test_llm_env();
    let replay_dir = runtime_live_temp_dir("hosted-grant-persist-failure");
    std::fs::create_dir_all(&replay_dir).expect("create replay directory");
    let replay_ledger = replay_dir.join("registration-replay.json");
    std::fs::write(&replay_ledger, b"[]").expect("seed readable replay ledger");
    std::fs::set_permissions(&replay_dir, std::fs::Permissions::from_mode(0o500))
        .expect("make replay directory unwritable");

    let issuer_private_key = hex::encode([94_u8; 32]);
    let issuer_public_key =
        crate::viewer::derive_hosted_registration_issuer_public_key(issuer_private_key.as_str())
            .expect("derive issuer public key");
    unsafe {
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV,
            issuer_public_key,
        );
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV,
            replay_ledger.as_os_str(),
        );
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let hosted_player_id = "hosted-player-account-persist-failure";
    let (hosted_public_key, hosted_private_key) = test_signer(95);
    let registration_grant = crate::viewer::issue_hosted_registration_grant(
        hosted_player_id,
        hosted_public_key.as_str(),
        "device-persist-failure",
        "nonce-persist-failure",
        test_now_unix_ms(),
        issuer_private_key.as_str(),
    )
    .expect("issue registration grant");
    let request = |nonce| {
        signed_session_register_request(
            crate::viewer::AuthoritativeSessionRegisterRequest {
                player_id: hosted_player_id.to_string(),
                public_key: None,
                registration_grant: Some(registration_grant.clone()),
                auth: None,
                requested_agent_id: Some(agent_id.clone()),
                force_rebind: false,
            },
            nonce,
            hosted_public_key.as_str(),
            hosted_private_key.as_str(),
        )
    };

    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(1),
        })
        .expect_err("replay-ledger persistence must fail");
    assert!(
        error.message.contains("registration replay ledger"),
        "unexpected error: {error:?}"
    );
    assert_eq!(
        server.llm_sidecar.bound_agent_for_player(hosted_player_id),
        None,
        "failed registration must roll back the committed binding"
    );
    assert!(
        server
            .session_policy
            .validate_known_session_key(hosted_player_id, hosted_public_key.as_str())
            .is_err(),
        "failed registration must not commit the active session key"
    );
    assert_eq!(
        server
            .llm_sidecar
            .player_auth_last_nonce
            .get(hosted_player_id),
        None,
        "failed registration must not consume the request auth nonce"
    );

    std::fs::set_permissions(&replay_dir, std::fs::Permissions::from_mode(0o700))
        .expect("restore replay directory permissions");
    let _ = std::fs::remove_file(&replay_ledger);
    std::fs::write(&replay_ledger, b"[]").expect("restore writable replay ledger");
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(2),
        })
        .expect("the same grant must remain retryable after rollback");
    assert_eq!(ack.status, AuthoritativeRecoveryStatus::SessionRegistered);

    unsafe {
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV);
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV);
    }
    let _ = std::fs::remove_dir_all(replay_dir);
}

#[test]
fn hosted_registration_recovery_not_committed_keeps_grant_retryable_after_restart() {
    let _guard = lock_test_llm_env();
    let recovery_dir = runtime_live_temp_dir("hosted-grant-recovery-not-committed");
    let replay_ledger = recovery_dir.with_extension("registration-replay.json");
    let issuer_private_key = hex::encode([97_u8; 32]);
    let issuer_public_key =
        crate::viewer::derive_hosted_registration_issuer_public_key(issuer_private_key.as_str())
            .expect("derive issuer public key");
    unsafe {
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV,
            issuer_public_key,
        );
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV,
            replay_ledger.as_os_str(),
        );
    }

    let hosted_player_id = "hosted-player-account-recovery-not-committed";
    let (hosted_public_key, hosted_private_key) = test_signer(98);
    let registration_grant = crate::viewer::issue_hosted_registration_grant(
        hosted_player_id,
        hosted_public_key.as_str(),
        "device-recovery-not-committed",
        "nonce-recovery-not-committed",
        test_now_unix_ms(),
        issuer_private_key.as_str(),
    )
    .expect("issue registration grant");

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    server.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let request = |nonce| {
        signed_session_register_request(
            crate::viewer::AuthoritativeSessionRegisterRequest {
                player_id: hosted_player_id.to_string(),
                public_key: None,
                registration_grant: Some(registration_grant.clone()),
                auth: None,
                requested_agent_id: Some(agent_id.clone()),
                force_rebind: false,
            },
            nonce,
            hosted_public_key.as_str(),
            hosted_private_key.as_str(),
        )
    };

    install_viewer_recovery_precommit_failure(&recovery_dir);
    let error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(1),
        })
        .expect_err("recovery generation must not commit");
    assert_eq!(error.code, "recovery_persistence_commit_failed");
    let replay_bytes = std::fs::read(&replay_ledger).expect("grant nonce ledger must be durable");
    assert!(
        String::from_utf8_lossy(&replay_bytes).contains("nonce-recovery-not-committed"),
        "the grant must have reached the durable replay ledger before recovery persistence failed"
    );
    drop(server);

    std::fs::remove_file(
        recovery_dir.join(".distfs-state/sidecar-generations/.test-fail-before-index-commit"),
    )
    .expect("remove precommit failpoint");
    let mut restarted = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("restarted runtime server");
    restarted.set_authoritative_recovery_dir_override(Some(recovery_dir.clone()));
    let (ack, _) = restarted
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(2),
        })
        .expect("same grant must converge after a definitely uncommitted recovery generation");
    assert_eq!(ack.status, AuthoritativeRecoveryStatus::SessionRegistered);

    let other_recovery_dir = runtime_live_temp_dir("hosted-grant-other-recovery-domain");
    let mut other_server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("other runtime server");
    other_server.set_authoritative_recovery_dir_override(Some(other_recovery_dir.clone()));
    let replay = other_server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(3),
        })
        .expect_err("another recovery domain must not resume the claimed grant");
    assert!(
        replay
            .message
            .contains("registration grant replay detected")
    );

    unsafe {
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV);
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV);
    }
    let _ = std::fs::remove_dir_all(recovery_dir);
    let _ = std::fs::remove_dir_all(other_recovery_dir);
    let _ = std::fs::remove_file(replay_ledger);
}

#[test]
fn hosted_registration_grant_rotates_reload_key_without_requiring_login() {
    let _guard = lock_test_llm_env();
    let replay_dir = runtime_live_temp_dir("hosted-grant-reload-key");
    let replay_ledger = replay_dir.join("registration-replay.json");
    let issuer_private_key = hex::encode([96_u8; 32]);
    let issuer_public_key =
        crate::viewer::derive_hosted_registration_issuer_public_key(issuer_private_key.as_str())
            .expect("derive issuer public key");
    unsafe {
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV,
            issuer_public_key,
        );
        oasis7::env_mut::set_var(
            crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV,
            replay_ledger.as_os_str(),
        );
    }

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::TwoBases)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_ids: Vec<_> = server
        .world
        .state()
        .agents
        .keys()
        .take(2)
        .cloned()
        .collect();
    assert_eq!(agent_ids.len(), 2, "two bases must seed two agents");
    let agent_id = agent_ids[0].clone();
    let other_agent_id = agent_ids[1].clone();
    let hosted_player_id = "hosted-player-test-login-reload-key";
    let (old_public_key, _) = test_signer(97);
    let (new_public_key, new_private_key) = test_signer(98);

    let initial_session_plan = server
        .session_policy
        .validate_session_registration(hosted_player_id, old_public_key.as_str(), false)
        .expect("plan pre-reload session");
    server
        .session_policy
        .commit_session_registration(initial_session_plan);
    server
        .llm_sidecar
        .bind_agent_player(
            agent_id.as_str(),
            hosted_player_id,
            Some(old_public_key.as_str()),
            false,
        )
        .expect("seed pre-reload player binding");

    let registration_grant = crate::viewer::issue_hosted_registration_grant(
        hosted_player_id,
        new_public_key.as_str(),
        "device-reload-key",
        "nonce-reload-key",
        test_now_unix_ms(),
        issuer_private_key.as_str(),
    )
    .expect("issue refreshed registration grant");
    let request = |requested_agent_id: &str, auth_nonce| {
        signed_session_register_request(
            crate::viewer::AuthoritativeSessionRegisterRequest {
                player_id: hosted_player_id.to_string(),
                public_key: None,
                registration_grant: Some(registration_grant.clone()),
                auth: None,
                requested_agent_id: Some(requested_agent_id.to_string()),
                force_rebind: false,
            },
            auth_nonce,
            new_public_key.as_str(),
            new_private_key.as_str(),
        )
    };

    let unauthorized_rebind = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(other_agent_id.as_str(), 2),
        })
        .expect_err("reload key rotation must not imply permission to change agents");
    assert_eq!(unauthorized_rebind.code, "player_bind_failed");
    assert!(
        !replay_ledger.exists(),
        "rejected agent change must not consume the reload grant"
    );

    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(agent_id.as_str(), 3),
        })
        .expect("authenticated refresh grant should rotate the browser reload key");
    assert_eq!(ack.status, AuthoritativeRecoveryStatus::SessionRegistered);
    assert_eq!(ack.player_id.as_deref(), Some(hosted_player_id));
    assert_eq!(ack.agent_id.as_deref(), Some(agent_id.as_str()));
    assert!(
        server
            .session_policy
            .validate_known_session_key(hosted_player_id, old_public_key.as_str())
            .is_err(),
        "the pre-reload key must no longer authorize the hosted session"
    );
    assert!(
        server
            .session_policy
            .validate_known_session_key(hosted_player_id, new_public_key.as_str())
            .is_ok(),
        "the refreshed browser key must become the active session key"
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_public_key_bindings
            .get(agent_id.as_str())
            .map(String::as_str),
        Some(new_public_key.as_str()),
        "the existing player/agent binding must rotate to the refreshed browser key"
    );

    let replay_error = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(agent_id.as_str(), 4),
        })
        .expect_err("a committed reload grant must remain single use");
    assert_eq!(replay_error.code, "auth_invalid");
    assert!(replay_error.message.contains("registration grant replay"));

    unsafe {
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV);
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV);
    }
    let _ = std::fs::remove_dir_all(replay_dir);
}

#[test]
fn runtime_agent_chat_rejects_intent_seq_conflict_on_payload_change() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key, private_key) = test_signer(22);
    let first = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: Some(10),
            intent_seq: Some(6),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        6,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        5,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    server
        .handle_agent_chat(first)
        .expect("first request accepted");

    let conflict = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id,
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "changed".to_string(),
            intent_tick: Some(10),
            intent_seq: Some(6),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        6,
        public_key.as_str(),
        private_key.as_str(),
    );
    let err = server
        .handle_agent_chat(conflict)
        .expect_err("same seq with different payload must fail");
    assert_eq!(err.code, "intent_seq_conflict");
}

#[test]
fn runtime_agent_chat_rejects_intent_seq_nonce_mismatch() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    let (public_key, private_key) = test_signer(23);
    let request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: Some(3),
            intent_seq: Some(8),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        9,
        public_key.as_str(),
        private_key.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        8,
        public_key.as_str(),
        private_key.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    let err = server
        .handle_agent_chat(request)
        .expect_err("intent seq mismatch should fail");
    assert_eq!(err.code, "intent_seq_invalid");
}

#[test]
fn runtime_authoritative_recovery_rotate_and_revoke_session_enforced_for_agent_chat() {
    let _guard = lock_test_llm_env();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("runtime server");
    let agent_id = server
        .world
        .state()
        .agents
        .keys()
        .next()
        .cloned()
        .expect("seed agent");
    seed_agent_chat_oc(&mut server, agent_id.as_str());
    let (public_key_v1, private_key_v1) = test_signer(31);
    let (public_key_v2, private_key_v2) = test_signer(32);

    let first_request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "hello".to_string(),
            intent_tick: Some(1),
            intent_seq: Some(2),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        2,
        public_key_v1.as_str(),
        private_key_v1.as_str(),
    );
    let register_ack = register_runtime_session(
        &mut server,
        "player-a",
        Some(agent_id.as_str()),
        1,
        public_key_v1.as_str(),
        private_key_v1.as_str(),
    );
    assert_eq!(
        register_ack.status,
        AuthoritativeRecoveryStatus::SessionRegistered
    );
    assert_eq!(register_ack.agent_id.as_deref(), Some(agent_id.as_str()));
    let _ = server
        .handle_agent_chat(first_request)
        .expect("first key should be accepted");

    let (rotate_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RotateSession {
            request: AuthoritativeSessionRotateRequest {
                player_id: "player-a".to_string(),
                old_session_pubkey: public_key_v1.clone(),
                new_session_pubkey: public_key_v2.clone(),
                rotate_reason: "security_rotation".to_string(),
                rotated_by: Some("ops".to_string()),
            },
        })
        .expect("rotate session");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        rotate_ack.status,
        AuthoritativeRecoveryStatus::SessionRotated
    );
    assert_eq!(
        rotate_ack.session_pubkey.as_deref(),
        Some(public_key_v1.as_str())
    );
    assert_eq!(
        rotate_ack.replaced_by_pubkey.as_deref(),
        Some(public_key_v2.as_str())
    );

    let stale_request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "stale".to_string(),
            intent_tick: Some(2),
            intent_seq: Some(2),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        2,
        public_key_v1.as_str(),
        private_key_v1.as_str(),
    );
    let stale_err = server
        .handle_agent_chat(stale_request)
        .expect_err("old key should be rejected after rotation");
    assert_eq!(stale_err.code, "session_revoked");

    let replaces_intent_id = server
        .world
        .state()
        .agents
        .get(agent_id.as_str())
        .and_then(|cell| cell.intent.as_ref())
        .map(|intent| intent.intent_id.clone());
    let rotated_request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            canonical_authority: None,
            agent_id: agent_id.clone(),
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "rotated".to_string(),
            intent_tick: Some(3),
            intent_seq: Some(1),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id,
        },
        1,
        public_key_v2.as_str(),
        private_key_v2.as_str(),
    );
    let _ = server
        .handle_agent_chat(rotated_request)
        .expect("new key should be accepted");

    let (revoke_ack, emit_snapshot_after_ack) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RevokeSession {
            request: AuthoritativeSessionRevokeRequest {
                player_id: "player-a".to_string(),
                session_pubkey: Some(public_key_v2.clone()),
                revoke_reason: "compromised".to_string(),
                revoked_by: Some("ops".to_string()),
            },
        })
        .expect("revoke session");
    assert!(!emit_snapshot_after_ack);
    assert_eq!(
        revoke_ack.status,
        AuthoritativeRecoveryStatus::SessionRevoked
    );
    assert_eq!(revoke_ack.revoke_reason.as_deref(), Some("compromised"));
    assert_eq!(revoke_ack.revoked_by.as_deref(), Some("ops"));

    let revoked_reconnect_err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::ReconnectSync {
            request: AuthoritativeReconnectSyncRequest {
                player_id: "player-a".to_string(),
                session_pubkey: Some(public_key_v2.clone()),
                last_known_log_cursor: None,
                expected_reorg_epoch: None,
            },
        })
        .expect_err("reconnect should surface revoke metadata");
    assert_eq!(revoked_reconnect_err.code, "session_revoked");
    assert_eq!(
        revoked_reconnect_err.revoke_reason.as_deref(),
        Some("compromised")
    );
    assert_eq!(revoked_reconnect_err.revoked_by.as_deref(), Some("ops"));

    let revoked_register_request = signed_session_register_request(
        crate::viewer::AuthoritativeSessionRegisterRequest {
            player_id: "player-a".to_string(),
            public_key: None,
            registration_grant: None,
            auth: None,
            requested_agent_id: Some(agent_id.clone()),
            force_rebind: false,
        },
        5,
        public_key_v2.as_str(),
        private_key_v2.as_str(),
    );
    let revoked_register_err = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: revoked_register_request,
        })
        .expect_err("register should surface revoke metadata");
    assert_eq!(revoked_register_err.code, "session_revoked");
    assert_eq!(
        revoked_register_err.revoke_reason.as_deref(),
        Some("compromised")
    );
    assert_eq!(revoked_register_err.revoked_by.as_deref(), Some("ops"));

    let revoked_request = signed_agent_chat_request(
        crate::viewer::AgentChatRequest {
            agent_id,
            player_id: Some("player-a".to_string()),
            public_key: None,
            auth: None,
            message: "revoked".to_string(),
            intent_tick: Some(4),
            intent_seq: Some(2),
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            replaces_intent_id: None,
            canonical_authority: None,
        },
        2,
        public_key_v2.as_str(),
        private_key_v2.as_str(),
    );
    let revoked_err = server
        .handle_agent_chat(revoked_request)
        .expect_err("revoked key should be rejected");
    assert_eq!(revoked_err.code, "session_revoked");
}
