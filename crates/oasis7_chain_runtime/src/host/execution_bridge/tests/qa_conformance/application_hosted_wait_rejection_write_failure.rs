//! Actual App-private atomic-rename failure at the unique Wait cleanup persist.
use super::*;
use std::path::{Path, PathBuf};

#[test]
fn real_tcp_hosted_rejected_wait_cleanup_write_failure_restores_runtime_and_sidecar() {
    application_harness::run_isolated_application_mode(
        false,
        false,
        false,
        true,
        false,
        false,
        "wait-rejection-write-failure",
    );
}

pub(super) fn verify(client: &RemoteWorldServiceClient) {
    // The ordinary listener drives the production pipeline; no direct persistence invocation.
    application_hosted_wait_rejection::verify(client);
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

pub(super) fn start(
    root: PathBuf,
    fixture: &Fixture,
) -> thread::JoinHandle<Result<FaultProof, String>> {
    let trace = fixture.lookup_digests.clone();
    let driver = fixture.driver.clone();
    thread::spawn(move || {
        await_marker(&root, "wait-rejection-before-persist")?;
        let original_trace = trace.lock().unwrap().clone();
        let original_economy = serde_json::to_value(
            driver
                .lock()
                .unwrap()
                .execution_world
                .cognition_economy()
                .unwrap(),
        )
        .unwrap();
        let target = root.join("hosted-wait-private-lineage.json");
        let backup = root.join("wait-rejection-checkpoint-backup.json");
        let original_bytes = fs::read(&target).map_err(|error| error.to_string())?;
        private_write(
            &root.join("wait-rejection-original-checkpoint.json"),
            &original_bytes,
        )?;
        let before = read_json(&root, "wait-rejection-before.json")?;
        let staged = read_json(&root, "wait-rejection-staged.json")?;
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
                &root.join("wait-rejection-release"),
                b"actual target is a directory",
            )?;
            await_marker(&root, "wait-rejection-unpublished")?;
            let after = read_json(&root, "wait-rejection-publication-original.json")?;
            if *trace.lock().unwrap() != original_trace {
                return Err("unpublished rejected Wait dispatched new work".into());
            }
            if serde_json::to_value(
                driver
                    .lock()
                    .unwrap()
                    .execution_world
                    .cognition_economy()
                    .unwrap(),
            )
            .unwrap()
                != original_economy
            {
                return Err("unpublished rejected Wait changed economy".into());
            }
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
            &root.join("wait-rejection-retry-release"),
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
        "pre2-wait-rejection-fs-{}-{}",
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
            "hosted_wait_fs_artifact={} bytes={} blake3={}",
            artifact.join(name).display(),
            bytes.len(),
            blake3::hash(&bytes)
        );
    }
    for name in [
        "wait-rejection-before.json",
        "wait-rejection-staged.json",
        "wait-rejection-publication-original.json",
        "wait-rejection-original-checkpoint.json",
        "hosted-wait-private-lineage.json",
    ] {
        if let Ok(bytes) = fs::read(root.join(name)) {
            private_write(&artifact.join(name), &bytes).unwrap();
            println!(
                "hosted_wait_fs_artifact={} bytes={} blake3={}",
                artifact.join(name).display(),
                bytes.len(),
                blake3::hash(&bytes)
            );
        }
    }
    println!("{}", String::from_utf8_lossy(&output.stdout));
    println!("{}", String::from_utf8_lossy(&output.stderr));
    let proof = proof.expect("real atomic-rename failure and filesystem restoration");
    let original_preserved = root.join("wait-rejection-original-published").is_file();
    let staged_changed = proof.before != proof.staged;
    let actual_failure = root.join("wait-rejection-unpublished").is_file();
    let snapshots = proof
        .before
        .as_object()
        .expect("actual transaction snapshot");
    println!(
        "hosted_wait_actual_fs_failure marker={actual_failure} original_preserved={original_preserved} staged_changed={staged_changed} compared_fields={} original_checkpoint_blake3={} actual_worker_joined=true",
        snapshots.len(),
        blake3::hash(&proof.original_bytes)
    );
    assert!(
        actual_failure && original_preserved && staged_changed,
        "actual unpublished failure must retain and publish the same original after repair"
    );
    for field in [
        "proposals",
        "pending_wakes",
        "resume",
        "wait",
        "restored_resume",
        "admission",
        "restored_original",
        "restored_queued_waits",
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
    assert_eq!(proof.staged["runtime_ledgers"].as_array().unwrap().len(), 5);
    for (snapshot, persisted) in [
        ("proposals", "provider_continuation_proposals"),
        ("pending_wakes", "pending_runtime_wakes"),
        ("wait", "hosted_wait"),
        ("resume", "hosted_resume"),
        ("admission", "hosted_admission"),
        ("leases", "provider_cognition_leases"),
        ("completed_queue", "provider_completed_decisions"),
        ("service_pending", "provider_service_pending"),
        ("scheduler_pending", "provider_scheduler_pending"),
        ("active_turns", "provider_active_turns"),
        ("contexts", "provider_contexts"),
        ("retry_contexts", "provider_retry_contexts"),
        ("held_decisions", "provider_held_decisions"),
        ("session_ids", "provider_session_ids"),
        ("context_seq", "provider_context_seq"),
        ("memory", "provider_memory_store"),
        ("terminal_states", "provider_terminal_states"),
    ] {
        assert!(
            proof.after[persisted] == proof.before[snapshot]
                || proof.after[persisted] == proof.staged[snapshot],
            "captured rejected Wait cannot introduce another identity/value: {snapshot}"
        );
    }
    assert_ne!(
        proof.before["runtime_ledgers"],
        proof.staged["runtime_ledgers"]
    );
    application_hosted_wait_rejection_recovery::assert_original_terminal(fixture, root);
    println!("PRE2_REJECTED_WAIT_RUNTIME_AND_SIDECAR_OWNED_RETRY_PASSED runtime_ledgers=5");
}
