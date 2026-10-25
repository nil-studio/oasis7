use super::*;

fn check(kind: &str, auth: Option<LocalConsensusActionAuthEnvelope>) -> Result<(), NodeError> {
    let envelope = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth,
        body: LocalConsensusActionPayloadBody::RuntimeAction {
            action: serde_json::json!({ "type": kind, "data": {} }),
        },
    };
    submit(serde_cbor::to_vec(&envelope).unwrap())
}

fn submit(payload: Vec<u8>) -> Result<(), NodeError> {
    let config =
        NodeConfig::new("node-auth", "world-auth", crate::NodeRole::Observer).expect("node config");
    let runtime = NodeRuntime::new(config);
    let result = runtime.submit_consensus_action_payload(1, payload);
    let pending = runtime.pending_consensus_actions.lock().unwrap();
    assert_eq!(pending.len(), usize::from(result.is_ok()));
    result
}

#[test]
fn admission_ingress_requires_governance_proof_for_every_operation() {
    for kind in [
        "SubmitGovernanceValidatorAdmission",
        "ApproveGovernanceValidatorAdmission",
        "ActivateGovernanceValidatorAdmission",
        "RevokeGovernanceValidatorAdmission",
    ] {
        assert!(check(kind, None).is_err(), "{kind}");
        let raw = serde_cbor::to_vec(&serde_json::json!({
            "type": kind,
            "data": {
                "controller_account_id": "forged-controller",
                "candidate_id": "candidate",
                "node_id": "forged-node",
                "finality_signer_public_key": "forged-key",
                "operator_owner": "forged-owner",
                "public_manifest_hash": "forged-manifest",
                "requested_at_epoch": 1,
                "reason": "forged revoke"
            }
        }))
        .unwrap();
        let error =
            submit(raw).expect_err("legacy raw admission must fail before queue acceptance");
        assert!(
            error.to_string().contains("missing_governance_auth"),
            "{error}"
        );
        let wrong_proof = LocalMainTokenActionAuthProof {
            scheme: LocalMainTokenActionAuthScheme::Ed25519,
            account_id: "controller".into(),
            public_key: None,
            signature: None,
            threshold: None,
            participant_signatures: Vec::new(),
        };
        assert!(
            check(
                kind,
                Some(LocalConsensusActionAuthEnvelope::MainTokenAction(
                    wrong_proof
                ))
            )
            .is_err()
        );
        for proof in [
            serde_json::json!({"schema_version": 2, "signatures": {"key": "signature"}}),
            serde_json::json!({"schema_version": 1, "signatures": {}}),
            serde_json::json!({"schema_version": 1, "signatures": {"key": ""}}),
        ] {
            assert!(
                check(
                    kind,
                    Some(LocalConsensusActionAuthEnvelope::GovernanceAction(proof))
                )
                .is_err()
            );
        }
        // A structurally plausible forgery cannot enter consensus without authoritative preflight.
        let proof = serde_json::json!({"schema_version": 1, "signatures": {"key": "signature"}});
        assert!(
            check(
                kind,
                Some(LocalConsensusActionAuthEnvelope::GovernanceAction(proof))
            )
            .is_err()
        );
    }
}

#[test]
fn governance_proof_cannot_authorize_an_unrelated_action() {
    let proof = serde_json::json!({"schema_version": 1, "signatures": {"key": "signature"}});
    assert!(
        check(
            "TransferMainToken",
            Some(LocalConsensusActionAuthEnvelope::GovernanceAction(proof))
        )
        .is_err()
    );
}

#[test]
fn unrelated_registration_still_reaches_the_consensus_queue() {
    let action = serde_json::json!({
        "type": "RegisterAgent",
        "data": {"agent_id": "governance-test-agent", "pos": {"x": 0.0, "y": 0.0, "z": 0.0}}
    });
    submit(serde_cbor::to_vec(&action).unwrap()).expect("legacy unrelated registration");
    let envelope = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: None,
        body: LocalConsensusActionPayloadBody::RuntimeAction { action },
    };
    submit(serde_cbor::to_vec(&envelope).unwrap()).expect("v1 unrelated registration");
}

#[test]
fn governance_submission_invokes_authoritative_preflight_before_queueing() {
    struct RejectingHook(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    impl crate::NodeExecutionHook for RejectingHook {
        fn validate_governance_action_payload(&self, payload: &[u8]) -> Result<(), String> {
            assert!(!payload.is_empty());
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Err("recorded authoritative denial".into())
        }
        fn on_commit(
            &mut self,
            _: crate::NodeExecutionCommitContext,
        ) -> Result<crate::NodeExecutionCommitResult, String> {
            panic!("rejected ingress must not commit")
        }
    }
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let runtime = NodeRuntime::new(
        NodeConfig::new(
            "node-preflight",
            "world-preflight",
            crate::NodeRole::Observer,
        )
        .unwrap(),
    )
    .with_execution_hook(RejectingHook(calls.clone()));
    let envelope = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: Some(LocalConsensusActionAuthEnvelope::GovernanceAction(
            serde_json::json!({"schema_version": 1, "signatures": {"key": "forgery"}}),
        )),
        body: LocalConsensusActionPayloadBody::RuntimeAction {
            action: serde_json::json!({"type": "ApproveGovernanceValidatorAdmission", "data": {}}),
        },
    };
    let error = runtime
        .submit_consensus_action_payload(1, serde_cbor::to_vec(&envelope).unwrap())
        .expect_err("preflight denied");
    assert!(error.to_string().contains("recorded authoritative denial"));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(runtime.pending_consensus_actions.lock().unwrap().is_empty());
}

