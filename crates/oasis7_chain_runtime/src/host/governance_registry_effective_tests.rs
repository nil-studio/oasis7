use super::*;

#[test]
fn world_effective_finality_registry_overrides_node_pos_config_after_validator_activation() {
    let temp_dir = temp_dir("effective-finality-override");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_execution_policy(GovernanceExecutionPolicy {
            epoch_length_ticks: 10,
            ..GovernanceExecutionPolicy::default()
        })
        .expect("set governance policy");
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "validator-a".to_string(),
                    "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                ),
                (
                    "validator-b".to_string(),
                    "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                ("validator-a".to_string(), 100),
                ("validator-b".to_string(), 100),
            ]),
        })
        .expect("set finality registry");
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                "ecosystem_pool".to_string(),
                "liveops".to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from(["liveops".to_string()]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "liveops".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 1,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
    // This audit/projection fixture starts with established historical membership.
    // It does not submit a new admission or bypass the command authorization path.
    let mut snapshot = world.snapshot();
    snapshot.state.governance_validator_admissions.insert(
        "candidate-c".to_string(),
        oasis7_runtime::GovernanceValidatorAdmissionRecord {
            candidate_id: "candidate-c".to_string(),
            node_id: "validator-c".to_string(),
            finality_signer_public_key:
                "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
            stake: 35,
            operator_owner: "ops.team".to_string(),
            public_manifest_hash: "manifest-c".to_string(),
            requested_at_epoch: 0,
            last_transition_tick: 0,
            approved_at_epoch: Some(0),
            activation_epoch: Some(0),
            status: oasis7_runtime::GovernanceValidatorAdmissionStatus::Active,
            revoked_at_epoch: None,
            revocation_reason: None,
        },
    );
    snapshot.state.node_identity_bindings.insert(
        "validator-c".to_string(),
        "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
    );
    snapshot.state.node_main_token_account_bindings.insert(
        "validator-c".to_string(),
        oasis7_runtime::main_token_account_id_from_node_public_key(
            "3333333333333333333333333333333333333333333333333333333333333333",
        ),
    );
    let world = oasis7_runtime::World::from_snapshot(snapshot, world.journal().clone())
        .expect("restore established membership fixture");
    world.save_to_dir(&temp_dir).expect("save execution world");

    let config = NodeConfig::new("node-a", "world-a", NodeRole::Sequencer).expect("node config");
    let config = apply_world_governance_registry_overrides(config, &temp_dir)
        .expect("apply registry overrides");

    let validator_ids = config
        .pos_config
        .validators
        .iter()
        .map(|validator| validator.validator_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        validator_ids,
        vec!["validator-a", "validator-b", "validator-c"]
    );
    assert_eq!(
        config
            .pos_config
            .validator_signer_public_keys
            .get("validator-c")
            .map(String::as_str),
        Some("3333333333333333333333333333333333333333333333333333333333333333")
    );
    assert_eq!(
        config
            .pos_config
            .validators
            .iter()
            .find(|validator| validator.validator_id == "validator-c")
            .map(|validator| validator.stake),
        Some(35)
    );
}
