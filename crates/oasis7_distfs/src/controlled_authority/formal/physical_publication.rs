//! Crash-safe immutable publication and completion of durable selector batches.
use super::*;
const STAGING: &str = ".physical-staging-v1";
pub(super) fn publish(root: &Path, target: &Path, bytes: &[u8]) -> Result<(), FormalError> {
    publish_inner(root, target, bytes, true)
}
fn publish_inner(
    root: &Path,
    target: &Path,
    bytes: &[u8],
    flush_dirs: bool,
) -> Result<(), FormalError> {
    let staging = root.join(STAGING);
    directory(&staging)?;
    let temporary = staging.join(super::super::fencing::fresh_challenge().map_err(io)?);
    create(&temporary, bytes)?;
    if target.parent() == Some(root.join(FRAMES).as_path()) {
        fault(1)?;
    }
    match fs::hard_link(&temporary, target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if read(target, LIMIT)? != bytes {
                return Err(invalid("immutable physical publication differs"));
            }
            sync(target)?;
        }
        Err(e) => return Err(io(e)),
    }
    if flush_dirs {
        sync(
            target
                .parent()
                .ok_or_else(|| invalid("publication parent"))?,
        )?;
    }
    if target.parent() == Some(root.join(SELECTORS).as_path()) {
        fault(2)?;
    }
    fs::remove_file(&temporary).map_err(io)?;
    if flush_dirs {
        sync(&staging)?;
    }
    Ok(())
}
const FRAME_BATCH_BYTES: usize = 8 * 1024 * 1024;
/// This invocation owns the publication mutex and live retirement pin. Each
/// file is durable before its no-clobber link. Only frame/staging directories
/// share barriers, in at most 8 MiB pages, before any recipe can reference them.
/// An interrupted page has no selector authority and is safe to verify/retry.
pub(super) fn publish_frames(
    root: &Path,
    frames: BTreeMap<String, Vec<u8>>,
    writer: &PublicationGuard,
) -> Result<(), FormalError> {
    if writer.root != root || frames.len() > MAX_OBJECTS {
        return Err(invalid("physical frame publication scope/budget"));
    }
    let total = frames.values().try_fold(0usize, |total, bytes| {
        total
            .checked_add(bytes.len())
            .filter(|total| *total <= LIMIT)
            .ok_or_else(|| invalid("physical frame publication byte budget"))
    })?;
    if total == 0 {
        return if frames.is_empty() {
            sync(&root.join(FRAMES))
        } else {
            Err(invalid("physical frame publication empty frame"))
        };
    }
    let mut pending = 0usize;
    for (id, bytes) in frames {
        hash_hex(&id)?;
        if bytes.is_empty() || bytes.len() > FRAME || blake3::hash(&bytes).to_hex().as_str() != id {
            return Err(invalid("physical frame publication checksum/bound"));
        }
        if pending + bytes.len() > FRAME_BATCH_BYTES {
            flush_frame_directories(root)?;
            pending = 0;
        }
        publish_inner(root, &root.join(FRAMES).join(id), &bytes, false)?;
        pending += bytes.len();
    }
    flush_frame_directories(root)
}
fn flush_frame_directories(root: &Path) -> Result<(), FormalError> {
    // Test a crash after durable files/links but before their directory barrier.
    fault(8)?;
    sync(&root.join(FRAMES))?;
    sync(&root.join(STAGING))?;
    #[cfg(test)]
    FRAME_BATCH_BARRIERS.with(|count| count.set(count.get() + 1));
    Ok(())
}
#[cfg(test)]
thread_local! { static FRAME_BATCH_BARRIERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
/// Caller holds the stable root's exclusive kernel fence and endpoint lease.
/// A durable selector binds a complete immutable recipe; finish all members,
/// including members not present in a retry's smaller object input set.
pub(super) fn recover(root: &Path) -> Result<(), FormalError> {
    #[cfg(unix)]
    let mut verified = std::collections::BTreeSet::new();
    for entry in fs::read_dir(root.join(SELECTORS)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("selector UTF8"))?;
        hash_hex(&id)?;
        let logical = root.join(&id);
        if selected_path(&logical, &id)?.parent() != Some(root) {
            continue;
        }
        #[cfg(unix)]
        {
            let metadata = entry.metadata().map_err(io)?;
            // Memoization is an optimization, never a whole-history inventory.
            // Eviction may repeat validation but cannot skip a required recipe.
            if verified.len() >= 4096 {
                verified.clear();
            }
            if !verified.insert((metadata.dev(), metadata.ino())) {
                continue;
            }
        }
        recover_names(root, vec![id])?;
    }
    Ok(())
}

