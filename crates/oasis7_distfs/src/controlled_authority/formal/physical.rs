//! Exact bytes, independent bounded frames and content sharing. Explicit
//! selector locators distinguish representation metadata from arbitrary raw
//! payloads; fresh encoded objects need no duplicate canonical alias. Neither this namespace nor its checksums grant world authority.
use super::{
    FormalError,
    crypto::{hash_hex, invalid},
};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
const SELECTORS: &str = ".representation-v1";
const FRAMES: &str = ".frames-v1";
const ENABLED: &str = ".physical-write-v1";
const GUARD: &str = ".physical-guard-v1";
const PUBLICATION_GUARD: &str = ".physical-publication-guard-v1";
const RETIRE: &str = ".physical-retire-v1.cbor";
const LIMIT: usize = 64 * 1024 * 1024;
const META_LIMIT: usize = 16 * 1024 * 1024;
const FRAME: usize = 65536;
const MAX_OBJECTS: usize = 65536;
#[path = "physical_batch.rs"]
mod batch;
pub(crate) use batch::read_durable_batch;
struct Span {
    id: String,
    start: u64,
    length: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    stored: String,
    original: String,
    length: u32,
    compressed: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Recipe {
    version: u32,
    chunker: u32,
    length: u64,
    #[serde(deserialize_with = "span::objects")]
    objects: Vec<Span>,
    #[serde(deserialize_with = "span::frames")]
    frames: Vec<Frame>,
}
fn io(e: impl std::fmt::Display) -> FormalError {
    FormalError::Io(e.to_string())
}
fn directory(path: &Path) -> Result<(), FormalError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    match builder.create(path) {
        Ok(()) => {
            sync(path)?;
            if let Some(p) = path.parent() {
                sync(p)?;
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(io(e)),
    }
    let m = fs::symlink_metadata(path).map_err(io)?;
    if !m.is_dir() || m.file_type().is_symlink() {
        return Err(invalid("physical regular directory"));
    }
    paths::directory(path)?;
    Ok(())
}
fn read(path: &Path, bound: usize) -> Result<Vec<u8>, FormalError> {
    paths::parents(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    paths::metadata(&metadata, false)?;
    if !metadata.is_file() || metadata.len() > bound as u64 {
        return Err(invalid("physical file bound/type"));
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid takes no pointers and has no caller preconditions.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(invalid("physical registered owner/write mode"));
        }
    }
    let mut bytes = Vec::new();
    file.take(bound as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > bound {
        return Err(invalid("physical read bound"));
    }
    Ok(bytes)
}
fn create(path: &Path, bytes: &[u8]) -> Result<(), FormalError> {
    paths::parents(path)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    let mut file = options.open(path).map_err(io)?;
    paths::metadata(&file.metadata().map_err(io)?, false)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
pub(super) fn enabled(directory: &Path) -> Result<bool, FormalError> {
    let path = directory.join(ENABLED);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io(e)),
        Ok(_) => {
            if read(&path, 64)? != b"oasis7.formal.physical.v1\n" {
                return Err(invalid("physical writer capability version"));
            }
            Ok(true)
        }
    }
}
pub(super) fn initialize(directory: &Path) -> Result<(), FormalError> {
    directory_setup(directory)?;
    let existing = enabled(directory)?;
    for name in [GUARD, PUBLICATION_GUARD] {
        let guard = directory.join(name);
        if !guard.try_exists().map_err(io)? {
            if existing {
                return Err(invalid("enabled physical store lost its fixed fence"));
            }
            create(&guard, b"")?;
            sync(directory)?;
        }
    }
    if !existing {
        roots::initialize(directory)?;
    }
    if !enabled(directory)? {
        publication::publish(
            directory,
            &directory.join(ENABLED),
            b"oasis7.formal.physical.v1\n",
        )?;
        sync(directory)?;
    }
    let _writer = publication_guard(directory)?;
    if existing {
        roots::count(directory)?;
    }
    publication::recover(directory)?;
    Ok(())
}
/// Kernel leases use open file descriptions, not PID/mtime/TTL. A crashed
/// process loses its lease; a surviving reader retains it across writer death.
/// The durable fixed lock inode must never be renamed or replaced.
pub(super) fn pin(root: &Path, exclusive: bool) -> Result<Option<File>, FormalError> {
    let path = root.join(GUARD);
    paths::parents(&path)?;
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if enabled(root)? {
                return Err(invalid("physical reader lease missing"));
            }
            return Ok(None);
        }
        Err(e) => return Err(io(e)),
        Ok(metadata) if !metadata.is_file() => return Err(invalid("physical lease regular file")),
        Ok(_) => {}
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path).map_err(io)?;
    paths::metadata(&file.metadata().map_err(io)?, false)?;
    if exclusive {
        file.try_lock()
            .map_err(|_| FormalError::Conflict("physical reader/writer lease busy".into()))?;
    } else {
        file.try_lock_shared()
            .map_err(|_| FormalError::Conflict("physical retire fence active".into()))?;
    }
    Ok(Some(file))
}
/// Lock ordering is always retirement fence (shared), then publication mutex.
/// Readers take only the first. Immutable append is safe while they retain it;
/// generation selection and reclamation still require exclusive retirement.
pub(crate) struct PublicationGuard {
    root: PathBuf,
    _publication: Option<File>,
    _retirement: Option<File>,
}
pub(crate) fn publication_guard(root: &Path) -> Result<PublicationGuard, FormalError> {
    let retirement = pin(root, false)?;
    let publication = if enabled(root)? {
        let path = root.join(PUBLICATION_GUARD);
        paths::parents(&path)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW);
        let file = options.open(&path).map_err(io)?;
        let metadata = file.metadata().map_err(io)?;
        paths::metadata(&metadata, false)?;
        if metadata.len() != 0 {
            return Err(invalid("physical publication fence payload"));
        }
        file.try_lock()
            .map_err(|_| FormalError::Conflict("physical publication busy".into()))?;
        Some(file)
    } else {
        None
    };
    Ok(PublicationGuard {
        root: root.to_path_buf(),
        _retirement: retirement,
        _publication: publication,
    })
}

