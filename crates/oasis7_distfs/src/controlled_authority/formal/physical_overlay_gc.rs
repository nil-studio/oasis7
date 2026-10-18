//! Paged overlay closure verification precedes every root-frame sweep.
//! Durable catalogs record progress, but a reopened owner revalidates the source
//! closure. A saved sealed bit alone never authorizes deletion.
use super::*;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    selected: String,
    logical_count: u64,
    selectors: [i128; 4],
    frames: [i128; 4],
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    source: Binding,
    marks: Catalogs,
    sealed: bool,
    cleaning: bool,
    retired: Vec<catalog::Retired>,
}
#[derive(Default)]
pub(super) struct Session {
    nonce: Option<String>,
    commitment: Option<[u8; 32]>,
    selectors: Option<fs::ReadDir>,
    frames: Option<fs::ReadDir>,
    pending_selector: Option<fs::DirEntry>,
    pending_frame: Option<fs::DirEntry>,
}
fn identity(path: &Path) -> Result<[i128; 4], FormalError> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    super::super::paths::metadata(&meta, true)?;
    #[cfg(unix)]
    return Ok([
        meta.dev() as i128,
        meta.ino() as i128,
        meta.ctime() as i128,
        meta.ctime_nsec() as i128,
    ]);
    #[cfg(not(unix))]
    Ok([
        meta.len() as i128,
        meta.modified()
            .map_err(io)?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io)?
            .as_nanos() as i128,
        0,
        0,
    ])
}
fn binding(root: &Path, selected: &str) -> Result<Binding, FormalError> {
    Ok(Binding {
        selected: selected.into(),
        logical_count: roots::count(root)?,
        selectors: identity(&root.join(SELECTORS))?,
        frames: identity(&root.join(FRAMES))?,
    })
}
fn name(entry: &fs::DirEntry) -> Result<String, FormalError> {
    let id = entry
        .file_name()
        .into_string()
        .map_err(|_| invalid("overlay GC UTF8"))?;
    hash_hex(&id)?;
    super::super::paths::metadata(&fs::symlink_metadata(entry.path()).map_err(io)?, false)?;
    Ok(id)
}
fn directory_for(root: &Path, nonce: &str) -> PathBuf {
    root.join(format!(".generation-temp-overlay-{nonce}"))
}
fn commitment(build: &Build) -> Result<[u8; 32], FormalError> {
    Ok(*blake3::hash(&serde_cbor::to_vec(&build.overlay).map_err(io)?).as_bytes())
}
fn save_progress(root: &Path, build: &Build, session: &mut Session) -> Result<(), FormalError> {
    save(root, build)?;
    session.commitment = Some(commitment(build)?);
    Ok(())
}
pub(super) fn advance(
    root: &Path,
    build: &mut Build,
    selected: &str,
    session: &mut Session,
    budget: GenerationBudget,
) -> Result<bool, FormalError> {
    let dir = directory_for(root, &build.nonce);
    if session.nonce.as_ref() == Some(&build.nonce)
        && session.commitment != Some(commitment(build)?)
    {
        return Err(invalid(
            "overlay GC durable progress differs from live owner",
        ));
    }
    let source = binding(root, selected)?;
    if session.nonce.as_ref() != Some(&build.nonce)
        || build
            .overlay
            .as_ref()
            .is_none_or(|state| state.source != source)
    {
        directory(&dir)?;
        build.overlay = Some(State {
            source,
            marks: Catalogs::new(),
            sealed: false,
            cleaning: false,
            retired: Vec::new(),
        });
        session.nonce = Some(build.nonce.clone());
        session.selectors = Some(fs::read_dir(root.join(SELECTORS)).map_err(io)?);
        session.frames = None;
        session.pending_selector = None;
        session.pending_frame = None;
        save_progress(root, build, session)?;
    }
    let state = build
        .overlay
        .as_mut()
        .ok_or_else(|| invalid("overlay GC state"))?;
    if state.cleaning {
        return retirement::cleanup_overlay_catalog(&dir, budget);
    }
    if !state.retired.is_empty() {
        catalog::prune_with_limit(
            &dir,
            [&state.marks, &state.marks],
            &mut state.retired,
            budget.objects,
        )?;
        save_progress(root, build, session)?;
        return Ok(false);
    }
    if !state.sealed {
        let entries = session
            .selectors
            .as_mut()
            .ok_or_else(|| invalid("overlay GC capture"))?;
        let mut bytes = 0usize;
        let mut exhausted = false;
        for _ in 0..budget.objects {
            let entry = match session.pending_selector.take() {
                Some(entry) => entry,
                None => match entries.next() {
                    Some(entry) => entry.map_err(io)?,
                    None => {
                        exhausted = true;
                        break;
                    }
                },
            };
            let id = name(&entry)?;
            let path = root.join(&id);
            // retire already owns this exact root exclusively. A business-pack
            // helper would attempt a nested shared pin and conflict with it.
            let (_, recipe) =
                selector(&path, &id)?.ok_or_else(|| invalid("overlay GC selector missing"))?;
            let length = recipe
                .objects
                .iter()
                .find(|object| object.id == id)
                .ok_or_else(|| invalid("overlay GC object length missing"))?
                .length;
            if bytes > 0 && (bytes as u64).saturating_add(length) > budget.bytes as u64 {
                session.pending_selector = Some(entry);
                break;
            }
            let logical = read_object(&path, &id, true)?
                .ok_or_else(|| invalid("overlay GC logical object missing"))?;
            bytes = bytes
                .checked_add(logical.len())
                .ok_or_else(|| invalid("overlay GC bytes"))?;
            // The original single-object bound also bounds this temporary list.
            for frames in recipe.frames.chunks(1024) {
                let ids = frames
                    .iter()
                    .map(|frame| frame.stored.clone())
                    .collect::<Vec<_>>();
                catalog::merge_retired(
                    &mut state.retired,
                    catalog::add(&dir, &mut state.marks, &ids)?,
                );
            }
            if bytes >= budget.bytes || budget.expired() {
                break;
            }
        }
        if exhausted {
            if binding(root, selected)? != state.source {
                session.nonce = None;
            } else {
                state.sealed = true;
                session.selectors = None;
                session.frames = Some(fs::read_dir(root.join(FRAMES)).map_err(io)?);
            }
        }
        save_progress(root, build, session)?;
        return Ok(false);
    }
    // All protocol mutations use atomic publication under this physical fence.
    // Directory identity/change stamps only invalidate the mark; they never
    // certify the payload, which was freshly verified during the entire mark.
    if binding(root, selected)? != state.source {
        session.nonce = None;
        return Ok(false);
    }
    let entries = session
        .frames
        .as_mut()
        .ok_or_else(|| invalid("overlay GC sweep"))?;
    let mut reclaim = Vec::new();
    let mut bytes = 0usize;
    let mut exhausted = false;
    for _ in 0..budget.objects {
        let entry = match session.pending_frame.take() {
            Some(entry) => entry,
            None => match entries.next() {
                Some(entry) => entry.map_err(io)?,
                None => {
                    exhausted = true;
                    break;
                }
            },
        };
        let id = name(&entry)?;
        let length = fs::symlink_metadata(entry.path()).map_err(io)?.len();
        if bytes > 0 && (bytes as u64).saturating_add(length) > budget.bytes as u64 {
            session.pending_frame = Some(entry);
            break;
        }
        let data = read(&entry.path(), FRAME)?;
        if blake3::hash(&data).to_hex().as_str() != id {
            return Err(invalid("overlay GC frame checksum"));
        }
        bytes = bytes
            .checked_add(data.len())
            .ok_or_else(|| invalid("overlay GC frame bytes"))?;
        if !catalog::contains(&dir, &state.marks, &id)? {
            reclaim.push(id);
        }
        if bytes >= budget.bytes || budget.expired() {
            break;
        }
    }
    // Validate this whole bounded batch before its first unlink.
    for id in reclaim {
        fs::remove_file(root.join(FRAMES).join(id)).map_err(io)?;
    }
    sync(&root.join(FRAMES))?;
    state.source = binding(root, selected)?;
    if exhausted {
        state.cleaning = true;
        session.frames = None;
    }
    save_progress(root, build, session)?;
    Ok(false)
}

#[cfg(test)]
#[path = "physical_overlay_gc_tests.rs"]
mod tests;
