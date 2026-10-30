//! Real filesystem failure at the sole native-consumption/ACK checkpoint write.
//! Registered only once the matching test-tier serving probe is connected.
use super::*;
use std::path::{Path, PathBuf};
#[test]
fn real_tcp_private_memory_ack_write_failure_restores_native_and_sidecar_ledgers() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "memory-ack-write-failure",
    );
}
fn await_marker(root: &Path, name: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join(name).exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    root.join(name)
        .exists()
        .then_some(())
        .ok_or_else(|| format!("actual ACK checkpoint marker missing: {name}"))
}
fn json(root: &Path, name: &str) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&fs::read(root.join(name)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub(super) struct Proof {
    before: serde_json::Value,
    staged: serde_json::Value,
    after: serde_json::Value,
    original: Vec<u8>,
    trace: Vec<serde_json::Value>,
    economy: serde_json::Value,
}
pub(super) fn start(root: PathBuf, fixture: &Fixture) -> thread::JoinHandle<Result<Proof, String>> {
    let gate = fixture.world_gate.clone();
    let driver = fixture.driver.clone();
    thread::spawn(move || {
        await_marker(&root, "feedback-ack-before-persist")?;
        let trace = gate.feedback_ack_trace();
        let economy = serde_json::to_value(
            driver
                .lock()
                .unwrap()
                .execution_world
                .cognition_economy()
                .unwrap(),
        )
        .unwrap();
        let target = root.join("native-memory-lineage.json");
        let backup = root.join("feedback-ack-original-lineage.json");
        let original = fs::read(&target).map_err(|e| e.to_string())?;
        let before = json(&root, "feedback-ack-before.json")?;
        let staged = json(&root, "feedback-ack-staged.json")?;
        fs::rename(&target, &backup).map_err(|e| e.to_string())?;
        if let Err(e) = fs::create_dir(&target) {
            fs::rename(&backup, &target).map_err(|e| e.to_string())?;
            return Err(e.to_string());
        }
        let fault = (|| {
            fs::write(
                root.join("feedback-ack-persist-release"),
                b"real target directory",
            )
            .map_err(|e| e.to_string())?;
            await_marker(&root, "feedback-ack-unpublished")?;
            let after = json(&root, "feedback-ack-publication-original.json")?;
            if gate.feedback_ack_trace() != trace {
                return Err("unpublished ACK checkpoint cannot dispatch an ACK".into());
            }
            let held_economy = serde_json::to_value(
                driver
                    .lock()
                    .unwrap()
                    .execution_world
                    .cognition_economy()
                    .unwrap(),
            )
            .unwrap();
            if held_economy != economy {
                return Err("unpublished ACK checkpoint changed economic state".into());
            }
            if fs::read(&backup).map_err(|e| e.to_string())? != original {
                return Err("real original checkpoint changed on failed consumption".into());
            }
            Ok(Proof {
                before,
                staged,
                after,
                original,
                trace,
                economy,
            })
        })();
        fs::remove_dir(&target).map_err(|e| e.to_string())?;
        fs::rename(&backup, &target).map_err(|e| e.to_string())?;
        fs::write(
            root.join("feedback-ack-retry-release"),
            b"original target restored",
        )
        .map_err(|e| e.to_string())?;
        fault
    })
}
pub(super) fn finish(
    fixture: &Fixture,
    root: &Path,
    output: &std::process::Output,
    worker: thread::JoinHandle<Result<Proof, String>>,
) {
    let proof = worker
        .join()
        .unwrap()
        .expect("real atomic checkpoint failure/restore");
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "actual memory rollback serving failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(root.join("feedback-ack-unpublished").is_file());
    assert!(
        root.join("feedback-ack-original-published").is_file(),
        "same captured original must publish after filesystem repair"
    );
    assert_eq!(proof.before["model_calls"], 1);
    assert_eq!(proof.staged["model_calls"], 1);
    assert_ne!(
        proof.before, proof.staged,
        "real consumption must stage memory/native changes"
    );
    // Deferred capture retains prepared state rather than rolling native state
    // back after the later worker failure. The exact encoded original remains
    // immutable and no ACK/economic work can pass its publication fence.
    for (snapshot, persisted) in [
        ("memory", "provider_memory_store"),
        ("pending", "provider_service_pending"),
    ] {
        assert!(
            proof.after[persisted] == proof.before[snapshot]
                || proof.after[persisted] == proof.staged[snapshot],
            "retained ACK capsule must come from this original prepared transaction"
        );
    }
    assert_eq!(proof.before["runtime_ledgers"].as_array().unwrap().len(), 5);
    assert_ne!(
        proof.before["runtime_ledgers"],
        proof.staged["runtime_ledgers"]
    );
    assert!(
        !proof.staged["memory"]["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let restored: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("native-memory-lineage.json")).unwrap())
            .unwrap();
    assert_eq!(
        restored["provider_memory_store"], proof.staged["memory"],
        "same original native memory must persist exactly once"
    );
    assert!(
        restored["provider_service_pending"]
            .as_object()
            .unwrap()
            .is_empty()
    );
    let ack = &proof.staged["pending"]["agent-a"]["feedback_ack"];
    application_feedback_ack_recovery::assert_full_ack_readback(fixture, ack);
    application_feedback_ack_recovery::assert_recovery_delta(
        fixture,
        &proof.trace,
        ack,
        &proof.staged["pending"]["agent-a"],
        &proof.economy,
    );
    application_feedback_ack_recovery::assert_initial_economic_requests(fixture, 1, 1);
    println!(
        "PRE2_PRIVATE_MEMORY_ACK_REAL_FS_OWNED_RETRY_PASSED native_ledgers=5 original_bytes_blake3={}",
        blake3::hash(&proof.original)
    );
}
