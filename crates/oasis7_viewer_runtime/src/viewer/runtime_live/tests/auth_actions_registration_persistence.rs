use super::*;

#[test]
fn hosted_registration_persist_failure_rolls_back_binding_and_keeps_grant_retryable() {
    let _guard = lock_test_llm_env();
    let replay_dir = runtime_live_temp_dir("hosted-grant-persist-failure");
    std::fs::create_dir_all(&replay_dir).expect("create replay directory");
    let replay_ledger = replay_dir.join("registration-replay.json");
    std::fs::write(&replay_ledger, b"[]").expect("seed readable replay ledger");
    // Keep lock acquisition/read preflight valid, then fail the atomic ledger
    // write after registration validation/planning, before binding/session commit.
    let write_blocker = replay_dir.join(format!(".registration-replay-{}.tmp", std::process::id()));
    std::fs::create_dir(&write_blocker).expect("block ledger staging file creation");

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
        error
            .message
            .starts_with("create registration replay ledger failed:"),
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

    assert_eq!(
        std::fs::read(&replay_ledger).expect("read original ledger"),
        b"[]",
        "failed persistence must preserve the original nonce ledger"
    );
    std::fs::remove_dir(&write_blocker).expect("unblock ledger staging file creation");
    let (ack, _) = server
        .handle_authoritative_recovery(AuthoritativeRecoveryCommand::RegisterSession {
            request: request(1),
        })
        .expect("the same grant must remain retryable after rollback");
    assert_eq!(ack.status, AuthoritativeRecoveryStatus::SessionRegistered);

    unsafe {
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_ISSUER_PUBLIC_KEY_ENV);
        oasis7::env_mut::remove_var(crate::viewer::HOSTED_REGISTRATION_REPLAY_LEDGER_PATH_ENV);
    }
    let _ = std::fs::remove_dir_all(replay_dir);
}
