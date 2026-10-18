use super::{
    apply_world_governance_registry_overrides, ensure_world_governance_validator_registry,
};
use oasis7::network_tier_manifest::{
    LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1, NetworkTierClaimsPolicy,
    NetworkTierEndpointPolicy, NetworkTierManifest, NetworkTierPromotionPolicy,
    NetworkTierRuntimeRefs, NetworkTierTokenPolicy, NetworkTierValidatorPolicy,
};
use oasis7_node::{NodeConfig, NodeRole};
use oasis7_runtime::{
    Action, GovernanceExecutionPolicy, GovernanceFinalitySignerRegistry,
    GovernanceMainTokenControllerRegistry, GovernanceThresholdSignerPolicy,
    RollbackAuthorityRecord, RollbackAuthorityRegistry, RollbackAuthorityRole,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("duration")
        .as_nanos();
    std::env::temp_dir().join(format!("oasis7-chain-governance-{prefix}-{unique}"))
}

fn public_testnet_loaded_manifest() -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: "test-network-tier.json".to_string(),
        manifest: NetworkTierManifest {
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.to_string(),
            tier: "public_testnet".to_string(),
            status: "live".to_string(),
            network_id: "oasis7-public-testnet".to_string(),
            chain_id: "oasis7-public-testnet".to_string(),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".to_string(),
                genesis_ref: "genesis.json".to_string(),
                bootstrap_peer_ref: "bootstrap.txt".to_string(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "rpc.json".to_string(),
                explorer_ref: "explorer.json".to_string(),
                faucet_ref: Some("faucet.json".to_string()),
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "governance_registry".to_string(),
                validator_admission: "allowlist_or_governed_candidate".to_string(),
                target_validator_count: 3,
                allow_observer_nodes: true,
            },
            token_policy: NetworkTierTokenPolicy {
                symbol: "O7T".to_string(),
                faucet_mode: "guarded_testnet_faucet".to_string(),
                reset_policy: Some("resettable".to_string()),
                value_semantics: "testnet".to_string(),
            },
            claims_policy: NetworkTierClaimsPolicy {
                allowed_claims: vec!["public_testnet".to_string()],
                denied_claims: vec![
                    "mainnet_live".to_string(),
                    "production_oc_settlement".to_string(),
                ],
            },
            promotion_policy: NetworkTierPromotionPolicy {
                promote_from: vec!["local_devnet".to_string()],
                required_gates: vec!["public_testnet_rehearsal_pass".to_string()],
            },
            evidence_refs: Vec::new(),
        },
        bootstrap_peers: Vec::new(),
    }
}

fn public_testnet_loaded_manifest_with_paths(
    source_path: &Path,
    genesis_ref: &str,
    bootstrap_peer_ref: &str,
) -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: source_path.display().to_string(),
        manifest: NetworkTierManifest {
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.to_string(),
            tier: "public_testnet".to_string(),
            status: "rehearsal".to_string(),
            network_id: "oasis7-public-testnet-governed".to_string(),
            chain_id: "oasis7-public-testnet-governed".to_string(),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".to_string(),
                genesis_ref: genesis_ref.to_string(),
                bootstrap_peer_ref: bootstrap_peer_ref.to_string(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "rpc.json".to_string(),
                explorer_ref: "explorer.json".to_string(),
                faucet_ref: Some("faucet.json".to_string()),
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "governance_registry".to_string(),
                validator_admission: "allowlist_or_governed_candidate".to_string(),
                target_validator_count: 2,
                allow_observer_nodes: true,
            },
            token_policy: NetworkTierTokenPolicy {
                symbol: "OC".to_string(),
                faucet_mode: "guarded_testnet_faucet".to_string(),
                reset_policy: Some("resettable".to_string()),
                value_semantics: "testnet".to_string(),
            },
            claims_policy: NetworkTierClaimsPolicy {
                allowed_claims: vec!["public_testnet".to_string()],
                denied_claims: vec!["mainnet_live".to_string()],
            },
            promotion_policy: NetworkTierPromotionPolicy {
                promote_from: vec!["local_devnet".to_string()],
                required_gates: vec!["public_testnet_rehearsal_pass".to_string()],
            },
            evidence_refs: Vec::new(),
        },
        bootstrap_peers: Vec::new(),
    }
}

