//! Preserve verified source packing/chunk boundaries instead of recompressing
//! hash-ordered logical batches and destroying existing cross-object sharing.
use super::*;
fn same_inode(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    #[cfg(unix)]
    return (left.dev(), left.ino()) == (right.dev(), right.ino());
    #[cfg(not(unix))]
    {
        let _ = (left, right);
        false
    }
}
#[cfg(not(unix))]
fn same_bytes(source: &Path, target: &Path) -> Result<bool, FormalError> {
    use std::io::Read;
    let mut source = fs::File::open(source).map_err(io)?;
    let mut target = fs::File::open(target).map_err(io)?;
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    loop {
        let a = source.read(&mut left).map_err(io)?;
        let b = target.read(&mut right).map_err(io)?;
        if a != b || left[..a] != right[..b] {
            return Ok(false);
        }
        if a == 0 {
            return Ok(true);
        }
    }
}
fn regular(path: &Path, limit: u64) -> Result<fs::Metadata, FormalError> {
    super::super::paths::parents(path)?;
    let meta = fs::symlink_metadata(path).map_err(io)?;
    super::super::paths::metadata(&meta, false)?;
    if meta.len() > limit {
        return Err(invalid("generation import file bound"));
    }
    Ok(meta)
}
fn link(source: &Path, target: &Path, limit: u64, frame: Option<&str>) -> Result<(), FormalError> {
    let original = regular(source, limit)?;
    match fs::hard_link(source, target) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = regular(target, limit)?;
            if !same_inode(&original, &existing) {
                if let Some(id) = frame {
                    if blake3::hash(&read(target, FRAME)?).to_hex().as_str() != id {
                        return Err(invalid("generation existing imported frame checksum"));
                    }
                } else {
                    #[cfg(not(unix))]
                    if same_bytes(source, target)? {
                        return sync(target);
                    }
                    // A retry must retain exactly the imported representation;
                    // original logical readback below is still mandatory.
                    return Err(invalid("generation imported alias identity changed"));
                }
            }
        }
        Err(error) => return Err(io(error)),
    }
    sync(target)
}
pub(super) fn object(
    root: &Path,
    target: &Path,
    id: &str,
    expected: &[u8],
) -> Result<(), FormalError> {
    let _source_pin = pin(root, false)?;
    let _writer = publication_guard(target)?;
    let source = selected_path(&root.join(id), id)?;
    let actual = super::super::super::business_pack::read_object_durable(&root.join(id), id)?
        .ok_or_else(|| invalid("generation import source missing"))?;
    if actual != expected || blake3::hash(&actual).to_hex().as_str() != id {
        return Err(invalid("generation import original bytes differ"));
    }
    if let Some((physical_root, recipe)) = selector(&source, id)? {
        // Metadata work is structurally bounded by the original <=64MiB recipe.
        // It may outlast the soft page window. Only captured logical spans are
        // payload authority; each is freshly read at source and destination.
        for frame in &recipe.frames {
            link(
                &physical_root.join(FRAMES).join(&frame.stored),
                &target.join(FRAMES).join(&frame.stored),
                FRAME as u64,
                Some(&frame.stored),
            )?;
        }
        sync(&target.join(FRAMES))?;
        publication::fault(1)?;
        let selected = physical_root.join(SELECTORS).join(id);
        link(
            &selected,
            &target.join(SELECTORS).join(id),
            META_LIMIT as u64,
            None,
        )?;
        sync(&target.join(SELECTORS))?;
        publication::fault(2)?;
        // The selector alone is the canonical physical locator. A second
        // optional root alias adds no readable content or recovery authority.
    } else {
        // The original raw/legacy pack inode and its complete index stay intact;
        // importing one member does not retain other members as logical roots.
        link(
            &source,
            &target.join(id),
            super::super::super::business_pack::MAX_PACK,
            None,
        )?;
    }
    sync(target)?;
    publication::fault(3)?;
    if super::super::super::business_pack::read_object_durable(&target.join(id), id)?.as_deref()
        != Some(expected)
    {
        return Err(invalid("generation imported independent original readback"));
    }
    // Top-level retention captures history; this private generation is indexed
    // by its authenticated manifest catalog, not a duplicate retention log.
    Ok(())
}
#[cfg(test)]
#[path = "physical_generation_import_tests.rs"]
mod tests;
