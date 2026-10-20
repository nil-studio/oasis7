use super::*;
use oasis7::network_tier_manifest::{
    LoadedNetworkTierManifest, NETWORK_TIER_MANIFEST_SCHEMA_V1, NetworkTierClaimsPolicy,
    NetworkTierEndpointPolicy, NetworkTierManifest, NetworkTierPromotionPolicy,
    NetworkTierRuntimeRefs, NetworkTierTokenPolicy, NetworkTierValidatorPolicy,
};
use oasis7_runtime::{
    ChainResourceDerivationContext, GovernanceFinalitySignerRegistry, World as RuntimeWorld,
};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir() -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let process_id = std::process::id();
    for _ in 0..64 {
        let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-runtime-authority-{process_id}-{timestamp}-{sequence}"
        ));
        match fs::create_dir(path.as_path()) {
            Ok(()) => return path,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create temp dir {} failed: {error}", path.display()),
        }
    }
    panic!("create unique temp dir failed after 64 attempts");
}

fn loaded_manifest(path: &Path) -> LoadedNetworkTierManifest {
    LoadedNetworkTierManifest {
        source_path: path.to_string_lossy().to_string(),
        manifest: NetworkTierManifest {
            release_policy: None,
            world_policy: None,
            authority_policy: None,
            schema_version: NETWORK_TIER_MANIFEST_SCHEMA_V1.to_string(),
            tier: "public_testnet".to_string(),
            status: "rehearsal".to_string(),
            network_id: "oasis7-public-testnet-governed-20260606".to_string(),
            chain_id: "oasis7-public-testnet-governed-20260606".to_string(),
            runtime_refs: NetworkTierRuntimeRefs {
                release_candidate_bundle_ref: "bundle.json".to_string(),
                genesis_ref: "genesis.json".to_string(),
                bootstrap_peer_ref: "peers.txt".to_string(),
            },
            endpoint_policy: NetworkTierEndpointPolicy {
                rpc_ref: "http://127.0.0.1:6631".to_string(),
                explorer_ref: "http://127.0.0.1:6632".to_string(),
                faucet_ref: None,
            },
            validator_policy: NetworkTierValidatorPolicy {
                governance_mode: "governance_registry".to_string(),
                validator_admission: "allowlist_or_governed_candidate".to_string(),
                target_validator_count: 3,
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
                required_gates: vec!["runtime_bootstrap".to_string()],
            },
            evidence_refs: Vec::new(),
        },
        bootstrap_peers: Vec::new(),
    }
}

