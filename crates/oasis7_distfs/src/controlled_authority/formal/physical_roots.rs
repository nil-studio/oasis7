//! Conservative append-only retention, never a business authority. Fixed-size
//! hash-chained records amortize metadata across 64 KiB segments. The atomic
//! head commits only a durable prefix; completed segments are never appended.
use super::*;
use std::io::{Seek, SeekFrom};
const ROOTS: &str = ".retained-logical-v1";
const HEAD: &str = "head.cbor";
const RECORD_BYTES: usize = 64;
const RECORDS: u64 = 1024;
const SEGMENT_BYTES: usize = RECORD_BYTES * RECORDS as usize;
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Head {
    version: u32,
    count: u64,
    checksum: String,
}
fn initial() -> String {
    blake3::hash(b"oasis7.formal.logical-retention.v1")
        .to_hex()
        .to_string()
}
fn checksum(previous: &str, logical: &[u8]) -> Result<String, FormalError> {
    let mut bytes = hex::decode(previous).map_err(io)?;
    bytes.extend_from_slice(logical);
    Ok(blake3::hash(&bytes).to_hex().to_string())
}
fn segment(root: &Path, index: u64) -> PathBuf {
    root.join(ROOTS).join(format!("{index:020}.bin"))
}
fn head(root: &Path) -> Result<Head, FormalError> {
    decode_head(&read(&root.join(ROOTS).join(HEAD), 1024)?)
}
fn decode_head(bytes: &[u8]) -> Result<Head, FormalError> {
    let head: Head = serde_cbor::from_slice(bytes).map_err(io)?;
    hash_hex(&head.checksum)?;
    if head.version != 1 || (head.count == 0 && head.checksum != initial()) {
        return Err(invalid("logical retention head version/empty checksum"));
    }
    Ok(head)
}
/// New-world initialization only; never reset an existing retention head.
pub(crate) fn initialize(root: &Path) -> Result<(), FormalError> {
    let dir = root.join(ROOTS);
    directory(&dir)?;
    match fs::symlink_metadata(dir.join(HEAD)) {
        Ok(_) => {
            head(root)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Before the first atomic head rename, only unselected private
            // temporary heads may exist. A segment here means a published
            // registry lost its head, so never manufacture an empty history.
            for entry in fs::read_dir(&dir).map_err(io)? {
                let entry = entry.map_err(io)?;
                let name = entry.file_name();
                let name = name
                    .to_str()
                    .ok_or_else(|| invalid("logical retention initialization filename"))?;
                let Some(id) = name.strip_prefix(".head-") else {
                    return Err(invalid("logical retention missing published head"));
                };
                hash_hex(id)?;
                read(&entry.path(), 1024)?;
            }
            publish_head(
                root,
                &Head {
                    version: 1,
                    count: 0,
                    checksum: initial(),
                },
            )?;
        }
        Err(e) => return Err(io(e)),
    }
    Ok(())
}
fn publish_head(root: &Path, state: &Head) -> Result<(), FormalError> {
    let dir = root.join(ROOTS);
    let temp = dir.join(format!(
        ".head-{}",
        super::super::fencing::fresh_challenge().map_err(io)?
    ));
    create(&temp, &serde_cbor::to_vec(state).map_err(io)?)?;
    fs::rename(&temp, dir.join(HEAD)).map_err(io)?;
    sync(&dir)
}
/// Caller holds the endpoint/publisher lease; no nested physical pin. Aliases
/// and representation dependencies are durable before this registration.
// The original retention algorithm owns ordering; implementations account for
// each actual operation. This is a data-I/O seam, never retention authority.
trait RetainIo {
    fn step(&mut self) -> Result<(), FormalError>;
    fn read(&mut self, path: &Path, max: usize) -> Result<Vec<u8>, FormalError>;
    fn head(&mut self, root: &Path) -> Result<Head, FormalError>;
    fn segment_path(&mut self, root: &Path, index: u64) -> Result<PathBuf, FormalError>;
    fn directory_path(&mut self, root: &Path) -> Result<PathBuf, FormalError>;
    fn open_segment(&mut self, path: &Path) -> Result<File, FormalError>;
    fn metadata(&mut self, file: &File) -> Result<fs::Metadata, FormalError>;
    fn seek(&mut self, file: &mut File, position: SeekFrom) -> Result<u64, FormalError>;
    fn set_len(&mut self, file: &File, len: u64) -> Result<(), FormalError>;
    fn read_exact(&mut self, file: &mut File, bytes: &mut [u8]) -> Result<(), FormalError>;
    fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> Result<(), FormalError>;
    fn fsync(&mut self, file: &File) -> Result<(), FormalError>;
    fn sync_dir(&mut self, path: &Path) -> Result<(), FormalError>;
    fn checksum(&mut self, previous: &str, logical: &[u8]) -> Result<String, FormalError>;
    fn decode_hex(&mut self, text: &str) -> Result<Vec<u8>, FormalError>;
    fn encode_hex(&mut self, bytes: &[u8]) -> Result<String, FormalError>;
    fn publish_head(&mut self, root: &Path, state: &Head) -> Result<(), FormalError>;
}
struct OriginalRetainIo;
impl RetainIo for OriginalRetainIo {
    fn step(&mut self) -> Result<(), FormalError> {
        Ok(())
    }
    fn read(&mut self, path: &Path, max: usize) -> Result<Vec<u8>, FormalError> {
        read(path, max)
    }
    fn head(&mut self, root: &Path) -> Result<Head, FormalError> {
        head(root)
    }
    fn segment_path(&mut self, root: &Path, index: u64) -> Result<PathBuf, FormalError> {
        Ok(segment(root, index))
    }
    fn directory_path(&mut self, root: &Path) -> Result<PathBuf, FormalError> {
        Ok(root.join(ROOTS))
    }
    fn open_segment(&mut self, path: &Path) -> Result<File, FormalError> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        options.open(path).map_err(io)
    }
    fn metadata(&mut self, file: &File) -> Result<fs::Metadata, FormalError> {
        file.metadata().map_err(io)
    }
    fn seek(&mut self, file: &mut File, position: SeekFrom) -> Result<u64, FormalError> {
        file.seek(position).map_err(io)
    }
    fn set_len(&mut self, file: &File, len: u64) -> Result<(), FormalError> {
        file.set_len(len).map_err(io)
    }
    fn read_exact(&mut self, file: &mut File, bytes: &mut [u8]) -> Result<(), FormalError> {
        file.read_exact(bytes).map_err(io)
    }
    fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> Result<(), FormalError> {
        file.write_all(bytes).map_err(io)
    }
    fn fsync(&mut self, file: &File) -> Result<(), FormalError> {
        file.sync_all().map_err(io)
    }
    fn sync_dir(&mut self, path: &Path) -> Result<(), FormalError> {
        sync(path)
    }
    fn checksum(&mut self, previous: &str, logical: &[u8]) -> Result<String, FormalError> {
        checksum(previous, logical)
    }
    fn decode_hex(&mut self, text: &str) -> Result<Vec<u8>, FormalError> {
        hex::decode(text).map_err(io)
    }
    fn encode_hex(&mut self, bytes: &[u8]) -> Result<String, FormalError> {
        Ok(hex::encode(bytes))
    }
    fn publish_head(&mut self, root: &Path, state: &Head) -> Result<(), FormalError> {
        publish_head(root, state)
    }
}
/// The legacy caller preserves its initialization and original I/O behavior.
pub(crate) fn retain(root: &Path, ids: &[String]) -> Result<(), FormalError> {
    if ids.is_empty() {
        return Ok(());
    }
    initialize(root)?;
    retain_initialized_with_io(root, ids, &mut OriginalRetainIo)
}
fn retain_initialized_with_io<S: AsRef<str>>(
    root: &Path,
    ids: &[S],
    operations: &mut impl RetainIo,
) -> Result<(), FormalError> {
    let mut state = operations.head(root)?;
    if state.count > 0 && state.count % RECORDS == 0 {
        let previous_path = operations.segment_path(root, state.count / RECORDS - 1)?;
        let previous = operations.read(&previous_path, SEGMENT_BYTES)?;
        if previous.len() != SEGMENT_BYTES
            || operations.encode_hex(&previous[SEGMENT_BYTES - 32..])? != state.checksum
        {
            return Err(invalid(
                "logical retention sealed published tail missing/checksum",
            ));
        }
    }
    let mut offset = 0usize;
    while offset < ids.len() {
        operations.step()?;
        let index = state.count / RECORDS;
        let committed = (state.count % RECORDS) as usize * RECORD_BYTES;
        let path = operations.segment_path(root, index)?;
        let mut file = operations.open_segment(&path)?;
        let meta = operations.metadata(&file)?;
        if !meta.is_file() || meta.len() > SEGMENT_BYTES as u64 || meta.len() < committed as u64 {
            return Err(invalid(
                "logical retention published segment truncated/type/bound",
            ));
        }
        // SAFETY: geteuid has no caller preconditions.
        #[cfg(unix)]
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
            return Err(invalid("logical retention owner/write mode"));
        }
        if committed > 0 {
            operations.seek(&mut file, SeekFrom::Start((committed - 32) as u64))?;
            let mut tail = [0u8; 32];
            operations.read_exact(&mut file, &mut tail)?;
            if operations.encode_hex(&tail)? != state.checksum {
                return Err(invalid("logical retention published tail checksum"));
            }
        }
        // Only the exact suffix beyond the published head can be discarded.
        operations.set_len(&file, committed as u64)?;
        operations.seek(&mut file, SeekFrom::End(0))?;
        let take = ((SEGMENT_BYTES - committed) / RECORD_BYTES).min(ids.len() - offset);
        for logical in &ids[offset..offset + take] {
            operations.step()?;
            let logical = logical.as_ref();
            hash_hex(logical)?;
            let digest = operations.decode_hex(logical)?;
            state.checksum = operations.checksum(&state.checksum, &digest)?;
            operations.write_all(&mut file, &digest)?;
            let checksum = operations.decode_hex(&state.checksum)?;
            operations.write_all(&mut file, &checksum)?;
            state.count = state
                .count
                .checked_add(1)
                .ok_or_else(|| invalid("logical retention count exhausted"))?;
        }
        operations.fsync(&file)?;
        let directory = operations.directory_path(root)?;
        operations.sync_dir(&directory)?;
        operations.publish_head(root, &state)?;
        offset += take;
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PageCursor {
    pub count: u64,
    pub checksum: String,
}
impl PageCursor {
    pub(crate) fn start() -> Self {
        start()
    }
}
pub(crate) fn start() -> PageCursor {
    PageCursor {
        count: 0,
        checksum: initial(),
    }
}
fn check_cursor(root: &Path, cursor: &PageCursor, state: &Head) -> Result<(), FormalError> {
    hash_hex(&cursor.checksum)?;
    if cursor.count > state.count {
        return Err(invalid("logical retention cursor past published head"));
    }
    if cursor.count == 0 {
        if cursor.checksum != initial() {
            return Err(invalid("logical retention initial cursor checksum"));
        }
    } else {
        let previous = cursor.count - 1;
        let bytes = read(&segment(root, previous / RECORDS), SEGMENT_BYTES)?;
        let offset = (previous % RECORDS) as usize * RECORD_BYTES + 32;
        let checksum = bytes
            .get(offset..offset + 32)
            .ok_or_else(|| invalid("logical retention cursor prefix truncated"))?;
        if hex::encode(checksum) != cursor.checksum {
            return Err(invalid("logical retention cursor prefix changed"));
        }
    }
    Ok(())
}
/// A cursor is the caller's previously verified continuation, not an authority
/// grant. Persist it with the copied catalog transaction. Each next page checks
/// its exact retained prefix and authenticates every newly returned record.
pub(crate) fn page(
    root: &Path,
    cursor: &PageCursor,
    limit: usize,
) -> Result<(Vec<String>, PageCursor, bool), FormalError> {
    if limit == 0 || limit > RECORDS as usize {
        return Err(invalid("logical retention page count bound"));
    }
    let state = head(root)?;
    check_cursor(root, cursor, &state)?;
    let mut next = cursor.clone();
    let mut ids = Vec::new();
    while next.count < state.count && ids.len() < limit {
        let bytes = read(&segment(root, next.count / RECORDS), SEGMENT_BYTES)?;
        let begin = (next.count % RECORDS) as usize * RECORD_BYTES;
        let remaining = (state.count - next.count).min(RECORDS - next.count % RECORDS) as usize;
        let take = remaining.min(limit - ids.len());
        let records = bytes
            .get(begin..begin + take * RECORD_BYTES)
            .ok_or_else(|| invalid("logical retention page published prefix truncated"))?;
        for record in records.chunks_exact(RECORD_BYTES) {
            next.checksum = checksum(&next.checksum, &record[..32])?;
            if hex::encode(&record[32..]) != next.checksum {
                return Err(invalid("logical retention page chain checksum"));
            }
            ids.push(hex::encode(&record[..32]));
            next.count += 1;
        }
    }
    let complete = next.count == state.count;
    if complete && next.checksum != state.checksum {
        return Err(invalid("logical retention page head checksum"));
    }
    Ok((ids, next, complete))
}
pub(crate) fn count(root: &Path) -> Result<u64, FormalError> {
    let state = head(root)?;
    check_cursor(
        root,
        &PageCursor {
            count: state.count,
            checksum: state.checksum.clone(),
        },
        &state,
    )?;
    Ok(state.count)
}
/// Stream and authenticate all published records, then independently read each
/// retained logical alias. No history-count cap or transient all-ID set exists.
pub(crate) fn verify(root: &Path) -> Result<(), FormalError> {
    let state = head(root)?;
    let mut observed = initial();
    let mut done = 0u64;
    while done < state.count {
        let index = done / RECORDS;
        let records = (state.count - done).min(RECORDS) as usize;
        let bytes = read(&segment(root, index), SEGMENT_BYTES)?;
        if bytes.len() < records * RECORD_BYTES
            || (records == RECORDS as usize && bytes.len() != SEGMENT_BYTES)
        {
            return Err(invalid("logical retention published prefix truncated"));
        }
        for record in bytes[..records * RECORD_BYTES].chunks_exact(RECORD_BYTES) {
            observed = checksum(&observed, &record[..32])?;
            if hex::encode(&record[32..]) != observed {
                return Err(invalid("logical retention chain checksum"));
            }
            let logical = hex::encode(&record[..32]);
            super::super::business_pack::read_object_durable(&root.join(&logical), &logical)?
                .ok_or_else(|| invalid("logical retention registered alias missing"))?;
            done += 1;
        }
    }
    if observed != state.checksum {
        return Err(invalid("logical retention head/prefix checksum"));
    }
    // Unknown files cannot silently become sweepable metadata. Known .head-
    // temporaries and an append suffix are explicitly unselected crash debris.
    for entry in fs::read_dir(root.join(ROOTS)).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| invalid("logical retention filename encoding"))?;
        if name == HEAD {
            continue;
        }
        if let Some(id) = name.strip_prefix(".head-") {
            hash_hex(id)?;
            read(&entry.path(), 1024)?;
            continue;
        }
        let Some(index) = name.strip_suffix(".bin") else {
            return Err(invalid("logical retention unknown file"));
        };
        if index.len() != 20 || !index.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid("logical retention segment name"));
        }
        let index: u64 = index.parse().map_err(io)?;
        if index > state.count / RECORDS {
            return Err(invalid("logical retention unexpected segment"));
        }
        read(&entry.path(), SEGMENT_BYTES)?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "physical_roots_tests.rs"]
mod tests;

// Shared measured wire only: no store, retention or execution permission.
fn encode_retention_head(
    value: &Head,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    work: &super::super::endpoint::FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>, FormalError> {
    super::super::endpoint::store::stage_io::encode_wire(value, memory, work, 104, cancelled)
}

#[path = "physical_roots_bounded_append.rs"]
mod bounded_append;
pub(in crate::controlled_authority::formal) use bounded_append::{
    RetainedAppend, append_one_bounded_v1, append_staged_history_bounded_v1,
    append_staged_node_bounded_v1, append_staged_request_bounded_v1,
};