#[test]
fn world_registry_overrides_node_controller_binding() {
    let temp_dir = temp_dir("registry-override");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_main_token_controller_registry(GovernanceMainTokenControllerRegistry {
            genesis_controller_account_id: "msig.genesis.v1".to_string(),
            treasury_bucket_controller_slots: BTreeMap::from([(
                "staking_reward_pool".to_string(),
                "msig.staking_governance.v1".to_string(),
            )]),
            restricted_starter_claim_admin_account_ids: BTreeSet::from([
                "msig.staking_governance.v1".to_string(),
            ]),
            controller_signer_policies: BTreeMap::from([
                (
                    "msig.genesis.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "6249e5a58278dbc4e629a16b5d33f6b84c39e3ceeb10e963bb9ef64ea4daac30"
                                .to_string(),
                            "7014e88a6336ec91fc7e6ffb044b50232e4411ec403f90123fa8a202a3420a04"
                                .to_string(),
                        ]),
                    },
                ),
                (
                    "msig.staking_governance.v1".to_string(),
                    GovernanceThresholdSignerPolicy {
                        threshold: 2,
                        allowed_public_keys: BTreeSet::from([
                            "13c160fc0f516b9a5663aa00c2a5446be6467f68ce341fdd79cdb64224dffd20"
                                .to_string(),
                            "10fa4d90abf753ec1aa54aee3ea53bab25f43e7078897e1fb6a3777af2255bcb"
                                .to_string(),
                        ]),
                    },
                ),
            ]),
        })
        .expect("set controller registry");
    world.save_to_dir(&temp_dir).expect("save execution world");

    let config = NodeConfig::new("node-a", "world-a", NodeRole::Sequencer).expect("node config");
    let config = apply_world_governance_registry_overrides(config, &temp_dir)
        .expect("apply registry overrides");

    assert_eq!(
        config
            .main_token_controller_binding
            .genesis_controller_account_id,
        "msig.genesis.v1"
    );
    assert_eq!(
        config
            .main_token_controller_binding
            .treasury_bucket_controller_slots
            .get("staking_reward_pool")
            .map(String::as_str),
        Some("msig.staking_governance.v1")
    );
    assert_eq!(
        config
            .main_token_controller_binding
            .controller_signer_policies
            .get("msig.genesis.v1")
            .map(|policy| policy.threshold),
        Some(2)
    );
}

#[test]
fn world_finality_registry_overrides_node_pos_config() {
    let temp_dir = temp_dir("finality-override");
    let mut world = oasis7::runtime_host::world::new();
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
                (
                    "validator-c".to_string(),
                    "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                ("validator-a".to_string(), 70),
                ("validator-b".to_string(), 20),
                ("validator-c".to_string(), 10),
            ]),
        })
        .expect("set finality registry");
    world.save_to_dir(&temp_dir).expect("save execution world");

    let mut config =
        NodeConfig::new("node-a", "world-a", NodeRole::Sequencer).expect("node config");
    config.pos_config.slot_duration_ms = 12_000;
    config.pos_config.ticks_per_slot = 10;
    config.pos_config.proposal_tick_phase = 9;
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
    let validator_stakes = config
        .pos_config
        .validators
        .iter()
        .map(|validator| (validator.validator_id.as_str(), validator.stake))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(validator_stakes.get("validator-a"), Some(&70));
    assert_eq!(validator_stakes.get("validator-b"), Some(&20));
    assert_eq!(validator_stakes.get("validator-c"), Some(&10));
    assert_eq!(
        config
            .pos_config
            .validator_signer_public_keys
            .get("validator-b")
            .map(String::as_str),
        Some("2222222222222222222222222222222222222222222222222222222222222222")
    );
    assert_eq!(
        config
            .pos_config
            .validator_player_ids
            .get("validator-c")
            .map(String::as_str),
        Some("validator-c")
    );
    assert_eq!(config.pos_config.slot_duration_ms, 12_000);
    assert_eq!(config.pos_config.ticks_per_slot, 10);
    assert_eq!(config.pos_config.proposal_tick_phase, 9);
}

