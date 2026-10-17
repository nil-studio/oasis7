//! Durable, non-activating shadow copy with a frozen-source identity and cursor.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
const PROGRESS: &str = ".shadow-progress-v1";
const STATE: &str = "state.cbor";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    source: PathBuf,
    fingerprint: String,
    optimized: bool,
    end: Option<super::super::physical::roots::PageCursor>,
    cursor: super::super::physical::roots::PageCursor,
    complete: bool,
    verifying: bool,
    cleanup: bool,
    verified_objects: u64,
    logical_bytes: u64,
}
#[derive(Debug, Serialize)]
pub struct FormalStorageShadowProgress {
    pub complete: bool,
    pub phase: String,
    pub processed_records: u64,
    pub total_records: u64,
    pub report: Option<FormalStorageShadowReport>,
}
fn fingerprint(endpoint: &Path) -> Result<String, FormalError> {
    let source = endpoint.join("formal-objects-v1");
    let mut values = Vec::new();
    type FileIdentity = (u64, u64, u64, i64, i64, i64, i64);
    for path in [
        source.clone(),
        source.join(".representation-v1"),
        source.join(".retained-logical-v1/head.cbor"),
        source.join(".generation-current-v1"),
        endpoint.join("formal-checkpoint-v1.cbor"),
    ] {
        let value: Option<FileIdentity> = match fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io(e)),
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(invalid("shadow source identity symlink"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    Some((
                        metadata.dev(),
                        metadata.ino(),
                        metadata.len(),
                        metadata.mtime(),
                        metadata.mtime_nsec(),
                        metadata.ctime(),
                        metadata.ctime_nsec(),
                    ))
                }
                #[cfg(not(unix))]
                {
                    let _ = metadata;
                    return Err(invalid("shadow identity requires Unix"));
                }
            }
        };
        values.push((path, value));
    }
    Ok(blake3::hash(&serde_cbor::to_vec(&values).map_err(io)?)
        .to_hex()
        .to_string())
}
fn save(dir: &Path, state: &State) -> Result<(), FormalError> {
    let temp = dir.join(format!(
        ".state-{}",
        super::super::fencing::fresh_challenge().map_err(io)?
    ));
    write_raw(
        dir,
        temp.file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("shadow state filename"))?,
        &super::super::maintenance_encoding::encode(state)?,
    )?;
    fs::rename(temp, dir.join(STATE)).map_err(io)?;
    sync_directory(dir)
}
fn next(
    cursor: &super::super::physical::roots::PageCursor,
    id: &str,
) -> Result<super::super::physical::roots::PageCursor, FormalError> {
    let mut bytes = hex::decode(&cursor.checksum).map_err(io)?;
    bytes.extend(hex::decode(id).map_err(io)?);
    Ok(super::super::physical::roots::PageCursor {
        count: cursor
            .count
            .checked_add(1)
            .ok_or_else(|| invalid("shadow cursor overflow"))?,
        checksum: blake3::hash(&bytes).to_hex().to_string(),
    })
}
fn finish_report(
    source: &Path,
    destination: &Path,
    state: &State,
) -> Result<FormalStorageShadowReport, FormalError> {
    let source_objects = inventory_storage(source);
    let shadow_objects = inventory_storage(destination);
    if !source_objects.complete || !shadow_objects.complete {
        return Err(invalid("shadow final allocation inventory incomplete"));
    }
    Ok(FormalStorageShadowReport {
        verified_objects: usize::try_from(state.verified_objects)
            .map_err(|_| invalid("shadow report count platform overflow"))?,
        logical_bytes: state.logical_bytes,
        source_objects,
        shadow_objects,
    })
}
const VERIFIED: &str = "verified";
/// The first ledger ordinal is immutable. A crash before the cursor commit can
/// recount that same ordinal; later duplicate ordinals never count it again.
fn account_verified(
    progress: &Path,
    state: &mut State,
    id: &str,
    length: u64,
) -> Result<(), FormalError> {
    let directory = progress.join(VERIFIED);
    let path = directory.join(id);
    let (first, previous_length) = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() != 16 {
                return Err(invalid("shadow verification marker type/length"));
            }
            let mut opening = OpenOptions::new();
            opening.read(true);
            #[cfg(unix)]
            opening.custom_flags(libc::O_NOFOLLOW);
            let mut file = opening.open(&path).map_err(io)?;
            let mut bytes = [0u8; 16];
            use std::io::Read;
            file.read_exact(&mut bytes).map_err(io)?;
            let mut ordinal = [0u8; 8];
            ordinal.copy_from_slice(&bytes[..8]);
            let mut size = [0u8; 8];
            size.copy_from_slice(&bytes[8..]);
            (u64::from_be_bytes(ordinal), u64::from_be_bytes(size))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut bytes = state.cursor.count.to_be_bytes().to_vec();
            bytes.extend_from_slice(&length.to_be_bytes());
            let temp = format!(
                ".marker-{}",
                super::super::fencing::fresh_challenge().map_err(io)?
            );
            write_raw(&directory, &temp, &bytes)?;
            fs::hard_link(directory.join(&temp), &path).map_err(io)?;
            fs::remove_file(directory.join(temp)).map_err(io)?;
            (state.cursor.count, length)
        }
        Err(error) => return Err(io(error)),
    };
    if first > state.cursor.count || previous_length != length {
        return Err(invalid("shadow verification marker ordinal/bytes"));
    }
    if first == state.cursor.count {
        state.verified_objects = state
            .verified_objects
            .checked_add(1)
            .ok_or_else(|| invalid("shadow unique object overflow"))?;
        state.logical_bytes = state
            .logical_bytes
            .checked_add(length)
            .ok_or_else(|| invalid("shadow unique byte overflow"))?;
    }
    Ok(())
}
/// Scratch retirement is independently paged; no payload is re-read for reporting.
fn cleanup_step(progress: &Path) -> Result<bool, FormalError> {
    let mut removed = 0usize;
    for name in [VERIFIED, ".retained-logical-v1"] {
        let directory = progress.join(name);
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io(error)),
            Ok(metadata) if !metadata.is_dir() => {
                return Err(invalid("shadow cleanup scratch directory"));
            }
            Ok(_) => {}
        }
        for entry in fs::read_dir(&directory).map_err(io)? {
            if removed == 1024 {
                break;
            }
            let entry = entry.map_err(io)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(io)?;
            let filename = entry
                .file_name()
                .into_string()
                .map_err(|_| invalid("shadow cleanup filename"))?;
            if !metadata.is_file() {
                return Err(invalid("shadow cleanup scratch regular file"));
            }
            if name == VERIFIED {
                let digest = filename.strip_prefix(".marker-").unwrap_or(&filename);
                super::super::crypto::hash_hex(digest)?;
                if metadata.len() > 16 {
                    return Err(invalid("shadow cleanup marker bound"));
                }
            } else if filename != "head.cbor" {
                if let Some(digest) = filename.strip_prefix(".head-") {
                    super::super::crypto::hash_hex(digest)?;
                } else if !filename.strip_suffix(".bin").is_some_and(|number| {
                    number.len() == 20 && number.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    return Err(invalid("shadow cleanup catalog filename"));
                }
                if metadata.len() > 65536 {
                    return Err(invalid("shadow cleanup catalog bound"));
                }
            } else if metadata.len() > 1024 {
                return Err(invalid("shadow cleanup head bound"));
            }
            fs::remove_file(entry.path()).map_err(io)?;
            removed += 1;
        }
        sync_directory(&directory)?;
        if fs::read_dir(&directory).map_err(io)?.next().is_none() {
            fs::remove_dir(&directory).map_err(io)?;
            sync_directory(progress)?;
        }
        if removed == 1024 {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Each subsequent call reacquires the source lease and rejects identity drift.
/// No accepted source object, authority, checkpoint or selector is changed.
pub fn advance_formal_storage_shadow(
    source_endpoint: &Path,
    destination: &Path,
    options: &FormalStorageShadowOptions,
) -> Result<FormalStorageShadowProgress, FormalError> {
    if !cfg!(unix)
        || options.max_logical_bytes == 0
        || options.max_logical_bytes > MAX_LOGICAL_BYTES
    {
        return Err(invalid("resumable shadow platform/step budget"));
    }
    regular_directory(source_endpoint)?;
    let source_endpoint = source_endpoint.canonicalize().map_err(io)?;
    let source = source_endpoint.join("formal-objects-v1");
    regular_directory(&source)?;
    let lock = source_endpoint.join(".formal-endpoint.lock");
    if !fs::symlink_metadata(&lock).map_err(io)?.is_file() {
        return Err(invalid("resumable shadow source lease type"));
    }
    let mut options_open = OpenOptions::new();
    options_open.read(true).write(true);
    #[cfg(unix)]
    options_open.custom_flags(libc::O_NOFOLLOW);
    let lease = options_open.open(lock).map_err(io)?;
    lease
        .try_lock()
        .map_err(|_| invalid("resumable shadow source busy"))?;
    let parent = destination
        .parent()
        .ok_or_else(|| invalid("resumable shadow destination parent"))?;
    regular_directory(parent)?;
    let resolved = parent.canonicalize().map_err(io)?.join(
        destination
            .file_name()
            .ok_or_else(|| invalid("resumable shadow destination name"))?,
    );
    if resolved.starts_with(&source_endpoint) {
        return Err(invalid("resumable shadow target inside source"));
    }
    let identity = fingerprint(&source_endpoint)?;
    let progress = destination.join(PROGRESS);
    let mut state = if fs::symlink_metadata(destination).is_ok() {
        regular_directory(destination)?;
        regular_directory(&progress)?;
        if !fs::symlink_metadata(destination.join(SHADOW_MARKER))
            .map_err(io)?
            .is_file()
        {
            return Err(invalid("resumable shadow marker missing"));
        }
        let state_path = progress.join(STATE);
        let mut opening = OpenOptions::new();
        opening.read(true);
        #[cfg(unix)]
        opening.custom_flags(libc::O_NOFOLLOW);
        let file = opening.open(state_path).map_err(io)?;
        if !file.metadata().map_err(io)?.is_file() || file.metadata().map_err(io)?.len() > 8192 {
            return Err(invalid("shadow progress state file bound/type"));
        }
        let mut encoded = Vec::new();
        use std::io::Read;
        file.take(8193).read_to_end(&mut encoded).map_err(io)?;
        if encoded.len() > 8192 {
            return Err(invalid("resumable shadow state bound"));
        }
        let value: State = super::super::maintenance_encoding::decode(&encoded)?;
        if value.version != 2
            || value.source != source_endpoint
            || value.fingerprint != identity
            || value.optimized != options.optimized
        {
            return Err(invalid(
                "resumable shadow source or codec identity changed; source retained",
            ));
        }
        value
    } else {
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(destination).map_err(io)?;
        write_raw(
            destination,
            SHADOW_MARKER,
            b"Shadow objects only; never activate as an endpoint.\n",
        )?;
        sync_directory(destination)?;
        sync_directory(parent)?;
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&progress).map_err(io)?;
        sync_directory(destination)?;
        let state = State {
            version: 2,
            source: source_endpoint.clone(),
            fingerprint: identity.clone(),
            optimized: options.optimized,
            end: None,
            cursor: super::super::physical::roots::PageCursor::start(),
            complete: false,
            verifying: false,
            cleanup: false,
            verified_objects: 0,
            logical_bytes: 0,
        };
        save(&progress, &state)?;
        state
    };
    if state.complete && state.end.as_ref() != Some(&state.cursor) {
        return Err(invalid(
            "completed shadow cursor differs from captured head",
        ));
    }
    if state.complete {
        return Ok(FormalStorageShadowProgress {
            complete: true,
            phase: "complete".into(),
            processed_records: state.cursor.count,
            total_records: state.cursor.count,
            report: Some(finish_report(&source, destination, &state)?),
        });
    }
    if state.cleanup {
        if state.end.as_ref() != Some(&state.cursor) || !state.verifying {
            return Err(invalid("shadow cleanup before exact verification"));
        }
        state.complete = cleanup_step(&progress)?;
        save(&progress, &state)?;
        return Ok(FormalStorageShadowProgress {
            complete: state.complete,
            phase: if state.complete {
                "complete"
            } else {
                "cleanup"
            }
            .into(),
            processed_records: state.cursor.count,
            total_records: state.cursor.count,
            report: if state.complete {
                Some(finish_report(&source, destination, &state)?)
            } else {
                None
            },
        });
    }
    if state.end.is_none() {
        // No payload can have been published while capture was incomplete.
        // Rebuild only this builder's private, unselected name-index on retry.
        if state.cursor.count != 0 {
            return Err(invalid("shadow capture cursor invalid"));
        }
        let registry = progress.join(".retained-logical-v1");
        if registry.try_exists().map_err(io)? {
            fs::remove_dir_all(&registry).map_err(io)?;
        }
        super::super::physical::roots::initialize(&progress)?;
        let mut page = Vec::new();
        let mut end = super::super::physical::roots::PageCursor::start();
        let mut flush = |ids: &mut Vec<String>| -> Result<(), FormalError> {
            if ids.is_empty() {
                return Ok(());
            }
            super::super::physical::roots::retain(&progress, ids)?;
            let (_, next, complete) = super::super::physical::roots::page(&progress, &end, 1024)?;
            if !complete {
                return Err(invalid("shadow capture published tail mismatch"));
            }
            end = next;
            ids.clear();
            Ok(())
        };
        super::stream::visit(&source, |id| {
            page.push(id);
            if page.len() == 1024 {
                flush(&mut page)?;
            }
            Ok(())
        })?;
        flush(&mut page)?;
        if fingerprint(&source_endpoint)? != identity {
            return Err(invalid("shadow source changed during capture"));
        }
        state.end = Some(end);
        save(&progress, &state)?;
    }
    if options.optimized {
        super::super::physical::initialize(destination)?;
    }
    let end = state
        .end
        .clone()
        .ok_or_else(|| invalid("shadow captured head missing"))?;
    let mut used = 0u64;
    let mut records = 0usize;
    let mut batch = Vec::new();
    let mut pending = std::collections::BTreeSet::new();
    let mut batch_bytes = 0usize;
    while state.cursor.count < end.count && used < options.max_logical_bytes && records < 1024 {
        let (ids, _, _) =
            super::super::physical::roots::page(&progress, &state.cursor, 1024 - records)?;
        if ids.is_empty() {
            return Err(invalid("shadow captured catalog truncated"));
        }
        for id in ids {
            if used >= options.max_logical_bytes || records == 1024 {
                break;
            }
            records += 1;
            if state.verifying {
                let original = business_pack::read_object(&source.join(&id), &id)?
                    .ok_or_else(|| invalid("shadow verification source absent"))?;
                let rebuilt = business_pack::read_object(&destination.join(&id), &id)?
                    .ok_or_else(|| invalid("shadow verification target absent"))?;
                if original != rebuilt {
                    return Err(invalid("shadow final exact verification mismatch"));
                }
                used = used
                    .checked_add(original.len() as u64)
                    .ok_or_else(|| invalid("shadow verify byte overflow"))?;
                account_verified(&progress, &mut state, &id, original.len() as u64)?;
                state.cursor = next(&state.cursor, &id)?;
                continue;
            }
            let existing = if pending.contains(&id) {
                None
            } else {
                business_pack::read_object(&destination.join(&id), &id)?
            };
            if pending.contains(&id) {
                state.cursor = next(&state.cursor, &id)?;
                continue;
            }
            let bytes = if let Some(bytes) = existing {
                used = used
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(|| invalid("shadow step bytes overflow"))?;
                state.cursor = next(&state.cursor, &id)?;
                continue;
            } else {
                business_pack::read_object(&source.join(&id), &id)?
                    .ok_or_else(|| invalid("shadow captured source missing"))?
            };
            used = used
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| invalid("shadow step bytes overflow"))?;
            if bytes.is_empty() || bytes.len() as u64 > business_pack::MAX_PACK_NODE {
                if !batch.is_empty() {
                    super::stream::flush_verified(destination, &mut batch)?;
                    pending.clear();
                    batch_bytes = 0;
                }
                if !options.optimized
                    || !super::super::physical::persist(
                        destination,
                        &[(id.clone(), bytes.clone())],
                    )?
                {
                    write_raw(destination, &id, &bytes)?;
                }
                if business_pack::read_object(&destination.join(&id), &id)?.as_deref()
                    != Some(bytes.as_slice())
                {
                    return Err(invalid("shadow large copy readback"));
                }
            } else {
                if batch.len() == 1024 || batch_bytes + bytes.len() > BATCH_BYTES {
                    super::stream::flush_verified(destination, &mut batch)?;
                    pending.clear();
                    batch_bytes = 0;
                }
                pending.insert(id.clone());
                batch_bytes += bytes.len();
                batch.push(BusinessIndexNodeWriteV1 {
                    digest: id.clone(),
                    bytes,
                });
            }
            state.cursor = next(&state.cursor, &id)?;
        }
        if !batch.is_empty() {
            super::stream::flush_verified(destination, &mut batch)?;
            pending.clear();
            batch_bytes = 0;
        }
        sync_directory(destination)?;
        if state.verifying {
            sync_directory(&progress.join(VERIFIED))?;
        }
        save(&progress, &state)?;
    }
    if state.cursor.count > end.count || (state.cursor.count == end.count && state.cursor != end) {
        return Err(invalid("shadow catalog final identity mismatch"));
    }
    if fingerprint(&source_endpoint)? != state.fingerprint {
        return Err(invalid("shadow source changed before step commit"));
    }
    if state.cursor == end && !state.verifying {
        let verified = progress.join(VERIFIED);
        if !verified.try_exists().map_err(io)? {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            builder.create(&verified).map_err(io)?;
            sync_directory(&progress)?;
        } else {
            regular_directory(&verified)?;
            if fs::read_dir(&verified).map_err(io)?.next().is_some() {
                return Err(invalid(
                    "shadow unexpected verification scratch before phase",
                ));
            }
        }
        state.verifying = true;
        state.cursor = super::super::physical::roots::PageCursor::start();
        save(&progress, &state)?;
        return Ok(FormalStorageShadowProgress {
            complete: false,
            phase: "verify".into(),
            processed_records: 0,
            total_records: end.count,
            report: None,
        });
    }
    state.cleanup = state.cursor == end;
    save(&progress, &state)?;
    let report = None;
    Ok(FormalStorageShadowProgress {
        complete: state.complete,
        phase: if state.complete {
            "complete"
        } else if state.cleanup {
            "cleanup"
        } else if state.verifying {
            "verify"
        } else {
            "copy"
        }
        .into(),
        processed_records: state.cursor.count,
        total_records: end.count,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (PathBuf, State) {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "oasis7-shadow-count-{}",
            super::super::super::fencing::fresh_challenge().unwrap()
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join(VERIFIED)).unwrap();
        (
            path,
            State {
                version: 2,
                source: PathBuf::new(),
                fingerprint: String::new(),
                optimized: true,
                end: None,
                cursor: super::super::super::physical::roots::PageCursor::start(),
                complete: false,
                verifying: true,
                cleanup: false,
                verified_objects: 0,
                logical_bytes: 0,
            },
        )
    }
    #[test]
    fn verification_statistics_retry_same_ordinal_once_and_deduplicate_later_records() {
        let (path, mut state) = fixture();
        save(&path, &state).unwrap();
        let id = "ab".repeat(32);
        account_verified(&path, &mut state, &id, 42).unwrap();
        // Simulate durable marker publication followed by death before State commit.
        let encoded = fs::read(path.join(STATE)).unwrap();
        let mut retry: State = super::super::super::maintenance_encoding::decode(&encoded).unwrap();
        account_verified(&path, &mut retry, &id, 42).unwrap();
        assert_eq!((retry.verified_objects, retry.logical_bytes), (1, 42));
        retry.cursor.count = 1;
        account_verified(&path, &mut retry, &id, 42).unwrap();
        assert_eq!((retry.verified_objects, retry.logical_bytes), (1, 42));
        assert!(account_verified(&path, &mut retry, &id, 43).is_err());
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn shadow_statistics_scratch_cleanup_is_paged_and_leaves_no_capacity_residue() {
        let (path, _) = fixture();
        for index in 0u32..1100 {
            let id = blake3::hash(&index.to_be_bytes()).to_hex().to_string();
            fs::write(path.join(VERIFIED).join(id), [0; 16]).unwrap();
        }
        super::super::super::physical::roots::initialize(&path).unwrap();
        assert!(!cleanup_step(&path).unwrap());
        assert_eq!(fs::read_dir(path.join(VERIFIED)).unwrap().count(), 76);
        assert!(cleanup_step(&path).unwrap());
        assert!(!path.join(VERIFIED).exists());
        assert!(!path.join(".retained-logical-v1").exists());
        fs::remove_dir_all(path).unwrap();
    }
}
