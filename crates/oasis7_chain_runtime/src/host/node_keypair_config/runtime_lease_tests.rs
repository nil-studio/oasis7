use super::*;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    binding: RuntimeIdentityBindingV1,
}
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("oasis7-runtime-lease-{}", io::random_id().unwrap()));
        io::private_dir(&root).unwrap();
        let key_path = root.join("key.toml");
        let key = transaction::ensure(&key_path, false).unwrap();
        let data_directory = root.join("writer");
        io::private_dir(&data_directory).unwrap();
        let derived =
            super::super::super::derive_node_libp2p_identity_keypair_config("node", &key).unwrap();
        let peer_id = oasis7_node::derive_libp2p_identity_keypair(&derived.private_key_hex)
            .unwrap()
            .public()
            .to_peer_id()
            .to_string();
        Self {
            root,
            binding: RuntimeIdentityBindingV1 {
                version: 1,
                key_path,
                data_directory,
                node_id: "node".into(),
                root_public_key: key.public_key_hex,
                peer_id,
                world_id: "world".into(),
                chain_id: "chain".into(),
                genesis_digest: "ab".repeat(32),
                profile: "controlled-v2".into(),
            },
        }
    }
    fn acquire(&self) -> Result<RuntimeIdentityLease, String> {
        acquire_runtime_identity_lease(&self.binding.key_path, &self.binding)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn durable_binding_reopens_without_private_material_and_arc_retains_lifetime_lock() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<RuntimeIdentityLease>();
    let f = Fixture::new();
    let lease = Arc::new(f.acquire().unwrap());
    assert_eq!(
        io::read::<RuntimeIdentityDomainV1>(&transaction::sidecar(
            &f.binding.key_path,
            KEY_BINDING
        ))
        .unwrap(),
        RuntimeIdentityDomainV1::from(&f.binding)
    );
    let retained = lease.clone();
    drop(lease);
    assert!(f.acquire().is_err());
    drop(retained);
    let bytes = fs::read(transaction::sidecar(&f.binding.key_path, KEY_BINDING)).unwrap();
    let key = read_existing_legacy_keypair(&f.binding.key_path).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(&key.private_key_hex));
    drop(f.acquire().unwrap());
}

