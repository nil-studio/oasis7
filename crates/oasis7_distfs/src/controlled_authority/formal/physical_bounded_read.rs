//! Fresh single-object physical bytes; no execution/publication qualification.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::io::{Seek, SeekFrom};

pub(crate) struct PhysicalReadWorkV1 {
    pub remaining_steps: u64,
    pub remaining_read_bytes: u64,
    pub remaining_metadata_bytes: u64,
}
impl PhysicalReadWorkV1 {
    fn step(&mut self, cancelled: &mut impl FnMut() -> bool) -> Result<(), FormalError> {
        if cancelled() {
            return Err(invalid("bounded physical read cancelled"));
        }
        self.remaining_steps = self
            .remaining_steps
            .checked_sub(1)
            .ok_or_else(|| invalid("bounded physical read steps"))?;
        Ok(())
    }
    fn read(
        &mut self,
        n: usize,
        metadata: bool,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<(), FormalError> {
        self.step(cancelled)?;
        let n = u64::try_from(n).map_err(|_| invalid("bounded physical read byte layout"))?;
        if self.remaining_read_bytes < n {
            self.remaining_read_bytes = 0;
            return Err(invalid("bounded physical read IO bytes"));
        }
        self.remaining_read_bytes -= n;
        if metadata {
            if self.remaining_metadata_bytes < n {
                self.remaining_metadata_bytes = 0;
                return Err(invalid("bounded physical metadata bytes"));
            }
            self.remaining_metadata_bytes -= n;
        }
        Ok(())
    }
}
/// The original metadata decoder holds an encoded input, an envelope body,
/// canonical envelope output and canonical inner output at different phases.
/// Each growable byte vector is covered up to twice its 16MiB wire bound.
/// Tuple inventories grow geometrically: cover two slots per allowed member,
/// plus each separately allocated <=64-byte digest String. There are <=65536
/// objects and <=LIMIT/4096+1 frames. Fixed-depth serde/preflight stacks and
/// short temporary digest/path buffers are covered by the final 64KiB.
fn metadata_layout(metadata_bytes: usize) -> Result<usize, FormalError> {
    let objects = MAX_OBJECTS.checked_mul(2 * std::mem::size_of::<Span>() + 64);
    let frames = (LIMIT / 4096 + 1).checked_mul(2 * std::mem::size_of::<Frame>() + 128);
    metadata_bytes
        .checked_mul(8)
        .and_then(|n| n.checked_add(objects?))
        .and_then(|n| n.checked_add(frames?))
        .and_then(|n| n.checked_add(64 * 1024))
        .ok_or_else(|| invalid("bounded physical metadata layout"))
}

// Locked zstd-sys 2.1.2+zstd.1.5.7, zstd/lib/zstd.h and
// decompress/zstd_decompress.c: this estimator takes a size_t window and
// returns sizeof(DCtx)+bounded input/output buffers, without allocating.
#[cfg(unix)]
unsafe extern "C" {
    fn ZSTD_estimateDStreamSize(max_window_size: usize) -> usize;
}
#[cfg(unix)]
fn frame_layout() -> Result<usize, FormalError> {
    // SAFETY: scalar-only ABI matches zstd's size_t; FRAME is the same maximum
    // window enforced by the original decoder's window_log_max(16).
    let estimate = unsafe { ZSTD_estimateDStreamSize(FRAME) };
    // SAFETY: ZSTD_isError accepts every size_t result, with no pointer input.
    if unsafe { zstd::zstd_safe::zstd_sys::ZSTD_isError(estimate) } != 0 {
        return Err(invalid("bounded physical decoder estimate"));
    }
    estimate
        .checked_add(zstd::zstd_safe::DCtx::in_size())
        .and_then(|n| n.checked_add(2 * (FRAME + 2)))
        .and_then(|n| n.checked_add(2 * std::mem::size_of::<Vec<u8>>()))
        .ok_or_else(|| invalid("bounded physical frame layout"))
}
/// Bounded exact-capacity collection used by both original-codec callbacks.
/// Every actual read, including the final EOF probe, debits work before IO.
fn collect(
    reader: &mut dyn Read,
    bound: usize,
    work: &mut PhysicalReadWorkV1,
    metadata: bool,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<u8>, FormalError> {
    let capacity = bound
        .checked_add(1)
        .ok_or_else(|| invalid("bounded physical read capacity"))?;
    let mut output = Vec::with_capacity(capacity);
    if output.capacity() > capacity {
        return Err(invalid("bounded physical capacity exceeds admission"));
    }
    let mut buffer = [0u8; 8192];
    loop {
        let take = (capacity - output.len()).min(buffer.len());
        if take == 0 {
            return Err(invalid("bounded physical byte limit"));
        }
        work.read(take, metadata, cancelled)?;
        let n = reader.read(&mut buffer[..take]).map_err(io)?;
        if cancelled() {
            return Err(invalid("bounded physical read cancelled"));
        }
        if n == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..n]);
        if output.len() > bound {
            return Err(invalid("bounded physical byte limit"));
        }
    }
    Ok(output)
}
#[cfg(unix)]
fn stored(
    path: &Path,
    bound: usize,
    work: &mut PhysicalReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Vec<u8>, FormalError> {
    work.step(cancelled)?;
    paths::parents(path)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(io)?;
    let metadata = file.metadata().map_err(io)?;
    paths::metadata(&metadata, false)?;
    if metadata.len() > bound as u64 {
        return Err(invalid("bounded physical stored file limit"));
    }
    // SAFETY: geteuid has no arguments or pointer preconditions.
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
        return Err(invalid("bounded physical registered owner/write mode"));
    }
    collect(&mut file, bound, work, false, cancelled)
}
#[cfg(unix)]
pub(crate) fn read_unknown_length_durable_v1(
    path: &Path,
    id: &str,
    max: usize,
    memory: &MemoryBudget,
    work: &mut PhysicalReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
    work.step(cancelled)?;
    hash_hex(id)?;
    let root = path
        .parent()
        .ok_or_else(|| invalid("bounded physical root"))?;
    let path_bytes = root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| invalid("bounded physical path layout"))?;
    let _path_fee = memory
        .reserve(path_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded physical path admission"))?;
    let _pin = pin(root, false)?;
    work.step(cancelled)?;
    let Some((selected_root, mut file)) = batch::unix::open_selector(path, id)? else {
        work.step(cancelled)?;
        return Ok(None);
    };
    let metadata = file.metadata().map_err(io)?;
    paths::metadata(&metadata, false)?;
    let metadata_bytes =
        usize::try_from(metadata.len()).map_err(|_| invalid("bounded physical metadata size"))?;
    if metadata_bytes > META_LIMIT {
        return Err(invalid("bounded physical metadata size"));
    }
    let _metadata_fee = memory
        .reserve(metadata_layout(metadata_bytes)?, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded physical metadata admission"))?;
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    let bytes = collect(&mut file, metadata_bytes, work, true, cancelled)?;
    let after = file.metadata().map_err(io)?;
    paths::metadata(&after, false)?;
    if after.len() != metadata.len() || bytes.len() != metadata_bytes {
        return Err(invalid("bounded physical selector changed during read"));
    }
    work.step(cancelled)?;
    let recipe = recipe::decode_recipe(&bytes)?;
    let index = recipe
        .objects
        .binary_search_by(|v| v.id.as_str().cmp(id))
        .map_err(|_| invalid("bounded physical selector member absent"))?;
    let object = &recipe.objects[index];
    let length =
        usize::try_from(object.length).map_err(|_| invalid("bounded physical logical length"))?;
    if length > max {
        return Err(invalid("bounded physical caller output cap"));
    }
    let paid = length
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or_else(|| invalid("bounded physical output layout"))?;
    let output_fee = memory
        .reserve(paid, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded physical output admission"))?;
    let mut output = Vec::with_capacity(length);
    if output.capacity() > length {
        return Err(invalid("bounded physical output capacity"));
    }
    let _frame_fee = memory
        .reserve(frame_layout()?, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded physical frame admission"))?;
    let shared = std::cell::RefCell::new((work, cancelled));
    let mut position = 0u64;
    for frame in &recipe.frames {
        {
            let mut state = shared.borrow_mut();
            let (work, stop) = &mut *state;
            work.step(*stop)?;
        }
        let next = position
            .checked_add(u64::from(frame.length))
            .ok_or_else(|| invalid("bounded physical frame offset"))?;
        let end = object
            .start
            .checked_add(object.length)
            .ok_or_else(|| invalid("bounded physical object span"))?;
        if object.start < next && end > position {
            {
                let mut state = shared.borrow_mut();
                let (work, stop) = &mut *state;
                work.step(*stop)?;
            }
            let mut read_stored = |path: &Path, bound: usize| {
                let mut state = shared.borrow_mut();
                let (work, stop) = &mut *state;
                stored(path, bound, *work, *stop)
            };
            let mut read_decoded = |reader: &mut dyn Read, bound: usize| {
                let mut state = shared.borrow_mut();
                let (work, stop) = &mut *state;
                collect(reader, bound, *work, false, *stop)
            };
            let decoded = super::decode_frame_with_readers(
                &selected_root,
                frame,
                &mut read_stored,
                &mut read_decoded,
            )?;
            let begin = object.start.max(position);
            let finish = end.min(next);
            let slice = &decoded[(begin - position) as usize..(finish - position) as usize];
            if output
                .len()
                .checked_add(slice.len())
                .is_none_or(|n| n > length)
            {
                return Err(invalid("bounded physical output coverage"));
            }
            output.extend_from_slice(slice);
        }
        position = next;
    }
    if output.len() != length || blake3::hash(&output).to_hex().as_str() != id {
        return Err(invalid("bounded physical logical coverage/hash mismatch"));
    }
    {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
        file.sync_all().map_err(io)?;
    }
    for frame in &recipe.frames {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
        sync(&selected_root.join(FRAMES).join(&frame.stored))?;
    }
    for dir in [
        selected_root.join(FRAMES),
        selected_root.join(SELECTORS),
        selected_root.clone(),
    ] {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
        sync(&dir)?;
    }
    {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
    }
    Ok(Some(output_fee.retain(output)))
}
#[cfg(all(test, unix))]
#[path = "physical_bounded_read_tests.rs"]
mod tests;
