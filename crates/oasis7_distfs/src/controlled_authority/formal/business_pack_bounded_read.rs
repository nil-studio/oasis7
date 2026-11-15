//! Prepaid unknown-length legacy content, using the sole original pack parser.
//! A checked byte owner is data only; no Frozen/publication qualification.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::cell::RefCell;
pub(crate) struct LegacyReadWorkV1 {
    pub remaining_steps: u64,
    pub remaining_read_bytes: u64,
}
impl LegacyReadWorkV1 {
    fn step(&mut self, cancelled: &mut impl FnMut() -> bool) -> Result<(), FormalError> {
        if cancelled() {
            return Err(invalid("bounded legacy read cancelled"));
        }
        self.remaining_steps = self
            .remaining_steps
            .checked_sub(1)
            .ok_or_else(|| invalid("bounded legacy read steps"))?;
        Ok(())
    }
    fn read(&mut self, n: usize, cancelled: &mut impl FnMut() -> bool) -> Result<(), FormalError> {
        self.step(cancelled)?;
        let n = u64::try_from(n).map_err(|_| invalid("bounded legacy byte layout"))?;
        if self.remaining_read_bytes < n {
            self.remaining_read_bytes = 0;
            return Err(invalid("bounded legacy IO bytes"));
        }
        self.remaining_read_bytes -= n;
        Ok(())
    }
}
/// EOF is the only recoverable inner IO result. In particular source errors,
/// work exhaustion and cancellation must not enter the magic-raw fallback.
fn exact(
    file: &mut File,
    mut bytes: &mut [u8],
    work: &mut LegacyReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<std::io::Result<()>, FormalError> {
    while !bytes.is_empty() {
        let take = bytes.len().min(8192);
        work.read(take, cancelled)?;
        let n = match file.read(&mut bytes[..take]) {
            Ok(0) => return Ok(Err(std::io::ErrorKind::UnexpectedEof.into())),
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(error)),
        };
        if cancelled() {
            return Err(invalid("bounded legacy read cancelled"));
        }
        bytes = &mut bytes[n..];
    }
    Ok(Ok(()))
}
fn raw_hash(
    file: &mut File,
    len: u64,
    work: &mut LegacyReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<blake3::Hash, FormalError> {
    let mut buffer = [0u8; 65536];
    let mut hash = blake3::Hasher::new();
    let mut total = 0u64;
    loop {
        let take = usize::try_from(
            len.checked_sub(total)
                .ok_or_else(|| invalid("bounded legacy raw length changed"))?
                .min(buffer.len() as u64),
        )
        .map_err(|_| invalid("bounded legacy raw layout"))?;
        // A one-byte EOF probe prevents accepting a file that grew after metadata.
        let take = take.max(1);
        work.read(take, cancelled)?;
        let n = match file.read(&mut buffer[..take]) {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(io(error)),
        };
        if cancelled() {
            return Err(invalid("bounded legacy read cancelled"));
        }
        if n == 0 {
            if total != len {
                return Err(invalid("bounded legacy raw length changed"));
            }
            break;
        }
        total = total
            .checked_add(n as u64)
            .ok_or_else(|| invalid("bounded legacy raw length overflow"))?;
        if total > len {
            return Err(invalid("bounded legacy raw length changed"));
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize())
}
pub(crate) fn read_legacy_unknown_length_durable_v1(
    path: &Path,
    id: &str,
    max: usize,
    memory: &MemoryBudget,
    work: &mut LegacyReadWorkV1,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Option<ReservedValue<Vec<u8>>>, FormalError> {
    work.step(cancelled)?;
    hash_hex(id)?;
    let root = path
        .parent()
        .ok_or_else(|| invalid("bounded legacy root"))?;
    // Original parser keeps only MAGIC prefix, 24-byte header, one 44-byte
    // binary-search row and a 32-byte digest. Its fallback hash uses a 64KiB
    // stack array. Cover that fixed workspace and bounded path/name clones.
    let layout = root
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_mul(4)
        .and_then(|n| n.checked_add(65536 + 4096))
        .ok_or_else(|| invalid("bounded legacy control layout"))?;
    let _control = memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded legacy control admission"))?;
    let _pin = super::super::physical::pin(root, false)?;
    let shared = RefCell::new((work, cancelled));
    let mut read_exact = |file: &mut File, bytes: &mut [u8]| {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        exact(file, bytes, *work, *stop)
    };
    let mut seek = |file: &mut File, at: SeekFrom| {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
        file.seek(at).map_err(io)?;
        Ok(())
    };
    let mut hash_raw = |file: &mut File, len: u64| {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        raw_hash(file, len, *work, *stop)
    };
    let Some(mut object) = super::open_expected_with_readers(
        path,
        id,
        None,
        max,
        &mut read_exact,
        &mut seek,
        &mut hash_raw,
    )?
    else {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
        return Ok(None);
    };
    let length =
        usize::try_from(object.len).map_err(|_| invalid("bounded legacy logical layout"))?;
    if length > max {
        return Err(invalid("bounded legacy caller output cap"));
    }
    let paid = length
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or_else(|| invalid("bounded legacy output layout"))?;
    let fee = memory
        .reserve(paid, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded legacy output admission"))?;
    let mut output = Vec::with_capacity(length);
    if output.capacity() > length {
        return Err(invalid("bounded legacy output capacity"));
    }
    output.resize(length, 0);
    let before = object.file.metadata().map_err(io)?;
    super::super::physical::paths::metadata(&before, false)?;
    seek(&mut object.file, SeekFrom::Start(object.start))?;
    read_exact(&mut object.file, &mut output)?.map_err(io)?;
    if blake3::hash(&output).to_hex().as_str() != id {
        return Err(invalid("logical CAS object checksum"));
    }
    let after = object.file.metadata().map_err(io)?;
    super::super::physical::paths::metadata(&after, false)?;
    if before.len() != after.len() {
        return Err(invalid("bounded legacy anchor length changed"));
    }
    {
        let mut state = shared.borrow_mut();
        let (work, stop) = &mut *state;
        work.step(*stop)?;
        object.file.sync_all().map_err(io)?;
        work.step(*stop)?;
    }
    Ok(Some(fee.retain(output)))
}
#[cfg(all(test, unix))]
#[path = "business_pack_bounded_read_tests.rs"]
mod tests;
