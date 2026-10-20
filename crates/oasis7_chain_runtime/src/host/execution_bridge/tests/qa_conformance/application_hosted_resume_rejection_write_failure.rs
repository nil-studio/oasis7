//! Actual App-private atomic-rename failure at the unique Resume rejection cleanup persist.
use super::*;
use std::path::{Path, PathBuf};

#[test]
fn real_tcp_hosted_rejected_resume_rejection_cleanup_write_failure_restores_runtime_and_sidecar() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "resume-rejection-write-failure",
    );
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
fn await_marker(root: &Path, name: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join(name).exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    if root.join(name).exists() {
        Ok(())
    } else {
        Err(format!("actual cleanup marker absent: {name}"))
    }
}
fn read_json(root: &Path, name: &str) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&fs::read(root.join(name)).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}
pub(super) struct FaultProof {
    before: serde_json::Value,
    staged: serde_json::Value,
    after: serde_json::Value,
    original_bytes: Vec<u8>,
}

pub(super) fn start(root: PathBuf) -> thread::JoinHandle<Result<FaultProof, String>> {
    thread::spawn(move || {
        await_marker(&root, "resume-rejection-before-persist")?;
        let target = root.join("hosted-wait-private-lineage.json");
        let backup = root.join("resume-rejection-checkpoint-backup.json");
        let original_bytes = fs::read(&target).map_err(|error| error.to_string())?;
        private_write(
            &root.join("resume-rejection-original-checkpoint.json"),
            &original_bytes,
        )?;
        let before = read_json(&root, "resume-rejection-before.json")?;
        let staged = read_json(&root, "resume-rejection-staged.json")?;
        fs::rename(&target, &backup).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&backup, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::create_dir(&target) {
            fs::rename(&backup, &target).map_err(|restore| restore.to_string())?;
            return Err(error.to_string());
        }
        let fault = (|| {
            private_write(
                &root.join("resume-rejection-release"),
                b"actual target is a directory",
            )?;
            await_marker(&root, "resume-rejection-rollback")?;
            let after = read_json(&root, "resume-rejection-after.json")?;
            if fs::read(&backup).map_err(|error| error.to_string())? != original_bytes {
                return Err("actual original checkpoint backup changed".into());
            }
            Ok(FaultProof {
                before,
                staged,
                after,
                original_bytes,
            })
        })();
        // Restore the real original target before permitting any retry, including failure cleanup.
        fs::remove_dir(&target).map_err(|error| error.to_string())?;
        fs::rename(&backup, &target).map_err(|error| error.to_string())?;
        private_write(
            &root.join("resume-rejection-retry-release"),
            b"actual checkpoint target restored",
        )?;
        fault
    })
}

pub(super) fn finish(
    fixture: &Fixture,
    root: &Path,
    output: &std::process::Output,
    worker: thread::JoinHandle<Result<FaultProof, String>>,
) {
    let proof = worker.join().expect("actual fault worker must join");
    let artifact = std::env::temp_dir().join(format!(
        "pre2-resume-rejection-fs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&artifact).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&artifact, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let canonical = fixture
        .driver
        .lock()
        .unwrap()
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for (name, bytes) in [
        ("stdout.log", output.stdout.clone()),
        ("stderr.log", output.stderr.clone()),
        ("exit-status.txt", output.status.to_string().into_bytes()),
        (
            "canonical-results.json",
            serde_json::to_vec(&canonical).unwrap(),
        ),
    ] {
        private_write(&artifact.join(name), &bytes).unwrap();
        println!(
            "hosted_resume_fs_artifact={} bytes={} blake3={}",
            artifact.join(name).display(),
            bytes.len(),
            blake3::hash(&bytes)
        );
    }
    for name in [
        "resume-rejection-before.json",
        "resume-rejection-staged.json",
        "resume-rejection-after.json",
        "resume-rejection-original-checkpoint.json",
        "hosted-wait-private-lineage.json",
    ] {
        if let Ok(bytes) = fs::read(root.join(name)) {
            private_write(&artifact.join(name), &bytes).unwrap();
            println!(
                "hosted_resume_fs_artifact={} bytes={} blake3={}",
                artifact.join(name).display(),
                bytes.len(),
                blake3::hash(&bytes)
            );
        }
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
    let proof = proof.expect("real atomic-rename failure and filesystem restoration");
    let exact_rollback = proof.before == proof.after;
    let staged_changed = proof.before != proof.staged;
    let actual_failure = String::from_utf8_lossy(&output.stderr)
        .contains("hosted_resume_rejection_persistence_failed");
    let snapshots = proof
        .before
        .as_object()
        .expect("actual transaction snapshot");
    println!(
        "hosted_wait_actual_fs_failure marker={actual_failure} exact_rollback={exact_rollback} staged_changed={staged_changed} compared_fields={} original_checkpoint_blake3={} actual_worker_joined=true",
        snapshots.len(),
        blake3::hash(&proof.original_bytes)
    );
    assert!(
        actual_failure && exact_rollback && staged_changed,
        "actual atomic failure must roll back the whole real transaction snapshot"
    );
    assert_eq!(
        snapshots.len(),
        18,
        "all actual Resume sidecar fields and Runtime ledgers"
    );
    for field in [
        "proposals",
        "pending_wakes",
        "resume",
        "admission",
        "restored_original",
        "leases",
        "completed_queue",
        "service_pending",
        "scheduler_pending",
        "active_turns",
        "contexts",
        "retry_contexts",
        "held_decisions",
        "session_ids",
        "context_seq",
        "memory",
    ] {
        assert!(
            snapshots.contains_key(field),
            "missing actual invariant snapshot field {field}"
        );
    }
    assert!(snapshots.contains_key("terminal_states"));
    let ledgers = proof.before["runtime_ledgers"]
        .as_array()
        .expect("actual five Runtime ledger digests");
    assert_eq!(ledgers.len(), 5);
    assert_eq!(
        proof.before["runtime_ledgers"],
        proof.after["runtime_ledgers"]
    );
    assert_ne!(
        proof.before["runtime_ledgers"],
        proof.staged["runtime_ledgers"]
    );
    application_hosted_resume_rejection_recovery::assert_original_terminal(fixture, root);
    println!("PRE2_REJECTED_RESUME_RUNTIME_AND_SIDECAR_ROLLBACK_PASSED runtime_ledgers=5");
}
