//! Bounded immutable packs follow the existing business-pack hard-link model.
//! This IWS physical profile adds explicit IO/cancellation and shared admission;
//! its index is only a locator. Page/frame hashes remain authoritative.
use super::*;
use std::io::{Read, Seek, SeekFrom, Write};

pub(super) const MAGIC: &[u8] = b"oasis7.iws.page-pack.v1\0";
const HEADER: usize = MAGIC.len() + 23;
const ROW: usize = 44;
pub(super) const MAX_ENTRIES: usize = 256;
pub(super) const MAX_PACK_BYTES: usize = 4 << 20;

fn kind(directory: &str) -> Result<u8> {
    match directory {
        "frames" => Ok(1),
        "recipes" => Ok(2),
        _ => Err(PageStoreError::Corrupt("pack directory kind")),
    }
}
fn read_at(
    file: &mut File,
    offset: u64,
    bytes: &mut [u8],
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
) -> Result<()> {
    check_cancelled(cancelled)?;
    io.read(bytes.len())?;
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(bytes)?;
    check_cancelled(cancelled)
}
pub(super) fn read_member(
    file: &mut File,
    path: &Path,
    length: u64,
    bound: usize,
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>> {
    let bad = || PageStoreError::Corrupt("pack header/index/span");
    if length > MAX_PACK_BYTES as u64 || length < HEADER as u64 {
        return Err(bad());
    }
    let directory = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|s| s.to_str())
        .ok_or_else(bad)?;
    let expected_kind = kind(directory)?;
    let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(bad)?;
    if name.len() != 64
        || !name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(bad());
    }
    let wanted: [u8; 32] = hex::decode(name)
        .map_err(|_| bad())?
        .try_into()
        .map_err(|_| bad())?;
    let mut header = [0; 23];
    read_at(file, MAGIC.len() as u64, &mut header, io, cancelled)?;
    let version = u32::from_be_bytes(header[..4].try_into().unwrap());
    let count = u16::from_be_bytes(header[5..7].try_into().unwrap()) as usize;
    let index = u64::from_be_bytes(header[7..15].try_into().unwrap());
    let total = u64::from_be_bytes(header[15..23].try_into().unwrap());
    if version != 1
        || header[4] != expected_kind
        || count == 0
        || count > MAX_ENTRIES
        || total != length
        || index < HEADER as u64
        || index.checked_add((count * ROW) as u64) != Some(total)
    {
        return Err(bad());
    }
    let (mut low, mut high) = (0, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let mut row = [0; ROW];
        read_at(file, index + (middle * ROW) as u64, &mut row, io, cancelled)?;
        match row[..32].cmp(&wanted) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => {
                let offset = u64::from_be_bytes(row[32..40].try_into().unwrap());
                let size = u32::from_be_bytes(row[40..44].try_into().unwrap()) as usize;
                if size == 0
                    || size > bound
                    || offset < HEADER as u64
                    || offset
                        .checked_add(size as u64)
                        .is_none_or(|end| end > index)
                {
                    return Err(bad());
                }
                let mut bytes = vec![0; size];
                read_at(file, offset, &mut bytes, io, cancelled)?;
                if file.metadata()?.len() != total {
                    return Err(bad());
                }
                return Ok(bytes);
            }
        }
    }
    Err(PageStoreError::Corrupt("pack lacks linked member"))
}

