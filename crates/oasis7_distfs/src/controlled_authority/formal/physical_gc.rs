//! Physical representation reclamation, conservatively rooting EVERY logical
//! alias. No logical history is swept. Unknown representation schemas block it.
use super::*;
use crate::storage_inventory::{StorageInventory, inventory_storage};
use std::collections::BTreeSet;
#[derive(Debug, Serialize)]
pub struct FormalPhysicalReclaimReport {
    pub verified_logical_objects: u64,
    pub verified_logical_bytes: u64,
    pub reclaimed_frames: u64,
    pub reclaimed_stored_bytes: u64,
    pub before: StorageInventory,
    pub after: StorageInventory,
}
#[derive(Serialize, Deserialize)]
struct Retirement {
    version: u32,
    frames: Vec<String>,
}
/// Called only while the stable exclusive physical fence is held. A selected
/// generation owns its own frames; the root frames are retained exactly when
/// an overlay selector references them. This bounded sweep never removes an
/// alias or assumes the selected generation covers a newly published overlay.
pub(super) fn retire_overlay_frames(root: &Path) -> Result<bool, FormalError> {
    let mut marked = BTreeSet::new();
    let mut checked = BTreeSet::new();
    let mut logical_bytes = 0u64;
    for entry in fs::read_dir(root.join(SELECTORS)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("generation overlay selector UTF8"))?;
        hash_hex(&id)?;
        let path = root.join(&id);
        let (_, recipe) =
            selector(&path, &id)?.ok_or_else(|| invalid("generation overlay selector missing"))?;
        // Metadata can be shared by many aliases. Validate every logical alias,
        // but account and mark the recipe's frames only once.
        let logical = read_object(&path, &id, true)?
            .ok_or_else(|| invalid("generation overlay object missing"))?;
        logical_bytes = logical_bytes
            .checked_add(logical.len() as u64)
            .ok_or_else(|| invalid("generation overlay verification overflow"))?;
        if logical_bytes > 1024 * 1024 * 1024 {
            return Err(invalid(
                "generation overlay verification budget; retry after overlay compaction",
            ));
        }
        let identity = blake3::hash(&encode_recipe(&recipe)?).to_hex().to_string();
        if checked.insert(identity) {
            marked.extend(recipe.frames.iter().map(|frame| frame.stored.clone()));
            if marked.len() > MAX_OBJECTS {
                return Err(invalid("generation overlay frame mark budget"));
            }
        }
    }
    let mut reclaim = Vec::new();
    let mut bytes = 0usize;
    let mut more = false;
    for entry in fs::read_dir(root.join(FRAMES)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("generation orphan frame UTF8"))?;
        hash_hex(&id)?;
        if marked.contains(&id) {
            continue;
        }
        let data = read(&entry.path(), FRAME)?;
        if blake3::hash(&data).to_hex().as_str() != id {
            return Err(invalid("generation orphan frame checksum"));
        }
        if reclaim.len() >= 1024 || bytes.saturating_add(data.len()) > 8 * 1024 * 1024 {
            more = true;
            break;
        }
        bytes += data.len();
        reclaim.push(id);
    }
    // All entries in this batch were checked before the first unlink. A crash
    // simply leaves a smaller orphan set, and the next call re-marks overlay.
    for id in reclaim {
        fs::remove_file(root.join(FRAMES).join(id)).map_err(io)?;
    }
    sync(&root.join(FRAMES))?;
    Ok(!more)
}
fn names(root: &Path) -> Result<Vec<String>, FormalError> {
    let mut names = Vec::new();
    for entry in fs::read_dir(root).map_err(io)? {
        let entry = entry.map_err(io)?;
        let id = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("physical inventory UTF8"))?;
        hash_hex(&id)?;
        if !fs::symlink_metadata(entry.path()).map_err(io)?.is_file() || names.len() >= MAX_OBJECTS
        {
            return Err(invalid("physical inventory type/count"));
        }
        names.push(id);
    }
    names.sort();
    Ok(names)
}
pub(crate) fn collect(
    root: &Path,
    roots: &BTreeSet<String>,
    checkpoint: &[u8],
) -> Result<FormalPhysicalReclaimReport, FormalError> {
    if !enabled(root)? {
        return Err(invalid("physical reclamation capability missing"));
    }
    let _fence = pin(root, true)?;
    let (staging_files, _) = staging_preflight(root)?;
    if read(
        &root
            .parent()
            .ok_or_else(|| invalid("physical endpoint root"))?
            .join("formal-checkpoint-v1.cbor"),
        LIMIT,
    )? != checkpoint
    {
        return Err(invalid(
            "physical checkpoint changed after typed root verification",
        ));
    }
    let before = inventory_storage(root);
    if !before.complete {
        return Err(invalid("physical allocation inventory incomplete"));
    }
    for id in roots {
        hash_hex(id)?;
        if super::super::business_pack::read_object_unpinned(&root.join(id), id)?.is_none() {
            return Err(invalid("physical retained root missing"));
        }
    }
    let mut marked = BTreeSet::new();
    let mut objects = 0u64;
    let mut bytes = 0u64;
    // Any interrupted intent is bounded/validated, then the entire current
    // selector graph is re-marked. No old deletion list authorizes an unlink.
    let intent = root.join(RETIRE);
    if intent.try_exists().map_err(io)? {
        let pending: Retirement = serde_cbor::from_slice(&read(&intent, META_LIMIT)?)
            .map_err(|e| invalid(e.to_string()))?;
        if pending.version != 1 || pending.frames.len() > MAX_OBJECTS {
            return Err(invalid("physical retire intent version/budget"));
        }
        for id in pending.frames {
            hash_hex(&id)?;
        }
    }
    for id in names(&root.join(SELECTORS))? {
        let path = root.join(&id);
        let (_, recipe) = selector(&path, &id)?.ok_or_else(|| invalid("physical selector lost"))?;
        let logical = read_object(&path, &id, true)?
            .ok_or_else(|| invalid("physical selected object missing"))?;
        bytes = bytes
            .checked_add(logical.len() as u64)
            .ok_or_else(|| invalid("physical closure byte overflow"))?;
        if bytes > 1024 * 1024 * 1024 {
            return Err(invalid(
                "physical closure verification budget exhausted; no sweep",
            ));
        }
        objects += 1;
        marked.extend(recipe.frames.iter().map(|f| f.stored.clone()));
    }
    let mut reclaim = Vec::new();
    let mut stored_bytes = 0u64;
    for id in names(&root.join(FRAMES))? {
        if marked.contains(&id) {
            continue;
        }
        let data = read(&root.join(FRAMES).join(&id), FRAME)?;
        if blake3::hash(&data).to_hex().as_str() != id {
            return Err(invalid(
                "unrecognized/corrupt orphan representation; no sweep",
            ));
        }
        stored_bytes = stored_bytes
            .checked_add(data.len() as u64)
            .ok_or_else(|| invalid("physical reclaim accounting overflow"))?;
        reclaim.push(id);
    }
    let record = serde_cbor::to_vec(&Retirement {
        version: 1,
        frames: reclaim.clone(),
    })
    .map_err(|e| invalid(e.to_string()))?;
    let staging = root.join(format!(
        ".retire-{}",
        super::super::fencing::fresh_challenge().map_err(io)?
    ));
    create(&staging, &record)?;
    fs::rename(staging, &intent).map_err(io)?;
    sync(root)?;
    // Reference publication, pin acquisition and this unlink share the exact
    // same inode lease. The durable intent fences writers after a crash.
    for id in &reclaim {
        fs::remove_file(root.join(FRAMES).join(id)).map_err(io)?;
    }
    sync(&root.join(FRAMES))?;
    reclaim_staging(root, &staging_files)?;
    fs::remove_file(intent).map_err(io)?;
    sync(root)?;
    let after = inventory_storage(root);
    if !after.complete {
        return Err(invalid("physical post-reclaim accounting incomplete"));
    }
    Ok(FormalPhysicalReclaimReport {
        verified_logical_objects: objects,
        verified_logical_bytes: bytes,
        reclaimed_frames: reclaim.len() as u64,
        reclaimed_stored_bytes: stored_bytes,
        before,
        after,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_overlay_reclaim_is_bounded_and_keeps_shared_live_frames() {
        let f = super::super::tests::Fixture::new();
        let object = super::super::tests::object(vec![7; 200000]);
        persist(&f.0, std::slice::from_ref(&object)).unwrap();
        for value in 0u32..1025 {
            let bytes = value.to_le_bytes();
            let id = blake3::hash(&bytes).to_hex().to_string();
            create(&f.0.join(FRAMES).join(id), &bytes).unwrap();
        }
        let _fence = pin(&f.0, true).unwrap();
        assert!(!retire_overlay_frames(&f.0).unwrap());
        assert!(retire_overlay_frames(&f.0).unwrap());
        assert_eq!(
            read_object(&f.0.join(&object.0), &object.0, false).unwrap(),
            Some(object.1)
        );
        let (_, recipe) = selector(&f.0.join(&object.0), &object.0).unwrap().unwrap();
        let live: BTreeSet<_> = recipe
            .frames
            .iter()
            .map(|frame| frame.stored.clone())
            .collect();
        let remaining: BTreeSet<_> = names(&f.0.join(FRAMES)).unwrap().into_iter().collect();
        assert_eq!(remaining, live);
    }
    #[test]
    fn reader_fences_gc_and_interrupted_retire_does_not_delete_live_frames() {
        let f = super::super::tests::Fixture::new();
        let object = super::super::tests::object(vec![9; 200000]);
        assert!(persist(&f.0, std::slice::from_ref(&object)).unwrap());
        let orphan = b"unreferenced frame";
        let id = blake3::hash(orphan).to_hex().to_string();
        create(&f.0.join(FRAMES).join(&id), orphan).unwrap();
        let reader = pin(&f.0, false).unwrap();
        assert!(matches!(
            collect(&f.0, &BTreeSet::from([object.0.clone()]), b"checkpoint"),
            Err(FormalError::Conflict(_))
        ));
        drop(reader);
        let (_, recipe) = selector(&f.0.join(&object.0), &object.0).unwrap().unwrap();
        create(
            &f.0.join(RETIRE),
            &serde_cbor::to_vec(&Retirement {
                version: 1,
                frames: vec![recipe.frames[0].stored.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(persist(&f.0, &[super::super::tests::object(vec![8; 10000])]).is_err());
        let report = collect(&f.0, &BTreeSet::from([object.0.clone()]), b"checkpoint").unwrap();
        assert_eq!(report.reclaimed_frames, 1);
        assert!(!f.0.join(FRAMES).join(id).exists());
        assert_eq!(
            read_object(&f.0.join(&object.0), &object.0, true)
                .unwrap()
                .unwrap(),
            object.1
        );
    }
    #[test]
    fn missing_dependency_and_unknown_schema_block_every_unlink() {
        let f = super::super::tests::Fixture::new();
        let object = super::super::tests::object(vec![1; 100000]);
        persist(&f.0, std::slice::from_ref(&object)).unwrap();
        let orphan = b"keep on failure";
        let id = blake3::hash(orphan).to_hex().to_string();
        create(&f.0.join(FRAMES).join(&id), orphan).unwrap();
        let (_, recipe) = selector(&f.0.join(&object.0), &object.0).unwrap().unwrap();
        fs::remove_file(f.0.join(FRAMES).join(&recipe.frames[0].stored)).unwrap();
        assert!(collect(&f.0, &BTreeSet::from([object.0.clone()]), b"checkpoint").is_err());
        assert!(f.0.join(FRAMES).join(id).exists());
    }
}
