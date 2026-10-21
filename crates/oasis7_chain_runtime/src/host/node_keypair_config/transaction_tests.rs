use super::*;
use std::process::{Child, Command, Stdio};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("oasis7-identity-{}", io::random_id().unwrap()));
        io::private_dir(&root).unwrap();
        Self(root.join("config.toml"))
    }
    fn intent(&self) -> Intent {
        io::read(&sidecar(&self.0, ".identity-v2.json")).unwrap()
    }
    fn candidate(&self) -> PathBuf {
        sidecar(
            &self.0,
            &format!(".identity-{}.toml", self.intent().candidate_id),
        )
    }
    fn worker(&self) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "node_keypair_config::transaction::tests::process_worker",
                "--nocapture",
            ])
            .env("OASIS7_IDENTITY_TEST_PATH", &self.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    }
    fn stopped(&self, stage: &str) -> Child {
        let _ = fs::remove_file(sidecar(&self.0, ".test-ready"));
        let mut child = self
            .worker()
            .env("OASIS7_IDENTITY_TEST_STOP_AT", stage)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while !sidecar(&self.0, ".test-ready").exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "worker exited before {stage}"
            );
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("worker did not reach {stage}");
            }
            thread::sleep(Duration::from_millis(10));
        }
        child
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().unwrap()).unwrap();
    }
}

#[test]
fn process_worker() {
    let Some(path) = std::env::var_os("OASIS7_IDENTITY_TEST_PATH") else {
        return;
    };
    let result = ensure(Path::new(&path), false);
    if std::env::var_os("OASIS7_IDENTITY_TEST_FAIL_AT").is_some() {
        assert!(result.is_err());
    } else {
        result.unwrap();
    }
}

fn kill(mut child: Child) {
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
}

#[test]
fn sigkill_at_each_durable_stage_resumes_the_same_identity() {
    for stage in [
        "intent",
        "candidate-synced",
        "prepared",
        "published",
        "complete",
    ] {
        let fixture = Fixture::new();
        let child = fixture.stopped(stage);
        let before = if fixture.candidate().exists() {
            Some(
                prepare_config(load_config_table(&fixture.candidate()).unwrap())
                    .unwrap()
                    .0,
            )
        } else {
            None
        };
        kill(child);
        let key = ensure(&fixture.0, false).unwrap_or_else(|e| panic!("{stage}: {e}"));
        if let Some(before) = before {
            assert_eq!(key, before, "{stage}");
        }
        assert_eq!(ensure(&fixture.0, false).unwrap(), key);
        assert!(fixture.intent().complete);
        assert!(!fixture.candidate().exists());
    }
}

