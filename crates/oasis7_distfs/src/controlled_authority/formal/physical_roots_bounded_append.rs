//! A single logical-retention append under a genuine live Stage owner.
//! Retention is data durability, never publication or execution authority.
use super::*;
use crate::controlled_authority::formal::endpoint::FormalArchiveReadWorkV1;
use crate::controlled_authority::formal::endpoint::store::StagedIndexBlobV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::io::{Read, Write};

pub(in crate::controlled_authority::formal) struct RetainedAppend {
    _phase: ReservedValue<()>,
}
struct BoundedRetainIo<'a> {
    memory: &'a MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    stop: &'a mut dyn FnMut() -> bool,
}
fn checked_add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded retention layout overflow"))
}
fn phase_layout(root: &Path) -> Result<usize, FormalError> {
    let directory = checked_add(root.as_os_str().as_encoded_bytes().len(), 1 + ROOTS.len())?;
    // Actual simultaneously live paths: caller's segment, kernel directory,
    // publish directory, temporary head and target head. Include temporary component/challenge.
    let paths = [
        checked_add(directory, 25)?,
        directory,
        directory,
        checked_add(directory, 71)?,
        checked_add(directory, 10)?,
    ];
    let mut n = 0;
    for path in paths {
        n = checked_add(n, checked_add(path, std::mem::size_of::<PathBuf>())?)?;
    }
    // One capped segment read (extra byte detects concurrent growth), retained
    // decoded head String (<=1024 raw bytes), old/new checksum Strings (64 each),
    // digest32, checksum work Vec64, write checksum Vec32, tail hex String64,
    // challenge64 and temporary component70. No generic DTO admission.
    for bytes in [SEGMENT_BYTES + 1, 1024, 64, 64, 32, 64, 32, 64, 64, 70] {
        n = checked_add(n, bytes)?;
    }
    checked_add(
        n,
        std::mem::size_of::<Head>()
            + 6 * std::mem::size_of::<String>()
            + 5 * std::mem::size_of::<Vec<u8>>()
            + std::mem::size_of::<RetainedAppend>(),
    )
}
fn head_workspace(n: usize) -> Result<usize, FormalError> {
    // Locked serde_cbor_2 0.13 SliceRead scratch<=max(2n,8), and partial
    // checksum/field identifiers<=2n. Unexpected::Str Debug escaping<=6n;
    // String growth gives first diagnostic<=12n+2K. Once from_slice returns
    // Err, scratch/partial fields have dropped: two diagnostic Strings from
    // original map_err(io) coexist, <=24n+4K+64.
    const LABELS: &[&str] = &[
        "Head",
        "version",
        "count",
        "checksum",
        "struct Head",
        "field identifier",
        "a string",
        "u32",
        "u64",
        "unknown field ``, expected ",
        "invalid type: , expected ",
        "invalid value: , expected ",
        "duplicate field ``",
        "missing field ``",
        " at offset ",
        "one of ",
        " or ",
    ];
    // Six numeric slots, each at most 20 digits; 32 list/quote delimiters;
    // four primitive Debug descriptions, each <=26 bytes (f64/bool/unit).
    let mut k = 6 * 20 + 32 + 4 * 26;
    for label in LABELS {
        k = checked_add(k, label.len())?;
    }
    let parse = n.checked_mul(16).and_then(|v| v.checked_add(2 * k + 8));
    let bridge = n.checked_mul(24).and_then(|v| v.checked_add(4 * k + 64));
    checked_add(
        parse
            .zip(bridge)
            .map(|(a, b)| a.max(b))
            .ok_or_else(|| invalid("bounded retention head layout"))?,
        std::mem::size_of::<serde_cbor::Error>()
            + std::mem::size_of::<Head>()
            + 3 * std::mem::size_of::<String>()
            + 2 * std::mem::size_of::<Vec<u8>>(),
    )
}
impl RetainedAppend {
    pub(in crate::controlled_authority::formal) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self._phase.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal) fn reserved_bytes(&self) -> usize {
        self._phase.reserved_bytes()
    }
}
impl BoundedRetainIo<'_> {
    fn bytes(&mut self, n: usize) -> Result<(), FormalError> {
        self.work.consume_read_bytes(
            u64::try_from(n).map_err(|_| invalid("bounded retention byte layout"))?,
            self.stop,
        )
    }
    fn path(&mut self, root: &Path, component: &str) -> Result<PathBuf, FormalError> {
        self.step()?;
        let cap = checked_add(
            root.as_os_str().as_encoded_bytes().len(),
            checked_add(1, component.len())?,
        )?;
        let mut path = PathBuf::with_capacity(cap);
        if path.capacity() > cap {
            return Err(invalid("bounded retention path capacity"));
        }
        path.push(root);
        path.push(component);
        if path.capacity() > cap {
            return Err(invalid("bounded retention path growth"));
        }
        Ok(path)
    }
    fn write_loop(&mut self, file: &mut impl Write, bytes: &[u8]) -> Result<(), FormalError> {
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + 8192).min(bytes.len());
            self.bytes(end - offset)?;
            match file.write(&bytes[offset..end]) {
                Ok(0) => return Err(io(std::io::Error::from(std::io::ErrorKind::WriteZero))),
                Ok(n) if n <= end - offset => offset += n,
                Ok(_) => return Err(invalid("bounded retention writer exceeded supplied slice")),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io(e)),
            }
        }
        Ok(())
    }
    fn read_loop(&mut self, reader: &mut impl Read, out: &mut [u8]) -> Result<(), FormalError> {
        let mut offset = 0;
        while offset < out.len() {
            let end = (offset + 8192).min(out.len());
            self.bytes(end - offset)?;
            match reader.read(&mut out[offset..end]) {
                Ok(0) => return Err(io(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))),
                Ok(n) if n <= end - offset => offset += n,
                Ok(_) => return Err(invalid("bounded retention reader exceeded supplied slice")),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io(e)),
            }
        }
        Ok(())
    }
}
impl RetainIo for BoundedRetainIo<'_> {
    fn step(&mut self) -> Result<(), FormalError> {
        self.work.consume_steps(1, self.stop)
    }
    fn read(&mut self, path: &Path, bound: usize) -> Result<Vec<u8>, FormalError> {
        if bound != 1024 && bound != SEGMENT_BYTES {
            return Err(invalid("bounded retention read schema"));
        }
        paths::parents_with_step(path, &mut || self.work.consume_steps(1, self.stop))?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        self.step()?;
        let mut file = options.open(path).map_err(io)?;
        let meta = self.metadata(&file)?;
        paths::metadata(&meta, false)?;
        if !meta.is_file() || meta.len() > bound as u64 {
            return Err(invalid("physical file bound/type"));
        }
        #[cfg(unix)]
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
            return Err(invalid("physical registered owner/write mode"));
        }
        let cap = checked_add(bound, 1)?;
        let mut bytes = Vec::with_capacity(cap);
        if bytes.capacity() > cap {
            return Err(invalid("bounded retention read capacity"));
        }
        let mut buf = [0; 8192];
        while bytes.len() < cap {
            let request = (cap - bytes.len()).min(buf.len());
            self.bytes(request)?;
            match file.read(&mut buf[..request]) {
                Ok(0) => break,
                Ok(n) => bytes.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(io(e)),
            }
        }
        if bytes.len() > bound {
            return Err(invalid("physical read bound"));
        }
        Ok(bytes)
    }
    fn head(&mut self, root: &Path) -> Result<Head, FormalError> {
        let directory = self.path(root, ROOTS)?;
        let path = self.path(&directory, HEAD)?;
        let raw = self.read(&path, 1024)?;
        let _decode_fee = self
            .memory
            .reserve(head_workspace(raw.len())?, MemoryClass::Ordinary)
            .map_err(|_| invalid("bounded retention head admission"))?;
        self.bytes(raw.len())?;
        let head = decode_head(&raw)?;
        self.step()?;
        Ok(head)
    }
    fn segment_path(&mut self, root: &Path, index: u64) -> Result<PathBuf, FormalError> {
        let directory = self.path(root, ROOTS)?;
        let mut name = String::with_capacity(24);
        if name.capacity() > 24 {
            return Err(invalid("bounded retention segment name capacity"));
        }
        std::fmt::Write::write_fmt(&mut name, format_args!("{index:020}.bin"))
            .map_err(|_| invalid("bounded retention segment formatting"))?;
        self.path(&directory, &name)
    }
    fn directory_path(&mut self, root: &Path) -> Result<PathBuf, FormalError> {
        self.path(root, ROOTS)
    }
    fn open_segment(&mut self, path: &Path) -> Result<File, FormalError> {
        paths::parents_with_step(path, &mut || self.work.consume_steps(1, self.stop))?;
        self.step()?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        options.open(path).map_err(io)
    }
    fn metadata(&mut self, file: &File) -> Result<fs::Metadata, FormalError> {
        self.step()?;
        file.metadata().map_err(io)
    }
    fn seek(&mut self, file: &mut File, pos: SeekFrom) -> Result<u64, FormalError> {
        self.step()?;
        file.seek(pos).map_err(io)
    }
    fn set_len(&mut self, file: &File, len: u64) -> Result<(), FormalError> {
        self.step()?;
        file.set_len(len).map_err(io)
    }
    fn read_exact(&mut self, file: &mut File, out: &mut [u8]) -> Result<(), FormalError> {
        self.read_loop(file, out)
    }
    fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> Result<(), FormalError> {
        self.write_loop(file, bytes)
    }
    fn fsync(&mut self, file: &File) -> Result<(), FormalError> {
        self.step()?;
        file.sync_all().map_err(io)
    }
    fn sync_dir(&mut self, path: &Path) -> Result<(), FormalError> {
        paths::sync_with_step(path, &mut || self.work.consume_steps(1, self.stop))
    }
    fn checksum(&mut self, previous: &str, logical: &[u8]) -> Result<String, FormalError> {
        self.bytes(previous.len() + logical.len())?;
        checksum(previous, logical)
    }
    fn decode_hex(&mut self, text: &str) -> Result<Vec<u8>, FormalError> {
        self.bytes(text.len())?;
        hex::decode(text).map_err(io)
    }
    fn encode_hex(&mut self, bytes: &[u8]) -> Result<String, FormalError> {
        self.bytes(bytes.len())?;
        Ok(hex::encode(bytes))
    }
    fn publish_head(&mut self, root: &Path, state: &Head) -> Result<(), FormalError> {
        let directory = self.path(root, ROOTS)?;
        #[cfg(not(unix))]
        return Err(io(
            crate::controlled_authority::formal::fencing::FencingError::FencingUnproven,
        ));
        let challenge =
            crate::controlled_authority::formal::fencing::fresh_challenge_with_reader(|entropy| {
                self.step()?;
                let bridge = |e: std::io::Error| {
                    io(
                        crate::controlled_authority::formal::fencing::FencingError::Io(
                            e.to_string(),
                        ),
                    )
                };
                let mut random = File::open("/dev/urandom").map_err(bridge)?;
                let mut offset = 0;
                while offset < entropy.len() {
                    self.bytes(entropy.len() - offset)?;
                    match random.read(&mut entropy[offset..]) {
                        Ok(0) => {
                            return Err(bridge(std::io::Error::from(
                                std::io::ErrorKind::UnexpectedEof,
                            )));
                        }
                        Ok(n) => offset += n,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(bridge(e)),
                    }
                }
                // The shared helper's subsequent hex32->String64 is bounded CPU.
                self.bytes(entropy.len())
            })?;
        let mut component = String::with_capacity(70);
        if component.capacity() > 70 {
            return Err(invalid("bounded retention temporary capacity"));
        }
        component.push_str(".head-");
        component.push_str(&challenge);
        let temp = self.path(&directory, &component)?;
        let target = self.path(&directory, HEAD)?;
        let encoded = super::encode_retention_head(state, self.memory, self.work, self.stop)?;
        paths::parents_with_step(&temp, &mut || self.work.consume_steps(1, self.stop))?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        self.step()?;
        let mut file = options.open(&temp).map_err(io)?;
        paths::metadata(&self.metadata(&file)?, false)?;
        self.write_all(&mut file, encoded.as_slice())?;
        self.fsync(&file)?;
        self.step()?;
        fs::rename(&temp, &target).map_err(io)?;
        self.sync_dir(&directory)
    }
}
pub(in crate::controlled_authority::formal) fn append_one_bounded_v1(
    staged: &StagedIndexBlobV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<RetainedAppend, FormalError> {
    work.consume_steps(1, stop)?;
    let (guard, id) = staged.retention_source(memory)?;
    if guard._publication.is_none() || guard._retirement.is_none() {
        return Err(invalid(
            "bounded retention requires initialized staging guard",
        ));
    }
    append_source_bounded(guard, id, memory, work, stop)
}

fn append_source_bounded(
    guard: &PublicationGuard,
    id: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<RetainedAppend, FormalError> {
    let phase = memory
        .reserve(phase_layout(&guard.root)?, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded retention phase admission"))?
        .retain(());
    let mut io = BoundedRetainIo { memory, work, stop };
    retain_initialized_with_io(&guard.root, &[id], &mut io)?;
    io.step()?;
    Ok(RetainedAppend { _phase: phase })
}

pub(in crate::controlled_authority::formal) fn append_staged_node_bounded_v1(
    node: &crate::controlled_authority::formal::endpoint::store::stage_io::StagedIndexNodeV1<'_>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<RetainedAppend, FormalError> {
    let (guard, id) = node.retention_source(memory, work, stop)?;
    append_source_bounded(guard, id, memory, work, stop)
}

#[cfg(test)]
#[path = "physical_roots_bounded_append_tests.rs"]
mod tests;

pub(in crate::controlled_authority::formal) fn append_staged_request_bounded_v1(
    source: &crate::controlled_authority::formal::endpoint::store::stage_io::StagedRequestStateV1<
        '_,
    >,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<RetainedAppend, FormalError> {
    let (guard, id) = source.retention_source(memory, work, stop)?;
    append_source_bounded(guard, id, memory, work, stop)
}
