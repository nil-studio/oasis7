//! Root-owned admission seams for immutable data staging; no retention or authority.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::formal::physical;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;

pub(in crate::controlled_authority::formal) fn encode_wire<T: Serialize>(
    value: &T,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    max: usize,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    work.with_child_counters(cancelled, |steps, bytes, stop| {
        let mut counters = bounded_wire::BoundedWireWorkV1 {
            max_wire_bytes: max,
            remaining_steps: steps,
            remaining_bytes: bytes,
        };
        let result =
            bounded_wire::encode_bounded_wire_v1(value, memory, &mut counters, &mut || stop())
                .map_err(|error| invalid(format!("bounded stage encoding: {error}")));
        (result, counters.remaining_steps, counters.remaining_bytes)
    })
}
fn marker_read(
    path: &Path,
    bound: usize,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Vec<u8>, FormalError> {
    // The only control reader is the original physical enabled marker (<=64).
    if bound != 64 {
        return Err(invalid("bounded guard control layout"));
    }
    physical::paths::parents_with_step(path, &mut || work.consume_steps(1, cancelled))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    work.consume_steps(1, cancelled)?;
    let mut file = options.open(path).map_err(io)?;
    work.consume_steps(1, cancelled)?;
    let metadata = file.metadata().map_err(io)?;
    physical::paths::metadata(&metadata, false)?;
    if metadata.len() > bound as u64 {
        return Err(invalid("physical file bound/type"));
    }
    let mut bytes = Vec::with_capacity(65);
    if bytes.capacity() > 65 {
        return Err(invalid("bounded guard control capacity"));
    }
    let mut buffer = [0u8; 65];
    while bytes.len() < 65 {
        let requested = 65 - bytes.len();
        work.consume_read_bytes(requested as u64, cancelled)?;
        let n = match file.read(&mut buffer[..requested]) {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(error)),
        };
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);
    }
    if bytes.len() > bound {
        return Err(invalid("physical read bound"));
    }
    work.consume_steps(1, cancelled)?;
    Ok(bytes)
}
pub(super) fn guard_initialized(
    root: &Path,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<physical::PublicationGuard>, FormalError> {
    work.consume_steps(1, cancelled)?;
    let n = root.as_os_str().as_encoded_bytes().len();
    // One retained exact-capacity root; at most two temporary join buffers,
    // each <=2*(n+suffix). A pin conflict prints BOTH root and guard path:
    // each can expand <=6 chars per raw byte, and formatting String capacity
    // can grow <2x. PID/inode/errno/labels fit the fixed 512-byte tail.
    let suffix = [
        ".physical-guard-v1",
        ".physical-publication-guard-v1",
        ".physical-write-v1",
    ]
    .iter()
    .map(|s| s.len() + 1)
    .max()
    .unwrap();
    let joins = n.checked_add(suffix).and_then(|v| v.checked_mul(4));
    let two_paths = n.checked_mul(2).and_then(|v| v.checked_add(suffix));
    let error = two_paths
        .and_then(|v| v.checked_mul(6))
        .and_then(|v| v.checked_add(512))
        .and_then(|v| v.checked_mul(2));
    let layout = joins
        .zip(error)
        .and_then(|(j, e)| n.checked_add(j)?.checked_add(e))
        .and_then(|v| {
            v.checked_add(
                512 + std::mem::size_of::<physical::PublicationGuard>()
                    + 4 * std::mem::size_of::<PathBuf>(),
            )
        })
        .ok_or_else(|| invalid("bounded publication guard layout"))?;
    let fee = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded publication guard admission"))?;
    let mut owned_root = PathBuf::with_capacity(n);
    if owned_root.capacity() > n {
        return Err(invalid("bounded guard root capacity"));
    }
    owned_root.push(root);
    if owned_root.capacity() > n {
        return Err(invalid("bounded guard root growth"));
    }
    // Error construction is prepaid here; returning FormalError keeps its
    // existing String contract, without claiming a retained error charge.
    let shared = RefCell::new(cancelled);
    let mut step = || work.consume_steps(1, &mut **shared.borrow_mut());
    let mut read =
        |path: &Path, bound: usize| marker_read(path, bound, work, &mut **shared.borrow_mut());
    let guard = physical::publication_guard_initialized_with_callbacks(
        root, owned_root, &mut step, &mut read,
    )?;
    Ok(fee.retain(guard))
}

/// Only this producer can mint a staged node; the borrowed guard keeps the
/// single writer session and its real dual leases alive through retention.
struct StagedObjectV1<'g> {
    guard: &'g ReservedValue<physical::PublicationGuard>,
    work: &'g FormalArchiveReadWorkV1,
    bytes: ReservedValue<Vec<u8>>,
    id: String,
    layout: ReservedValue<()>,
}
impl std::fmt::Debug for StagedObjectV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StagedObjectV1")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl StagedObjectV1<'_> {
    pub(super) fn id(&self) -> &str {
        &self.id
    }
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.guard.uses_budget(memory)
            && self.bytes.uses_budget(memory)
            && self.layout.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal) fn retention_source(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(&physical::PublicationGuard, &str), FormalError> {
        work.consume_steps(1, stop)?;
        if !std::ptr::eq(self.work, work) {
            return Err(invalid("bounded staged node foreign Work"));
        }
        if !self.uses_budget(memory) {
            return Err(invalid("bounded staged node foreign pool"));
        }
        self.guard.initialized_root()?;
        Ok((&self.guard, &self.id))
    }
}