#[test]
fn genesis_validator_registry_initializes_empty_world_and_uses_stake() {
    let temp_dir = temp_dir("genesis-finality-registry");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let manifest_path = temp_dir.join("genesis-validator-registry.json");
    std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "slot_id": "governance.finality.v1",
                "threshold": 2,
                "validators": [
                    {
                        "node_id": "validator-a",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "1111111111111111111111111111111111111111111111111111111111111111",
                        "stake": 70
                    },
                    {
                        "node_id": "validator-b",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "2222222222222222222222222222222222222222222222222222222222222222",
                        "stake": 20
                    },
                    {
                        "node_id": "validator-c",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "3333333333333333333333333333333333333333333333333333333333333333",
                        "stake": 10
                    }
                ]
            }))
            .expect("encode manifest"),
        )
        .expect("write manifest");

    ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        Some(manifest_path.as_path()),
        Some(&public_testnet_loaded_manifest()),
    )
    .expect("ensure genesis registry");
    let config = apply_world_governance_registry_overrides(
        NodeConfig::new("node-a", "world-a", NodeRole::Sequencer).expect("node config"),
        temp_dir.as_path(),
    )
    .expect("apply registry overrides");

    let validator_stakes = config
        .pos_config
        .validators
        .iter()
        .map(|validator| (validator.validator_id.as_str(), validator.stake))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(validator_stakes.get("validator-a"), Some(&70));
    assert_eq!(validator_stakes.get("validator-b"), Some(&20));
    assert_eq!(validator_stakes.get("validator-c"), Some(&10));
    assert_eq!(
        config
            .pos_config
            .validator_signer_public_keys
            .get("validator-c")
            .map(String::as_str),
        Some("3333333333333333333333333333333333333333333333333333333333333333")
    );
}

