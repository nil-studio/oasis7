//! Paid uncached namespace selection. The pin covers the consumer callback;
//! the selected pathname and its metadata are data, never a serving permit.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
pub(crate) struct GenerationSelectedReadWorkV1 {
    pub remaining_steps: u64,
    pub remaining_read_bytes: u64,
}
impl GenerationSelectedReadWorkV1 {
    fn step(&mut self, cancelled: &mut impl FnMut() -> bool) -> Result<(), FormalError> {
        if cancelled() {
            return Err(invalid("bounded generation selection cancelled"));
        }
        self.remaining_steps = self
            .remaining_steps
            .checked_sub(1)
            .ok_or_else(|| invalid("bounded generation selection steps"))?;
        Ok(())
    }
    fn read(&mut self, n: usize, cancelled: &mut impl FnMut() -> bool) -> Result<(), FormalError> {
        self.step(cancelled)?;
        let n = u64::try_from(n).map_err(|_| invalid("bounded generation read layout"))?;
        if self.remaining_read_bytes < n {
            self.remaining_read_bytes = 0;
            return Err(invalid("bounded generation read bytes"));
        }
        self.remaining_read_bytes -= n;
        Ok(())
    }
}
struct PaidMetadata(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for PaidMetadata {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
fn read_metadata(
    path: &Path,
    limit: usize,
    memory: &MemoryBudget,
    work: &mut GenerationSelectedReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<PaidMetadata, FormalError> {
    work.step(cancelled)?;
    paths::parents(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let mut file = options.open(path).map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    paths::metadata(&metadata, false)?;
    let length =
        usize::try_from(metadata.len()).map_err(|_| invalid("bounded generation metadata size"))?;
    if length > limit {
        return Err(invalid("bounded generation metadata bound"));
    }
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no parameters or caller preconditions.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(invalid("physical registered owner/write mode"));
        }
    }
    let paid = length
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or_else(|| invalid("bounded generation metadata layout"))?;
    let fee = memory
        .reserve(paid, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded generation metadata admission"))?;
    let mut bytes = Vec::with_capacity(length);
    if bytes.capacity() > length {
        return Err(invalid("bounded generation metadata capacity"));
    }
    bytes.resize(length, 0);
    let mut at = 0;
    while at < length {
        let count = (length - at).min(8192);
        work.read(count, cancelled)?;
        let n = match file.read(&mut bytes[at..at + count]) {
            Ok(0) => return Err(invalid("bounded generation truncated metadata")),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io(e)),
        };
        if cancelled() {
            return Err(invalid("bounded generation selection cancelled"));
        }
        at += n;
    }
    let mut probe = [0];
    loop {
        work.read(1, cancelled)?;
        match file.read(&mut probe) {
            Ok(0) => break,
            Ok(_) => return Err(invalid("bounded generation metadata grew")),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(io(e)),
        }
    }
    let after = file.metadata().map_err(io)?;
    paths::metadata(&after, false)?;
    if after.len() != metadata.len() {
        return Err(invalid("bounded generation metadata changed"));
    }
    work.step(cancelled)?;
    Ok(PaidMetadata(fee.retain(bytes)))
}
pub(crate) fn with_selected_path_bounded_v1<T, C: FnMut() -> bool>(
    path: &Path,
    id: &str,
    memory: &MemoryBudget,
    work: &mut GenerationSelectedReadWorkV1,
    cancelled: &mut C,
    consume: impl FnOnce(&Path, &mut GenerationSelectedReadWorkV1, &mut C) -> Result<T, FormalError>,
) -> Result<T, FormalError> {
    work.step(cancelled)?;
    hash_hex(id)?;
    let root = path
        .parent()
        .ok_or_else(|| invalid("generation logical root"))?;
    // Flat Manifest has a version, one nonce and one optional catalog NodeRef;
    // each digest is 64 bytes. Current ID, retained generation ID, and transient
    // original schema/path/error strings fit this fixed workspace; paths are
    // charged separately for their actual byte length. Catalog layouts come from
    // its owner, including two live node frames along a single hash-prefix path.
    let catalog_workspace = catalog::lookup_workspace_bytes()?;
    let fixed = std::mem::size_of::<Manifest>()
        .checked_add(6 * 64 + 4096)
        .and_then(|n| n.checked_add(root.as_os_str().as_encoded_bytes().len().checked_mul(8)?))
        .and_then(|n| n.checked_add(catalog_workspace))
        .ok_or_else(|| invalid("bounded generation lookup layout"))?;
    let _workspace = memory
        .reserve(fixed, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded generation lookup admission"))?;
    let _pin = pin(root, false)?;
    let shared = RefCell::new((work, cancelled));
    let mut read = |path: &Path, limit: usize| {
        let mut s = shared.borrow_mut();
        let (work, stop) = &mut *s;
        read_metadata(path, limit, memory, *work, *stop)
    };
    let selected = if let Some(generation) = super::current_with_reader(root, &mut read)? {
        let manifest = super::manifest_with_reader(root, &generation, &mut read)?;
        let dir = root.join(GENERATIONS).join(generation);
        if catalog::contains_uncached_with_reader(&dir, &manifest.catalogs, id, &mut read)? {
            let logical = dir.join(id);
            let regular = |path: &Path| -> Result<bool, FormalError> {
                {
                    let mut s = shared.borrow_mut();
                    let (work, stop) = &mut *s;
                    work.step(*stop)?;
                }
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
            logical
        } else {
            path.to_path_buf()
        }
    } else {
        path.to_path_buf()
    };
    let (work, cancelled) = shared.into_inner();
    work.step(cancelled)?;
    let result = consume(&selected, work, cancelled)?;
    work.step(cancelled)?;
    Ok(result)
}
#[cfg(all(test, unix))]
#[path = "physical_generation_selected_read_tests.rs"]
mod tests;