pub(super) fn recover_objects(
    root: &Path,
    objects: &[(String, Vec<u8>)],
) -> Result<(), FormalError> {
    let mut names = Vec::new();
    for (id, _) in objects {
        hash_hex(id)?;
        if root.join(SELECTORS).join(id).try_exists().map_err(io)? {
            names.push(id.clone());
        }
    }
    recover_names(root, names)
}
fn recover_names(root: &Path, mut names: Vec<String>) -> Result<(), FormalError> {
    names.sort();
    #[cfg(unix)]
    let mut verified = std::collections::BTreeSet::new();
    for id in names {
        let selected = root.join(SELECTORS).join(&id);
        let metadata = fs::symlink_metadata(&selected).map_err(io)?;
        if !metadata.is_file() {
            return Err(invalid("selector recovery regular file"));
        }
        #[cfg(unix)]
        if !verified.insert((metadata.dev(), metadata.ino())) {
            continue;
        }
        let recipe = decode_recipe(&read(&selected, META_LIMIT)?)?;
        if recipe
            .objects
            .binary_search_by(|entry| entry.id.cmp(&id))
            .is_err()
        {
            return Err(invalid("selector recovery member mismatch"));
        }
        let mut incomplete = false;
        for object in &recipe.objects {
            for (path, required) in [
                (root.join(SELECTORS).join(&object.id), true),
                (root.join(&object.id), false),
            ] {
                match fs::symlink_metadata(path) {
                    Ok(m) => {
                        if !m.is_file() {
                            return Err(invalid("physical recovery alias type"));
                        }
                        #[cfg(unix)]
                        if (m.dev(), m.ino()) != (metadata.dev(), metadata.ino()) {
                            return Err(invalid("physical recovery alias identity"));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => incomplete |= required,
                    Err(e) => return Err(io(e)),
                }
            }
        }
        // Complete immutable batches need no history-wide payload replay.
        // Ordinary logical reads still verify frames and full object hashes.
        if !incomplete {
            continue;
        }
        for object in &recipe.objects {
            let (_, bytes) = range(root, &recipe, &object.id, 0, LIMIT as u32)?;
            if blake3::hash(&bytes).to_hex().as_str() != object.id {
                return Err(invalid("selector recovery logical checksum"));
            }
        }
        sync(&selected)?;
        for frame in &recipe.frames {
            sync(&root.join(FRAMES).join(&frame.stored))?;
        }
        sync(&root.join(FRAMES))?;
        for object in &recipe.objects {
            for alias in [root.join(SELECTORS).join(&object.id)] {
                match fs::symlink_metadata(&alias) {
                    Ok(m) => {
                        if !m.is_file() {
                            return Err(invalid("selector recovery alias type"));
                        }
                        #[cfg(unix)]
                        if (m.dev(), m.ino()) != (metadata.dev(), metadata.ino()) {
                            return Err(invalid("selector recovery immutable alias changed"));
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        fs::hard_link(&selected, &alias).map_err(io)?;
                    }
                    Err(e) => return Err(io(e)),
                }
            }
        }
        sync(&root.join(SELECTORS))?;
        sync(root)?;
    }
    Ok(())
}

/// A run-local marker amortizes fsync only. Every requested object is still
/// decoded and hash checked before this function can observe a marker.
pub(super) fn sync_recipe_once(
    root: &Path,
    id: &str,
    recipe: &Recipe,
    markers: Option<&Path>,
) -> Result<(), FormalError> {
    let selected = root.join(SELECTORS).join(id);
    let marker = if let Some(markers) = markers {
        let metadata = fs::symlink_metadata(&selected).map_err(io)?;
        #[cfg(unix)]
        let key = format!(
            "physical-recipe-{}-{}-{}",
            blake3::hash(root.as_os_str().as_encoded_bytes()).to_hex(),
            metadata.dev(),
            metadata.ino()
        );
        #[cfg(not(unix))]
        let key = format!("physical-recipe-{id}");
        let marker = markers.join(key);
        match fs::symlink_metadata(&marker) {
            Ok(meta) if meta.is_file() => return Ok(()),
            Ok(_) => return Err(invalid("physical sync marker type")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
        Some(marker)
    } else {
        None
    };
    #[cfg(test)]
    RECIPE_SYNCS.with(|count| count.set(count.get() + 1));
    sync(&selected)?;
    for frame in &recipe.frames {
        sync(&root.join(FRAMES).join(&frame.stored))?;
    }
    sync(&root.join(FRAMES))?;
    sync(&root.join(SELECTORS))?;
    sync(root)?;
    if let Some(marker) = marker {
        create(&marker, b"")?;
    }
    Ok(())
}

#[cfg(test)]
thread_local! {static FAILURE:std::cell::Cell<u8>=const {std::cell::Cell::new(0)};}
pub(super) fn fault(_stage: u8) -> Result<(), FormalError> {
    #[cfg(test)]
    if FAILURE.with(|failure| {
        if failure.get() == _stage {
            failure.set(0);
            true
        } else {
            false
        }
    }) {
        return Err(FormalError::Io(
            "injected physical publication interruption".into(),
        ));
    }
    Ok(())
}
#[cfg(test)]
pub(super) fn fail_at(stage: u8) {
    FAILURE.with(|failure| failure.set(stage));
}

/// One publication guard invocation owns this private ephemeral scratch space.
/// It contains only fsync deduplication identities, never bytes or durable proof.
/// A crash may leave a system temporary directory; a later operation never reuses it.
pub(super) fn with_readback_markers<T>(
    run: impl FnOnce(&Path) -> Result<T, FormalError>,
) -> Result<T, FormalError> {
    let parent = std::env::temp_dir().canonicalize().map_err(io)?;
    if !fs::symlink_metadata(&parent).map_err(io)?.is_dir() {
        return Err(invalid("physical readback temporary parent"));
    }
    let path = parent.join(format!(
        "oasis7-physical-readback-{}",
        super::super::fencing::fresh_challenge().map_err(io)?
    ));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(&path).map_err(io)?;
    struct Owned(PathBuf);
    impl Drop for Owned {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let owned = Owned(path);
    let result = run(&owned.0);
    // Drop only the fresh directory this invocation successfully created.
    drop(owned);
    result
}
#[cfg(test)]
thread_local! {static RECIPE_SYNCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };}
#[cfg(test)]
pub(super) fn reset_recipe_syncs() {
    RECIPE_SYNCS.with(|count| count.set(0));
}
#[cfg(test)]
pub(super) fn recipe_syncs() -> usize {
    RECIPE_SYNCS.with(|count| count.get())
}

#[cfg(test)]
mod frame_batch_tests {
    use super::super::tests::{Fixture, object};
    use super::*;

    #[test]
    fn frame_publication_batches_directory_barriers_with_eight_mib_cap() {
        let fixture = Fixture::new();
        let writer = publication_guard(&fixture.0).unwrap();
        let mut frames = BTreeMap::new();
        for index in 0u16..129 {
            let mut bytes = vec![0; FRAME];
            bytes[..2].copy_from_slice(&index.to_be_bytes());
            frames.insert(blake3::hash(&bytes).to_hex().to_string(), bytes);
        }
        FRAME_BATCH_BARRIERS.with(|count| count.set(0));
        publish_frames(&fixture.0, frames.clone(), &writer).unwrap();
        assert_eq!(
            FRAME_BATCH_BARRIERS.with(|count| count.get()),
            2,
            "129 durable files share two bounded frame/staging directory barriers"
        );
        for (id, expected) in frames {
            assert_eq!(
                read(&fixture.0.join(FRAMES).join(id), FRAME).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn frame_batch_failure_before_directory_barrier_keeps_source_and_retries_exactly() {
        let fixture = Fixture::new();
        let item = object((0..300000).map(|value| (value % 251) as u8).collect());
        let source = fixture.0.join("retained-input-source");
        fs::write(&source, &item.1).unwrap();
        File::open(&source).unwrap().sync_all().unwrap();
        fail_at(8);
        assert!(persist(&fixture.0, std::slice::from_ref(&item)).is_err());
        assert!(
            !fixture.0.join(SELECTORS).join(&item.0).exists(),
            "no selector can reference a frame batch whose directory flush failed"
        );
        assert!(
            fs::read_dir(fixture.0.join(FRAMES))
                .unwrap()
                .next()
                .is_some()
        );
        assert_eq!(fs::read(&source).unwrap(), item.1);
        // Existing files from the interrupted batch are reread and fsynced by
        // persist_guarded before the retry's frame-directory barrier and selector.
        assert!(persist(&fixture.0, std::slice::from_ref(&item)).unwrap());
        assert_eq!(
            read_object(&fixture.0.join(&item.0), &item.0, true)
                .unwrap()
                .unwrap(),
            item.1
        );
        assert_eq!(fs::read(&source).unwrap(), item.1);
    }

    #[test]
    fn frame_batch_rejects_changed_existing_target_without_clobber() {
        let fixture = Fixture::new();
        let writer = publication_guard(&fixture.0).unwrap();
        let bytes = vec![17; FRAME];
        let id = blake3::hash(&bytes).to_hex().to_string();
        let frames = BTreeMap::from([(id.clone(), bytes)]);
        publish_frames(&fixture.0, frames.clone(), &writer).unwrap();
        fs::write(fixture.0.join(FRAMES).join(&id), b"corrupt-existing").unwrap();
        assert!(publish_frames(&fixture.0, frames, &writer).is_err());
        assert_eq!(
            fs::read(fixture.0.join(FRAMES).join(&id)).unwrap(),
            b"corrupt-existing"
        );
        assert_eq!(fs::read_dir(fixture.0.join(SELECTORS)).unwrap().count(), 0);
    }
}