fn write_authority_fixture() -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let root = unique_temp_dir();
    let registry = root.join("registry.json");
    let inventory = root.join("inventory.json");
    let validator_nodes = serde_json::json!({
        "sequencer-204": {
            "node_id": "triad-testnet-sequencer",
            "roles": ["validator", "sequencer"],
            "stake": 100,
            "finality_signer_public_key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "storage-205": {
            "node_id": "triad-testnet-storage",
            "roles": ["validator", "storage"],
            "stake": 100,
            "finality_signer_public_key": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        },
        "validator-47": {
            "node_id": "triad-testnet-validator-47",
            "roles": ["validator", "checkpoint_provider", "full_storage_provider"],
            "stake": 100,
            "finality_signer_public_key": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "libp2p_peer_id": "12D3KooWFixture"
        }
    });
    let mut inventory_value = serde_json::json!({
        "schema_version": "oasis7.public_testnet_validator_triad_inventory.v1",
        "repository": "eng-cc/oasis7",
        "network_tier": "public_testnet",
        "topology": "three_equal_validator",
        "validator_set": {
            "count": 3,
            "stakes": [100, 100, 100],
            "total_stake": 300,
            "required_stake": 200,
            "quorum": {"numerator": 2, "denominator": 3}
        },
        "authority": {
            "world_id": "oasis7-public-testnet-governed-20260606",
            "chain_id": "oasis7-public-testnet-governed-20260606",
            "network_tier": "public_testnet",
            "registry_ref": "registry.json",
            "generated_registry_sha256": "",
            "generated_registry_semantic_sha256": ""
        },
        "governance": {"signer_count": 3, "threshold": 2, "threshold_bps": 6667},
        "nodes": validator_nodes
    });
    let registry_bytes = serde_json::to_vec(&serde_json::json!({
                "slot_id": "governance.finality.v1",
                "threshold": 2,
                "threshold_bps": 0,
                "validators": [
                    {"node_id": "triad-testnet-sequencer", "finality_signer_public_key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "stake": 100},
                    {"node_id": "triad-testnet-storage", "finality_signer_public_key": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "stake": 100},
                    {"node_id": "triad-testnet-validator-47", "finality_signer_public_key": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc", "stake": 100}
                ]
            }))
            .expect("encode registry");
    fs::write(registry.as_path(), registry_bytes).expect("write registry");
    let registry_sha256 = hex::encode(Sha256::digest(
        fs::read(registry.as_path())
            .expect("read registry")
            .as_slice(),
    ));
    inventory_value["authority"]["generated_registry_sha256"] =
        serde_json::Value::String(registry_sha256.clone());
    let fixture_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load fixture registry for semantic digest");
    let registry_semantic_sha256 =
        semantic_registry_sha256(&fixture_registry).expect("semantic registry digest");
    inventory_value["authority"]["generated_registry_semantic_sha256"] =
        serde_json::Value::String(registry_semantic_sha256.clone());
    fs::write(
        inventory.as_path(),
        serde_json::to_vec(&inventory_value).expect("encode inventory"),
    )
    .expect("write inventory");
    let inventory_sha256 = hex::encode(Sha256::digest(
        fs::read(inventory.as_path())
            .expect("read inventory")
            .as_slice(),
    ));
    let manifest_path = root.join("manifest.json");
    fs::write(
            manifest_path.as_path(),
            format!(
                "{{\"deployment_inventory\":{{\"ref\":\"scripts/public-testnet-validator-triad-inventory.v1.json\",\"sha256\":\"{inventory_sha256}\"}},\"deployment_validator_registry\":{{\"ref\":\"registry.json\",\"sha256\":\"{registry_sha256}\",\"semantic_sha256\":\"{registry_semantic_sha256}\"}}}}\n"
            ),
        )
        .expect("write manifest");
    (registry, inventory, manifest_path)
}

fn write_observer_authority_fixture() -> (std::path::PathBuf, std::path::PathBuf) {
    let root = unique_temp_dir();
    let registry = root.join("observer-registry.json");
    let registry_bytes = serde_json::to_vec(&serde_json::json!({
            "slot_id": "governance.finality.v1",
            "threshold": 2,
            "threshold_bps": 0,
            "validators": [
                {
                    "node_id": "triad-testnet-sequencer",
                    "scheme": "ed25519",
                    "finality_signer_public_key": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "stake": 100
                },
                {
                    "node_id": "triad-testnet-storage",
                    "scheme": "ed25519",
                    "finality_signer_public_key": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "stake": 50
                }
            ]
        }))
        .expect("encode observer registry");
    fs::write(registry.as_path(), registry_bytes).expect("write observer registry");
    let registry_sha256 = hex::encode(Sha256::digest(
        fs::read(registry.as_path())
            .expect("read observer registry")
            .as_slice(),
    ));
    let fixture_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load observer registry for semantic digest");
    let registry_semantic_sha256 =
        semantic_registry_sha256(&fixture_registry).expect("observer semantic digest");
    let config_dir = root.join("config");
    fs::create_dir_all(config_dir.as_path()).expect("create observer manifest root");
    let manifest_path = config_dir.join("observer-manifest.json");
    fs::write(
            manifest_path.as_path(),
            format!(
                "{{\"deployment_validator_registry\":{{\"ref\":\"config/observer-registry.json\",\"sha256\":\"{registry_sha256}\",\"semantic_sha256\":\"{registry_semantic_sha256}\"}}}}\n"
            ),
        )
        .expect("write observer manifest");
    let localized_registry = config_dir.join("observer-registry.json");
    fs::copy(registry.as_path(), localized_registry.as_path()).expect("localize observer registry");
    (localized_registry, manifest_path)
}

fn save_authority_world(world: &RuntimeWorld, world_dir: &Path) {
    world
        .save_to_dir_with_chain_resource_context(
            world_dir,
            ChainResourceDerivationContext {
                world_id: "oasis7-public-testnet-governed-20260606",
                chain_id: "oasis7-public-testnet-governed-20260606",
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: 0,
            },
            "fixture-world-config",
            "fixture-generation",
        )
        .expect("save authority world");
}

fn persisted_tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn collect(root: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).expect("read persisted tree") {
            let entry = entry.expect("read persisted tree entry");
            let entry_path = entry.path();
            let file_type = entry.file_type().expect("read persisted tree file type");
            if file_type.is_dir() {
                collect(root, entry_path.as_path(), files);
            } else {
                assert!(
                    file_type.is_file(),
                    "persisted tree contains unsupported entry"
                );
                let relative = entry_path
                    .strip_prefix(root)
                    .expect("persisted tree relative path")
                    .to_string_lossy()
                    .into_owned();
                files.insert(
                    relative,
                    fs::read(entry_path.as_path()).expect("read persisted tree file"),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

#[test]
fn captures_actual_refs_and_file_digests() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let binding = load_runtime_authority_binding(
        manifest_path.parent().expect("fixture root"),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect("authority binding")
    .expect("binding present");
    assert_eq!(binding.registry_ref, registry.to_string_lossy());
    assert_eq!(
        binding.inventory_ref,
        "scripts/public-testnet-validator-triad-inventory.v1.json"
    );
    assert_eq!(
        binding.registry_sha256,
        hex::encode(Sha256::digest(
            fs::read(registry.as_path())
                .expect("read registry")
                .as_slice(),
        ))
    );
    assert_eq!(binding.registry_semantic_sha256.len(), 64);
    assert_eq!(
        binding.inventory_sha256,
        hex::encode(Sha256::digest(
            fs::read(inventory.as_path())
                .expect("read inventory")
                .as_slice(),
        ))
    );
}

#[test]
fn rejects_partial_authority_inputs() {
    let error = load_runtime_authority_binding(
        Path::new("world"),
        Some(Path::new("registry.json")),
        None,
        None,
    )
    .expect_err("partial binding must fail closed");
    assert!(error.contains("--deployment-inventory"));
}

#[test]
fn rejects_public_testnet_without_authority_inputs() {
    let error = load_runtime_authority_binding(
        Path::new("world"),
        None,
        None,
        Some(&loaded_manifest(Path::new("missing-manifest.json"))),
    )
    .expect_err("public testnet authority must fail closed");
    assert!(error.contains("public-testnet deployment authority"));
}

#[test]
fn rejects_manifest_inventory_digest_drift() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let manifest = fs::read_to_string(manifest_path.as_path()).expect("read manifest");
    fs::write(
        manifest_path.as_path(),
        manifest.replace(
            &hex::encode(Sha256::digest(
                fs::read(inventory.as_path())
                    .expect("read inventory")
                    .as_slice(),
            )),
            &"0".repeat(64),
        ),
    )
    .expect("rewrite manifest");
    let error = load_runtime_authority_binding(
        manifest_path.parent().expect("fixture root"),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("digest drift must fail closed");
    assert!(error.contains("digest mismatch"));
}

#[test]
fn rejects_generated_registry_digest_drift() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let mut registry_bytes = fs::read(registry.as_path()).expect("read fixture registry");
    registry_bytes.push(b' ');
    fs::write(registry.as_path(), registry_bytes).expect("rewrite fixture registry");
    let error = load_runtime_authority_binding(
        manifest_path.parent().expect("fixture root"),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("changed generated registry must fail closed");
    assert!(error.contains("generated validator registry digest mismatch"));
}

#[test]
fn rejects_inventory_digest_drift_without_mutating_existing_world() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let world_dir = manifest_path.parent().expect("fixture root").join("world");
    let explicit_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load fixture registry");
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world
        .set_governance_finality_signer_registry(explicit_registry)
        .expect("set persisted effective registry");
    save_authority_world(&world, world_dir.as_path());
    let persisted_tree_before = persisted_tree_bytes(world_dir.as_path());

    let manifest = fs::read_to_string(manifest_path.as_path()).expect("read manifest");
    fs::write(
        manifest_path.as_path(),
        manifest.replace(
            &hex::encode(Sha256::digest(
                fs::read(inventory.as_path())
                    .expect("read inventory")
                    .as_slice(),
            )),
            &"0".repeat(64),
        ),
    )
    .expect("rewrite manifest");
    let error = load_runtime_authority_binding(
        world_dir.as_path(),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("digest drift must fail before world bootstrap mutation");
    assert!(error.contains("digest mismatch"));
    assert_eq!(
        persisted_tree_before,
        persisted_tree_bytes(world_dir.as_path()),
        "authority rejection must not mutate any persisted world file"
    );
}

#[test]
fn rejects_explicit_registry_that_does_not_match_persisted_effective_registry() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let world_dir = manifest_path.parent().expect("fixture root").join("world");
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world
        .set_governance_finality_signer_registry(GovernanceFinalitySignerRegistry {
            slot_id: "governance.finality.v1".to_string(),
            threshold: 2,
            threshold_bps: 0,
            signer_bindings: BTreeMap::from([
                (
                    "governance.finality.v1.triad-testnet-sequencer".to_string(),
                    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".to_string(),
                ),
                (
                    "governance.finality.v1.triad-testnet-storage".to_string(),
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                ),
                (
                    "governance.finality.v1.triad-testnet-validator-47".to_string(),
                    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string(),
                ),
            ]),
            validator_stakes: BTreeMap::from([
                (
                    "governance.finality.v1.triad-testnet-sequencer".to_string(),
                    100,
                ),
                (
                    "governance.finality.v1.triad-testnet-storage".to_string(),
                    100,
                ),
                (
                    "governance.finality.v1.triad-testnet-validator-47".to_string(),
                    100,
                ),
            ]),
        })
        .expect("set persisted effective registry");
    save_authority_world(&world, world_dir.as_path());
    let error = load_runtime_authority_binding(
        world_dir.as_path(),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("stale explicit registry must fail closed");
    assert!(error.contains("does not match persisted effective governance registry"));
}

#[test]
fn accepts_governed_registry_after_persisted_world_normalizes_threshold_bps() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let world_dir = manifest_path.parent().expect("fixture root").join("world");
    let explicit_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load fixture registry");
    assert_eq!(explicit_registry.threshold_bps, 0);
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world
        .set_governance_finality_signer_registry(explicit_registry)
        .expect("set persisted effective registry");
    assert_eq!(
        world
            .resolve_governance_effective_finality_signer_registry()
            .expect("resolve effective registry")
            .expect("effective registry")
            .threshold_bps,
        6667
    );
    save_authority_world(&world, world_dir.as_path());

    let expected_registry_semantic_sha256 = semantic_registry_sha256(
        &super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("reload fixture registry"),
    )
    .expect("fixture semantic digest");
    let binding = load_runtime_authority_binding(
        world_dir.as_path(),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect("same governed authority must survive persisted normalization")
    .expect("binding present");
    assert_eq!(
        binding.registry_semantic_sha256,
        expected_registry_semantic_sha256
    );
}

#[test]
fn accepts_registry_only_authority_for_non_managed_observer() {
    let (registry, manifest_path) = write_observer_authority_fixture();
    let mut loaded = loaded_manifest(manifest_path.as_path());
    loaded.manifest.validator_policy.target_validator_count = 2;
    let binding = load_runtime_authority_binding_for_node(
        manifest_path
            .parent()
            .expect("observer fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-local",
        NodeRole::Observer,
        Some(registry.as_path()),
        None,
        Some(&loaded),
    )
    .expect("observer registry-only authority should pass")
    .expect("observer authority binding");
    assert_eq!(
        binding.registry_ref,
        fs::canonicalize(registry.as_path())
            .expect("canonical registry path")
            .to_string_lossy()
    );
    assert_eq!(binding.registry_sha256.len(), 64);
    assert_eq!(binding.registry_semantic_sha256.len(), 64);
    assert!(binding.inventory_ref.is_empty());
    assert!(binding.inventory_sha256.is_empty());
    assert_eq!(
        binding.validator_stakes.get("triad-testnet-storage"),
        Some(&50)
    );
    assert_eq!(
        binding
            .validator_signer_public_keys
            .get("triad-testnet-sequencer"),
        Some(&"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string())
    );
}

#[test]
fn managed_triad_observer_role_cannot_use_registry_only_authority() {
    let (registry, manifest_path) = write_observer_authority_fixture();
    let mut loaded = loaded_manifest(manifest_path.as_path());
    loaded.manifest.validator_policy.target_validator_count = 2;
    let error = load_runtime_authority_binding_for_node(
        manifest_path
            .parent()
            .expect("observer fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-validator-47",
        NodeRole::Observer,
        Some(registry.as_path()),
        None,
        Some(&loaded),
    )
    .expect_err("managed triad observer spoof must require inventory");
    assert!(error.contains("--deployment-inventory"));
}

#[test]
fn observer_registry_semantic_digest_drift_fails_closed() {
    let (registry, manifest_path) = write_observer_authority_fixture();
    let manifest = fs::read_to_string(manifest_path.as_path()).expect("read observer manifest");
    fs::write(
        manifest_path.as_path(),
        manifest.replace(
            &semantic_registry_sha256(
                &super::super::governance_registry::load_genesis_finality_registry(
                    registry.as_path(),
                )
                .expect("load observer registry"),
            )
            .expect("observer semantic digest"),
            &"0".repeat(64),
        ),
    )
    .expect("rewrite observer manifest");
    let mut loaded = loaded_manifest(manifest_path.as_path());
    loaded.manifest.validator_policy.target_validator_count = 2;
    let error = load_runtime_authority_binding_for_node(
        manifest_path
            .parent()
            .expect("observer fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-local",
        NodeRole::Observer,
        Some(registry.as_path()),
        None,
        Some(&loaded),
    )
    .expect_err("observer semantic drift must fail closed");
    assert!(error.contains("semantic digest mismatch"));
}

#[test]
fn observer_registry_authority_survives_persisted_world_restart_without_mutation() {
    let (registry, manifest_path) = write_observer_authority_fixture();
    let world_dir = manifest_path
        .parent()
        .expect("observer fixture root")
        .join("world");
    let explicit_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load observer registry");
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world
        .set_governance_finality_signer_registry(explicit_registry)
        .expect("set persisted observer registry");
    save_authority_world(&world, world_dir.as_path());
    let persisted_tree_before = persisted_tree_bytes(world_dir.as_path());

    let mut loaded = loaded_manifest(manifest_path.as_path());
    loaded.manifest.validator_policy.target_validator_count = 2;
    let binding = load_runtime_authority_binding_for_node(
        world_dir.as_path(),
        "triad-testnet-local",
        NodeRole::Observer,
        Some(registry.as_path()),
        None,
        Some(&loaded),
    )
    .expect("persisted observer authority should pass")
    .expect("observer authority binding");
    assert_eq!(binding.validator_stakes.len(), 2);
    assert_eq!(
        persisted_tree_before,
        persisted_tree_bytes(world_dir.as_path()),
        "authority restart must not mutate any persisted world file"
    );
}

#[test]
fn rejects_foreign_node_id_with_governed_triad_inventory() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let error = load_runtime_authority_binding_for_node(
        manifest_path
            .parent()
            .expect("fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-validator-47-shadow",
        NodeRole::Storage,
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("foreign node identity must not use governed triad inventory authority");
    assert!(
        error.contains("managed triad identity"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_persisted_unbound_world_for_public_testnet_authority() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let world_dir = manifest_path.parent().expect("fixture root").join("world");
    let explicit_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load fixture registry");
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world
        .set_governance_finality_signer_registry(explicit_registry)
        .expect("set persisted effective registry");
    world
        .save_to_dir(world_dir.as_path())
        .expect("save legacy unbound world");
    fs::remove_dir_all(world_dir.join(".distfs-state")).expect("remove sidecar for legacy fixture");
    let snapshot_path = world_dir.join("snapshot.json");
    let mut snapshot: serde_json::Value =
        serde_json::from_slice(&fs::read(snapshot_path.as_path()).expect("read snapshot"))
            .expect("parse snapshot");
    snapshot["chain_resource_manifest"]["world_id"] =
        serde_json::Value::String("unbound".to_string());
    fs::write(
        snapshot_path.as_path(),
        serde_json::to_vec_pretty(&snapshot).expect("encode unbound snapshot"),
    )
    .expect("write unbound snapshot");

    let error = load_runtime_authority_binding(
        world_dir.as_path(),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("persisted unbound world must not enter public-testnet authority");
    assert!(error.contains("unbound"), "unexpected error: {error}");
}

#[test]
fn accepts_observer_registry_path_after_canonicalization() {
    let (registry, manifest_path) = write_observer_authority_fixture();
    let mut loaded = loaded_manifest(manifest_path.as_path());
    loaded.manifest.validator_policy.target_validator_count = 2;
    let registry_name = registry.file_name().expect("registry name");
    let noncanonical_registry = registry
        .parent()
        .expect("registry parent")
        .join("..")
        .join(
            registry
                .parent()
                .expect("registry parent")
                .file_name()
                .expect("config name"),
        )
        .join(registry_name);

    let binding = load_runtime_authority_binding_for_node(
        manifest_path
            .parent()
            .expect("observer fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-local",
        NodeRole::Observer,
        Some(noncanonical_registry.as_path()),
        None,
        Some(&loaded),
    )
    .expect("canonicalized observer path should pass")
    .expect("observer authority binding");
    assert_eq!(
        binding.registry_ref,
        fs::canonicalize(registry.as_path())
            .expect("canonical registry path")
            .to_string_lossy()
    );
}

#[test]
fn rejects_authority_when_persisted_world_id_does_not_match_inventory() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let world_dir = manifest_path.parent().expect("fixture root").join("world");
    let explicit_registry =
        super::super::governance_registry::load_genesis_finality_registry(registry.as_path())
            .expect("load fixture registry");
    let mut world = oasis7::runtime_host::world::new_production_hardened();
    world
        .set_governance_finality_signer_registry(explicit_registry)
        .expect("set persisted effective registry");
    world
        .save_to_dir_with_chain_resource_context(
            world_dir.as_path(),
            ChainResourceDerivationContext {
                world_id: "wrong-persisted-world",
                chain_id: "oasis7-public-testnet-governed-20260606",
                genesis_ref: None,
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: 0,
            },
            "fixture-world-config",
            "fixture-generation",
        )
        .expect("save fixture world");
    let persisted_tree_before = persisted_tree_bytes(world_dir.as_path());

    let error = load_runtime_authority_binding(
        world_dir.as_path(),
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("persisted world identity drift must fail closed");
    assert!(error.contains("world id"), "unexpected error: {error}");
    assert_eq!(
        persisted_tree_before,
        persisted_tree_bytes(world_dir.as_path()),
        "persisted world identity rejection must not mutate any persisted world file"
    );
}

#[test]
fn rejects_authority_when_effective_runtime_world_id_override_drifts() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let error = load_runtime_authority_binding_for_node_with_world_id(
        manifest_path
            .parent()
            .expect("fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-sequencer",
        NodeRole::Sequencer,
        "operator-selected-different-world",
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("effective world-id override must not bypass governed authority");
    assert!(
        error.contains("effective runtime world id"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_managed_node_with_wrong_runtime_role() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let error = load_runtime_authority_binding_for_node(
        manifest_path
            .parent()
            .expect("fixture root")
            .join("world")
            .as_path(),
        "triad-testnet-storage",
        NodeRole::Sequencer,
        Some(registry.as_path()),
        Some(inventory.as_path()),
        Some(&loaded_manifest(manifest_path.as_path())),
    )
    .expect_err("managed storage identity must not run as sequencer");
    assert!(error.contains("runtime role"), "unexpected error: {error}");
}

#[test]
fn accepts_managed_nodes_with_governed_runtime_roles() {
    let (registry, inventory, manifest_path) = write_authority_fixture();
    let loaded = loaded_manifest(manifest_path.as_path());
    for (node_id, node_role) in [
        ("triad-testnet-sequencer", NodeRole::Sequencer),
        ("triad-testnet-storage", NodeRole::Storage),
        ("triad-testnet-validator-47", NodeRole::Storage),
    ] {
        load_runtime_authority_binding_for_node(
            manifest_path
                .parent()
                .expect("fixture root")
                .join(node_id)
                .as_path(),
            node_id,
            node_role,
            Some(registry.as_path()),
            Some(inventory.as_path()),
            Some(&loaded),
        )
        .expect("governed managed role should pass")
        .expect("managed authority binding");
    }
}
