use super::*;
fn fixture() -> (std::path::PathBuf, Vec<BusinessIndexNodeWriteV1>) {
    let dir = std::env::temp_dir().join(format!(
        "oasis7-pack-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&dir).unwrap();
    let writes = writes();
    (dir, writes)
}
fn writes() -> Vec<BusinessIndexNodeWriteV1> {
    let mut writes: Vec<_> = [
        b"first logical node".to_vec(),
        b"second logical node".to_vec(),
    ]
    .into_iter()
    .map(|bytes| BusinessIndexNodeWriteV1 {
        digest: blake3::hash(&bytes).to_hex().to_string(),
        bytes,
    })
    .collect();
    writes.sort_by(|a, b| a.digest.cmp(&b.digest));
    writes
}
#[test]
fn pack_logical_hash_range_raw_compatibility_and_partial_link_retry() {
    let (dir, writes) = fixture();
    assert!(persist_inner(&dir, &writes, Some(1)).is_err());
    assert_eq!(fs::read_dir(dir.join(STAGING)).unwrap().count(), 0);
    assert!(dir.join(&writes[0].digest).exists());
    assert!(!dir.join(&writes[1].digest).exists());
    persist(&dir, &writes).unwrap();
    for write in &writes {
        let path = dir.join(&write.digest);
        assert!(read_object(&path, &write.digest).unwrap().unwrap() == write.bytes);
        let (total, range) = read_range(&path, &write.digest, 2, 3).unwrap().unwrap();
        assert_eq!(total, write.bytes.len() as u64);
        assert_eq!(range, write.bytes[2..5]);
    }
    let raw = b"legacy raw object";
    let id = blake3::hash(raw).to_hex().to_string();
    fs::write(dir.join(&id), raw).unwrap();
    assert_eq!(read_object(&dir.join(&id), &id).unwrap().unwrap(), raw);
    let marker_raw = [MAGIC, b"raw payload not a pack"].concat();
    let marker_id = blake3::hash(&marker_raw).to_hex().to_string();
    fs::write(dir.join(&marker_id), &marker_raw).unwrap();
    assert_eq!(
        read_object(&dir.join(&marker_id), &marker_id)
            .unwrap()
            .unwrap(),
        marker_raw
    );
    assert_eq!(
        read_range(&dir.join(&marker_id), &marker_id, 0, 1024)
            .unwrap()
            .unwrap()
            .1,
        marker_raw
    );
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn pack_referenced_corruption_truncation_and_overflow_fail_closed() {
    let (dir, writes) = fixture();
    persist(&dir, &writes).unwrap();
    let path = dir.join(&writes[0].digest);
    let original = fs::read(&path).unwrap();
    let mut corrupt = original.clone();
    corrupt[HEADER] ^= 1;
    fs::write(&path, &corrupt).unwrap();
    assert!(read_object(&path, &writes[0].digest).is_err());
    let mut overflow = original.clone();
    overflow[MAGIC.len() + 8..MAGIC.len() + 16].copy_from_slice(&u64::MAX.to_be_bytes());
    fs::write(&path, &overflow).unwrap();
    assert!(read_object(&path, &writes[0].digest).is_err());
    fs::write(&path, &original[..HEADER - 1]).unwrap();
    assert!(read_object(&path, &writes[0].digest).is_err());
    fs::write(&path, &original).unwrap();
    let absent = blake3::hash(b"absent object").to_hex().to_string();
    fs::hard_link(&path, dir.join(&absent)).unwrap();
    assert!(read_object(&dir.join(&absent), &absent).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let link = dir.join("symlink");
        symlink(&path, &link).unwrap();
        assert!(read_object(&link, &writes[0].digest).is_err());
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn abandoned_staging_restart_cleanup_retains_logical_digest_links() {
    let (dir, writes) = fixture();
    persist(&dir, &writes).unwrap();
    let staging = staging_directory(&dir).unwrap();
    for _ in 0..4 {
        let stage = staging.join(format!("{STAGE_PREFIX}{}", "ab".repeat(32)));
        fs::hard_link(dir.join(&writes[0].digest), &stage).unwrap();
        cleanup_staging_locked(&dir).unwrap();
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
        assert!(
            read_object(&dir.join(&writes[0].digest), &writes[0].digest)
                .unwrap()
                .unwrap()
                == writes[0].bytes
        );
    }
    fs::write(staging.join("operator-file"), b"preserve").unwrap();
    assert!(cleanup_staging_locked(&dir).is_err());
    assert!(staging.join("operator-file").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
#[ignore = "internal SIGKILL harness child, invoked by pack_durable_boundary_sigkill_reopen_cleanup"]
fn pack_sigkill_harness_child() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("OASIS7_PACK_CRASH_TEST_DIRECTORY").expect("parent harness directory"),
    );
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("test-exclusive.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    persist_inner(&directory, &writes(), Some(usize::MAX)).unwrap();
    panic!("SIGKILL hook must terminate before links or receipt-equivalent success");
}
#[cfg(unix)]
#[test]
fn pack_durable_boundary_sigkill_reopen_cleanup() {
    use std::os::unix::process::ExitStatusExt;
    let (dir, writes) = fixture();
    let acquire = || {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(dir.join("test-exclusive.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        lock
    };
    let lock = acquire();
    persist(&dir, &writes[..1]).unwrap();
    drop(lock);
    for _ in 0..3 {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "controlled_authority::formal::business_pack::tests::pack_sigkill_harness_child",
                "--test-threads=1",
            ])
            .env("OASIS7_PACK_CRASH_TEST_DIRECTORY", &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .status()
            .unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        let lock = acquire();
        assert_eq!(fs::read_dir(dir.join(STAGING)).unwrap().count(), 1);
        assert!(
            read_object(&dir.join(&writes[0].digest), &writes[0].digest)
                .unwrap()
                .unwrap()
                == writes[0].bytes
        );
        assert!(!dir.join(&writes[1].digest).exists());
        cleanup_staging_locked(&dir).unwrap();
        assert_eq!(fs::read_dir(dir.join(STAGING)).unwrap().count(), 0);
        drop(lock);
    }
    let lock = acquire();
    assert!(persist_inner(&dir, &writes, Some(0)).is_err());
    assert_eq!(fs::read_dir(dir.join(STAGING)).unwrap().count(), 0);
    persist(&dir, &writes).unwrap();
    assert!(
        read_object(&dir.join(&writes[1].digest), &writes[1].digest)
            .unwrap()
            .unwrap()
            == writes[1].bytes
    );
    drop(lock);
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn registered_readback_syncs_pack_once_but_checks_every_logical_object() {
    let (dir, writes) = fixture();
    persist(&dir, &writes).unwrap();
    let markers = dir.join("readback-run");
    fs::create_dir(&markers).unwrap();
    for write in &writes {
        assert_eq!(
            read_object_durable_once(&dir.join(&write.digest), &write.digest, &markers)
                .unwrap()
                .unwrap(),
            write.bytes
        );
    }
    // Both digest links address one physical pack. The run records exactly one
    // successful physical durability operation, not one per logical link.
    assert_eq!(fs::read_dir(&markers).unwrap().count(), 1);
    let first = &writes[0];
    assert_eq!(
        read_object_durable_once(&dir.join(&first.digest), &first.digest, &markers)
            .unwrap()
            .unwrap(),
        first.bytes
    );
    assert_eq!(fs::read_dir(&markers).unwrap().count(), 1);
    // A prior sync marker must never bypass later content validation.
    let second = &writes[1];
    let path = dir.join(&second.digest);
    let mut file = OpenOptions::new().write(true).open(&path).unwrap();
    let len = file.metadata().unwrap().len();
    file.seek(SeekFrom::Start(len - 1)).unwrap();
    file.write_all(&[0xff]).unwrap();
    assert!(read_object_durable_once(&path, &second.digest, &markers).is_err());
    fs::remove_file(&path).unwrap();
    assert!(
        read_object_durable_once(&path, &second.digest, &markers)
            .unwrap()
            .is_none()
    );
    std::os::unix::fs::symlink(dir.join(&first.digest), &path).unwrap();
    assert!(read_object_durable_once(&path, &second.digest, &markers).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn registered_readback_rejects_other_uid_writable_cas_even_with_valid_hash() {
    let (dir, _) = fixture();
    let markers = dir.join("readback-run");
    fs::create_dir(&markers).unwrap();
    let bytes = b"valid hash is insufficient for externally writable storage";
    let id = blake3::hash(bytes).to_hex().to_string();
    let path = dir.join(&id);
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        read_object_durable_once(&path, &id, &markers)
            .unwrap()
            .unwrap(),
        bytes
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o664)).unwrap();
    assert!(read_object_durable_once(&path, &id, &markers).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn retry_registers_durable_objects_after_interrupted_retention_publication() {
    let (dir, writes) = fixture();
    super::physical::initialize(&dir).unwrap();
    let first = &writes[0];
    // Reproduce publication completed before its retention ledger update.
    super::physical::persist(&dir, &[(first.digest.clone(), first.bytes.clone())]).unwrap();
    assert_eq!(super::physical::roots::count(&dir).unwrap(), 0);
    // A mixed retry must register both existing and newly published objects.
    persist(&dir, &writes).unwrap();
    let (ids, _, complete) =
        super::physical::roots::page(&dir, &super::physical::roots::start(), 1024).unwrap();
    assert!(complete);
    for write in &writes {
        assert!(ids.contains(&write.digest));
    }
    // An entirely existing retry must repair an unpublished ledger as well.
    let (other, _) = fixture();
    super::physical::initialize(&other).unwrap();
    super::physical::persist(&other, &[(first.digest.clone(), first.bytes.clone())]).unwrap();
    persist(&other, std::slice::from_ref(first)).unwrap();
    let (ids, _, complete) =
        super::physical::roots::page(&other, &super::physical::roots::start(), 1024).unwrap();
    assert!(complete);
    assert_eq!(ids, vec![first.digest.clone()]);
    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(other).unwrap();
}
