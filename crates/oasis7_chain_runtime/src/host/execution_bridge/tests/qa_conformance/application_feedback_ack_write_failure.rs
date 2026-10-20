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
            await_marker(&root, "feedback-ack-rollback")?;
            let after = json(&root, "feedback-ack-after.json")?;
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
    let actual_error = json(root, "feedback-ack-rollback").unwrap();
    let error = actual_error["actual_error"].as_str().unwrap();
    assert!(
        error.contains("provider lineage checkpoint commit failed"),
        "must be actual checkpoint atomic commit error, got {error}"
    );
    assert_eq!(proof.before["model_calls"], 1);
    assert_eq!(proof.staged["model_calls"], 1);
    assert_eq!(proof.after["model_calls"], 1);
    assert_ne!(
        proof.before, proof.staged,
        "real consumption must stage memory/native changes"
    );
    assert_eq!(
        proof.before, proof.after,
        "all actual native and private sidecar ledgers must roll back"
    );
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
        "PRE2_PRIVATE_MEMORY_ACK_REAL_FS_ROLLBACK_PASSED native_ledgers=5 original_bytes_blake3={}",
        blake3::hash(&proof.original)
    );
}
