use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("oasis7-network-tier-{label}-{nonce}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn load_manifest_reads_bootstrap_peer_file() {
    let dir = temp_dir("load");
    let peers_path = dir.join("bootstrap.txt");
    let genesis_path = dir.join("genesis.json");
    fs::write(
        &peers_path,
        "# comment\n/ip4/127.0.0.1/tcp/4100\n/dns4/bootstrap.example/tcp/4101\n",
    )
    .expect("write peers");
    fs::write(&genesis_path, "{}\n").expect("write genesis");
    let manifest_path = dir.join("manifest.json");
    fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": "{NETWORK_TIER_MANIFEST_SCHEMA_V1}",
  "tier": "public_testnet",
  "status": "rehearsal",
  "network_id": "oasis7-public-testnet",
  "chain_id": "oasis7-public-testnet",
  "runtime_refs": {{
    "release_candidate_bundle_ref": "output/release-candidates/public-testnet.json",
    "genesis_ref": "{}",
    "bootstrap_peer_ref": "{}"
  }},
  "endpoint_policy": {{
    "rpc_ref": "https://public-testnet.example.invalid/rpc",
    "explorer_ref": "https://public-testnet.example.invalid/explorer",
    "faucet_ref": "https://public-testnet.example.invalid/faucet"
  }},
  "validator_policy": {{
    "governance_mode": "shared_ops",
    "validator_admission": "allowlist_or_governed_candidate",
    "target_validator_count": 4,
    "allow_observer_nodes": true
  }},
  "token_policy": {{
    "symbol": "OC",
    "faucet_mode": "guarded_testnet_faucet",
    "reset_policy": "resettable",
    "value_semantics": "testnet"
  }},
  "claims_policy": {{
    "allowed_claims": ["public_testnet"],
    "denied_claims": ["mainnet_live", "production_oc_settlement"]
  }},
  "promotion_policy": {{
    "promote_from": ["local_devnet"],
    "required_gates": ["public_testnet_rehearsal_pass", "public_rpc_ready", "faucet_guard_ready", "reset_policy_announced"]
  }},
  "evidence_refs": ["doc/testing/evidence/public-testnet.md"]
}}"#,
                genesis_path.display(),
                peers_path.display()
            ),
        )
        .expect("write manifest");

    let loaded = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect("load");
    assert_eq!(loaded.manifest.tier, "public_testnet");
    assert_eq!(
        loaded.bootstrap_peers,
        vec![
            "/ip4/127.0.0.1/tcp/4100".to_string(),
            "/dns4/bootstrap.example/tcp/4101".to_string()
        ]
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_manifest_rejects_unknown_tier() {
    let dir = temp_dir("invalid");
    let peers_path = dir.join("bootstrap.txt");
    fs::write(&peers_path, "/ip4/127.0.0.1/tcp/4100\n").expect("write peers");
    let manifest_path = dir.join("manifest.json");
    fs::write(
        &manifest_path,
        format!(
            r#"{{
  "schema_version": "{NETWORK_TIER_MANIFEST_SCHEMA_V1}",
  "tier": "wrong",
  "status": "planned",
  "network_id": "oasis7-public-testnet",
  "chain_id": "oasis7-public-testnet",
  "runtime_refs": {{
    "release_candidate_bundle_ref": "a",
    "genesis_ref": "b",
    "bootstrap_peer_ref": "{}"
  }},
  "endpoint_policy": {{
    "rpc_ref": "https://public-testnet.example.invalid/rpc",
    "explorer_ref": "https://public-testnet.example.invalid/explorer",
    "faucet_ref": null
  }},
  "validator_policy": {{
    "governance_mode": "shared_ops",
    "validator_admission": "shared_allowlist",
    "target_validator_count": 3,
    "allow_observer_nodes": true
  }},
  "token_policy": {{
    "symbol": "OC",
    "faucet_mode": "operator_grant",
    "reset_policy": "resettable",
    "value_semantics": "preview"
  }},
  "claims_policy": {{
    "allowed_claims": [],
    "denied_claims": []
  }},
  "promotion_policy": {{
    "promote_from": [],
    "required_gates": []
  }},
  "evidence_refs": []
}}"#,
            peers_path.display()
        ),
    )
    .expect("write manifest");

    let err = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect_err("reject");
    assert!(err.contains("invalid tier"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_manifest_rejects_missing_genesis_ref_file() {
    let dir = temp_dir("missing-genesis");
    let peers_path = dir.join("bootstrap.txt");
    fs::write(&peers_path, "/ip4/127.0.0.1/tcp/4100\n").expect("write peers");
    let manifest_path = dir.join("manifest.json");
    fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": "{NETWORK_TIER_MANIFEST_SCHEMA_V1}",
  "tier": "public_testnet",
  "status": "specified_skeleton_only",
  "network_id": "oasis7-public-testnet",
  "chain_id": "oasis7-public-testnet",
  "runtime_refs": {{
    "release_candidate_bundle_ref": "output/release-candidates/public-testnet.json",
    "genesis_ref": "missing-genesis.json",
    "bootstrap_peer_ref": "{}"
  }},
  "endpoint_policy": {{
    "rpc_ref": "https://public-testnet.example.invalid/rpc",
    "explorer_ref": "https://public-testnet.example.invalid/explorer",
    "faucet_ref": "https://public-testnet.example.invalid/faucet"
  }},
  "validator_policy": {{
    "governance_mode": "shared_ops",
    "validator_admission": "allowlist_or_governed_candidate",
    "target_validator_count": 4,
    "allow_observer_nodes": true
  }},
  "token_policy": {{
    "symbol": "OC",
    "faucet_mode": "guarded_testnet_faucet",
    "reset_policy": "resettable",
    "value_semantics": "testnet"
  }},
  "claims_policy": {{
    "allowed_claims": ["public_testnet"],
    "denied_claims": ["mainnet_live", "production_oc_settlement"]
  }},
  "promotion_policy": {{
    "promote_from": ["local_devnet"],
    "required_gates": ["public_testnet_rehearsal_pass", "public_rpc_ready", "faucet_guard_ready", "reset_policy_announced"]
  }},
  "evidence_refs": ["doc/testing/evidence/public-testnet.md"]
}}"#,
                peers_path.display()
            ),
        )
        .expect("write manifest");

    let err = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect_err("reject");
    assert!(err.contains("missing runtime_refs.genesis_ref"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_manifest_rejects_public_testnet_without_production_settlement_deny() {
    let dir = temp_dir("claims");
    let peers_path = dir.join("bootstrap.txt");
    let genesis_path = dir.join("genesis.json");
    fs::write(&peers_path, "/ip4/127.0.0.1/tcp/4100\n").expect("write peers");
    fs::write(&genesis_path, "{}\n").expect("write genesis");
    let manifest_path = dir.join("manifest.json");
    fs::write(
            &manifest_path,
            format!(
                r#"{{
  "schema_version": "{NETWORK_TIER_MANIFEST_SCHEMA_V1}",
  "tier": "public_testnet",
  "status": "specified_skeleton_only",
  "network_id": "oasis7-public-testnet",
  "chain_id": "oasis7-public-testnet",
  "runtime_refs": {{
    "release_candidate_bundle_ref": "output/release-candidates/public-testnet.json",
    "genesis_ref": "{}",
    "bootstrap_peer_ref": "{}"
  }},
  "endpoint_policy": {{
    "rpc_ref": "https://public-testnet.example.invalid/rpc",
    "explorer_ref": "https://public-testnet.example.invalid/explorer",
    "faucet_ref": "https://public-testnet.example.invalid/faucet"
  }},
  "validator_policy": {{
    "governance_mode": "shared_ops",
    "validator_admission": "allowlist_or_governed_candidate",
    "target_validator_count": 4,
    "allow_observer_nodes": true
  }},
  "token_policy": {{
    "symbol": "OC",
    "faucet_mode": "guarded_testnet_faucet",
    "reset_policy": "resettable",
    "value_semantics": "testnet"
  }},
  "claims_policy": {{
    "allowed_claims": ["public_testnet"],
    "denied_claims": ["mainnet_live"]
  }},
  "promotion_policy": {{
    "promote_from": ["local_devnet"],
    "required_gates": ["public_testnet_rehearsal_pass", "public_rpc_ready", "faucet_guard_ready", "reset_policy_announced"]
  }},
  "evidence_refs": ["doc/testing/evidence/public-testnet.md"]
}}"#,
                genesis_path.display(),
                peers_path.display()
            ),
        )
        .expect("write manifest");

    let err = LoadedNetworkTierManifest::load(manifest_path.as_path()).expect_err("reject");
    assert!(err.contains("production_oc_settlement"));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn claim_list_contains_ascii_case_insensitive_substrings_without_joining() {
    let claims = vec![
        "PUBLIC_TESTNET_READY".to_string(),
        "production_OC_settlement_denied".to_string(),
    ];

    assert!(string_list_contains_ascii_case_insensitive(
        claims.as_slice(),
        "public_testnet"
    ));
    assert!(string_list_contains_ascii_case_insensitive(
        claims.as_slice(),
        "production_oc_settlement"
    ));
    assert!(!string_list_contains_ascii_case_insensitive(
        claims.as_slice(),
        "mainnet"
    ));
}
