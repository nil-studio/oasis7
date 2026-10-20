//! Exercise the shipping descriptor against real World, execution proof and CAS bytes.
use super::super::{local_io as io, model, tests::Fixture};
use super::describe_with_minimum;
use crate::execution_bridge_real_tests::real_execution_bridge as bridge;
use oasis7::world_service::WorldIdentity;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_runtime::{
    BlobStore, ChainResourceDerivationContext, LocalCasStore, ReleaseSecurityPolicy,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct PersistedFixture {
    _identity_fixture: Fixture,
    config: model::NodeConfig,
    world: PathBuf,
    records: PathBuf,
}
impl PersistedFixture {
    fn new() -> Self {
        let identity_fixture = Fixture::new();
        let config = identity_fixture.initialize();
        let state = config.identity.data_dir.join("state");
        let world_dir = state.join("world");
        let records = state.join("records");
        let world_id = config.identity.chain.world_id();
        let mut world = oasis7::runtime_host::world::new();
        world.step().expect("actual World tick");
        world
            .bind_cognition_runtime(world_id, "main", 0, None, "pending", 0)
            .expect("bind actual World runtime");
        let genesis = format!("sha256:{}", config.identity.chain.genesis_sha256().unwrap());
        world
            .save_to_dir_with_chain_resource_context(
                &world_dir,
                ChainResourceDerivationContext {
                    world_id,
                    chain_id: config.identity.chain.chain_id(),
                    genesis_ref: Some(&genesis),
                    created_at_height: 1,
                    manifest_height: 1,
                    commit_block_hash: None,
                    tick: 1,
                },
                hex::encode([0x11; 32]),
                hex::encode([0x22; 32]),
            )
            .expect("persist real World layout");
        io::write(
            &world_dir.join("world-service-identity.json"),
            &WorldIdentity {
                world_id: world_id.into(),
                genesis_digest: genesis,
            },
        )
        .unwrap();
        let bootstrap = bridge::derive_local_execution_bootstrap(
            &world_dir,
            &records,
            world_id,
            1,
            "compatibility-fixture-boundary",
            &ReleaseSecurityPolicy::default(),
        )
        .expect("derive actual persisted World bootstrap");
        let mut driver = bridge::NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            state.join("execution.json"),
            world_dir.clone(),
            records.clone(),
            state.join("cas"),
            &oasis7_proto::storage_profile::StorageProfileConfig::default(),
            bootstrap,
        )
        .expect("create shipping driver");
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: world_id.into(),
                node_id: "compatibility-node".into(),
                proposer_id: "compatibility-node".into(),
                height: 2,
                slot: 2,
                epoch: 0,
                node_block_hash: "compatibility-node-h2".into(),
                action_root: compute_consensus_action_root(&[]).unwrap(),
                committed_actions: Vec::new(),
                committed_at_unix_ms: 2_000,
            })
            .expect("publish real execution proof and Snapshot/Journal CAS closure");
        Self {
            _identity_fixture: identity_fixture,
            config,
            world: world_dir,
            records,
        }
    }
    fn describe(&self) -> Result<serde_json::Value, String> {
        let before = inventory(&self.config.identity.data_dir);
        let result = describe_with_minimum(
            &self.config.identity.data_dir.join(model::CONFIG_FILE),
            Some(&self.config.identity.data_dir),
            None,
            None,
        );
        assert_eq!(
            before,
            inventory(&self.config.identity.data_dir),
            "descriptor must preserve every original persisted byte and directory entry"
        );
        result
    }
}
fn inventory(root: &Path) -> Vec<(PathBuf, Option<String>)> {
    let mut pending = vec![root.to_path_buf()];
    let mut output = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(!metadata.file_type().is_symlink());
        let digest = if metadata.is_dir() {
            pending.extend(fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            None
        } else {
            Some(io::sha256(&path).unwrap())
        };
        output.push((path.strip_prefix(root).unwrap().to_path_buf(), digest));
    }
    output.sort();
    output
}