pub(in crate::controlled_authority::formal) struct StagedIndexNodeV1<'g> {
    object: StagedObjectV1<'g>,
}
impl std::fmt::Debug for StagedIndexNodeV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.object.fmt(f)
    }
}
impl StagedIndexNodeV1<'_> {
    pub(super) fn id(&self) -> &str {
        self.object.id()
    }
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.object.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal) fn retention_source(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(&physical::PublicationGuard, &str), FormalError> {
        self.object.retention_source(memory, work, stop)
    }
}

pub(in crate::controlled_authority::formal) struct StagedRequestStateV1<'g> {
    object: StagedObjectV1<'g>,
}
impl std::fmt::Debug for StagedRequestStateV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.object.fmt(f)
    }
}
impl StagedRequestStateV1<'_> {
    pub(super) fn id(&self) -> &str {
        self.object.id()
    }
    pub(super) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.object.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal) fn retention_source(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<(&physical::PublicationGuard, &str), FormalError> {
        self.object.retention_source(memory, work, stop)
    }
}
fn text_wire(n: usize) -> Result<usize, FormalError> {
    let header = if n < 24 {
        1
    } else if n <= u8::MAX as usize {
        2
    } else if n <= u16::MAX as usize {
        3
    } else if n <= u32::MAX as usize {
        5
    } else {
        9
    };
    n.checked_add(header)
        .ok_or_else(|| invalid("bounded Index wire layout"))
}
fn node_wire_size(node: &Index) -> Result<usize, FormalError> {
    // Original named enum + named struct fields, using the actual text lengths
    // and scalar encoding. This is the original Index codec, not a reader cap.
    let n = match node {
        Index::Leaf { request, entry } => 21usize
            .checked_add(text_wire(request.len())?)
            .and_then(|n| n.checked_add(text_wire(entry.len()).ok()?)),
        Index::Fork { bit, left, right } => {
            let scalar = if *bit < 24 {
                1
            } else if *bit <= u8::MAX as u16 {
                2
            } else {
                3
            };
            22usize
                .checked_add(scalar)
                .and_then(|n| n.checked_add(text_wire(left.len()).ok()?))
                .and_then(|n| n.checked_add(text_wire(right.len()).ok()?))
        }
    }
    .ok_or_else(|| invalid("bounded Index wire layout"))?;
    if n > LIMIT {
        return Err(invalid("physical publication logical object bound"));
    }
    Ok(n)
}
pub(super) fn stage_index_node_bounded_v1<'g>(
    guard: &'g ReservedValue<physical::PublicationGuard>,
    node: &Index,
    memory: &MemoryBudget,
    work: &'g FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<StagedIndexNodeV1<'g>, FormalError> {
    let (child_cap, layout) = stage_object_layout(
        guard,
        memory,
        work,
        stop,
        std::mem::size_of::<StagedIndexNodeV1<'_>>(),
    )?;
    let max = node_wire_size(node)?;
    let bytes = encode_wire(node, memory, work, max, stop)?;
    if bytes.as_slice().len() != max {
        return Err(invalid("bounded Index original wire size mismatch"));
    }
    let id = super::stage_core::stage_wire_with_layout(
        guard,
        bytes.as_slice(),
        max,
        child_cap,
        super::stage_core::TEMP_NAME_CAP,
        memory,
        work,
        stop,
    )?;
    Ok(StagedIndexNodeV1 {
        object: StagedObjectV1 {
            guard,
            work,
            bytes,
            id,
            layout,
        },
    })
}

fn stage_object_layout(
    guard: &ReservedValue<physical::PublicationGuard>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
    token_bytes: usize,
) -> Result<(usize, ReservedValue<()>), FormalError> {
    work.consume_steps(1, stop)?;
    if !guard.uses_budget(memory) {
        return Err(invalid("bounded staged node guard pool"));
    }
    let root = guard.initialized_root()?;
    let child_cap = root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(1 + 64usize.max(super::stage_core::TEMP_NAME_CAP))
        .ok_or_else(|| invalid("bounded staged node path layout"))?;
    let layout_bytes = child_cap
        .checked_mul(2)
        .and_then(|n| {
            n.checked_add(
                super::stage_core::TEMP_NAME_CAP
                    + 64
                    + 2 * std::mem::size_of::<PathBuf>()
                    + std::mem::size_of::<String>()
                    + token_bytes,
            )
        })
        .ok_or_else(|| invalid("bounded staged node layout"))?;
    let layout = memory
        .reserve(layout_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded staged node admission"))?
        .retain(());
    Ok((child_cap, layout))
}

pub(super) fn stage_request_state_bounded_v1<'g>(
    guard: &'g ReservedValue<physical::PublicationGuard>,
    source: &PaidRequestStateV1,
    memory: &MemoryBudget,
    work: &'g FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<StagedRequestStateV1<'g>, FormalError> {
    work.consume_steps(1, stop)?;
    if !source.uses_budget(memory) {
        return Err(invalid("bounded staged request foreign pool"));
    }
    let (child_cap, layout) = stage_object_layout(
        guard,
        memory,
        work,
        stop,
        std::mem::size_of::<StagedRequestStateV1<'_>>(),
    )?;
    let bytes = encode_wire(source.state(), memory, work, LIMIT, stop)?;
    let id = super::stage_core::stage_wire_with_layout(
        guard,
        bytes.as_slice(),
        LIMIT,
        child_cap,
        super::stage_core::TEMP_NAME_CAP,
        memory,
        work,
        stop,
    )?;
    Ok(StagedRequestStateV1 {
        object: StagedObjectV1 {
            guard,
            work,
            bytes,
            id,
            layout,
        },
    })
}
