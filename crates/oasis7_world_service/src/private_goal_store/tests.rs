use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "oasis7-private-goal-{}",
                hex::encode({
                    let mut n = [0; 16];
                    getrandom::fill(&mut n).unwrap();
                    n
                })
            ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn store(&self) -> std::path::PathBuf {
        self.0.join("private")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn binding() -> PrivateGoalBindingV1 {
    PrivateGoalBindingV1 {
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        task_id: "private-goal-task-identity-for-encryption-regression".into(),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_public_key: "ab".repeat(32),
        owner_generation: DecimalU64(1),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        goal_revision: DecimalU64(1),
    }
}
fn content() -> GoalContentV1 {
    GoalContentV1 {
        goal_text: "secret complete goal 正文".into(),
        soft_constraints: vec!["keep order".into(), "keep order".into()],
    }
}
#[test]
fn private_goal_store_restart_random_refs_encrypted_complete_content() {
    let f = Fixture::new();
    let b = binding();
    let c = content();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let first = store.put(&b, &c).unwrap();
    let second = store.put(&b, &c).unwrap();
    assert_ne!(first, second);
    let bytes = fs::read(f.store().join(first.as_str())).unwrap();
    let digest = game_goal_version_digest(&b.task_id, b.goal_revision, &c).unwrap();
    for secret in [
        c.goal_text.as_bytes(),
        digest.as_str().as_bytes(),
        b.task_id.as_bytes(),
        b.owner_public_key.as_bytes(),
    ] {
        assert!(!bytes.windows(secret.len()).any(|window| window == secret));
    }
    assert_eq!(
        fs::metadata(f.store()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(f.store().join(first.as_str()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    drop(store);
    let reopened = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let goal = reopened.get(&first, &b).unwrap();
    assert_eq!(goal.content, c);
    assert_eq!(goal.goal_digest, digest);
    assert_eq!(fs::read(f.store().join(first.as_str())).unwrap(), bytes);
    assert_eq!(fs::read_dir(f.store()).unwrap().count(), 3);
}
#[test]
fn private_goal_store_wrong_key_all_identity_fences_and_corruption_rejected() {
    let f = Fixture::new();
    let b = binding();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let r = store.put(&b, &content()).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [8; 32]).is_err());
    let mut variants = Vec::new();
    for field in 0..10 {
        let mut changed = b.clone();
        match field {
            0 => changed.world.world_id.push('x'),
            1 => changed.world.genesis_digest.push('x'),
            2 => changed.task_id.push('x'),
            3 => changed.agent_id.push('x'),
            4 => changed.owner_binding.push('x'),
            5 => changed.owner_public_key = "cd".repeat(32),
            6 => changed.owner_generation.0 += 1,
            7 => changed.branch_id.push('x'),
            8 => changed.reorg_epoch.0 += 1,
            _ => changed.goal_revision.0 += 1,
        }
        variants.push(changed);
    }
    for changed in variants {
        assert!(store.get(&r, &changed).is_err());
    }
    let path = f.store().join(r.as_str());
    let original = fs::read(&path).unwrap();
    for offset in [0, 8, 32, original.len() - 1] {
        let mut bytes = original.clone();
        bytes[offset] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(store.get(&r, &b).is_err());
    }
    fs::write(&path, original).unwrap();
    assert!(store.get(&r, &b).is_ok());
}
#[test]
fn private_goal_store_rejects_links_permissions_invalid_content_and_oversize() {
    let f = Fixture::new();
    let b = binding();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let r = store.put(&b, &content()).unwrap();
    let path = f.store().join(r.as_str());
    let backup = f.0.join("outside");
    fs::rename(&path, &backup).unwrap();
    symlink(&backup, &path).unwrap();
    assert!(store.get(&r, &b).is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(&backup, &path).unwrap();
    assert!(store.get(&r, &b).is_err());
    fs::remove_file(&backup).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(store.get(&r, &b).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&path, vec![0; MAX_PRIVATE_GOAL_FILE_BYTES + 1]).unwrap();
    assert!(store.get(&r, &b).is_err());
    let mut c = content();
    c.goal_text = " ".into();
    assert!(store.put(&b, &c).is_err());
    c.goal_text = "x".repeat(4096);
    assert!(store.put(&b, &c).is_err());
    assert!(PrivateGoalRef::parse("../outside").is_err());
    assert!(PrivateGoalRef::parse(&"A".repeat(64)).is_err());
    let alias = f.0.join("alias");
    symlink(f.store(), &alias).unwrap();
    assert!(PrivateGoalStore::open(&alias, [7; 32]).is_err());
    let nested = alias.join("child");
    assert!(PrivateGoalStore::open(&nested, [7; 32]).is_err());
    fs::set_permissions(f.store(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    assert!(store.put(&b, &content()).is_err());
}

#[test]
fn private_goal_store_collision_never_overwrites_and_decoded_digest_is_checked() {
    let f = Fixture::new();
    let b = binding();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let existing = store.put(&b, &content()).unwrap();
    let original = fs::read(f.store().join(existing.as_str())).unwrap();
    let mut malformed = store.get(&existing, &b).unwrap();
    malformed.goal_digest = Digest32("blake3:".to_owned() + &"00".repeat(32));
    let plaintext = serde_cbor::to_vec(&malformed).unwrap();
    let new_ref = PrivateGoalRef::parse(&"12".repeat(32)).unwrap();
    let mut attempt = 0;
    let published = unix::put_with_reference(&store.directory, &store.key, &b, &plaintext, || {
        attempt += 1;
        Ok(if attempt == 1 {
            existing.clone()
        } else {
            new_ref.clone()
        })
    })
    .unwrap();
    assert_eq!(attempt, 2);
    assert_eq!(published, new_ref);
    assert!(store.get(&published, &b).is_err());
    assert_eq!(
        fs::read(f.store().join(existing.as_str())).unwrap(),
        original
    );
    assert!(
        unix::put_with_reference(&store.directory, &store.key, &b, &plaintext, || Ok(
            existing.clone()
        ))
        .is_err()
    );
    assert_eq!(fs::read_dir(f.store()).unwrap().count(), 3);
}

#[test]
fn private_goal_store_fifo_and_relocated_ciphertext_rejected() {
    let f = Fixture::new();
    let b = binding();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let r = store.put(&b, &content()).unwrap();
    let path = f.store().join(r.as_str());
    let relocated = PrivateGoalRef::parse(&"34".repeat(32)).unwrap();
    fs::copy(&path, f.store().join(relocated.as_str())).unwrap();
    assert!(store.get(&relocated, &b).is_err());
    fs::remove_file(&path).unwrap();
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let before = std::time::Instant::now();
    assert!(store.get(&r, &b).is_err());
    assert!(before.elapsed() < std::time::Duration::from_secs(1));
}
#[test]
fn private_goal_store_restrictive_umask_fails_before_publication() {
    const MARKER: &str = "OASIS7_PRIVATE_GOAL_UMASK_CHILD";
    if std::env::var_os(MARKER).is_none() {
        let result=std::process::Command::new(std::env::current_exe().unwrap()).env(MARKER,"1")
            .args(["--exact","world_service::private_goal_store::tests::private_goal_store_restrictive_umask_fails_before_publication","--test-threads=1"]).output().unwrap();
        assert!(
            result.status.success(),
            "umask child failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let f = Fixture::new();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let previous = unsafe { libc::umask(0o400) };
    let result = store.put(&binding(), &content());
    unsafe { libc::umask(previous) };
    assert!(result.is_err());
    assert_eq!(fs::read_dir(f.store()).unwrap().count(), 1);
}

#[test]
fn private_goal_store_key_marker_corruption_deletion_and_orphans_fail_closed() {
    let f = Fixture::new();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let goal = store.put(&binding(), &content()).unwrap();
    let marker = f.store().join(".key-verifier-v1");
    let original = fs::read(&marker).unwrap();
    for offset in [0, 8, 32, original.len() - 1] {
        let mut bytes = original.clone();
        bytes[offset] ^= 1;
        fs::write(&marker, bytes).unwrap();
        assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
        assert!(store.put(&binding(), &content()).is_err());
        assert!(store.get(&goal, &binding()).is_err());
    }
    fs::write(&marker, vec![0; 257]).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    fs::write(&marker, &original).unwrap();
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    fs::set_permissions(&marker, fs::Permissions::from_mode(0o600)).unwrap();
    fs::remove_file(&marker).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    assert!(PrivateGoalStore::open(&f.store(), [8; 32]).is_err());
    assert!(store.put(&binding(), &content()).is_err());
    assert_eq!(fs::read_dir(f.store()).unwrap().count(), 1);
    let orphan = f.0.join("orphan");
    fs::create_dir(&orphan).unwrap();
    fs::set_permissions(&orphan, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(orphan.join(".interrupted.tmp"), b"orphan").unwrap();
    assert!(PrivateGoalStore::open(&orphan, [7; 32]).is_err());
    assert_eq!(fs::read_dir(&orphan).unwrap().count(), 1);
}
#[test]
fn private_goal_store_key_marker_links_fifo_and_wrong_key_open_rejected() {
    let f = Fixture::new();
    let store = PrivateGoalStore::open(&f.store(), [7; 32]).unwrap();
    let marker = f.store().join(".key-verifier-v1");
    let original = fs::read(&marker).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [8; 32]).is_err());
    assert_eq!(fs::read(&marker).unwrap(), original);
    let backup = f.0.join("marker-backup");
    fs::rename(&marker, &backup).unwrap();
    symlink(&backup, &marker).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    fs::remove_file(&marker).unwrap();
    fs::hard_link(&backup, &marker).unwrap();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    fs::remove_file(&marker).unwrap();
    use std::os::unix::ffi::OsStrExt;
    let cpath = std::ffi::CString::new(marker.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cpath.as_ptr(), 0o600) }, 0);
    let start = std::time::Instant::now();
    assert!(PrivateGoalStore::open(&f.store(), [7; 32]).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert!(store.put(&binding(), &content()).is_err());
}
#[test]
fn private_goal_store_concurrent_initialization_binds_one_external_key() {
    let f = Fixture::new();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [7u8, 8]
        .into_iter()
        .map(|byte| {
            let root = f.store();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                (byte, PrivateGoalStore::open(&root, [byte; 32]))
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|(_, store)| store.is_ok()).count(), 1);
    let (winner, store) = results
        .into_iter()
        .find(|(_, store)| store.is_ok())
        .unwrap();
    let store = store.unwrap();
    let r = store.put(&binding(), &content()).unwrap();
    assert!(
        PrivateGoalStore::open(&f.store(), [winner; 32])
            .unwrap()
            .get(&r, &binding())
            .is_ok()
    );
    let loser = if winner == 7 { 8 } else { 7 };
    assert!(PrivateGoalStore::open(&f.store(), [loser; 32]).is_err());
    assert_eq!(fs::read_dir(f.store()).unwrap().count(), 2);
}