#[test]
fn shipping_compatibility_descriptor_verifies_real_world_checkpoint_and_cas_without_writes() {
    let fixture = PersistedFixture::new();
    let result = fixture.describe().expect("real valid checkpoint accepted");
    let probe = &result["stored_state_probe"];
    assert_eq!(probe["state"], "verified");
    assert_eq!(probe["execution_history_verified"], true);
    assert_eq!(probe["execution_head"]["height"], 2);
    assert_ne!(
        probe["execution_head"]["decoded_world_snapshot_root"],
        probe["execution_head"]["execution_state_root"],
        "the real post-commit materialized cache has a different resource context"
    );
    assert_eq!(
        probe["execution_head"]["pinned_world_snapshot_root"],
        probe["execution_head"]["execution_state_root"],
        "the independently decoded authoritative CAS cut remains exact"
    );
    assert_eq!(
        probe["execution_head"]["world_cache_matches_execution_head"],
        false
    );
    assert_eq!(
        probe["execution_head"]["head_source"],
        "verified_record_and_snapshot_journal_cas"
    );
    assert_eq!(result["process_started"], false);
    assert_eq!(result["chain_authority_claim"], false);
}

#[test]
fn shipping_compatibility_descriptor_rejects_valid_cas_snapshot_outside_exact_record_cut() {
    let fixture = PersistedFixture::new();
    fixture
        .describe()
        .expect("establish valid real fixture before substitution");
    let world = oasis7::runtime_host::world::load_from_dir(&fixture.world).unwrap();
    let substituted = serde_cbor::to_vec(&world.snapshot()).unwrap();
    let snapshot_root = oasis7_runtime::blake3_hex(&substituted);
    let latest_path = fixture.records.join("latest.json");
    let mut record: bridge::ExecutionBridgeRecord = io::read(&latest_path).unwrap();
    assert_ne!(
        snapshot_root, record.execution_state_root,
        "substitution must change the authoritative execution root"
    );
    // Store genuine, decodable snapshot bytes at their real content address.
    // Both record pointers select that object, while the accepted root and
    // its independently stored WorldHeadProof remain bound to the old cut.
    let store = LocalCasStore::new(fixture.config.identity.data_dir.join("state/cas"));
    let substituted_ref = store.put_bytes(&substituted).unwrap();
    assert_eq!(store.get_verified(&substituted_ref).unwrap(), substituted);
    record.snapshot_ref = Some(substituted_ref.clone());
    record.latest_state_ref = Some(substituted_ref);
    io::write(&latest_path, &record).unwrap();
    io::write(
        &fixture.records.join(format!("{:020}.json", record.height)),
        &record,
    )
    .unwrap();
    let error = fixture.describe().unwrap_err();
    assert!(
        error.contains("pinned snapshot/journal root or boundary mismatch"),
        "{error}"
    );
}

#[test]
fn shipping_compatibility_descriptor_rejects_unknown_and_lowered_world_selectors() {
    let fixture = PersistedFixture::new();
    fixture
        .describe()
        .expect("establish valid real fixture before selector mutation");
    let path = fixture.world.join("world-format.json");
    let original: serde_json::Value = io::read(&path).unwrap();
    for version in [0, 999] {
        let mut selector = original.clone();
        selector["format_version"] = version.into();
        io::write(&path, &selector).unwrap();
        let error = fixture.describe().unwrap_err();
        assert!(
            error.contains("unsupported World persistence selector"),
            "{error}"
        );
    }
}

#[test]
fn shipping_compatibility_descriptor_refuses_latest_below_unreadable_higher_record() {
    let fixture = PersistedFixture::new();
    fixture
        .describe()
        .expect("establish valid real checkpoint before adding higher record");
    fs::write(
        fixture.records.join("00000000000000000003.json"),
        b"unknown future schema",
    )
    .unwrap();
    let error = fixture.describe().unwrap_err();
    assert!(
        error.contains("not the highest declared numbered record"),
        "{error}"
    );
}