#[test]
fn network_tier_genesis_bootstrap_initializes_full_governance_world_for_empty_observer() {
    let temp_dir = temp_dir("network-tier-governance-bootstrap");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let execution_world_dir = temp_dir.join("world");
    let manifest_path = temp_dir.join("network-tier.json");
    let bootstrap_path = temp_dir.join("bootstrap.txt");
    let genesis_path = temp_dir.join("genesis.json");
    let governance_manifest_path = temp_dir.join("governance-public.json");
    let liveops_manifest_path = temp_dir.join("liveops-public.json");
    let validator_registry_path = temp_dir.join("genesis-validator-registry.json");
    std::fs::write(bootstrap_path.as_path(), b"").expect("write bootstrap peers");
    std::fs::write(
            governance_manifest_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "oasis7.signer_truth_bundle.v1",
                "entries": [
                    {
                        "slot_id": "msig.genesis.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "1111111111111111111111111111111111111111111111111111111111111111"
                    },
                    {
                        "slot_id": "msig.genesis.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "2222222222222222222222222222222222222222222222222222222222222222"
                    },
                    {
                        "slot_id": "msig.staking_governance.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "3333333333333333333333333333333333333333333333333333333333333333"
                    },
                    {
                        "slot_id": "msig.staking_governance.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "4444444444444444444444444444444444444444444444444444444444444444"
                    },
                    {
                        "slot_id": "msig.ecosystem_governance.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "6666666666666666666666666666666666666666666666666666666666666666"
                    },
                    {
                        "slot_id": "msig.ecosystem_governance.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "7777777777777777777777777777777777777777777777777777777777777777"
                    },
                    {
                        "slot_id": "msig.security_council.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "8888888888888888888888888888888888888888888888888888888888888888"
                    },
                    {
                        "slot_id": "msig.security_council.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "9999999999999999999999999999999999999999999999999999999999999999"
                    },
                    {
                        "slot_id": "ops.rollback.on_call.v1",
                        "signer_id": "rollback-on-call-01",
                        "scheme": "ed25519",
                        "threshold": 1,
                        "public_key_hex": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    },
                    {
                        "slot_id": "governance.rollback.v1",
                        "signer_id": "rollback-governance-01",
                        "scheme": "ed25519",
                        "threshold": 1,
                        "public_key_hex": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                    }
                ]
            }))
            .expect("encode governance manifest"),
        )
        .expect("write governance manifest");
    std::fs::write(
            liveops_manifest_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "oasis7.signer_truth_bundle.v1",
                "entries": [
                    {
                        "slot_id": "liveops",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "threshold": 1,
                        "public_key_hex": "5555555555555555555555555555555555555555555555555555555555555555"
                    }
                ]
            }))
            .expect("encode liveops manifest"),
        )
        .expect("write liveops manifest");
    std::fs::write(
            validator_registry_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "slot_id": "governance.finality.v1",
                "threshold": 2,
                "validators": [
                    {
                        "node_id": "triad-testnet-sequencer",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "stake": 60
                    },
                    {
                        "node_id": "triad-testnet-storage",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "stake": 40
                    }
                ]
            }))
            .expect("encode validator registry"),
        )
        .expect("write validator registry");
    std::fs::write(
            genesis_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "oasis7.genesis.example.v1",
                "network_tier": "public_testnet",
                "chain_id": "oasis7-public-testnet-governed",
                "world_id": "oasis7-public-testnet-governed",
                "governance_bootstrap_refs": {
                    "governance_public_manifest_ref": governance_manifest_path.file_name().unwrap().to_string_lossy(),
                    "liveops_public_manifest_ref": liveops_manifest_path.file_name().unwrap().to_string_lossy(),
                    "genesis_validator_registry_ref": validator_registry_path.file_name().unwrap().to_string_lossy()
                }
            }))
            .expect("encode genesis"),
        )
        .expect("write genesis");
    let loaded_manifest = public_testnet_loaded_manifest_with_paths(
        manifest_path.as_path(),
        genesis_path.file_name().unwrap().to_str().unwrap(),
        bootstrap_path.file_name().unwrap().to_str().unwrap(),
    );

    ensure_world_governance_validator_registry(
        execution_world_dir.as_path(),
        None,
        Some(&loaded_manifest),
    )
    .expect("bootstrap governance world");

    let world = oasis7::runtime_host::world::load_from_dir(execution_world_dir.as_path())
        .expect("load world");
    let finality_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve finality registry")
        .expect("finality registry");
    let controller_registry = world
        .governance_main_token_controller_registry()
        .cloned()
        .expect("controller registry");

    assert_eq!(finality_registry.threshold, 2);
    assert_eq!(
        finality_registry
            .signer_bindings
            .get("governance.finality.v1.triad-testnet-sequencer")
            .map(String::as_str),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        world.node_identity_public_key("governance.finality.v1.triad-testnet-sequencer"),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        controller_registry
            .controller_signer_policies
            .get("msig.genesis.v1")
            .map(|policy| policy.threshold),
        Some(2)
    );
    assert_eq!(
        controller_registry
            .controller_signer_policies
            .get("msig.security_council.v1")
            .map(|policy| policy.threshold),
        Some(2)
    );
    assert_eq!(world.snapshot().tick_consensus_records.len(), 0);
    assert_eq!(world.journal().len(), 0);
    assert_eq!(
        world.snapshot().rollback_authority_registry,
        RollbackAuthorityRegistry::new([
            RollbackAuthorityRecord {
                authority_id: "rollback-on-call-01".to_string(),
                role: RollbackAuthorityRole::OnCall,
                public_key_hex: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    .to_string(),
                active: true,
            },
            RollbackAuthorityRecord {
                authority_id: "rollback-governance-01".to_string(),
                role: RollbackAuthorityRole::Governance,
                public_key_hex: "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                    .to_string(),
                active: true,
            },
        ])
        .expect("valid governed rollback registry"),
        "chain bootstrap must durably provision the fixed rollback authority slots"
    );
}