#[test]
fn live_os_lock_is_not_stolen_and_sigkill_releases_same_inode() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let child = fixture.stopped("locked");
    let path = sidecar(&fixture.0, ".identity-v2.lock");
    let inode = fs::metadata(&path).unwrap().ino();
    let contender = io::private_file(&path).unwrap();
    assert!(matches!(
        contender.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(!fixture.0.exists());
    kill(child);
    contender.try_lock().unwrap();
    drop(contender);
    ensure(&fixture.0, false).unwrap();
    assert_eq!(fs::metadata(path).unwrap().ino(), inode);
}

#[test]
fn partial_candidate_and_changed_target_are_preserved_and_blocked() {
    for partial in [true, false] {
        let fixture = Fixture::new();
        let child = fixture.stopped(if partial {
            "candidate-created"
        } else {
            "prepared"
        });
        kill(child);
        if !partial {
            fs::write(&fixture.0, "[foreign]\nvalue = 3\n").unwrap();
        } else {
            fs::write(fixture.candidate(), "torn nonempty candidate").unwrap();
        }
        let candidate = fs::read(fixture.candidate()).unwrap();
        assert!(ensure(&fixture.0, false).is_err());
        assert_eq!(fs::read(fixture.candidate()).unwrap(), candidate);
        assert!(!fixture.intent().complete);
    }
}

#[test]
fn failure_after_publication_recovers_without_replacing_key() {
    let fixture = Fixture::new();
    assert!(
        fixture
            .worker()
            .env("OASIS7_IDENTITY_TEST_FAIL_AT", "published")
            .status()
            .unwrap()
            .success()
    );
    let bytes = fs::read(&fixture.0).unwrap();
    let candidate = fs::read(fixture.candidate()).unwrap();
    assert_eq!(bytes, candidate);
    assert!(!fixture.intent().complete);
    ensure(&fixture.0, false).unwrap();
    assert_eq!(fs::read(&fixture.0).unwrap(), bytes);
}

#[test]
fn private_only_completion_preserves_key_and_unrelated_configuration() {
    let fixture = Fixture::new();
    let private = hex::encode([7u8; 32]);
    fs::write(
        &fixture.0,
        format!("name = 'original'\n[node]\nprivate_key = '{private}'\n"),
    )
    .unwrap();
    let key = ensure(&fixture.0, false).unwrap();
    assert_eq!(key.private_key_hex, private);
    assert_eq!(
        load_config_table(&fixture.0).unwrap()["name"].as_str(),
        Some("original")
    );
    let changed = fs::read_to_string(&fixture.0)
        .unwrap()
        .replace("original", "updated");
    fs::write(&fixture.0, changed).unwrap();
    assert_eq!(ensure(&fixture.0, false).unwrap(), key);
    fs::remove_file(&fixture.0).unwrap();
    assert!(
        ensure(&fixture.0, false)
            .unwrap_err()
            .contains("restore original")
    );
    assert!(!fixture.0.exists());
}

#[test]
fn legacy_lock_and_untrusted_candidates_are_not_adopted() {
    let fixture = Fixture::new();
    fs::write(sidecar(&fixture.0, ".lock"), "old writer").unwrap();
    assert!(
        ensure(&fixture.0, false)
            .unwrap_err()
            .contains("offline migration")
    );
    assert!(!fixture.0.exists());
    fs::remove_file(sidecar(&fixture.0, ".lock")).unwrap();
    let child = fixture.stopped("prepared");
    kill(child);
    fs::write(fixture.candidate(), "corrupted").unwrap();
    assert!(ensure(&fixture.0, false).is_err());
    assert!(!fixture.0.exists());
}

#[test]
fn empty_unpublished_candidate_is_quarantined_under_original_intent() {
    let fixture = Fixture::new();
    let child = fixture.stopped("candidate-created");
    kill(child);
    let id = fixture.intent().candidate_id;
    let quarantine = sidecar(&fixture.candidate(), ".incomplete");
    ensure(&fixture.0, false).unwrap();
    assert_eq!(fixture.intent().candidate_id, id);
    assert_eq!(fs::metadata(quarantine).unwrap().len(), 0);
}

#[test]
fn existing_identity_is_adopted_without_rotation_and_loss_is_blocked() {
    let fixture = Fixture::new();
    let (original, table) = prepare_config(toml::map::Map::new()).unwrap();
    fs::write(&fixture.0, toml::to_string(&table).unwrap()).unwrap();
    assert_eq!(ensure(&fixture.0, false).unwrap(), original);
    fs::remove_file(&fixture.0).unwrap();
    assert!(ensure(&fixture.0, false).is_err());
    assert!(!fixture.0.exists());
}

#[test]
fn quarantine_interruption_and_repeated_empty_candidates_are_idempotent() {
    let fixture = Fixture::new();
    kill(fixture.stopped("candidate-created"));
    kill(fixture.stopped("quarantined"));
    kill(fixture.stopped("candidate-created"));
    let id = fixture.intent().candidate_id;
    ensure(&fixture.0, false).unwrap();
    assert_eq!(fixture.intent().candidate_id, id);
}

#[test]
fn group_writable_ancestor_cannot_redirect_identity_publication() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let ancestor = fixture.0.parent().unwrap();
    fs::set_permissions(ancestor, fs::Permissions::from_mode(0o770)).unwrap();
    let nested = ancestor.join("private");
    io::private_dir(&nested).unwrap();
    let path = nested.join("identity.toml");
    assert!(ensure(&path, false).unwrap_err().contains("ancestors"));
    assert!(!sidecar(&path, ".identity-v2.lock").exists());
}

#[test]
fn read_only_identity_loader_blocks_pending_publication_without_mutation() {
    let fixture = Fixture::new();
    let child = fixture.stopped("published");
    kill(child);
    let original = fs::read(&fixture.0).unwrap();
    let intent_path = sidecar(&fixture.0, ".identity-v2.json");
    let intent_bytes = fs::read(&intent_path).unwrap();
    assert!(
        read_existing_legacy_keypair(&fixture.0)
            .unwrap_err()
            .contains("incomplete")
    );
    assert_eq!(fs::read(&fixture.0).unwrap(), original);
    assert_eq!(fs::read(intent_path).unwrap(), intent_bytes);
    let key = ensure(&fixture.0, false).unwrap();
    assert_eq!(read_existing_legacy_keypair(&fixture.0).unwrap(), key);
}
