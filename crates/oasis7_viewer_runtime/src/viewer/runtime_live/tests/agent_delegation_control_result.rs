//! Same signed vectors exercise Runtime decisions and the live Viewer adapter.
use super::*;
use crate::runtime::{
    DelegationControlDisposition as Disposition, DelegationControlOperation as Operation,
    DelegationControlStage as Stage, DelegationReplayMatch as Replay,
};
use serde_json::json;

fn install_command(server: &ViewerRuntimeLiveServer, agent: &str) -> AgencyControlOperation {
    AgencyControlOperation::InstallDelegation {
        grant_id: "owner-grant".into(),
        object_id: agent.into(),
        action_kinds: vec!["MoveAgent".into()],
        period_id: "owner-test-period".into(),
        valid_from_tick: server.world.state().time,
        valid_until_tick: server.world.state().time.saturating_add(100),
        limit_units: 4,
    }
}
fn sign(
    server: &ViewerRuntimeLiveServer,
    agent: &str,
    command: AgencyControlOperation,
    nonce: u64,
    public: &str,
    private: &str,
) -> AgencyControlRequest {
    signed_agency_request(
        server,
        agent,
        &format!("delegation-{nonce}"),
        command,
        nonce,
        PLAYER_ID,
        public,
        private,
    )
}