#[test]
fn network_tier_genesis_bootstrap_resolves_repo_root_relative_child_refs() {
    let repo_root = temp_dir("network-tier-bootstrap-repo-root");
    let evidence_dir = repo_root.join("doc/testing/evidence");
    std::fs::create_dir_all(&evidence_dir).expect("create evidence dir");
    let execution_world_dir = repo_root.join("world");
    let manifest_path = repo_root.join("network-tier.json");
    let bootstrap_path = repo_root.join("bootstrap.txt");
    let genesis_path = evidence_dir.join("public-testnet-governed-bootstrap-genesis.json");
    let governance_manifest_path =
        evidence_dir.join("public-testnet-governance-public-signers.json");
    let liveops_manifest_path = evidence_dir.join("public-testnet-liveops-public-signers.json");
    let validator_registry_path =
        evidence_dir.join("public-testnet-governed-bootstrap-validator-registry.json");

    std::fs::write(bootstrap_path.as_path(), b"").expect("write bootstrap peers");
    std::fs::write(
            governance_manifest_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "oasis7.signer_truth_bundle.v1",
                "entries": [
                    {
                        "slot_id": "msig.genesis.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "1111111111111111111111111111111111111111111111111111111111111111"
                    },
                    {
                        "slot_id": "msig.genesis.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "2222222222222222222222222222222222222222222222222222222222222222"
                    },
                    {
                        "slot_id": "msig.staking_governance.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "3333333333333333333333333333333333333333333333333333333333333333"
                    },
                    {
                        "slot_id": "msig.staking_governance.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "4444444444444444444444444444444444444444444444444444444444444444"
                    },
                    {
                        "slot_id": "msig.ecosystem_governance.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "5555555555555555555555555555555555555555555555555555555555555555"
                    },
                    {
                        "slot_id": "msig.ecosystem_governance.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "6666666666666666666666666666666666666666666666666666666666666666"
                    },
                    {
                        "slot_id": "msig.security_council.v1",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "public_key_hex": "7777777777777777777777777777777777777777777777777777777777777777"
                    },
                    {
                        "slot_id": "msig.security_council.v1",
                        "signer_id": "signer02",
                        "scheme": "ed25519",
                        "public_key_hex": "8888888888888888888888888888888888888888888888888888888888888888"
                    },
                    {
                        "slot_id": "governance.finality.v1",
                        "signer_id": "triad-testnet-sequencer",
                        "scheme": "ed25519",
                        "threshold": 2,
                        "public_key_hex": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    },
                    {
                        "slot_id": "governance.finality.v1",
                        "signer_id": "triad-testnet-storage",
                        "scheme": "ed25519",
                        "threshold": 2,
                        "public_key_hex": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    },
                    {
                        "slot_id": "ops.rollback.on_call.v1",
                        "signer_id": "triad-rollback-on-call",
                        "scheme": "ed25519",
                        "threshold": 1,
                        "public_key_hex": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    },
                    {
                        "slot_id": "governance.rollback.v1",
                        "signer_id": "triad-rollback-governance",
                        "scheme": "ed25519",
                        "threshold": 1,
                        "public_key_hex": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
                    }
                ]
            }))
            .expect("encode governance manifest"),
        )
        .expect("write governance manifest");
    std::fs::write(
            liveops_manifest_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "oasis7.signer_truth_bundle.v1",
                "entries": [
                    {
                        "slot_id": "liveops",
                        "signer_id": "signer01",
                        "scheme": "ed25519",
                        "threshold": 1,
                        "public_key_hex": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                    }
                ]
            }))
            .expect("encode liveops manifest"),
        )
        .expect("write liveops manifest");
    std::fs::write(
            validator_registry_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "slot_id": "governance.finality.v1",
                "threshold": 2,
                "validators": [
                    {
                        "node_id": "triad-testnet-sequencer",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                        "stake": 60
                    },
                    {
                        "node_id": "triad-testnet-storage",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        "stake": 40
                    }
                ]
            }))
            .expect("encode validator registry"),
        )
        .expect("write validator registry");
    std::fs::write(
            genesis_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "oasis7.genesis.example.v1",
                "network_tier": "public_testnet",
                "chain_id": "oasis7-public-testnet-governed",
                "world_id": "oasis7-public-testnet-governed",
                "governance_bootstrap_refs": {
                    "governance_public_manifest_ref": "doc/testing/evidence/public-testnet-governance-public-signers.json",
                    "liveops_public_manifest_ref": "doc/testing/evidence/public-testnet-liveops-public-signers.json",
                    "genesis_validator_registry_ref": "doc/testing/evidence/public-testnet-governed-bootstrap-validator-registry.json"
                }
            }))
            .expect("encode genesis"),
        )
        .expect("write genesis");
    std::fs::write(manifest_path.as_path(), b"{}").expect("touch manifest path");

    let loaded_manifest = public_testnet_loaded_manifest_with_paths(
        manifest_path.as_path(),
        genesis_path.as_path().to_str().unwrap(),
        bootstrap_path.as_path().to_str().unwrap(),
    );

    ensure_world_governance_validator_registry(
        execution_world_dir.as_path(),
        None,
        Some(&loaded_manifest),
    )
    .expect("bootstrap governance world");

    let world = oasis7::runtime_host::world::load_from_dir(execution_world_dir.as_path())
        .expect("load world");
    let finality_registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve finality registry")
        .expect("finality registry");
    assert_eq!(
        finality_registry
            .signer_bindings
            .get("governance.finality.v1.triad-testnet-sequencer")
            .map(String::as_str),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert_eq!(
        world
            .governance_main_token_controller_registry()
            .and_then(|registry| registry.controller_signer_policies.get("msig.genesis.v1"))
            .map(|policy| policy.threshold),
        Some(2)
    );
}

