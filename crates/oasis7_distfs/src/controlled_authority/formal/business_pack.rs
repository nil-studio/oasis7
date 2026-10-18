//! Immutable bounded packs, addressed by logical node hashes through hard links.
//! The pack's index is an untrusted locator: every returned logical object is
//! hash checked. It never authorizes a reservation, proof or qualified head.
use super::business_index::{
    BUSINESS_INDEX_DEPTH, BUSINESS_INDEX_MAX_UPDATES, BusinessIndexNodeWriteV1,
};
use super::{crypto::*, *};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};
const MAGIC: &[u8] = b"oasis7.formal.business-cas.pack.v1\0";
const HEADER: usize = MAGIC.len() + 4 + 4 + 8 + 8;
const INDEX_ENTRY: u64 = 44;
pub(super) const MAX_PACK: u64 = 80 * 1024 * 1024;
const MAX_LOGICAL: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_PACK_NODE: u64 = 4 * 1024 * 1024 + 128;
const MAX_ENTRIES: usize = BUSINESS_INDEX_MAX_UPDATES * (BUSINESS_INDEX_DEPTH + 1);
#[path = "business_pack_legacy_batch.rs"]
mod legacy_batch;
#[cfg(test)]
static NEXT: AtomicU64 = AtomicU64::new(0);
fn io(error: std::io::Error) -> FormalError {
    FormalError::Io(error.to_string())
}
struct ObjectFile {
    file: File,
    start: u64,
    len: u64,
}
// Legacy raw payloads may themselves begin with the marker. Only this rare
// ambiguous/error path hashes the whole bounded file; normal pack lookup stays
// logarithmic and never reads unrelated logical objects.
fn raw_or_invalid(
    mut file: File,
    len: u64,
    id: &str,
    reason: &str,
    expected: Option<usize>,
) -> Result<Option<ObjectFile>, FormalError> {
    if expected.is_some_and(|expected| len != expected as u64) {
        return Err(invalid("CAS expected logical byte length differs"));
    }
    if len > MAX_LOGICAL {
        return Err(invalid(reason));
    }
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut hash = blake3::Hasher::new();
    let mut buffer = [0u8; 65536];
    loop {
        let size = file.read(&mut buffer).map_err(io)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    if hash.finalize().to_hex().as_str() != id {
        return Err(invalid(reason));
    }
    Ok(Some(ObjectFile {
        file,
        start: 0,
        len,
    }))
}
fn open(path: &Path, id: &str) -> Result<Option<ObjectFile>, FormalError> {
    open_expected(path, id, None)
}
fn open_expected(
    path: &Path,
    id: &str,
    expected: Option<usize>,
) -> Result<Option<ObjectFile>, FormalError> {
    hash_hex(id)?;
    super::physical::paths::parents(path)?;
    // Preserve the regular-path boundary on platforms without O_NOFOLLOW.
    // Unix also checks the descriptor below and closes the lookup/open race.
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(invalid("logical CAS path is not a regular file"));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io(error)),
    };
    let metadata = file.metadata().map_err(io)?;
    super::physical::paths::metadata(&metadata, false)?;
    if !metadata.is_file() || metadata.len() > MAX_PACK {
        return Err(invalid("logical CAS regular file/physical bound"));
    }
    let mut prefix = vec![0; std::cmp::min(metadata.len(), MAGIC.len() as u64) as usize];
    file.read_exact(&mut prefix).map_err(io)?;
    if prefix != MAGIC {
        if expected.is_some_and(|expected| metadata.len() != expected as u64) {
            return Err(invalid("CAS expected logical byte length differs"));
        }
        if metadata.len() > MAX_LOGICAL {
            return Err(invalid("raw logical CAS object byte bound"));
        }
        return Ok(Some(ObjectFile {
            file,
            start: 0,
            len: metadata.len(),
        }));
    }
    let mut rest = [0u8; 24];
    if file.read_exact(&mut rest).is_err() {
        return raw_or_invalid(
            file,
            metadata.len(),
            id,
            "truncated CAS pack header",
            expected,
        );
    }
    let version = u32::from_be_bytes(rest[0..4].try_into().unwrap());
    let count = u32::from_be_bytes(rest[4..8].try_into().unwrap()) as usize;
    let index = u64::from_be_bytes(rest[8..16].try_into().unwrap());
    let total = u64::from_be_bytes(rest[16..24].try_into().unwrap());
    let index_bytes = (count as u64)
        .checked_mul(INDEX_ENTRY)
        .ok_or_else(|| invalid("CAS pack index overflow"))?;
    if version != 1
        || count == 0
        || count > MAX_ENTRIES
        || total != metadata.len()
        || index < HEADER as u64
        || index.checked_add(index_bytes) != Some(total)
    {
        return raw_or_invalid(
            file,
            metadata.len(),
            id,
            "CAS pack header/count/index/length",
            expected,
        );
    }
    let wanted = hex::decode(id).map_err(|_| invalid("CAS pack object digest"))?;
    let (mut low, mut high) = (0usize, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let position = index
            .checked_add(
                (middle as u64)
                    .checked_mul(INDEX_ENTRY)
                    .ok_or_else(|| invalid("CAS pack lookup overflow"))?,
            )
            .ok_or_else(|| invalid("CAS pack lookup overflow"))?;
        file.seek(SeekFrom::Start(position)).map_err(io)?;
        let mut row = [0u8; INDEX_ENTRY as usize];
        file.read_exact(&mut row).map_err(io)?;
        match row[..32].cmp(&wanted) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => {
                let start = u64::from_be_bytes(row[32..40].try_into().unwrap());
                let len = u32::from_be_bytes(row[40..44].try_into().unwrap()) as u64;
                if start < HEADER as u64
                    || len == 0
                    || len > MAX_PACK_NODE
                    || start.checked_add(len).is_none_or(|end| end > index)
                {
                    return Err(invalid("CAS pack object span"));
                }
                if expected.is_some_and(|expected| len != expected as u64) {
                    return Err(invalid("CAS expected logical byte length differs"));
                }
                return Ok(Some(ObjectFile { file, start, len }));
            }
        }
    }
    raw_or_invalid(
        file,
        metadata.len(),
        id,
        "CAS pack lacks its linked logical object",
        expected,
    )
}
fn read_open_object(object: &mut ObjectFile, id: &str) -> Result<Vec<u8>, FormalError> {
    let mut bytes = vec![0; object.len as usize];
    object
        .file
        .seek(SeekFrom::Start(object.start))
        .and_then(|_| object.file.read_exact(&mut bytes))
        .map_err(io)?;
    if blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("logical CAS object checksum"));
    }
    Ok(bytes)
}
pub(crate) fn read_object(path: &Path, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
    let _pin = super::physical::pin(
        path.parent().ok_or_else(|| invalid("logical CAS parent"))?,
        false,
    )?;
    read_object_unpinned(path, id)
}
/// Only maintenance already holding the physical retire fence may bypass the
/// reader lease. Ordinary readers must use read_object.
pub(super) fn read_object_unpinned(path: &Path, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
    let selected_path = super::physical::selected_path(path, id)?;
    let path = selected_path.as_path();
    if let Some(bytes) = super::physical::read_object(path, id, false)? {
        return Ok(Some(bytes));
    }
    let Some(mut object) = open(path, id)? else {
        return Ok(None);
    };
    read_open_object(&mut object, id).map(Some)
}
pub(crate) fn read_object_durable(path: &Path, id: &str) -> Result<Option<Vec<u8>>, FormalError> {
    let _pin = super::physical::pin(
        path.parent().ok_or_else(|| invalid("logical CAS parent"))?,
        false,
    )?;
    let selected_path = super::physical::selected_path(path, id)?;
    let path = selected_path.as_path();
    if let Some(bytes) = super::physical::read_object(path, id, true)? {
        return Ok(Some(bytes));
    }
    let Some(mut object) = open(path, id)? else {
        return Ok(None);
    };
    let bytes = read_open_object(&mut object, id)?;
    object.file.sync_all().map_err(io)?;
    Ok(Some(bytes))
}
#[path = "business_pack_readback.rs"]
mod readback;
pub(crate) use readback::{begin_readback_trace, finish_readback_trace, read_object_durable_once};
pub(crate) fn read_range(
    path: &Path,
    id: &str,
    offset: u64,
    limit: u32,
) -> Result<Option<(u64, Vec<u8>)>, FormalError> {
    if limit == 0 || limit > 1024 * 1024 || offset > MAX_LOGICAL {
        return Err(invalid("logical CAS range bound"));
    }
    let _pin = super::physical::pin(
        path.parent().ok_or_else(|| invalid("logical CAS parent"))?,
        false,
    )?;
    let selected_path = super::physical::selected_path(path, id)?;
    let path = selected_path.as_path();
    if let Some(value) = super::physical::read_range(path, id, offset, limit)? {
        return Ok(Some(value));
    }
    let Some(mut object) = open(path, id)? else {
        return Ok(None);
    };
    if offset > object.len {
        return Err(invalid("logical CAS range offset"));
    }
    let len = std::cmp::min(limit as u64, object.len - offset) as usize;
    let mut bytes = vec![0; len];
    let start = object
        .start
        .checked_add(offset)
        .ok_or_else(|| invalid("logical CAS seek overflow"))?;
    object
        .file
        .seek(SeekFrom::Start(start))
        .and_then(|_| object.file.read_exact(&mut bytes))
        .map_err(io)?;
    Ok(Some((object.len, bytes)))
}
const STAGING: &str = ".business-pack-staging-v1";
const STAGE_PREFIX: &str = "pack-";
/// Called only after the owning endpoint's exclusive filesystem lock is held.
/// Only staging names are inspected; digest hard links are never removed.
pub(crate) fn cleanup_staging_locked(directory: &Path) -> Result<(), FormalError> {
    let staging = staging_directory(directory)?;
    for entry in fs::read_dir(&staging).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let Some(suffix) = name
            .to_str()
            .and_then(|name| name.strip_prefix(STAGE_PREFIX))
        else {
            return Err(invalid("unexpected business staging name"));
        };
        if suffix.len() != 64
            || hex::decode(suffix).is_err()
            || !entry.file_type().map_err(io)?.is_file()
        {
            return Err(invalid("business staging exact name/regular file"));
        }
        fs::remove_file(entry.path()).map_err(io)?;
    }
    File::open(staging)
        .and_then(|file| file.sync_all())
        .map_err(io)
}
fn staging_directory(directory: &Path) -> Result<std::path::PathBuf, FormalError> {
    let staging = directory.join(STAGING);
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(&staging) {
        Ok(()) => {
            File::open(directory)
                .and_then(|file| file.sync_all())
                .map_err(io)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(io(error)),
    }
    let metadata = fs::symlink_metadata(&staging).map_err(io)?;
    if !metadata.is_dir() {
        return Err(invalid("business staging regular directory"));
    }
    #[cfg(unix)]
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
        return Err(invalid("business staging private owner/mode"));
    }
    Ok(staging)
}
struct StagingGuard(std::path::PathBuf);
impl Drop for StagingGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        if let Some(parent) = self.0.parent() {
            let _ = File::open(parent).and_then(|file| file.sync_all());
        }
    }
}
/// All bytes are durable before names are durable; the caller publishes its
/// Prepared checkpoint only after this succeeds. Partial links remain orphans.
pub(crate) fn persist(
    directory: &Path,
    writes: &[BusinessIndexNodeWriteV1],
) -> Result<(), FormalError> {
    persist_inner(directory, writes, None)
}
// The physical selector was absent inside the caller's pinned batch. Inspect
// raw/legacy pack bounds before allocating a logical object, not afterwards.
#[cfg(any(not(unix), test))]
pub(super) fn read_legacy_durable_expected(
    path: &Path,
    id: &str,
    expected: usize,
) -> Result<Option<Vec<u8>>, FormalError> {
    let Some(mut object) = open_expected(path, id, Some(expected))? else {
        return Ok(None);
    };
    if object.len != expected as u64 {
        return Err(invalid("business existing logical length differs"));
    }
    let bytes = read_open_object(&mut object, id)?;
    object.file.sync_all().map_err(io)?;
    Ok(Some(bytes))
}
fn persist_inner(
    directory: &Path,
    writes: &[BusinessIndexNodeWriteV1],
    interrupt_after_links: Option<usize>,
) -> Result<(), FormalError> {
    let writer = super::physical::publication_guard(directory)?;
    if writes.len() > MAX_ENTRIES {
        return Err(invalid("business CAS pack entry bound"));
    }
    let mut needed = Vec::new();
    let mut previous = None;
    let mut data_len = 0usize;
    for write in writes {
        hash_hex(&write.digest)?;
        if write.bytes.is_empty()
            || write.bytes.len() as u64 > MAX_PACK_NODE
            || blake3::hash(&write.bytes).to_hex().as_str() != write.digest
            || previous.is_some_and(|old: &str| old >= write.digest.as_str())
        {
            return Err(invalid("business CAS pack node checksum/order/bound"));
        }
        previous = Some(write.digest.as_str());
        data_len = data_len
            .checked_add(write.bytes.len())
            .ok_or_else(|| invalid("business CAS pack data overflow"))?;
        if data_len > 64 * 1024 * 1024 {
            return Err(invalid("business CAS pack data bound"));
        }
    }
    // One read unit has bounded output and fresh physical evidence. Split the
    // candidate rather than retaining decoded frames across batches.
    let mut begin = 0usize;
    while begin < writes.len() {
        let mut end = begin;
        let mut bytes = 0usize;
        while end < writes.len() && end - begin < 1024 {
            let next = writes[end].bytes.len();
            if end > begin && bytes + next > 8 * 1024 * 1024 {
                break;
            }
            bytes += next;
            end += 1;
        }
        let _pin = super::physical::pin(directory, false)?;
        let requests = writes[begin..end]
            .iter()
            .map(|write| {
                Ok((
                    super::physical::selected_path(&directory.join(&write.digest), &write.digest)?,
                    write.digest.clone(),
                    write.bytes.len(),
                ))
            })
            .collect::<Result<Vec<_>, FormalError>>()?;
        let existing = if bytes <= 8 * 1024 * 1024 {
            super::physical::read_durable_batch(directory, &requests)?
        } else {
            // Larger logical objects retain the original single-object bound.
            vec![read_object_durable(&requests[0].0, &requests[0].1)?]
        };
        let legacy_requests = requests
            .iter()
            .zip(&existing)
            .filter(|(_, physical)| physical.is_none())
            .map(|(request, _)| request.clone())
            .collect::<Vec<_>>();
        let mut legacy = legacy_batch::read_durable_batch(&legacy_requests)?.into_iter();
        for (write, physical) in writes[begin..end].iter().zip(existing) {
            let current = match physical {
                Some(bytes) => Some(bytes),
                None => legacy
                    .next()
                    .ok_or_else(|| invalid("business legacy batch result missing"))?,
            };
            match current {
                Some(bytes) if bytes != write.bytes => {
                    return Err(invalid("business immutable logical CAS changed"));
                }
                Some(_) => {}
                None => needed.push(write),
            }
        }
        begin = end;
    }
    if needed.is_empty() {
        File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(io)?;
        if super::physical::enabled(directory)? {
            super::physical::roots::retain(
                directory,
                &writes.iter().map(|w| w.digest.clone()).collect::<Vec<_>>(),
            )?;
        }
        return Ok(());
    }
    let physical = needed
        .iter()
        .map(|w| (w.digest.clone(), w.bytes.clone()))
        .collect::<Vec<_>>();
    if super::physical::persist_guarded(directory, &physical, &writer)? {
        super::physical::roots::retain(
            directory,
            &writes.iter().map(|w| w.digest.clone()).collect::<Vec<_>>(),
        )?;
        return Ok(());
    }
    let index_offset = needed.iter().try_fold(HEADER as u64, |len, write| {
        len.checked_add(write.bytes.len() as u64)
            .ok_or_else(|| invalid("business pack length overflow"))
    })?;
    let total = index_offset
        .checked_add(
            (needed.len() as u64)
                .checked_mul(INDEX_ENTRY)
                .ok_or_else(|| invalid("business pack index overflow"))?,
        )
        .ok_or_else(|| invalid("business pack total overflow"))?;
    if total > MAX_PACK {
        return Err(invalid("business CAS physical pack byte bound"));
    }
    let staging = staging_directory(directory)?;
    let path = staging.join(format!(
        "{STAGE_PREFIX}{}",
        fencing::fresh_challenge().map_err(|error| FormalError::Io(error.to_string()))?
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path).map_err(io)?;
    let _cleanup = StagingGuard(path.clone());
    file.write_all(MAGIC)
        .and_then(|_| file.write_all(&1u32.to_be_bytes()))
        .and_then(|_| file.write_all(&(needed.len() as u32).to_be_bytes()))
        .and_then(|_| file.write_all(&index_offset.to_be_bytes()))
        .and_then(|_| file.write_all(&total.to_be_bytes()))
        .map_err(io)?;
    for write in &needed {
        file.write_all(&write.bytes).map_err(io)?;
    }
    let mut offset = HEADER as u64;
    for write in &needed {
        file.write_all(&hex::decode(&write.digest).map_err(|_| invalid("pack index digest"))?)
            .and_then(|_| file.write_all(&offset.to_be_bytes()))
            .and_then(|_| file.write_all(&(write.bytes.len() as u32).to_be_bytes()))
            .map_err(io)?;
        offset = offset
            .checked_add(write.bytes.len() as u64)
            .ok_or_else(|| invalid("business pack span overflow"))?;
    }
    file.sync_all().map_err(io)?;
    // Only the Rust test binary contains this hook; production has no env or
    // transport path that can select a crash point.
    #[cfg(all(test, unix))]
    if interrupt_after_links == Some(usize::MAX) {
        wait_for_test_sigkill();
    }
    for (linked, write) in needed.iter().enumerate() {
        if interrupt_after_links == Some(linked) {
            return Err(invalid(
                "injected interruption before durable name publication",
            ));
        }
        fs::hard_link(&path, directory.join(&write.digest)).map_err(io)?;
        #[cfg(all(test, unix))]
        if interrupt_after_links == Some(usize::MAX - 1) && linked == 0 {
            wait_for_test_sigkill();
        }
    }
    fs::remove_file(path).map_err(io)?;
    File::open(staging)
        .and_then(|file| file.sync_all())
        .map_err(io)?;
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(io)?;
    if super::physical::enabled(directory)? {
        super::physical::roots::retain(
            directory,
            &writes.iter().map(|w| w.digest.clone()).collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
fn wait_for_test_sigkill() -> ! {
    if unsafe { libc::kill(libc::getpid(), libc::SIGKILL) } != 0 {
        std::process::abort();
    }
    // macOS can return before delivering the signal. Never race it with SIGABRT.
    loop {
        std::thread::park();
    }
}

#[cfg(test)]
#[path = "business_pack_tests.rs"]
mod tests;

#[cfg(all(test, unix))]
pub(crate) fn persist_kill_after_first_link_for_test(
    directory: &Path,
    writes: &[BusinessIndexNodeWriteV1],
) -> Result<(), FormalError> {
    persist_inner(directory, writes, Some(usize::MAX - 1))
}
