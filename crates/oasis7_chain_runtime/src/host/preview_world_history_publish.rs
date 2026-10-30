//! Explicit offline publication of already replay-verified historical bytes.
use super::*;
use bridge::historical_material::{self, Pins};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

pub(crate) struct PublicationBudget {
    pub max_store_bytes: u64,
    pub max_new_bytes: u64,
    pub max_store_objects: usize,
    pub max_history_cuts: usize,
    #[cfg(test)]
    pub before_pin: Option<fn() -> Result<(), String>>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Independent trust, replay resources and publication quotas are explicit offline inputs."
)]
pub(crate) fn rehydrate(
    root: &Path,
    scratch: &Path,
    world: &str,
    node: &str,
    service_pin: &str,
    target: u64,
    replay: ReplayBudget,
    publication: PublicationBudget,
    local: Option<oasis7_runtime::DevLocalParticipantAuthority>,
) -> Result<CommitRef, String> {
    // Namespace admission precedes even replay and the first pin read. Do not
    // discover a redirected pin parent only after adding source CAS objects.
    historical_material::pin_namespace_present(&LocalCasStore::new(root.join("store")))?;
    // Origin validation rejects other profiles. Acquire the same exclusion used
    // by the live writer before observing and replaying any historical closure.
    super::super::preview_world_init::verify_existing_origin(root, world, node, service_pin)?;
    let _writer =
        super::super::world_writer_lock::acquire_live_world_writer_lock(&root.join("world"))?;
    let deadline = replay.deadline;
    let cut = reconstruct(
        root,
        scratch,
        world,
        node,
        service_pin,
        target,
        replay,
        local,
    )?;
    let mut check = || {
        if Instant::now() < deadline {
            Ok(())
        } else {
            Err("history publication deadline exhausted".to_owned())
        }
    };
    check()?;
    let source_record_path = root.join("records").join(format!("{target:020}.json"));
    let source_record_bytes = historical_material::read_regular(&source_record_path, 1024 * 1024)?
        .ok_or("history original record unavailable")?;
    let original: bridge::ExecutionBridgeRecord = serde_json::from_slice(&source_record_bytes)
        .map_err(|_| "history original record invalid")?;
    let source = LocalCasStore::new(root.join("store"));
    let reconstructed = LocalCasStore::new(scratch.join("store"));
    let expected = historical_material::references(&original, &source)?
        .ok_or("history original proof unavailable")?;
    let mut material = BTreeMap::new();
    for hash in &expected {
        check()?;
        let bytes = historical_material::cas(&reconstructed, hash)?
            .ok_or("history reconstructed closure unavailable")?;
        material.insert(hash.clone(), bytes);
    }
    // One cooperative lock spans quota admission, immutable blob publication
    // and exact pin publication. No locking CAS mutation is called recursively.
    let _retention = source
        .acquire_retention_guard()
        .map_err(|_| "history retention exclusion unavailable")?;
    let missing = preflight(&source, &material, &publication, &mut check)?;
    let pin_path = historical_material::shard(&source, target);
    let scope = pin_path.parent().ok_or("history pin parent invalid")?;
    let pins = serde_json::to_vec(&Pins {
        pins: expected.clone(),
    })
    .map_err(|_| "history pin encoding failed")?;
    let already_pinned = match historical_material::read_regular(&pin_path, 16 * 1024)? {
        Some(old) => {
            if old != pins {
                return Err("history existing pin conflict".into());
            }
            true
        }
        None => {
            if publication.max_history_cuts == 0
                || publication.max_history_cuts > historical_material::MAX_CUTS
            {
                return Err("history cut quota invalid".into());
            }
            let mut count = 0usize;
            match fs::read_dir(scope) {
                Ok(entries) => {
                    for entry in entries {
                        check()?;
                        let entry = entry.map_err(|_| "history scope unreadable")?;
                        let name = entry.file_name();
                        let name = name.to_str().ok_or("history pin name invalid")?;
                        let stem = name
                            .strip_suffix(".json")
                            .ok_or("history pin name invalid")?;
                        if stem.len() != 20
                            || !stem.bytes().all(|b| b.is_ascii_digit())
                            || !entry
                                .file_type()
                                .map_err(|_| "history pin type unavailable")?
                                .is_file()
                        {
                            return Err("history pin namespace invalid".into());
                        }
                        count = count.checked_add(1).ok_or("history cut count overflow")?;
                        if count >= publication.max_history_cuts {
                            return Err("history cut quota exhausted".into());
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("history scope unreadable".into()),
            }
            false
        }
    };
    for hash in &missing {
        check()?;
        publish_new(
            &source.blobs_dir().join(format!("{hash}.blob")),
            &material[hash],
        )?;
    }
    check()?;
    if historical_material::read_regular(&source_record_path, 1024 * 1024)?
        .ok_or("history original record disappeared")?
        != source_record_bytes
    {
        return Err("history original record changed during publication".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        for directory in [source.root().join("pin_scopes"), scope.to_path_buf()] {
            match fs::DirBuilder::new().mode(0o700).create(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if !fs::symlink_metadata(&directory)
                        .map_err(|_| "history pin directory unavailable")?
                        .file_type()
                        .is_dir()
                    {
                        return Err("history pin directory carrier invalid".into());
                    }
                }
                Err(_) => return Err("history pin directory publication failed".into()),
            }
        }
    }
    if !historical_material::pin_namespace_present(&source)? {
        return Err("history pin namespace disappeared".into());
    }
    fs::File::open(source.root().join("pin_scopes"))
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "history pin namespace sync failed")?;
    fs::File::open(source.root())
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "history store namespace sync failed")?;
    #[cfg(test)]
    if let Some(before_pin) = publication.before_pin {
        before_pin()?;
    }
    if !already_pinned {
        publish_new(&pin_path, &pins)?;
    }
    check()?;
    if historical_material::hydrate(&original, &source)?.is_none() {
        return Err("history published closure could not be read back".into());
    }
    Ok(cut.commit)
}

fn preflight(
    source: &LocalCasStore,
    material: &BTreeMap<String, Vec<u8>>,
    limits: &PublicationBudget,
    check: &mut impl FnMut() -> Result<(), String>,
) -> Result<BTreeSet<String>, String> {
    historical_material::pin_namespace_present(source)?;
    let mut bytes = 0u64;
    let mut objects = 0usize;
    for entry in
        fs::read_dir(source.blobs_dir()).map_err(|_| "history CAS inventory unavailable")?
    {
        check()?;
        let entry = entry.map_err(|_| "history CAS inventory unavailable")?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| "history CAS object unavailable")?;
        if !metadata.file_type().is_file() {
            return Err("history CAS object carrier invalid".into());
        }
        objects = objects
            .checked_add(1)
            .ok_or("history object count overflow")?;
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or("history storage bytes overflow")?;
        if objects > limits.max_store_objects || bytes > limits.max_store_bytes {
            return Err("history existing CAS quota exhausted".into());
        }
    }
    let mut new_bytes = 0u64;
    let mut missing = BTreeSet::new();
    for (hash, value) in material {
        check()?;
        if let Some(old) = historical_material::cas(source, hash)? {
            if old != *value {
                return Err("history existing CAS conflict".into());
            }
        } else {
            objects = objects
                .checked_add(1)
                .ok_or("history object count overflow")?;
            new_bytes = new_bytes
                .checked_add(value.len() as u64)
                .ok_or("history new bytes overflow")?;
            missing.insert(hash.clone());
        }
    }
    if new_bytes > limits.max_new_bytes
        || bytes
            .checked_add(new_bytes)
            .ok_or("history storage bytes overflow")?
            > limits.max_store_bytes
        || objects > limits.max_store_objects
    {
        return Err("history publication quota exhausted".into());
    }
    Ok(missing)
}