#[test]
fn genesis_public_manifest_requires_explicit_threshold_for_four_validators() {
    let temp_dir = temp_dir("genesis-public-manifest-threshold");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let manifest_path = temp_dir.join("public-manifest-entries.json");
    std::fs::write(
        manifest_path.as_path(),
        serde_json::to_vec_pretty(&serde_json::json!([
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-a",
                "scheme": "ed25519",
                "public_key_hex": "1111111111111111111111111111111111111111111111111111111111111111"
            },
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-b",
                "scheme": "ed25519",
                "public_key_hex": "2222222222222222222222222222222222222222222222222222222222222222"
            },
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-c",
                "scheme": "ed25519",
                "public_key_hex": "3333333333333333333333333333333333333333333333333333333333333333"
            },
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-d",
                "scheme": "ed25519",
                "public_key_hex": "4444444444444444444444444444444444444444444444444444444444444444"
            }
        ]))
        .expect("encode manifest"),
    )
    .expect("write manifest");

    let err = ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        Some(manifest_path.as_path()),
        Some(&public_testnet_loaded_manifest()),
    )
    .expect_err("public manifest entries require explicit threshold");

    assert!(err.contains("requires explicit threshold"), "{err}");
}

#[test]
fn genesis_public_manifest_preserves_explicit_three_of_four_threshold() {
    let temp_dir = temp_dir("genesis-public-manifest-three-of-four");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let manifest_path = temp_dir.join("public-manifest-entries.json");
    std::fs::write(
        manifest_path.as_path(),
        serde_json::to_vec_pretty(&serde_json::json!([
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-a",
                "scheme": "ed25519",
                "threshold": 3,
                "public_key_hex": "1111111111111111111111111111111111111111111111111111111111111111"
            },
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-b",
                "scheme": "ed25519",
                "threshold": 3,
                "public_key_hex": "2222222222222222222222222222222222222222222222222222222222222222"
            },
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-c",
                "scheme": "ed25519",
                "threshold": 3,
                "public_key_hex": "3333333333333333333333333333333333333333333333333333333333333333"
            },
            {
                "slot_id": "governance.finality.v1",
                "signer_id": "validator-d",
                "scheme": "ed25519",
                "threshold": 3,
                "public_key_hex": "4444444444444444444444444444444444444444444444444444444444444444"
            }
        ]))
        .expect("encode manifest"),
    )
    .expect("write manifest");

    ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        Some(manifest_path.as_path()),
        Some(&public_testnet_loaded_manifest()),
    )
    .expect("ensure genesis registry");
    let world = oasis7::runtime_host::world::load_from_dir(temp_dir.as_path()).expect("load world");
    let registry = world
        .resolve_governance_effective_finality_signer_registry()
        .expect("resolve registry")
        .expect("registry");

    assert_eq!(registry.threshold, 3);
    assert_eq!(registry.signer_bindings.len(), 4);
}