#[test]
fn delegation_runtime_outcomes_and_viewer_responses_share_signed_vectors() {
    let _lock = lock_test_llm_env();
    // Independent expected codes/dispositions keep two consumers from simply
    // agreeing on the same incorrect domain decision.
    for case in [
        "install",
        "exact",
        "terms",
        "order",
        "reservation",
        "scope",
        "apply",
        "revoke",
        "missing",
        "stale",
        "revoked-install",
        "exact-revoke",
    ] {
        for old_nonce in [false, true] {
            let (mut server, agent, public, private) = owner_server(141);
            let mut command = install_command(&server, &agent);
            if matches!(case, "revoked-install" | "exact-revoke") {
                let revoke = sign(
                    &server,
                    &agent,
                    AgencyControlOperation::RevokeDelegation {
                        grant_id: "owner-grant".into(),
                        expected_revision: 1,
                    },
                    1_999,
                    &public,
                    &private,
                );
                let mut candidate = server.world.clone();
                candidate
                    .apply_signed_delegation_control(&revoke, &server.current_owner_authority())
                    .unwrap();
                server.world = candidate;
            }
            let expected = match case {
                "revoked-install" => Err((Stage::Identity, "grant_identity_conflict")),
                "exact-revoke" => {
                    command = AgencyControlOperation::RevokeDelegation {
                        grant_id: "owner-grant".into(),
                        expected_revision: 1,
                    };
                    Ok(Disposition::ExactReplay)
                }
                "install" => {
                    if let AgencyControlOperation::InstallDelegation {
                        grant_id,
                        action_kinds,
                        ..
                    } = &mut command
                    {
                        *grant_id = "transfer-grant".into();
                        *action_kinds = vec!["TransferMaterial".into()];
                    }
                    Ok(Disposition::Applied)
                }
                "exact" => Ok(Disposition::ExactReplay),
                "terms" | "order" => {
                    if let AgencyControlOperation::InstallDelegation {
                        limit_units,
                        action_kinds,
                        ..
                    } = &mut command
                    {
                        if case == "terms" {
                            *limit_units = 5;
                        } else {
                            *action_kinds = vec!["MoveAgent".into(), "TransferMaterial".into()];
                        }
                    }
                    Err((Stage::Identity, "grant_identity_conflict"))
                }
                "reservation" => {
                    if let AgencyControlOperation::InstallDelegation { grant_id, .. } = &mut command
                    {
                        *grant_id = "new-id".into();
                    }
                    Err((Stage::Reservation, "grant_scope_already_reserved"))
                }
                "scope" => {
                    if let AgencyControlOperation::InstallDelegation { action_kinds, .. } =
                        &mut command
                    {
                        action_kinds.clear();
                    }
                    Err((Stage::Scope, "grant_scope_invalid"))
                }
                "apply" => {
                    if let AgencyControlOperation::InstallDelegation {
                        grant_id,
                        action_kinds,
                        limit_units,
                        ..
                    } = &mut command
                    {
                        *grant_id = "invalid-new-id".into();
                        *action_kinds = vec!["TransferMaterial".into()];
                        *limit_units = 0;
                    }
                    Err((Stage::Apply, "grant_install_rejected"))
                }
                "revoke" => {
                    command = AgencyControlOperation::RevokeDelegation {
                        grant_id: "owner-grant".into(),
                        expected_revision: 1,
                    };
                    Ok(Disposition::Applied)
                }
                "missing" => {
                    command = AgencyControlOperation::RevokeDelegation {
                        grant_id: "missing".into(),
                        expected_revision: 1,
                    };
                    Err((Stage::NotFound, "grant_not_found"))
                }
                "stale" => {
                    command = AgencyControlOperation::RevokeDelegation {
                        grant_id: "owner-grant".into(),
                        expected_revision: 9,
                    };
                    Err((Stage::Revision, "grant_revision_stale"))
                }
                _ => unreachable!(),
            };
            let request = sign(&server, &agent, command.clone(), 2_000, &public, &private);
            if old_nonce {
                server
                    .llm_sidecar
                    .consume_player_auth_nonce(PLAYER_ID, 2_000)
                    .unwrap();
            }
            let before = serde_json::to_value(server.world.snapshot()).unwrap();
            let mut candidate = server.world.clone();
            let outcome = candidate
                .apply_signed_delegation_control(&request, &server.current_owner_authority());
            match &expected {
                Ok(disposition) => assert_eq!(
                    outcome.as_ref().unwrap().disposition,
                    *disposition,
                    "{case}"
                ),
                Err((stage, _)) => {
                    assert_eq!(outcome.as_ref().unwrap_err().stage, *stage, "{case}")
                }
            }
            let replay = server
                .world
                .signed_delegation_control_replay(&request, &server.current_owner_authority())
                .unwrap();
            assert_eq!(
                serde_json::to_value(server.world.snapshot()).unwrap(),
                before
            );
            let response = server.handle_agency_control_request(request.clone());
            if old_nonce {
                match replay {
                    Replay::ExactReplay(authorization) => {
                        assert_eq!(response.status, "ok", "{case}: {response:?}");
                        assert_eq!(
                            response.data.unwrap(),
                            json!({"authorization": authorization, "idempotent_replay": true})
                        );
                    }
                    Replay::IdentityConflict => assert_eq!(
                        response.error.unwrap().code,
                        "grant_identity_conflict",
                        "{case}"
                    ),
                    Replay::NoMatch => {
                        assert_eq!(response.error.unwrap().code, "auth_nonce_replay", "{case}")
                    }
                }
                assert_eq!(
                    serde_json::to_value(server.world.snapshot()).unwrap(),
                    before
                );
            } else {
                match expected {
                    Ok(disposition) => {
                        let outcome = outcome.unwrap();
                        assert_eq!(response.status, "ok", "{case}: {response:?}");
                        let data = response.data.unwrap();
                        if disposition == Disposition::ExactReplay {
                            assert_eq!(
                                data,
                                json!({"authorization": outcome.authorization, "idempotent_replay": true})
                            );
                        } else {
                            assert!(
                                data["authorizations"].as_array().unwrap().contains(
                                    &serde_json::to_value(&outcome.authorization).unwrap()
                                )
                            );
                        }
                        assert_eq!(
                            serde_json::to_value(server.world.snapshot()).unwrap(),
                            serde_json::to_value(candidate.snapshot()).unwrap()
                        );
                        assert_eq!(
                            outcome.operation,
                            if matches!(case, "revoke" | "exact-revoke") {
                                Operation::Revoke
                            } else {
                                Operation::Install
                            }
                        );
                        let applied = serde_json::to_value(server.world.snapshot()).unwrap();
                        // Same nonce reads only; a fresh retry consumes its nonce too.
                        let retry = server.handle_agency_control_request(request);
                        assert_eq!(retry.status, "ok", "{retry:?}");
                        assert_eq!(retry.data.as_ref().unwrap()["idempotent_replay"], true);
                        let fresh =
                            sign(&server, &agent, command.clone(), 2_001, &public, &private);
                        let retry = server.handle_agency_control_request(fresh);
                        assert_eq!(retry.status, "ok", "{retry:?}");
                        assert_eq!(retry.data.as_ref().unwrap()["idempotent_replay"], true);
                        assert!(
                            server
                                .llm_sidecar
                                .validate_player_auth_nonce(PLAYER_ID, 2_001)
                                .is_err()
                        );
                        assert_eq!(
                            serde_json::to_value(server.world.snapshot()).unwrap(),
                            applied
                        );
                        server.world = crate::runtime::World::from_snapshot(
                            server.world.snapshot(),
                            server.world.journal().clone(),
                        )
                        .unwrap();
                        let restored = sign(&server, &agent, command, 2_001, &public, &private);
                        assert_eq!(server.handle_agency_control_request(restored).status, "ok");
                    }
                    Err((_, code)) => {
                        assert_eq!(response.error.unwrap().code, code, "{case}");
                        assert_eq!(
                            serde_json::to_value(server.world.snapshot()).unwrap(),
                            before
                        );
                        assert!(
                            server
                                .llm_sidecar
                                .validate_player_auth_nonce(PLAYER_ID, 2_000)
                                .is_ok()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn delegation_read_and_persist_failures_preserve_nonce_and_published_world() {
    let _lock = lock_test_llm_env();
    let (mut server, agent, public, private) = owner_server(142);
    let command = install_command(&server, &agent);
    let request = sign(&server, &agent, command, 2_000, &public, &private);
    let mut snapshot = server.world.snapshot();
    snapshot.cognition["agent_delegation"] = json!({"grants": true});
    server.world =
        crate::runtime::World::from_snapshot(snapshot, server.world.journal().clone()).unwrap();
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    let error = server
        .world
        .signed_delegation_control_replay(&request, &server.current_owner_authority())
        .unwrap_err();
    assert_eq!(error.stage, Stage::Read);
    let response = server.handle_agency_control_request(request.clone());
    assert_eq!(response.error.unwrap().code, "runtime_agency_read_failed");
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 2_000)
            .is_ok()
    );
    server
        .llm_sidecar
        .consume_player_auth_nonce(PLAYER_ID, 2_000)
        .unwrap();
    assert_eq!(
        server
            .handle_agency_control_request(request)
            .error
            .unwrap()
            .code,
        "auth_nonce_replay"
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );

    let (mut server, agent, public, private) = owner_server(143);
    let root = std::env::temp_dir().join(format!(
        "oasis7-delegation-result-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    server.world.save_to_dir(&root).unwrap();
    // Make the first compatibility persistence mirror unwritable without
    // relying on Unix mode bits (the same fault works on Windows/root).
    std::fs::remove_file(root.join("journal.json")).unwrap();
    std::fs::create_dir(root.join("journal.json")).unwrap();
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    let command = AgencyControlOperation::RevokeDelegation {
        grant_id: "owner-grant".into(),
        expected_revision: 1,
    };
    let request = sign(&server, &agent, command, 2_000, &public, &private);
    let mut candidate = server.world.clone();
    let error = candidate
        .apply_signed_delegation_control(&request, &server.current_owner_authority())
        .unwrap_err();
    assert_eq!(error.stage, Stage::Apply);
    assert!(matches!(error.source, crate::runtime::WorldError::Io(_)));
    assert_eq!(serde_json::to_value(candidate.snapshot()).unwrap(), before);
    let response = server.handle_agency_control_request(request);
    assert_eq!(response.error.unwrap().code, "grant_revoke_rejected");
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 2_000)
            .is_ok()
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
    // Exact replay bypasses persistence and must still succeed with broken storage.
    let exact = sign(
        &server,
        &agent,
        install_command(&server, &agent),
        2_000,
        &public,
        &private,
    );
    assert_eq!(
        server.handle_agency_control_request(exact.clone()).status,
        "ok"
    );
    assert_eq!(server.handle_agency_control_request(exact).status, "ok");
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
}

#[test]
fn delegation_local_profile_and_old_nonce_error_precedence_are_preserved() {
    let _lock = lock_test_llm_env();
    let (mut server, agent, public, private) = owner_server(144);
    for (kind, nonce) in [("WorldRule", 2_000), ("BuildFactory", 2_001)] {
        let mut command = install_command(&server, &agent);
        if let AgencyControlOperation::InstallDelegation { action_kinds, .. } = &mut command {
            *action_kinds = vec![kind.into()];
        }
        let request = sign(&server, &agent, command, nonce, &public, &private);
        assert_eq!(
            server
                .handle_agency_control_request(request.clone())
                .error
                .unwrap()
                .code,
            "grant_scope_invalid"
        );
        assert!(
            server
                .llm_sidecar
                .validate_player_auth_nonce(PLAYER_ID, nonce)
                .is_ok()
        );
        // A consumed nonce with same ID/different terms is an identity conflict,
        // even if those terms would fail the fresh-only Viewer profile.
        server
            .llm_sidecar
            .consume_player_auth_nonce(PLAYER_ID, nonce)
            .unwrap();
        assert_eq!(
            server
                .handle_agency_control_request(request)
                .error
                .unwrap()
                .code,
            "grant_identity_conflict"
        );
    }
    let request = sign(
        &server,
        &agent,
        install_command(&server, &agent),
        2_002,
        &public,
        &private,
    );
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    for field in ["world", "epoch", "signature", "identity"] {
        let mut bad = request.clone();
        let code = match field {
            "world" => {
                bad.world_id.push('x');
                "stale_world_position"
            }
            "epoch" => {
                bad.reorg_epoch += 1;
                "stale_world_position"
            }
            "signature" => {
                bad.request_id.push('x');
                "auth_proof_invalid"
            }
            "identity" => {
                bad.request_id.clear();
                "invalid_request"
            }
            _ => unreachable!(),
        };
        assert_eq!(
            server
                .handle_agency_control_request(bad)
                .error
                .unwrap()
                .code,
            code
        );
        assert!(
            server
                .llm_sidecar
                .validate_player_auth_nonce(PLAYER_ID, 2_002)
                .is_ok()
        );
    }
    server.authoritative_recovery_write_fence = Some("unresolved-delegation".into());
    let mut stale = request;
    stale.reorg_epoch += 1;
    assert_eq!(
        server
            .handle_agency_control_request(stale)
            .error
            .unwrap()
            .code,
        "runtime_recovery_fenced"
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
}

#[test]
fn delegation_old_nonce_can_only_read_exact_runtime_profile_result() {
    let _lock = lock_test_llm_env();
    let (mut server, agent, public, private) = owner_server(145);
    let mut command = install_command(&server, &agent);
    if let AgencyControlOperation::InstallDelegation {
        grant_id,
        action_kinds,
        ..
    } = &mut command
    {
        *grant_id = "runtime-world-rule".into();
        *action_kinds = vec!["WorldRule".into()];
    }
    let request = sign(&server, &agent, command.clone(), 2_000, &public, &private);
    let mut candidate = server.world.clone();
    let outcome = candidate
        .apply_signed_delegation_control(&request, &server.current_owner_authority())
        .unwrap();
    server.world = candidate;
    server
        .llm_sidecar
        .consume_player_auth_nonce(PLAYER_ID, 2_000)
        .unwrap();
    let before = serde_json::to_value(server.world.snapshot()).unwrap();
    let response = server.handle_agency_control_request(request);
    assert_eq!(response.status, "ok", "{response:?}");
    assert_eq!(
        response.data.unwrap(),
        json!({"authorization": outcome.authorization, "idempotent_replay": true})
    );
    let fresh = sign(&server, &agent, command, 2_001, &public, &private);
    assert_eq!(
        server
            .handle_agency_control_request(fresh)
            .error
            .unwrap()
            .code,
        "grant_scope_invalid"
    );
    assert!(
        server
            .llm_sidecar
            .validate_player_auth_nonce(PLAYER_ID, 2_001)
            .is_ok()
    );
    assert_eq!(
        serde_json::to_value(server.world.snapshot()).unwrap(),
        before
    );
}
