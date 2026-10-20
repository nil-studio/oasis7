use super::super::{checkpoint, driver::NodeRuntimeExecutionDriver};
use super::*;
use oasis7_node::{NodeExecutionCommitContext, NodeExecutionHook, compute_consensus_action_root};
use oasis7_runtime::{Action, BlobStore};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    driver: NodeRuntimeExecutionDriver,
    cache: Mutex<restore_cache::RestoreCache>,
    identity: WorldIdentity,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oasis7-pin-cache-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let mut driver = NodeRuntimeExecutionDriver::new(
            root.join("state.json"),
            root.join("world"),
            root.join("records"),
            root.join("store"),
        )
        .unwrap();
        driver
            .execution_world
            .bind_cognition_runtime("w1", "main", 0, None, "pending", 0)
            .unwrap();
        let mut fixture = Self {
            root,
            driver,
            cache: Mutex::new(restore_cache::RestoreCache::default()),
            identity: WorldIdentity {
                world_id: "w1".into(),
                genesis_digest: "explicit-test-genesis".into(),
            },
        };
        fixture.commit(1);
        fixture
    }
    fn commit(&mut self, height: u64) {
        self.driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".into(),
                node_id: "node-a".into(),
                proposer_id: "node-a".into(),
                height,
                slot: height,
                epoch: 0,
                node_block_hash: format!("test-node-{height}"),
                action_root: compute_consensus_action_root(&[]).unwrap(),
                committed_actions: Vec::new(),
                committed_at_unix_ms: height as i64 * 1000,
            })
            .unwrap();
    }
    fn pin(&self, height: u64) -> Result<PinnedWorld, String> {
        load_with_cache(
            &self.root.join("records"),
            &self.root.join("store"),
            &self.identity,
            Some(height),
            None,
            &self.cache,
        )
    }
    fn record_path(&self, height: u64) -> PathBuf {
        checkpoint::execution_bridge_record_path(&self.root.join("records"), height)
    }
    fn record(&self, height: u64) -> ExecutionBridgeRecord {
        checkpoint::load_execution_bridge_record(&self.record_path(height)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn warm_restore_still_requires_each_real_cas_blob_and_record() {
    let fixture = Fixture::new();
    let baseline = fixture.pin(1).unwrap();
    let record = fixture.record(1);
    for reference in [
        record.snapshot_ref.as_ref().unwrap(),
        record.journal_ref.as_ref().unwrap(),
        record.world_head_proof_ref.as_ref().unwrap(),
    ] {
        let path = fixture
            .root
            .join("store/blobs")
            .join(format!("{reference}.blob"));
        let original = fs::read(&path).unwrap();
        fs::remove_file(&path).unwrap();
        assert!(
            fixture.pin(1).is_err(),
            "warm cache cannot mask GC/deleted CAS"
        );
        fs::write(&path, b"corrupt-cas").unwrap();
        assert!(
            fixture.pin(1).is_err(),
            "warm cache cannot mask corrupted CAS"
        );
        fs::write(&path, original).unwrap();
        assert_eq!(fixture.pin(1).unwrap().commit, baseline.commit);
    }
    let path = fixture.record_path(1);
    let original = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert!(fixture.pin(1).is_err());
    fs::write(&path, b"malformed-record").unwrap();
    assert!(fixture.pin(1).is_err());
    fs::write(&path, original).unwrap();
    assert_eq!(fixture.pin(1).unwrap().commit, baseline.commit);
}

#[test]
fn warm_restore_cannot_hide_proof_identity_fixed_or_current_head_changes() {
    let mut fixture = Fixture::new();
    let baseline = fixture.pin(1).unwrap();
    let mut returned = fixture.pin(1).unwrap();
    returned.world.submit_action(Action::RegisterAgent {
        agent_id: "consumer-only".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    returned.world.step().unwrap();
    assert!(
        !fixture
            .pin(1)
            .unwrap()
            .world
            .state()
            .agents
            .contains_key("consumer-only")
    );
    let mut wrong_identity = fixture.identity.clone();
    wrong_identity.world_id = "another-world".into();
    assert!(
        load_with_cache(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &wrong_identity,
            Some(1),
            None,
            &fixture.cache
        )
        .is_err()
    );
    let mut wrong_fixed = baseline.commit.clone();
    wrong_fixed.world.genesis_digest = "other-genesis".into();
    assert!(
        load_with_cache(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            Some(1),
            Some(&wrong_fixed),
            &fixture.cache
        )
        .is_err()
    );
    wrong_fixed = baseline.commit.clone();
    wrong_fixed.binding.reorg_generation += 1;
    assert!(
        load_with_cache(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            Some(1),
            Some(&wrong_fixed),
            &fixture.cache
        )
        .is_err()
    );
    let path = fixture.record_path(1);
    let original = fs::read(&path).unwrap();
    let mut record = fixture.record(1);
    record.execution_block_hash = "wrong-execution-hash".into();
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(
        fixture.pin(1).is_err(),
        "proof/record exact binding remains mandatory"
    );
    fs::write(&path, original).unwrap();
    fixture.commit(2);
    let latest = load_with_cache(
        &fixture.root.join("records"),
        &fixture.root.join("store"),
        &fixture.identity,
        None,
        None,
        &fixture.cache,
    )
    .unwrap();
    assert_eq!(latest.commit.position, 2);
    assert_ne!(latest.commit, baseline.commit);
    assert_eq!(fixture.pin(1).unwrap().commit, baseline.commit);
}

#[test]
fn alternate_hash_valid_proof_and_invalid_snapshot_fail_after_warm_restore() {
    let fixture = Fixture::new();
    fixture.pin(1).unwrap();
    let original = fixture.record(1);
    let store = LocalCasStore::new(fixture.root.join("store"));
    let mut proof: WorldHeadProofV1 = serde_cbor::from_slice(
        &store
            .get_verified(original.world_head_proof_ref.as_ref().unwrap())
            .unwrap(),
    )
    .unwrap();
    proof.execution.node_block_hash = "different-node-block".into();
    let mut changed = original.clone();
    changed.world_head_proof_ref = Some(
        store
            .put_bytes(&serde_cbor::to_vec(&proof).unwrap())
            .unwrap(),
    );
    changed.world_head_proof_hash = Some(proof.proof_hash().unwrap());
    fs::write(
        fixture.record_path(1),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(fixture.pin(1).is_err());
    fs::write(
        fixture.record_path(1),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    assert!(fixture.pin(1).is_ok());
    // Hash-valid malformed artifact cannot be replaced by an unrelated warm restore.
    changed = original;
    changed.snapshot_ref = Some(store.put_bytes(b"not-a-snapshot").unwrap());
    changed.latest_state_ref = changed.snapshot_ref.clone();
    changed.execution_state_root = changed.snapshot_ref.clone().unwrap();
    fs::write(
        fixture.record_path(1),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    assert!(fixture.pin(1).is_err());
}

#[test]
fn compacted_historical_record_is_unavailable_while_partial_or_corrupt_fails_closed() {
    let fixture = Fixture::new();
    assert!(
        pin_retained_at_height(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            1
        )
        .unwrap()
        .is_some()
    );
    assert!(
        pin_retained_at_height(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            2
        )
        .unwrap()
        .is_none()
    );
    let mut record = fixture.record(1);
    record.snapshot_ref = None;
    record.latest_state_ref = None;
    record.journal_ref = None;
    checkpoint::persist_execution_bridge_record_only(&fixture.root.join("records"), &record)
        .unwrap();
    let result = pin_retained_at_height(
        &fixture.root.join("records"),
        &fixture.root.join("store"),
        &fixture.identity,
        1,
    );
    assert!(
        matches!(result, Ok(None)),
        "compacted record header is not a transient service error"
    );
    let wrong_world = WorldIdentity {
        world_id: "other".into(),
        genesis_digest: fixture.identity.genesis_digest.clone(),
    };
    assert!(
        pin_retained_at_height(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &wrong_world,
            1
        )
        .is_err()
    );
    record.snapshot_ref = Some(record.execution_state_root.clone());
    checkpoint::persist_execution_bridge_record_only(&fixture.root.join("records"), &record)
        .unwrap();
    assert!(
        pin_retained_at_height(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            1
        )
        .is_err()
    );
}

#[test]
fn retained_history_material_rejects_malformed_record_and_missing_cas() {
    let fixture = Fixture::new();
    let path = fixture.record_path(1);
    let original = fs::read(&path).unwrap();
    fs::write(&path, b"{").unwrap();
    assert!(
        pin_retained_at_height(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            1
        )
        .is_err()
    );
    fs::write(&path, original).unwrap();
    let mut record = fixture.record(1);
    record.journal_ref = Some("0".repeat(64));
    checkpoint::persist_execution_bridge_record_only(&fixture.root.join("records"), &record)
        .unwrap();
    assert!(
        pin_retained_at_height(
            &fixture.root.join("records"),
            &fixture.root.join("store"),
            &fixture.identity,
            1
        )
        .is_err()
    );
}