#[test]
fn genesis_validator_registry_rejects_existing_world_without_registry() {
    let temp_dir = temp_dir("genesis-existing-world");
    let world = oasis7::runtime_host::world::new();
    world.save_to_dir(&temp_dir).expect("save existing world");
    let manifest_path = temp_dir.join("genesis-validator-registry.json");
    std::fs::write(
            manifest_path.as_path(),
            serde_json::to_vec_pretty(&serde_json::json!({
                "slot_id": "governance.finality.v1",
                "threshold": 1,
                "validators": [
                    {
                        "node_id": "validator-a",
                        "scheme": "ed25519",
                        "finality_signer_public_key": "1111111111111111111111111111111111111111111111111111111111111111",
                        "stake": 100
                    }
                ]
            }))
            .expect("encode manifest"),
        )
        .expect("write manifest");

    let err = ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        Some(manifest_path.as_path()),
        Some(&public_testnet_loaded_manifest()),
    )
    .expect_err("existing world must use migration path");

    assert!(
        err.contains("refusing to import genesis validator registry"),
        "{err}"
    );
    assert!(
        err.contains("dedicated governance registry import"),
        "{err}"
    );
}

#[test]
fn genesis_validator_registry_allows_existing_world_with_registry() {
    let temp_dir = temp_dir("genesis-existing-world-with-registry");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 1,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([(
                "governance.finality.v1.validator-a".to_string(),
                "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            )]),
            validator_stakes: BTreeMap::from([(
                "governance.finality.v1.validator-a".to_string(),
                100,
            )]),
        })
        .expect("write registry");
    world.save_to_dir(&temp_dir).expect("save existing world");
    let manifest_path = temp_dir.join("genesis-validator-registry.json");
    std::fs::write(
        manifest_path.as_path(),
        serde_json::to_vec_pretty(&serde_json::json!({
            "slot_id": "governance.finality.v1",
            "threshold": 1,
            "validators": [
                {
                    "node_id": "validator-a",
                    "scheme": "ed25519",
                    "finality_signer_public_key": "1111111111111111111111111111111111111111111111111111111111111111",
                    "stake": 100
                }
            ]
        }))
        .expect("encode manifest"),
    )
    .expect("write manifest");

    ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        Some(manifest_path.as_path()),
        Some(&public_testnet_loaded_manifest()),
    )
    .expect("existing world with registry should be idempotent");
}