#[test]
fn domain_data_key_and_peer_conflicts_never_replace_original_records() {
    let f = Fixture::new();
    drop(f.acquire().unwrap());
    let path = transaction::sidecar(&f.binding.key_path, KEY_BINDING);
    let original = fs::read(&path).unwrap();
    for index in 0..7 {
        let mut changed = f.binding.clone();
        match index {
            0 => changed.world_id.push('x'),
            1 => changed.chain_id.push('x'),
            2 => changed.genesis_digest = "cd".repeat(32),
            3 => changed.node_id.push('x'),
            4 => changed.peer_id.push('x'),
            5 => changed.root_public_key = "ef".repeat(32),
            _ => changed.profile = "bft".into(),
        }
        assert!(acquire_runtime_identity_lease(&changed.key_path, &changed).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    // A different original identity cannot adopt the bound writer directory.
    let other = Fixture::new();
    let mut changed = other.binding.clone();
    changed.data_directory = f.binding.data_directory.clone();
    let live_original = f.acquire().unwrap();
    let error = acquire_runtime_identity_lease(&changed.key_path, &changed).unwrap_err();
    assert!(error.contains("already in use"));
    assert!(!transaction::sidecar(&changed.key_path, KEY_BINDING).exists());
    drop(live_original);
    assert!(acquire_runtime_identity_lease(&changed.key_path, &changed).is_err());
}

#[test]
fn stopped_identity_enrolls_empty_writer_but_live_or_copied_directory_is_rejected() {
    let f = Fixture::new();
    let original = f.acquire().unwrap();
    let key_path = transaction::sidecar(&f.binding.key_path, KEY_BINDING);
    let key_bytes = fs::read(&key_path).unwrap();
    let original_directory = f.binding.data_directory.join(DIRECTORY_BINDING);
    let original_bytes = fs::read(&original_directory).unwrap();
    let mut successor = f.binding.clone();
    successor.data_directory = f.root.join("successor-writer");
    io::private_dir(&successor.data_directory).unwrap();
    assert!(
        acquire_runtime_identity_lease(&successor.key_path, &successor)
            .unwrap_err()
            .contains("already in use")
    );
    assert!(!successor.data_directory.join(DIRECTORY_BINDING).exists());
    drop(original);
    drop(acquire_runtime_identity_lease(&successor.key_path, &successor).unwrap());
    assert_eq!(fs::read(&key_path).unwrap(), key_bytes);
    assert_eq!(fs::read(&original_directory).unwrap(), original_bytes);
    assert_eq!(
        io::read::<RuntimeIdentityBindingV1>(&successor.data_directory.join(DIRECTORY_BINDING))
            .unwrap(),
        successor
    );
    let mut copied = f.binding.clone();
    copied.data_directory = f.root.join("copied-writer");
    io::private_dir(&copied.data_directory).unwrap();
    io::write(&copied.data_directory.join(DIRECTORY_BINDING), &f.binding).unwrap();
    assert!(
        acquire_runtime_identity_lease(&copied.key_path, &copied)
            .unwrap_err()
            .contains("binding conflict")
    );
    drop(f.acquire().unwrap());
}

#[test]
fn malformed_unknown_oversized_and_missing_key_fail_closed_without_repair() {
    let f = Fixture::new();
    drop(f.acquire().unwrap());
    let path = transaction::sidecar(&f.binding.key_path, KEY_BINDING);
    let original = fs::read(&path).unwrap();
    for bytes in [
        b"broken".to_vec(),
        vec![b'x'; MAX_BINDING_BYTES as usize + 1],
        {
            let mut value = serde_json::to_value(&f.binding).unwrap();
            value["unknown"] = true.into();
            serde_json::to_vec(&value).unwrap()
        },
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(f.acquire().is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
    fs::write(&path, original).unwrap();
    fs::remove_file(&f.binding.key_path).unwrap();
    assert!(f.acquire().is_err());
    assert!(!f.binding.key_path.exists());
}

#[test]
fn symlinks_and_weak_directory_permissions_are_rejected() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new();
    let alias = f.root.join("alias");
    symlink(&f.binding.data_directory, &alias).unwrap();
    let mut changed = f.binding.clone();
    changed.data_directory = alias;
    assert!(acquire_runtime_identity_lease(&changed.key_path, &changed).is_err());
    fs::set_permissions(&f.binding.data_directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(f.acquire().is_err());
}

#[test]
fn one_published_record_recovers_same_binding_after_interruption() {
    let f = Fixture::new();
    publish(
        &transaction::sidecar(&f.binding.key_path, KEY_BINDING),
        &RuntimeIdentityDomainV1::from(&f.binding),
    )
    .unwrap();
    let _lease = f.acquire().unwrap();
    assert_eq!(
        io::read::<RuntimeIdentityBindingV1>(&f.binding.data_directory.join(DIRECTORY_BINDING))
            .unwrap(),
        f.binding
    );
}

#[test]
fn process_worker() {
    let Some(path) = std::env::var_os("OASIS7_RUNTIME_LEASE_TEST_BINDING") else {
        return;
    };
    let binding: RuntimeIdentityBindingV1 = io::read(Path::new(&path)).unwrap();
    let _lease = acquire_runtime_identity_lease(&binding.key_path, &binding).unwrap();
    fs::write(binding.data_directory.join("worker-ready"), b"locked").unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn sigkill_releases_runtime_lease_without_unlinking_stable_inode() {
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    let input = f.root.join("worker.json");
    io::write(&input, &f.binding).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "node_keypair_config::runtime_lease::tests::process_worker",
            "--nocapture",
        ])
        .env("OASIS7_RUNTIME_LEASE_TEST_BINDING", &input)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !f.binding.data_directory.join("worker-ready").exists() {
        if child.try_wait().unwrap().is_some() || Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("lease worker did not lock");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let lock = transaction::sidecar(&f.binding.key_path, KEY_LOCK);
    let inode = fs::metadata(&lock).unwrap().ino();
    assert!(f.acquire().is_err());
    child.kill().unwrap();
    child.wait().unwrap();
    let _resumed = f.acquire().unwrap();
    assert_eq!(fs::metadata(&lock).unwrap().ino(), inode);
}

#[test]
fn retry_completes_binding_after_link_succeeded_but_directory_sync_failed() {
    let f = Fixture::new();
    FAIL_BEFORE_BINDING_DIRECTORY_SYNC.with(|fail| fail.set(true));
    let error = f.acquire().unwrap_err();
    assert!(error.contains("injected binding directory sync failure"));
    let path = transaction::sidecar(&f.binding.key_path, KEY_BINDING);
    let original = fs::read(&path).unwrap();
    assert!(!f.binding.data_directory.join(DIRECTORY_BINDING).exists());
    let _lease = f.acquire().unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        io::read::<RuntimeIdentityDomainV1>(&transaction::sidecar(
            &f.binding.key_path,
            KEY_BINDING
        ))
        .unwrap(),
        RuntimeIdentityDomainV1::from(&f.binding)
    );
    assert!(f.binding.data_directory.join(DIRECTORY_BINDING).exists());
}

#[test]
fn malformed_private_toml_never_appears_in_lease_diagnostics() {
    let f = Fixture::new();
    let private = read_existing_legacy_keypair(&f.binding.key_path)
        .unwrap()
        .private_key_hex;
    let bytes = format!("[node]\nprivate_key = \"{private}\n");
    fs::write(&f.binding.key_path, &bytes).unwrap();
    let error = f.acquire().unwrap_err();
    assert!(!error.contains(&private));
    assert!(error.contains("content redacted"));
    assert_eq!(fs::read_to_string(&f.binding.key_path).unwrap(), bytes);
}

#[test]
fn legacy_marker_blocks_enrollment_and_preserves_original_marker_and_key() {
    let f = Fixture::new();
    let marker = transaction::sidecar(&f.binding.key_path, ".lock");
    fs::write(&marker, b"unknown older writer").unwrap();
    let original = fs::read(&f.binding.key_path).unwrap();
    assert!(
        f.acquire()
            .unwrap_err()
            .contains("legacy identity lock present")
    );
    assert_eq!(fs::read(&marker).unwrap(), b"unknown older writer");
    assert_eq!(fs::read(&f.binding.key_path).unwrap(), original);
    assert!(!transaction::sidecar(&f.binding.key_path, KEY_BINDING).exists());
}

#[test]
fn oversized_existing_key_configuration_is_never_parsed_or_repaired() {
    let f = Fixture::new();
    fs::write(&f.binding.key_path, vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    assert!(f.acquire().unwrap_err().contains("exceeds size limit"));
    assert_eq!(
        fs::metadata(&f.binding.key_path).unwrap().len(),
        4 * 1024 * 1024 + 1
    );
}