/// Raw CAS is an existing supported encoding. Hard-link publication is atomic
/// no-replace; a concurrent or corrupt existing target is never overwritten.
fn publish_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "history publication sequence exhausted")?;
        let parent = path.parent().ok_or("history publication parent missing")?;
        let temporary = parent.join(format!(".history-{}-{sequence}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temporary)
            .map_err(|_| "history publication temporary unavailable")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "history publication temporary write failed")?;
        fs::hard_link(&temporary, path).map_err(|_| "history publication no-replace failed")?;
        fs::remove_file(&temporary)
            .map_err(|_| "history publication temporary retirement failed")?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "history publication directory sync failed")?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        Err("history publication requires Unix".into())
    }
}

pub(crate) fn run<'a>(args: impl Iterator<Item = &'a str>) -> Result<(), String> {
    let mut values = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(key) = args.next() {
        if key == "--help" {
            println!(
                "rehydrate-preview-history --storage-profile dev_local --root <original> --scratch <absent> --world-id <id> --node-id <id> --service-public-key <independent-pin> --height <old-height> --max-replay-heights <n> --max-replay-bytes <n> --deadline-ms <n> --max-store-bytes <n> --max-new-bytes <n> --max-store-objects <n> --max-history-cuts <1..64> [--operator-public-key <independent-local-pin>]"
            );
            return Ok(());
        }
        if !matches!(
            key,
            "--storage-profile"
                | "--root"
                | "--scratch"
                | "--world-id"
                | "--node-id"
                | "--service-public-key"
                | "--height"
                | "--max-replay-heights"
                | "--max-replay-bytes"
                | "--deadline-ms"
                | "--max-store-bytes"
                | "--max-new-bytes"
                | "--max-store-objects"
                | "--max-history-cuts"
                | "--operator-public-key"
        ) || values
            .insert(key, args.next().ok_or("history argument value missing")?)
            .is_some()
        {
            return Err("unknown or duplicate history argument".into());
        }
    }
    let value = |key| {
        values
            .get(key)
            .copied()
            .ok_or("explicit history argument missing")
    };
    let number = |key| {
        value(key)?
            .parse::<u64>()
            .map_err(|_| "history numeric argument invalid")
    };
    if value("--storage-profile")? != "dev_local" {
        return Err("history rehydration requires explicit DevLocal profile".into());
    }
    let local = values
        .get("--operator-public-key")
        .map(|key| {
            oasis7_runtime::DevLocalParticipantAuthority::new(
                oasis7_proto::storage_profile::StorageProfile::DevLocal,
                &ReleaseSecurityPolicy::default(),
                (*key).to_owned(),
            )
            .map_err(|_| "history local authority invalid")
        })
        .transpose()?;
    let replay = ReplayBudget {
        max_heights: number("--max-replay-heights")?,
        max_total_bytes: usize::try_from(number("--max-replay-bytes")?)
            .map_err(|_| "history byte budget invalid")?,
        deadline: Instant::now()
            .checked_add(std::time::Duration::from_millis(number("--deadline-ms")?))
            .ok_or("history deadline invalid")?,
        #[cfg(test)]
        diagnostic_sink: None,
    };
    let publication = PublicationBudget {
        max_store_bytes: number("--max-store-bytes")?,
        max_new_bytes: number("--max-new-bytes")?,
        max_store_objects: usize::try_from(number("--max-store-objects")?)
            .map_err(|_| "history object budget invalid")?,
        max_history_cuts: usize::try_from(number("--max-history-cuts")?)
            .map_err(|_| "history cut budget invalid")?,
        #[cfg(test)]
        before_pin: None,
    };
    let commit = rehydrate(
        &PathBuf::from(value("--root")?),
        &PathBuf::from(value("--scratch")?),
        value("--world-id")?,
        value("--node-id")?,
        value("--service-public-key")?,
        number("--height")?,
        replay,
        publication,
        local,
    )?;
    println!("preview_history_rehydrated_height={}", commit.position);
    Ok(())
}