#[test]
fn public_tier_existing_world_with_finality_but_without_rollback_registry_fails_startup() {
    let temp_dir = temp_dir("public-existing-world-missing-rollback-registry");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 1,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([(
                "governance.finality.v1.validator-a".to_string(),
                "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            )]),
            validator_stakes: BTreeMap::from([(
                "governance.finality.v1.validator-a".to_string(),
                100,
            )]),
        })
        .expect("configure finality registry");
    world.step().expect("make world non-bootstrap");
    world.save_to_dir(&temp_dir).expect("save existing world");

    let err = ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        None,
        Some(&public_testnet_loaded_manifest()),
    )
    .expect_err("public-tier startup must require a governed rollback registry");

    assert!(err.contains("rollback"), "unexpected startup error: {err}");
    assert!(
        err.contains("governance registry import") || err.contains("migration"),
        "startup error must identify the remediation path: {err}"
    );
}

#[test]
fn public_tier_without_world_registry_or_genesis_manifest_fails() {
    let temp_dir = temp_dir("public-missing-registry");
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");

    let err = ensure_world_governance_validator_registry(
        temp_dir.as_path(),
        None,
        Some(&public_testnet_loaded_manifest()),
    )
    .expect_err("public tier must require registry truth");

    assert!(err.contains("public validator registry required"), "{err}");
    assert!(err.contains("public_testnet"), "{err}");
    assert!(err.contains("--genesis-validator-registry"), "{err}");
}

#[test]
fn world_finality_registry_strips_slot_prefix_from_validator_ids() {
    let temp_dir = temp_dir("finality-prefix-override");
    let mut world = oasis7::runtime_host::world::new();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "governance.finality.v1.triad-testnet-sequencer".to_string(),
                    "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
                ),
                (
                    "governance.finality.v1.triad-testnet-storage".to_string(),
                    "2222222222222222222222222222222222222222222222222222222222222222".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                (
                    "governance.finality.v1.triad-testnet-sequencer".to_string(),
                    60,
                ),
                (
                    "governance.finality.v1.triad-testnet-storage".to_string(),
                    40,
                ),
            ]),
        })
        .expect("set finality registry");
    world.save_to_dir(&temp_dir).expect("save execution world");

    let config = apply_world_governance_registry_overrides(
        NodeConfig::new("triad-testnet-sequencer", "world-a", NodeRole::Sequencer)
            .expect("node config"),
        &temp_dir,
    )
    .expect("apply registry overrides");

    let validator_ids = config
        .pos_config
        .validators
        .iter()
        .map(|validator| validator.validator_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        validator_ids,
        vec!["triad-testnet-sequencer", "triad-testnet-storage"]
    );
    assert_eq!(
        config
            .pos_config
            .validator_signer_public_keys
            .get("triad-testnet-sequencer")
            .map(String::as_str),
        Some("1111111111111111111111111111111111111111111111111111111111111111")
    );
    let validator_stakes = config
        .pos_config
        .validators
        .iter()
        .map(|validator| (validator.validator_id.as_str(), validator.stake))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(validator_stakes.get("triad-testnet-sequencer"), Some(&60));
    assert_eq!(validator_stakes.get("triad-testnet-storage"), Some(&40));
}

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
    world.submit_action(Action::SubmitGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c".to_string(),
        node_id: "validator-c".to_string(),
        finality_signer_public_key:
            "3333333333333333333333333333333333333333333333333333333333333333".to_string(),
        stake: 35,
        operator_owner: "ops.team".to_string(),
        public_manifest_hash: "manifest-c".to_string(),
    });
    world.step().expect("submit validator admission");
    world.submit_action(Action::ApproveGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c".to_string(),
    });
    world.step().expect("approve validator admission");
    world.submit_action(Action::ActivateGovernanceValidatorAdmission {
        controller_account_id: "msig.genesis.v1".to_string(),
        candidate_id: "candidate-c".to_string(),
        activation_epoch: 0,
    });
    world.step().expect("activate validator admission");
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