#[test]
fn non_governance_submission_does_not_depend_on_a_healthy_governance_hook() {
    struct Hook;
    impl crate::NodeExecutionHook for Hook {
        fn on_commit(
            &mut self,
            _: crate::NodeExecutionCommitContext,
        ) -> Result<crate::NodeExecutionCommitResult, String> {
            panic!("ingress-only test hook must not commit")
        }
    }

    let runtime = NodeRuntime::new(
        NodeConfig::new(
            "node-poisoned-preflight",
            "world-auth",
            crate::NodeRole::Observer,
        )
        .expect("node config"),
    )
    .with_execution_hook(Hook);
    let hook = runtime
        .execution_hook
        .as_ref()
        .expect("installed execution hook")
        .clone();
    let poisoned = hook.clone();
    std::thread::spawn(move || {
        let _guard = poisoned.lock().expect("lock hook before poisoning");
        panic!("poison governance preflight hook");
    })
    .join()
    .expect_err("test thread must poison the hook mutex");

    let ordinary = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: None,
        body: LocalConsensusActionPayloadBody::RuntimeAction {
            action: serde_json::json!({
                "type": "RegisterAgent",
                "data": {"agent_id": "ordinary", "pos": {"x": 0.0, "y": 0.0, "z": 0.0}}
            }),
        },
    };
    runtime
        .submit_consensus_action_payload(1, serde_cbor::to_vec(&ordinary).unwrap())
        .expect("non-governance ingress must not touch the governance hook");

    let governance = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: Some(LocalConsensusActionAuthEnvelope::GovernanceAction(
            serde_json::json!({"schema_version": 1, "signatures": {"key": "signature"}}),
        )),
        body: LocalConsensusActionPayloadBody::RuntimeAction {
            action: serde_json::json!({
                "type": "SubmitGovernanceValidatorAdmission",
                "data": {}
            }),
        },
    };
    let error = runtime
        .submit_consensus_action_payload(2, serde_cbor::to_vec(&governance).unwrap())
        .expect_err("governance ingress must still require a healthy authoritative hook");
    assert!(
        error
            .to_string()
            .contains("governance authorization preflight hook lock poisoned"),
        "{error}"
    );
    assert_eq!(runtime.pending_consensus_actions.lock().unwrap().len(), 1);
}

#[test]
fn non_governance_submission_finishes_while_the_execution_hook_is_held() {
    struct Hook;
    impl crate::NodeExecutionHook for Hook {
        fn validate_governance_action_payload(&self, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
        fn on_commit(
            &mut self,
            _: crate::NodeExecutionCommitContext,
        ) -> Result<crate::NodeExecutionCommitResult, String> {
            panic!("ingress-only test hook must not commit")
        }
    }

    let runtime = NodeRuntime::new(
        NodeConfig::new(
            "node-held-preflight",
            "world-auth",
            crate::NodeRole::Observer,
        )
        .expect("node config"),
    )
    .with_execution_hook(Hook);
    let hook = runtime
        .execution_hook
        .as_ref()
        .expect("installed execution hook")
        .clone();
    let guard = hook.lock().expect("hold execution hook");
    let ordinary = LocalConsensusActionPayloadEnvelope {
        version: 1,
        gameplay_submission_origin: None,
        auth: None,
        body: LocalConsensusActionPayloadBody::RuntimeAction {
            action: serde_json::json!({
                "type": "RegisterAgent",
                "data": {"agent_id": "ordinary-held", "pos": {"x": 0.0, "y": 0.0, "z": 0.0}}
            }),
        },
    };
    let payload = serde_cbor::to_vec(&ordinary).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (result_tx, result_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).expect("signal before submission");
        let result = runtime.submit_consensus_action_payload(1, payload);
        result_tx.send(result).expect("return submission result");
    });
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("submission thread started");
    let early_result = result_rx.recv_timeout(std::time::Duration::from_secs(1));
    let completed_while_locked = early_result.is_ok();

    // Always release the hook before waiting for or joining the worker. On the
    // buggy path the worker is blocked on this mutex; cleanup must not leak it.
    drop(guard);
    let result = match early_result {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => result_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("submission finishes after releasing the hook"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            panic!("submission worker exited without returning a result")
        }
    };
    worker.join().expect("submission worker joins");

    assert!(
        completed_while_locked,
        "ordinary ingress must not wait for the governance execution hook"
    );
    result.expect("ordinary ingress should succeed");
}
