//! Bounded-memory offline shadows for histories larger than one name page.
use super::*;
#[derive(Debug, Clone)]
pub struct FormalStorageShadowOptions {
    pub optimized: bool,
    /// Per-advance logical-byte budget; never exceeds the fixed 1GiB ceiling.
    pub max_logical_bytes: u64,
}
impl Default for FormalStorageShadowOptions {
    fn default() -> Self {
        Self {
            optimized: true,
            max_logical_bytes: MAX_LOGICAL_BYTES,
        }
    }
}
pub(super) fn visit(
    source: &Path,
    mut visitor: impl FnMut(String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    if super::super::physical::enabled(source)? {
        let mut cursor = super::super::physical::roots::PageCursor::start();
        loop {
            let (ids, next, complete) = super::super::physical::roots::page(source, &cursor, 1024)?;
            for id in ids {
                visitor(id)?;
            }
            cursor = next;
            if complete {
                break;
            }
        }
        return super::super::physical::visit_logical_ids(source, visitor);
    }
    for entry in fs::read_dir(source).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| invalid("stream shadow nonUTF8 entry"))?;
        if name == ".business-pack-staging-v1" {
            regular_directory(&entry.path())?;
            if fs::read_dir(entry.path()).map_err(io)?.next().is_some() {
                return Err(invalid("stream shadow pending legacy staging"));
            }
            continue;
        }
        super::super::crypto::hash_hex(&name)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io)?;
        if !metadata.is_file() || metadata.len() > business_pack::MAX_PACK {
            return Err(invalid("stream shadow source type/physical bound"));
        }
        visitor(name)?;
    }
    Ok(())
}
pub(super) fn flush_verified(
    destination: &Path,
    batch: &mut Vec<BusinessIndexNodeWriteV1>,
) -> Result<(), FormalError> {
    batch.sort_by(|a, b| a.digest.cmp(&b.digest));
    business_pack::persist(destination, batch)?;
    for object in batch.iter() {
        if business_pack::read_object(&destination.join(&object.digest), &object.digest)?.as_deref()
            != Some(object.bytes.as_slice())
        {
            return Err(invalid("stream shadow independent exact readback mismatch"));
        }
    }
    batch.clear();
    Ok(())
}
/// Complete a resumable shadow by repeatedly advancing bounded copy steps.
/// The initial frozen-name scan and final inode inventory are streaming metadata
/// passes; their time/temporary-index cost scales with the captured history.
pub fn build_formal_storage_shadow_with_options(
    source_endpoint: &Path,
    destination: &Path,
    options: &FormalStorageShadowOptions,
) -> Result<FormalStorageShadowReport, FormalError> {
    loop {
        let progress = super::advance_formal_storage_shadow(source_endpoint, destination, options)?;
        if let Some(report) = progress.report {
            return Ok(report);
        }
    }
}