pub(super) struct Entry {
    pub id: [u8; 32],
    pub bytes: Vec<u8>,
}
fn header(kind: u8, count: usize, index: usize, total: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER);
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.push(kind);
    bytes.extend_from_slice(&(count as u16).to_be_bytes());
    bytes.extend_from_slice(&(index as u64).to_be_bytes());
    bytes.extend_from_slice(&(total as u64).to_be_bytes());
    bytes
}
fn row(entry: &Entry, offset: usize) -> [u8; ROW] {
    let mut bytes = [0; ROW];
    bytes[..32].copy_from_slice(&entry.id);
    bytes[32..40].copy_from_slice(&(offset as u64).to_be_bytes());
    bytes[40..].copy_from_slice(&(entry.bytes.len() as u32).to_be_bytes());
    bytes
}
fn compare(
    file: &mut File,
    expected: &[u8],
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
) -> Result<()> {
    check_cancelled(cancelled)?;
    io.read(expected.len())?;
    let mut buffer = [0; frame_codec::FRAME_BYTES];
    if expected.len() > buffer.len() {
        return Err(PageStoreError::Corrupt("pack readback unit"));
    }
    file.read_exact(&mut buffer[..expected.len()])?;
    if &buffer[..expected.len()] != expected {
        return Err(PageStoreError::Corrupt("pack durable readback"));
    }
    Ok(())
}
/// Return IDs where a concurrent publisher selected a different representation.
/// The caller must verify/sync that winning recipe's actual frame closure.
fn publish(
    root: &Path,
    directory: &str,
    entries: &[Entry],
    io: &mut PageIoBudget,
    cancelled: &AtomicBool,
) -> Result<Vec<[u8; 32]>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let kind = kind(directory)?;
    if entries.len() > MAX_ENTRIES {
        return Err(PageStoreError::ResourceLimited("pack entries"));
    }
    let mut previous = None;
    let mut index = HEADER;
    for entry in entries {
        if previous.is_some_and(|old| old >= entry.id)
            || entry.bytes.is_empty()
            || entry.bytes.len() > frame_codec::FRAME_BYTES
        {
            return Err(PageStoreError::Corrupt("pack entry order/length"));
        }
        previous = Some(entry.id);
        index = index
            .checked_add(entry.bytes.len())
            .ok_or(PageStoreError::ResourceLimited("pack length"))?;
    }
    let total = index
        .checked_add(entries.len() * ROW)
        .filter(|n| *n <= MAX_PACK_BYTES)
        .ok_or(PageStoreError::ResourceLimited("pack bytes"))?;
    check_cancelled(cancelled)?;
    io.write(total)?;
    let (stage, mut file) = files::new_stage(root)?;
    let header = header(kind, entries.len(), index, total);
    file.write_all(&header)?;
    for entry in entries {
        check_cancelled(cancelled)?;
        file.write_all(&entry.bytes)?;
    }
    let mut offset = HEADER;
    for entry in entries {
        file.write_all(&row(entry, offset))?;
        offset += entry.bytes.len();
    }
    file.sync_all()?;
    file.seek(SeekFrom::Start(0))?;
    compare(&mut file, &header, io, cancelled)?;
    for entry in entries {
        compare(&mut file, &entry.bytes, io, cancelled)?;
    }
    offset = HEADER;
    for entry in entries {
        compare(&mut file, &row(entry, offset), io, cancelled)?;
        offset += entry.bytes.len();
    }
    check_cancelled(cancelled)?;
    io.read(1)?;
    if file.read(&mut [0])? != 0 || file.metadata()?.len() != total as u64 {
        return Err(PageStoreError::Corrupt("pack readback length"));
    }
    files::publication_point(
        root,
        if kind == 1 {
            "pack-frames"
        } else {
            "pack-recipes"
        },
        1,
    );
    let mut raced = Vec::new();
    for entry in entries {
        check_cancelled(cancelled)?;
        io.write(0)?; // bounded metadata publication work, independent of bytes
        let target = root.join(directory).join(hex::encode(entry.id));
        match fs::hard_link(&stage, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if kind == 1
                    && files::read(&target, frame_codec::FRAME_BYTES, io, cancelled)? != entry.bytes
                {
                    return Err(PageStoreError::Corrupt("concurrent packed frame differs"));
                }
                files::sync(&target)?;
                raced.push(entry.id);
            }
            Err(error) => return Err(error.into()),
        }
        files::publication_point(
            root,
            if kind == 1 {
                "pack-frames"
            } else {
                "pack-recipes"
            },
            2,
        );
    }
    files::sync(&root.join(directory))?;
    files::publication_point(
        root,
        if kind == 1 {
            "pack-frames"
        } else {
            "pack-recipes"
        },
        3,
    );
    check_cancelled(cancelled)?;
    fs::remove_file(stage)?;
    files::sync(&root.join("staging"))?;
    Ok(raced)
}

impl IncrementalPageStore {
    /// Bounded small Merkle pages only. The producer retains its original
    /// shared charge; this function admits all temporary encoded members.
    pub(super) fn put_small_page_batch(
        &self,
        pages: &[([u8; 32], &[u8])],
        class: MemoryClass,
        io: &mut PageIoBudget,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        check_cancelled(cancelled)?;
        if pages.len() > MAX_ENTRIES {
            return Err(PageStoreError::ResourceLimited("small page batch"));
        }
        let _workspace = self.memory.reserve(
            (128 << 10)
                + pages.len()
                    * (oasis7_formal_protocol::incremental_state::STATE_MAX_PAGE_BYTES
                        + recipe::MAX_RECIPE_BYTES
                        + 512)
                + if matches!(self.compression, PageCompression::Zstd) {
                    ENCODE_WORKSPACE
                } else {
                    RAW_WORKSPACE
                },
            class,
        )?;
        files::validate_store_io(&self.root, io, cancelled)?;
        files::validate_lease(&self.root, &self._lease)?;
        let mut frames = std::collections::BTreeMap::new();
        let mut recipes = Vec::with_capacity(pages.len());
        for (id, bytes) in pages {
            check_cancelled(cancelled)?;
            if bytes.is_empty()
                || bytes.len() > oasis7_formal_protocol::incremental_state::STATE_MAX_PAGE_BYTES
                || blake3::hash(bytes).as_bytes() != id
            {
                return Err(PageStoreError::Corrupt("small page length/hash"));
            }
            if files::exists(&self.root.join("recipes").join(hex::encode(id)))? {
                self.check_existing(id, bytes, class, io, cancelled)?;
                continue;
            }
            let (frame, stored) =
                frame_codec::encode(bytes, matches!(self.compression, PageCompression::Zstd))?;
            if frame_codec::decode(&frame, &stored)
                .map_err(|_| PageStoreError::Corrupt("packed frame encoding"))?
                .as_slice()
                != *bytes
            {
                return Err(PageStoreError::Corrupt("packed page encoding"));
            }
            if let Some(old) = frames.insert(frame.stored, stored.clone())
                && old != stored
            {
                return Err(PageStoreError::Corrupt("packed frame collision"));
            }
            recipes.push(Entry {
                id: *id,
                bytes: recipe::encode(id, bytes.len(), &[frame])?,
            });
        }
        recipes.sort_unstable_by_key(|entry| entry.id);
        if recipes.windows(2).any(|w| w[0].id == w[1].id) {
            return Err(PageStoreError::Corrupt("duplicate batch page"));
        }
        let frames: Vec<_> = frames
            .into_iter()
            .map(|(id, bytes)| Entry { id, bytes })
            .collect();
        publish(&self.root, "frames", &frames, io, cancelled)?;
        let raced = publish(&self.root, "recipes", &recipes, io, cancelled)?;
        for (id, bytes) in pages {
            check_cancelled(cancelled)?;
            if raced.contains(id) {
                self.check_existing(id, bytes, class, io, cancelled)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "incremental_page_pack_tests.rs"]
mod tests;
