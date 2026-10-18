//! Resumable bounded catalogs and exact-copy physical generations.
use super::*;
use std::collections::BTreeSet;
#[path = "physical_generation_catalog.rs"]
mod catalog;
#[path = "physical_generation_retire.rs"]
mod retirement;
#[path = "physical_generation_transaction.rs"]
mod transaction;
use catalog::Catalogs;
use retirement::retire;
const GENERATIONS: &str = ".generations-v1";
const CURRENT: &str = ".generation-current-v1";
const BUILD: &str = ".generation-build-v1.cbor";
const MANIFEST: &str = ".manifest-v3.cbor";
const BATCH: usize = 8 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    nonce: String,
    catalogs: Catalogs,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Build {
    version: u32,
    nonce: String,
    previous: Option<String>,
    previous_manifest: Option<Manifest>,
    objects: Catalogs,
    completed: Catalogs,
    cursor: roots::PageCursor,
    sealed: Option<String>,
    unsealing: Option<String>,
    retire_after: Option<String>,
    old_checked: Option<String>,
    old_verified: bool,
    catalog_retired: Vec<catalog::Retired>,
}
#[derive(Debug, Serialize)]
pub struct FormalGenerationProgress {
    pub building: bool,
    pub selected: Option<String>,
    pub copied_objects: usize,
    pub total_objects: usize,
}
fn current(root: &Path) -> Result<Option<String>, FormalError> {
    match fs::symlink_metadata(root.join(CURRENT)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(e)),
        Ok(_) => {
            let id = String::from_utf8(read(&root.join(CURRENT), 64)?).map_err(io)?;
            hash_hex(&id)?;
            Ok(Some(id))
        }
    }
}
fn manifest(root: &Path, id: &str) -> Result<Manifest, FormalError> {
    hash_hex(id)?;
    for dir in [root.join(GENERATIONS), root.join(GENERATIONS).join(id)] {
        if !fs::symlink_metadata(dir).map_err(io)?.is_dir() {
            return Err(invalid("generation regular directory"));
        }
    }
    let bytes = read(&root.join(GENERATIONS).join(id).join(MANIFEST), META_LIMIT)?;
    if blake3::hash(&bytes).to_hex().as_str() != id {
        return Err(invalid("generation manifest checksum"));
    }
    let value: Manifest = serde_cbor::from_slice(&bytes).map_err(io)?;
    if value.version != 3 {
        return Err(invalid("generation manifest schema"));
    }
    hash_hex(&value.nonce)?;
    catalog::validate(&value.catalogs)?;
    Ok(value)
}
pub(crate) fn selected_path(path: &Path, id: &str) -> Result<PathBuf, FormalError> {
    hash_hex(id)?;
    let root = path
        .parent()
        .ok_or_else(|| invalid("generation logical root"))?;
    let Some(generation) = current(root)? else {
        return Ok(path.to_path_buf());
    };
    let selected = manifest(root, &generation)?;
    let dir = root.join(GENERATIONS).join(generation);
    if catalog::contains(&dir, &selected.catalogs, id)? {
        let logical = dir.join(id);
        let regular = |path: &Path| -> Result<bool, FormalError> {
            match fs::symlink_metadata(path) {
                Ok(meta) if meta.is_file() => Ok(true),
                Ok(_) => Err(invalid("generation alias type")),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(io(e)),
            }
        };
        if !regular(&logical)? && !regular(&dir.join(SELECTORS).join(id))? {
            return Err(invalid(
                "selected generation object missing; stale overlay refused",
            ));
        }
        Ok(logical)
    } else {
        Ok(path.to_path_buf())
    }
}
fn visit_aliases(
    root: &Path,
    visitor: &mut impl FnMut(String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("generation nonUTF8 entry"))?;
        if [GUARD, PUBLICATION_GUARD].contains(&name.as_str()) {
            if !read(&entry.path(), 1)?.is_empty() {
                return Err(invalid("generation stable guard contents"));
            }
            continue;
        }
        if name.len() == 64 && hash_hex(&name).is_ok() {
            if !fs::symlink_metadata(entry.path()).map_err(io)?.is_file() {
                return Err(invalid("generation alias type"));
            }
            visitor(name)?;
        } else if ![
            SELECTORS,
            FRAMES,
            ENABLED,
            GUARD,
            PUBLICATION_GUARD,
            RETIRE,
            GENERATIONS,
            CURRENT,
            BUILD,
            ".physical-staging-v1",
            ".retained-logical-v1",
            ".business-pack-staging-v1",
        ]
        .contains(&name.as_str())
            && !name.starts_with(".generation-temp-")
        {
            return Err(invalid("generation unknown namespace entry"));
        }
    }
    for entry in fs::read_dir(root.join(SELECTORS)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("generation selector nonUTF8"))?;
        hash_hex(&id)?;
        if !fs::symlink_metadata(entry.path()).map_err(io)?.is_file() {
            return Err(invalid("generation selector type"));
        }
        visitor(id)?;
    }
    Ok(())
}
/// Streams selected catalogs and overlay aliases. Duplicates across these
/// immutable representations are intentional; consumers de-duplicate on disk.
pub(crate) fn visit_ids(
    root: &Path,
    mut visitor: impl FnMut(String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    if let Some(id) = current(root)? {
        let selected = manifest(root, &id)?;
        let dir = root.join(GENERATIONS).join(id);
        catalog::visit(&dir, &selected.catalogs, &mut visitor)?;
    }
    visit_aliases(root, &mut visitor)
}
/// Compatibility with the original bounded shadow operation; generation build
/// and the streaming shadow never use this all-ID collection.
pub(crate) fn ids(root: &Path) -> Result<BTreeSet<String>, FormalError> {
    let mut ids = BTreeSet::new();
    visit_ids(root, |id| {
        ids.insert(id);
        if ids.len() > MAX_OBJECTS {
            return Err(invalid(
                "bounded logical inventory count; use streaming operation",
            ));
        }
        Ok(())
    })?;
    Ok(ids)
}
fn atomic(root: &Path, path: &Path, bytes: &[u8]) -> Result<(), FormalError> {
    let temp = root.join(format!(
        ".generation-temp-{}",
        super::super::fencing::fresh_challenge().map_err(io)?
    ));
    create(&temp, bytes)?;
    fs::rename(&temp, path).map_err(io)?;
    sync(root)
}
fn save(root: &Path, state: &Build) -> Result<(), FormalError> {
    atomic(
        root,
        &root.join(BUILD),
        &super::super::maintenance_encoding::encode(state)?,
    )
}
fn load(root: &Path) -> Result<Build, FormalError> {
    let value: Build =
        super::super::maintenance_encoding::decode(&read(&root.join(BUILD), META_LIMIT)?)?;
    if value.version != 3 {
        return Err(invalid("generation build version"));
    }
    hash_hex(&value.nonce)?;
    if let Some(id) = &value.old_checked {
        hash_hex(id)?;
    }
    catalog::validate_retired(&value.catalog_retired)?;
    catalog::validate(&value.objects)?;
    catalog::validate(&value.completed)?;
    for id in [
        value.previous.as_ref(),
        value.sealed.as_ref(),
        value.unsealing.as_ref(),
        value.retire_after.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        hash_hex(id)?;
    }
    Ok(value)
}
fn building(root: &Path, state: &Build) -> PathBuf {
    if let Some(id) = &state.sealed {
        let sealed = root.join(GENERATIONS).join(id);
        if sealed.is_dir() {
            return sealed;
        }
    }
    root.join(GENERATIONS)
        .join(format!("building-{}", state.nonce))
}
fn progress(root: &Path, state: &Build) -> Result<FormalGenerationProgress, FormalError> {
    Ok(FormalGenerationProgress {
        building: true,
        selected: current(root)?,
        copied_objects: catalog::count(&state.completed)?,
        total_objects: catalog::count(&state.objects)?,
    })
}
/// Each call authenticates <=1024 registry records and copies <=1024 objects /
/// 8 MiB (one larger bounded object). No fixed total-world object or byte cap.
pub(crate) fn advance(root: &Path) -> Result<FormalGenerationProgress, FormalError> {
    if !enabled(root)? || pin(root, false)?.is_none() {
        return Err(invalid(
            "generation requires fresh-world physical capability and fixed fence",
        ));
    }
    if root.join(RETIRE).try_exists().map_err(io)? {
        return Err(invalid("generation waits for interrupted GC"));
    }
    directory(&root.join(GENERATIONS))?;
    let mut state = if root.join(BUILD).try_exists().map_err(io)? {
        load(root)?
    } else {
        let _fence = pin(root, true)?;
        let previous = current(root)?;
        let previous_manifest = previous.as_ref().map(|id| manifest(root, id)).transpose()?;
        let state = Build {
            version: 3,
            nonce: super::super::fencing::fresh_challenge().map_err(io)?,
            previous,
            previous_manifest,
            objects: Catalogs::new(),
            completed: Catalogs::new(),
            cursor: roots::PageCursor::start(),
            sealed: None,
            unsealing: None,
            retire_after: None,
            old_checked: None,
            old_verified: false,
            catalog_retired: Vec::new(),
        };
        save(root, &state)?;
        state
    };
    if let Some(selected) = current(root)? {
        if state.sealed.as_ref() == Some(&selected) {
            return retire(root, &mut state, &selected);
        }
        if Some(selected) != state.previous {
            return Err(invalid("generation selector changed outside transaction"));
        }
    } else if state.previous.is_some() {
        return Err(invalid("generation lost original selector"));
    }
    if state.unsealing.is_some() {
        transaction::recover_unseal(root, &mut state)?;
    }
    let dir = building(root, &state);
    directory(&dir)?;
    initialize(&dir)?;
    if !state.catalog_retired.is_empty() {
        catalog::prune(
            &dir,
            [&state.objects, &state.completed],
            &mut state.catalog_retired,
        )?;
        save(root, &state)?;
        return progress(root, &state);
    }
    if state.sealed.is_none() {
        let (ids, next, complete) = roots::page(root, &state.cursor, 1024)?;
        catalog::merge_retired(
            &mut state.catalog_retired,
            catalog::add(&dir, &mut state.objects, &ids)?,
        );
        state.cursor = next;
        let pending = catalog::pending(&dir, &state.objects, &state.completed, 1024)?;
        let mut batch = Vec::new();
        let mut size = 0usize;
        for id in pending {
            let bytes = super::super::business_pack::read_object(&root.join(&id), &id)?
                .ok_or_else(|| invalid("generation source missing"))?;
            if !batch.is_empty() && size.saturating_add(bytes.len()) > BATCH {
                break;
            }
            size = size
                .checked_add(bytes.len())
                .ok_or_else(|| invalid("generation byte overflow"))?;
            batch.push((id, bytes));
            if size >= BATCH {
                break;
            }
        }
        if !batch.is_empty() {
            let writes = batch
                .iter()
                .map(
                    |(id, bytes)| super::super::business_index::BusinessIndexNodeWriteV1 {
                        digest: id.clone(),
                        bytes: bytes.clone(),
                    },
                )
                .collect::<Vec<_>>();
            if writes.iter().all(|w| {
                !w.bytes.is_empty()
                    && w.bytes.len() as u64 <= super::super::business_pack::MAX_PACK_NODE
            }) {
                super::super::business_pack::persist(&dir, &writes)?;
            } else {
                for (id, bytes) in &batch {
                    if !persist(&dir, &[(id.clone(), bytes.clone())])? {
                        publication::publish(&dir, &dir.join(id), bytes)?;
                    }
                }
            }
            let mut done = Vec::new();
            for (id, bytes) in batch {
                if super::super::business_pack::read_object_durable(&dir.join(&id), &id)?.as_deref()
                    != Some(bytes.as_slice())
                {
                    return Err(invalid("generation exact independent readback mismatch"));
                }
                done.push(id);
            }
            catalog::merge_retired(
                &mut state.catalog_retired,
                catalog::add(&dir, &mut state.completed, &done)?,
            );
        }
        save(root, &state)?;
        if !complete || state.objects != state.completed || !state.catalog_retired.is_empty() {
            return progress(root, &state);
        }
        let _fence = pin(root, true)?;
        let (late, next, complete) = roots::page(root, &state.cursor, 1024)?;
        if !late.is_empty() || !complete {
            catalog::merge_retired(
                &mut state.catalog_retired,
                catalog::add(&dir, &mut state.objects, &late)?,
            );
            state.cursor = next;
            save(root, &state)?;
            return progress(root, &state);
        }
        let encoded = serde_cbor::to_vec(&Manifest {
            version: 3,
            nonce: state.nonce.clone(),
            catalogs: state.completed.clone(),
        })
        .map_err(io)?;
        let selected = blake3::hash(&encoded).to_hex().to_string();
        publication::publish(&dir, &dir.join(MANIFEST), &encoded)?;
        sync(&dir)?;
        state.sealed = Some(selected.clone());
        save(root, &state)?;
        let destination = root.join(GENERATIONS).join(&selected);
        if !destination.try_exists().map_err(io)? {
            fs::rename(&dir, &destination).map_err(io)?;
            sync(&root.join(GENERATIONS))?;
        }
        publication::fault(4)?;
        atomic(root, &root.join(CURRENT), selected.as_bytes())?;
        publication::fault(5)?;
        drop(_fence);
        return retire(root, &mut state, &selected);
    }
    // A crash after sealing but before selector publication resumes the exact
    // same generation; accepted overlay writes are captured before selection.
    let _fence = pin(root, true)?;
    let (_, _, complete) = roots::page(root, &state.cursor, 1024)?;
    if !complete || roots::count(root)? != state.cursor.count {
        drop(_fence);
        transaction::unseal(root, &mut state)?;
        return progress(root, &state);
    }
    let selected = state
        .sealed
        .clone()
        .ok_or_else(|| invalid("generation sealed identity missing"))?;
    manifest(root, &selected)?;
    atomic(root, &root.join(CURRENT), selected.as_bytes())?;
    drop(_fence);
    retire(root, &mut state, &selected)
}
#[cfg(test)]
#[path = "physical_generation_tests.rs"]
mod tests;
