use super::*;
use crate::node_lifecycle::local_io as io;
use std::{fs, path::PathBuf, process::Command};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir()
            .canonicalize()
            .expect("canonical temporary root")
            .join(format!(
                "o7-owner-attempt-test-{}",
                io::random_id().expect("scratch id")
            ));
        io::private_dir(&path).expect("private scratch directory");
        Self(path)
    }

    fn owner_path(&self) -> PathBuf {
        self.0.join("owner.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(pid: u32, incarnation: &str, config: &str) -> FormalRuntimeOwnerSnapshotV1 {
    FormalRuntimeOwnerSnapshotV1 {
        api_version: OWNER_API.into(),
        instance: super::super::FormalRuntimeInstanceV1 {
            api_version: OWNER_API.into(),
            operation_id: "healthy:test-operation".into(),
            request_digest: "11".repeat(32),
            incarnation: incarnation.into(),
            config_digest: config.into(),
            node_id: "test-node".into(),
            world_id: "test-world".into(),
            chain_id: "test-chain".into(),
            authority_id: "22".repeat(32),
            authority_epoch: 1,
            profile_version: 2,
            process_id: pid,
        },
        state: "failed".into(),
        last_error: Some("simulated startup failure".into()),
    }
}

fn owner_record(
    snapshot: FormalRuntimeOwnerSnapshotV1,
    attempt: u64,
    process_start_identity: Option<String>,
    attempt_history_digest: Option<String>,
) -> OwnerRecord {
    OwnerRecord {
        api_version: OWNER_API.into(),
        snapshot,
        attempt,
        process_start_identity,
        attempt_history_digest,
    }
}

#[test]
fn current_process_birth_matches_and_wrong_birth_never_targets_the_live_pid() {
    let pid = std::process::id();
    let actual = current_process_start_identity().expect("current process identity");
    let exact = owner_record(
        snapshot(pid, "current", &"33".repeat(32)),
        1,
        Some(actual.clone()),
        None,
    );
    assert_eq!(process_exit_observation(&exact).unwrap(), None);

    let mismatch = owner_record(
        snapshot(pid, "wrong-birth", &"33".repeat(32)),
        1,
        Some(format!("different:{actual}")),
        None,
    );
    assert_eq!(
        process_exit_observation(&mismatch).unwrap().as_deref(),
        Some("pid_reused_with_different_start_identity")
    );
    assert_eq!(
        process_start_identity(pid).unwrap().as_deref(),
        Some(actual.as_str())
    );
}

#[test]
fn legacy_live_pid_without_birth_identity_remains_fail_closed() {
    let record = owner_record(
        snapshot(std::process::id(), "legacy", &"33".repeat(32)),
        1,
        None,
        None,
    );
    assert_eq!(process_exit_observation(&record).unwrap(), None);
    assert!(retry_exit_observation(&record).is_err());
}

#[test]
fn killed_os_child_is_confirmed_gone_by_pid_and_birth_identity() {
    let mut child = Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn exact owner child");
    let pid = child.id();
    let birth = process_start_identity(pid)
        .expect("query child identity")
        .expect("child is live");
    let record = owner_record(
        snapshot(pid, "child-attempt", &"33".repeat(32)),
        1,
        Some(birth),
        None,
    );
    child.kill().expect("kill exact child handle");
    child.wait().expect("reap exact child");
    assert!(process_exit_observation(&record).unwrap().is_some());
    assert!(retry_exit_observation(&record).is_ok());
}

#[test]
fn failed_attempt_rejects_changed_config_without_replacing_receipts() {
    let scratch = Scratch::new();
    let path = scratch.owner_path();
    let original = snapshot(std::process::id(), "attempt-one", &"33".repeat(32));
    let empty_digest = empty_history_digest().unwrap();
    write_new_owner_attempt(&path, original.clone(), 1, "birth-one".into(), empty_digest).unwrap();
    let failed = read_owner_record(&path).unwrap().unwrap();
    let (next_attempt, history_digest) =
        record_interrupted_attempt(&path, &failed, "confirmed_test_exit".into()).unwrap();
    assert_eq!(next_attempt, 2);
    let owner_before = fs::read(&path).unwrap();
    let history_path = path.parent().unwrap().join("attempt-history.json");
    let history_before = fs::read(&history_path).unwrap();

    let mut changed = original;
    changed.instance.config_digest = "44".repeat(32);
    assert!(
        write_new_owner_attempt(
            &path,
            changed,
            next_attempt,
            "birth-two".into(),
            history_digest,
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), owner_before);
    assert_eq!(fs::read(&history_path).unwrap(), history_before);
}

#[test]
fn fixed_read_trust_mismatch_preserves_original_admission() {
    let scratch = Scratch::new();
    let request = "11".repeat(32);
    let config = "22".repeat(32);
    let trust = "33".repeat(32);
    super::super::fixed_trust::publish(&scratch.0, &request, &config, &trust).unwrap();
    let path = scratch.0.join(super::super::fixed_trust::FILE);
    let before = fs::read(&path).unwrap();
    assert!(
        super::super::fixed_trust::check(&scratch.0, &request, &"44".repeat(32), &trust,).is_err()
    );
    assert!(
        super::super::fixed_trust::check(&scratch.0, &request, &config, &"55".repeat(32),).is_err()
    );
    super::super::fixed_trust::check(&scratch.0, &request, &config, &trust).unwrap();
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn attempt_history_cap_rejects_without_dropping_old_failure_receipts() {
    let scratch = Scratch::new();
    let path = scratch.owner_path();
    let identity = snapshot(std::process::id(), "attempt-32", &"33".repeat(32));
    let attempts = (1..=MAX_RETAINED_ATTEMPTS)
        .map(|attempt| OwnerAttemptReceiptV1 {
            attempt: attempt as u64,
            snapshot: identity.clone(),
            process_start_identity: Some(format!("birth-{attempt}")),
            exit_observation: format!("confirmed_exit_{attempt}"),
        })
        .collect();
    let journal = Journal {
        api_version: JOURNAL_API.into(),
        attempts,
    };
    let history_digest = digest(&journal).unwrap();
    let history_path = path.parent().unwrap().join("attempt-history.json");
    io::write(&history_path, &journal).unwrap();
    let record = owner_record(
        identity,
        MAX_RETAINED_ATTEMPTS as u64 + 1,
        Some("birth-33".into()),
        Some(history_digest),
    );
    io::write(&path, &record).unwrap();
    let owner_before = fs::read(&path).unwrap();
    let history_before = fs::read(&history_path).unwrap();

    assert!(record_interrupted_attempt(&path, &record, "confirmed_exit_33".into()).is_err());
    assert_eq!(fs::read(&path).unwrap(), owner_before);
    assert_eq!(fs::read(&history_path).unwrap(), history_before);
}

#[test]
fn restored_successor_attempt_binds_first_receipt_and_rejects_later_identity_drift() {
    let scratch = Scratch::new();
    let path = scratch.owner_path();
    let mut restored = snapshot(std::process::id(), "restored-attempt", &"33".repeat(32));
    restored.instance.authority_epoch = 2;
    restored.instance.authority_id = "77".repeat(32);
    restored.state = "starting".into();
    write_new_owner_attempt(
        &path,
        restored.clone(),
        1,
        "birth-one".into(),
        empty_history_digest().unwrap(),
    )
    .unwrap();
    restored.state = "ready".into();
    write_owner_record(&path, restored.clone()).unwrap();
    assert_eq!(
        read_owner_record(&path).unwrap().unwrap().snapshot.instance,
        restored.instance
    );
    let before = fs::read(&path).unwrap();
    restored.instance.authority_epoch = 1;
    assert!(
        write_owner_record(&path, restored)
            .unwrap_err()
            .contains("changed exact attempt identity")
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn retry_launch_receipt_rejects_old_generation_and_requires_exact_birth() {
    assert!(!receipt_matches_spawn(
        100,
        Some("old-birth"),
        200,
        Some("new-birth")
    ));
    assert!(!receipt_matches_spawn(
        200,
        Some("old-birth"),
        200,
        Some("new-birth")
    ));
    assert!(!receipt_matches_spawn(200, None, 200, None));
    assert!(!receipt_matches_spawn(200, Some("new-birth"), 200, None));
    assert!(receipt_matches_spawn(
        200,
        Some("new-birth"),
        200,
        Some("new-birth")
    ));
}