fn directory_setup(root: &Path) -> Result<(), FormalError> {
    directory(&root.join(SELECTORS))?;
    directory(&root.join(FRAMES))
}
fn selector(
    path: &Path,
    id: &str,
) -> Result<Option<(PathBuf, std::sync::Arc<Recipe>)>, FormalError> {
    hash_hex(id)?;
    let root = path.parent().ok_or_else(|| invalid("physical root"))?;
    let selected = root.join(SELECTORS).join(id);
    let selected_meta = match fs::symlink_metadata(&selected) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io(e)),
    };
    let alias = match fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(io(e)),
    };
    if !selected_meta.is_file() || alias.as_ref().is_some_and(|m| !m.is_file()) {
        return Err(invalid("physical selector type"));
    }
    #[cfg(unix)]
    if alias
        .as_ref()
        .is_some_and(|m| (m.dev(), m.ino()) != (selected_meta.dev(), selected_meta.ino()))
    {
        return Err(invalid("physical ambiguous canonical alias changed"));
    }
    Ok(Some((
        root.to_path_buf(),
        recipe::decode_recipe_cached(&read(&selected, META_LIMIT)?)?,
    )))
}
fn decode_frame(root: &Path, frame: &Frame) -> Result<Vec<u8>, FormalError> {
    let stored = read(&root.join(FRAMES).join(&frame.stored), FRAME)?;
    if blake3::hash(&stored).to_hex().as_str() != frame.stored {
        return Err(invalid("physical stored checksum"));
    }
    let bytes = if frame.compressed {
        let mut decoder = zstd::stream::read::Decoder::new(stored.as_slice()).map_err(io)?;
        decoder.window_log_max(16).map_err(io)?;
        let mut output = Vec::new();
        decoder
            .take(frame.length as u64 + 1)
            .read_to_end(&mut output)
            .map_err(io)?;
        output
    } else {
        stored
    };
    if bytes.len() != frame.length as usize
        || blake3::hash(&bytes).to_hex().as_str() != frame.original
    {
        return Err(invalid("physical decoded length/checksum"));
    }
    Ok(bytes)
}
fn range(
    root: &Path,
    recipe: &Recipe,
    id: &str,
    offset: u64,
    limit: u32,
) -> Result<(u64, Vec<u8>), FormalError> {
    let index = recipe
        .objects
        .binary_search_by(|entry| entry.id.as_str().cmp(id))
        .map_err(|_| invalid("physical linked object missing"))?;
    let object = &recipe.objects[index];
    if offset > object.length {
        return Err(invalid("physical logical range offset"));
    }
    let start = object.start + offset;
    let end = start + u64::from(limit).min(object.length - offset);
    let mut result = Vec::with_capacity((end - start) as usize);
    let mut position = 0;
    for frame in &recipe.frames {
        let next = position + u64::from(frame.length);
        if position < end && next > start {
            let bytes = decode_frame(root, frame)?;
            result.extend_from_slice(
                &bytes[(start.saturating_sub(position)) as usize
                    ..((end - position).min(frame.length as u64)) as usize],
            );
        }
        position = next;
        if position >= end {
            break;
        }
    }
    Ok((object.length, result))
}
pub(super) fn read_range(
    path: &Path,
    id: &str,
    offset: u64,
    limit: u32,
) -> Result<Option<(u64, Vec<u8>)>, FormalError> {
    let Some((root, recipe)) = selector(path, id)? else {
        return Ok(None);
    };
    range(&root, &recipe, id, offset, limit).map(Some)
}
pub(super) fn read_object(
    path: &Path,
    id: &str,
    durable: bool,
) -> Result<Option<Vec<u8>>, FormalError> {
    read_object_inner(path, id, durable, None)
}
pub(super) fn read_object_durable_once(
    path: &Path,
    id: &str,
    markers: &Path,
) -> Result<Option<Vec<u8>>, FormalError> {
    read_object_inner(path, id, true, Some(markers))
}
fn read_object_inner(
    path: &Path,
    id: &str,
    durable: bool,
    markers: Option<&Path>,
) -> Result<Option<Vec<u8>>, FormalError> {
    let Some((root, recipe)) = selector(path, id)? else {
        return Ok(None);
    };
    let (_, bytes) = range(&root, &recipe, id, 0, LIMIT as u32)?;
    if blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("physical logical checksum"));
    }
    if durable {
        publication::sync_recipe_once(&root, id, &recipe, markers)?;
    }
    Ok(Some(bytes))
}
/// Return false if raw/legacy packing is smaller, including all new frame and
/// recipe bytes. Exact existing frames are shared only within this store root.
pub(super) fn persist(root: &Path, objects: &[(String, Vec<u8>)]) -> Result<bool, FormalError> {
    let writer = publication_guard(root)?;
    persist_guarded(root, objects, &writer)
}
pub(crate) fn persist_guarded(
    root: &Path,
    objects: &[(String, Vec<u8>)],
    writer: &PublicationGuard,
) -> Result<bool, FormalError> {
    if writer.root != root {
        return Err(invalid("physical publication guard scope"));
    }
    if !enabled(root)? || objects.is_empty() {
        return Ok(false);
    }
    if root.join(RETIRE).try_exists().map_err(io)? {
        return Err(invalid(
            "physical interrupted reclamation requires recovery",
        ));
    }
    publication::recover_objects(root, objects)?;
    let objects = publication::with_readback_markers(|markers| {
        objects
            .iter()
            .filter_map(
                |(id, bytes)| match read_object_durable_once(&root.join(id), id, markers) {
                    Ok(Some(previous)) if previous == *bytes => None,
                    Ok(Some(_)) => Some(Err(invalid(
                        "physical existing immutable logical value changed",
                    ))),
                    Ok(None) => Some(Ok((id.clone(), bytes.clone()))),
                    Err(error) => Some(Err(error)),
                },
            )
            .collect::<Result<Vec<_>, FormalError>>()
    })?;
    if objects.is_empty() {
        return Ok(true);
    }
    let objects = objects.as_slice();
    let mut payload = Vec::new();
    let mut spans = Vec::new();
    let mut previous = None;
    for (id, bytes) in objects {
        hash_hex(id)?;
        if objects.len() > MAX_OBJECTS
            || bytes.len() > LIMIT
            || payload
                .len()
                .checked_add(bytes.len())
                .is_none_or(|n| n > LIMIT)
            || previous.is_some_and(|old: &str| old >= id.as_str())
            || blake3::hash(bytes).to_hex().as_str() != id
        {
            return Err(invalid("physical write checksum/order/budget"));
        }
        spans.push(Span {
            id: id.clone(),
            start: payload.len() as u64,
            length: bytes.len() as u64,
        });
        payload.extend_from_slice(bytes);
        previous = Some(id.as_str());
    }
    if payload.len() < 4096 {
        return Ok(false);
    }
    directory_setup(root)?;
    let mut frames = Vec::new();
    let mut fresh = BTreeMap::new();
    for chunk in chunker::chunks(&payload) {
        let compressed = zstd::bulk::compress(chunk, 3).map_err(io)?;
        let (bytes, is_compressed) = if compressed.len() < chunk.len() {
            (compressed, true)
        } else {
            (chunk.to_vec(), false)
        };
        let stored = blake3::hash(&bytes).to_hex().to_string();
        let path = root.join(FRAMES).join(&stored);
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                if read(&path, FRAME)? != bytes {
                    return Err(invalid("physical immutable frame changed"));
                }
                sync(&path)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fresh.insert(stored.clone(), bytes);
            }
            Err(e) => return Err(io(e)),
        }
        frames.push(Frame {
            stored,
            original: blake3::hash(chunk).to_hex().to_string(),
            length: chunk.len() as u32,
            compressed: is_compressed,
        });
    }
    let recipe = Recipe {
        version: 1,
        chunker: 1,
        length: payload.len() as u64,
        objects: spans,
        frames,
    };
    let encoded = encode_recipe(&recipe)?;
    if encoded.len() > META_LIMIT {
        return Err(invalid("physical recipe metadata budget"));
    }
    let new_bytes = fresh.values().try_fold(encoded.len(), |size, bytes| {
        size.checked_add(bytes.len())
            .ok_or_else(|| invalid("physical size overflow"))
    })?;
    if new_bytes >= payload.len() {
        return Ok(false);
    }
    publication::publish_frames(root, fresh, writer)?;
    let first = &objects[0].0;
    let source = root.join(SELECTORS).join(first);
    publication::publish(root, &source, &encoded)?;
    sync(&root.join(SELECTORS))?;
    // A selector without its alias is incomplete and fails closed. Published
    // aliases always point to already durable data and immutable recipe bytes.
    for (id, _) in objects {
        if id != first {
            fs::hard_link(&source, root.join(SELECTORS).join(id)).map_err(io)?;
        }
        publication::fault(3)?;
    }
    sync(&root.join(SELECTORS))?;
    sync(root)?;
    publication::with_readback_markers(|markers| {
        for (id, bytes) in objects {
            if read_object_durable_once(&root.join(id), id, markers)?.as_deref() != Some(bytes) {
                return Err(invalid("physical publication readback"));
            }
        }
        Ok(())
    })?;
    Ok(true)
}

#[path = "physical_gc.rs"]
mod gc;
#[cfg(test)]
#[path = "physical_tests.rs"]
mod tests;
pub use gc::FormalPhysicalReclaimReport;
pub(super) use gc::collect as collect_verified;

#[path = "physical_roots.rs"]
pub(super) mod roots;

#[path = "physical_publication.rs"]
mod publication;

#[path = "physical_chunker.rs"]
mod chunker;

#[path = "physical_recipe.rs"]
mod recipe;
use recipe::{decode_recipe, encode_recipe};

#[path = "physical_generation.rs"]
mod generation;
pub use generation::FormalGenerationProgress;
pub(super) use generation::{
    advance as advance_generation, ids as logical_ids, selected_path,
    visit_ids as visit_logical_ids,
};

#[path = "physical_span.rs"]
mod span;

#[path = "physical_paths.rs"]
pub(super) mod paths;
use paths::sync;

#[path = "physical_staging.rs"]
mod staging;
pub(super) use staging::{preflight as staging_preflight, reclaim as reclaim_staging};
