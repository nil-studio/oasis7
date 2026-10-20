use oasis7::network_tier_manifest::{
    LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1, NetworkTierClaimsPolicy,
    NetworkTierEndpointPolicy, NetworkTierManifest, NetworkTierPromotionPolicy,
    NetworkTierRuntimeRefs, NetworkTierTokenPolicy, NetworkTierValidatorPolicy,
};
use oasis7_node::PosValidator;
use std::collections::BTreeMap;

use super::super::{
    build_validator_signer_public_keys, derive_node_consensus_signer_keypair,
    derive_node_libp2p_identity_keypair_config, network_tier_allows_open_observer_fetch,
    node_keypair_config,
};
use ed25519_dalek::SigningKey;

#[test]
fn public_testnet_manifest_allows_open_observer_fetch_without_validator_admission() {
    let loaded = test_network_tier_manifest("public_testnet", true);

    assert!(network_tier_allows_open_observer_fetch(Some(&loaded)));
}

#[test]
fn public_testnet_manifest_without_observer_policy_does_not_allow_open_observer_fetch() {
    let loaded = test_network_tier_manifest("public_testnet", false);

    assert!(!network_tier_allows_open_observer_fetch(Some(&loaded)));
}

#[test]
fn mainnet_manifest_does_not_allow_open_observer_fetch() {
    let loaded = test_network_tier_manifest("mainnet", true);

    assert!(!network_tier_allows_open_observer_fetch(Some(&loaded)));
}

fn test_network_tier_manifest(tier: &str, allow_observer_nodes: bool) -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: "test-manifest.json".to_string(),
        manifest: NetworkTierManifest {
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.to_string(),
            tier: tier.to_string(),
            status: "rehearsal".to_string(),
            network_id: format!("oasis7-{tier}"),
            chain_id: format!("oasis7-{tier}"),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".to_string(),
                genesis_ref: "genesis.json".to_string(),
                bootstrap_peer_ref: "peers.txt".to_string(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "http://127.0.0.1:6631".to_string(),
                explorer_ref: "http://127.0.0.1:6632/explorer".to_string(),
                faucet_ref: Some("none".to_string()),
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "governance_registry".to_string(),
                validator_admission: "allowlist_or_governed_candidate".to_string(),
                target_validator_count: 2,
                allow_observer_nodes,
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
            evidence_refs: vec![],
        },
        bootstrap_peers: vec![],
    }
}

#[test]
fn derive_node_consensus_signer_keypair_is_deterministic_for_oasis7_namespace() {
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };

    let signer_a =
        derive_node_consensus_signer_keypair("node-a", &keypair).expect("derive signer a");
    let signer_a_repeat =
        derive_node_consensus_signer_keypair("node-a", &keypair).expect("derive signer a repeat");
    let signer_b =
        derive_node_consensus_signer_keypair("node-b", &keypair).expect("derive signer b");

    assert_eq!(signer_a.private_key_hex, signer_a_repeat.private_key_hex);
    assert_eq!(signer_a.public_key_hex, signer_a_repeat.public_key_hex);
    assert_ne!(signer_a.public_key_hex, signer_b.public_key_hex);
}

#[test]
fn build_validator_signer_public_keys_prefers_explicit_overrides() {
    let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };
    let validators = vec![
        PosValidator {
            validator_id: "node-a".to_string(),
            stake: 60,
        },
        PosValidator {
            validator_id: "node-b".to_string(),
            stake: 40,
        },
    ];
    let mut overrides = BTreeMap::new();
    overrides.insert("node-b".to_string(), "deadbeef".to_string());

    let bindings = build_validator_signer_public_keys(validators.as_slice(), &keypair, &overrides)
        .expect("bindings should build");

    assert_eq!(bindings.get("node-b").map(String::as_str), Some("deadbeef"));
    assert_ne!(bindings.get("node-a"), bindings.get("node-b"));
}

#[test]
fn derive_node_libp2p_identity_keypair_is_deterministic_and_node_scoped() {
    let signing_key = SigningKey::from_bytes(&[11_u8; 32]);
    let keypair = node_keypair_config::NodeKeypairConfig {
        private_key_hex: hex::encode(signing_key.to_bytes()),
        public_key_hex: hex::encode(signing_key.verifying_key().to_bytes()),
    };

    let identity_a = derive_node_libp2p_identity_keypair_config("node-a", &keypair)
        .expect("derive libp2p identity a");
    let identity_a_repeat = derive_node_libp2p_identity_keypair_config("node-a", &keypair)
        .expect("derive libp2p identity a repeat");
    let identity_b = derive_node_libp2p_identity_keypair_config("node-b", &keypair)
        .expect("derive libp2p identity b");

    assert_eq!(
        identity_a.private_key_hex,
        identity_a_repeat.private_key_hex
    );
    assert_eq!(identity_a.public_key_hex, identity_a_repeat.public_key_hex);
    assert_ne!(identity_a.public_key_hex, identity_b.public_key_hex);

    let libp2p_a = oasis7_node::derive_libp2p_identity_keypair(identity_a.private_key_hex.as_str())
        .expect("libp2p keypair a");
    let libp2p_b = oasis7_node::derive_libp2p_identity_keypair(identity_b.private_key_hex.as_str())
        .expect("libp2p keypair b");
    assert_ne!(
        libp2p_a.public().to_peer_id(),
        libp2p_b.public().to_peer_id()
    );
}
