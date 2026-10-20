use super::*;
#[path = "membership_cli_formal_watch_tests.rs"]
mod formal_watch;
#[path = "membership_cli_watch_tests.rs"]
mod watch_tcp;

#[test]
fn parser_requires_explicit_state_subject_and_finite_polling() {
    assert!(parse(["--help"].into_iter()).unwrap().is_none());
    assert!(parse(["status", "--help"].into_iter()).unwrap().is_none());
    assert!(parse(["status"].into_iter()).is_err());
    assert!(parse(["wait", "--node-id", "node", "--state", "bft-applied"].into_iter()).is_err());
    assert!(parse(["watch", "--interval-ms", "0"].into_iter()).is_err());
    assert!(parse(["list", "--timeout-ms", "--cursor"].into_iter()).is_err());
    let ordinary = parse(["list"].into_iter()).unwrap().unwrap();
    assert!(!ordinary.engineering);
    let diagnostic = parse(
        [
            "wait",
            "--node-id",
            "node",
            "--allow-controlled-prerequisite",
        ]
        .into_iter(),
    )
    .unwrap()
    .unwrap();
    assert!(diagnostic.engineering);
    assert_eq!(diagnostic.state, "effective");
}

#[test]
fn validator_help_dispatches_before_runtime_configuration() {
    assert!(
        super::super::identity_receipt::dispatch(["validator", "--help"].into_iter())
            .unwrap()
            .is_ok()
    );
    assert!(
        super::super::identity_receipt::dispatch(["validator", "status"].into_iter())
            .unwrap()
            .is_err()
    );
}

#[test]
fn wait_uses_resolved_membership_and_observation_never_invents_effective_height() {
    use oasis7::world_service::{
        ExecutionBinding, PublicMembershipCandidate, PublicMembershipRegistry, WorldIdentity,
    };
    let commit = CommitRef {
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        binding: ExecutionBinding {
            provider_world_id: "world".into(),
            branch_id: "main".into(),
            finality_ref: "finality".into(),
            reorg_generation: 0,
            governing_manifest_ref: "manifest".into(),
            authority_generation: 0,
            permission_generation: 0,
        },
        position: 7,
        execution_block_hash: "block".into(),
        state_root_ref: "root".into(),
    };
    let mut response = ReadMembershipResponse {
        contract_version: 1,
        execution_evidence_scope: Some("replicated_durability_prerequisite".into()),
        commit: commit.clone(),
        state_root_hash: "state-root".into(),
        governance_epoch: 2,
        history_profile: "replicated_durability_prerequisite".into(),
        candidates: BTreeMap::from([(
            "candidate".into(),
            PublicMembershipCandidate {
                candidate_id: "candidate".into(),
                node_id: "node".into(),
                finality_signer_public_key: "key".into(),
                stake: 25,
                public_manifest_hash: "manifest".into(),
                requested_at_epoch: 0,
                last_transition_tick: 1,
                approved_at_epoch: Some(1),
                activation_epoch: Some(2),
                revoked_at_epoch: None,
                status: MembershipCandidateStatus::ProbationReady,
            },
        )]),
        selected_candidates: BTreeMap::from([("node".into(), "candidate".into())]),
        effective_registry: None,
        effective_registry_digest: "registry-digest".into(),
        supports_safe_runtime_update: false,
        applied_consensus_membership: None,
    };
    assert!(!reached(&response, "node", "effective"));
    assert!(
        output(&response, Some("node"), &commit)["node_status"]["governance_effective"].is_null()
    );
    response.effective_registry = Some(PublicMembershipRegistry {
        slot_id: "slot".into(),
        threshold: 1,
        threshold_bps: 0,
        signer_bindings: BTreeMap::from([("node".into(), "key".into())]),
        validator_stakes: BTreeMap::from([("node".into(), 25)]),
    });
    assert!(reached(&response, "node", "effective"));
    let value = output(&response, Some("node"), &commit);
    assert!(value["actual_first_effective_commit"].is_null());
    assert!(value["applied_consensus_membership"].is_null());
    assert_eq!(value["node_status"]["protocol_activation_epoch"], 2);
    let digest = state_digest(&response).unwrap();
    response.commit.position += 1;
    assert_eq!(digest, state_digest(&response).unwrap()); // no commit-only watch noise
    response.governance_epoch += 1;
    assert_eq!(digest, state_digest(&response).unwrap()); // no unrelated epoch-only noise
    let condition = observed_condition_digest(&response, Some("node")).unwrap();
    response
        .effective_registry
        .as_mut()
        .unwrap()
        .signer_bindings
        .insert("node".into(), "rotated-key".into());
    assert_ne!(
        condition,
        observed_condition_digest(&response, Some("node")).unwrap()
    );
    assert!(reached(&response, "node", "effective")); // identity remains eligible, key changed
}
