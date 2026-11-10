//! Concrete Index decoding and lookup, preserving the original serde wire.
//! Results identify data only: no entry decoding or publication permission.
use super::super::{FormalArchiveReadOwnerV1, FormalArchiveReadWorkV1};
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

use super::paid_read::PaidRead;
/// Locked Cargo serde_cbor alias = serde_cbor_2 0.13.0 IoRead grows scratch only after at most 16KiB
/// declared-length reserve; actual bytes then bound its capacity by 2n+16KiB.
/// Original Index owns at most two Strings with total capacity <=2n. Serde's
/// Unexpected::Str diagnostic can escape each input byte into <=6 characters;
/// geometric String growth gives <=12n+2K for its first error buffer.
/// serde_core 1.0.229 StringVisitor::visit_str uses to_owned; unknown_variant
/// uses Display, while invalid_type Unexpected::Str may use Debug escaping.
/// K sums all fixed Index identifiers, expected descriptions and error
/// templates below. The fixed formatting term separately covers six u64/usize
/// decimal slots (6*20, including offsets/lengths and expected/got values),
/// 32 quote/backtick/comma/space delimiters, and four 26-byte short scalar
/// descriptions (finite f64 Debug/scientific, bool/unit/type labels). These
/// alternatives are summed conservatively although one error uses fewer.
/// Hostile string content is accounted by 6n, never this fixed term.
/// During decode: 2n scratch + 2n fields + 12n error +16KiB+2K.
/// On from_reader Err, its Deserializer/scratch and partial fields are dropped
/// before map_err(invalid): only two formatted error buffers coexist, <=24n+4K+64.
/// The raw input owner is separately paid. Returning FormalError preserves the
/// old diagnostic contract; we do not claim its String retains this phase fee.
fn decode_workspace(n: usize) -> Result<usize, FormalError> {
    const FIXED: &[&str] = &[
        "Leaf",
        "Fork",
        "Index",
        "request",
        "entry",
        "bit",
        "left",
        "right",
        "variant identifier",
        "field identifier",
        "struct variant Index::Leaf",
        "struct variant Index::Fork",
        "a string",
        "u16",
        "unknown variant ``, expected ",
        "invalid type: , expected ",
        "invalid value: , expected ",
        "duplicate field ``",
        "missing field ``",
        " at offset ",
    ];
    const DECIMAL_SLOTS: usize = 6 * 20;
    const DELIMITERS: usize = 32;
    const SHORT_SCALARS: usize = 4 * 26;
    let fixed_format = DECIMAL_SLOTS + DELIMITERS + SHORT_SCALARS;
    let k = FIXED
        .iter()
        .try_fold(fixed_format, |n, s| n.checked_add(s.len()))
        .ok_or_else(|| invalid("bounded Index diagnostic layout"))?;
    let parse = n
        .checked_mul(16)
        .and_then(|v| v.checked_add(16 * 1024))
        .and_then(|v| v.checked_add(k.checked_mul(2)?));
    let bridge = n
        .checked_mul(24)
        .and_then(|v| v.checked_add(k.checked_mul(4)?))
        .and_then(|v| v.checked_add(64));
    let headers = std::mem::size_of::<Index>()
        + std::mem::size_of::<PaidRead<'_>>()
        + std::mem::size_of::<serde_cbor::Error>()
        + 4 * std::mem::size_of::<String>()
        + 4 * std::mem::size_of::<Vec<u8>>();
    parse
        .zip(bridge)
        .and_then(|(a, b)| a.max(b).checked_add(headers))
        .ok_or_else(|| invalid("bounded Index heap layout"))
}
pub(super) fn decode_index_bounded_v1(
    raw: &FormalArchiveReadOwnerV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<Index>, FormalError> {
    work.consume_steps(1, cancelled)?;
    if !raw.uses_budget(memory) || raw.as_slice().len() > LIMIT {
        return Err(invalid("bounded Index raw pool/limit"));
    }
    let mut fee = memory
        .reserve(
            decode_workspace(raw.as_slice().len())?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded Index heap admission"))?;
    let mut reader = PaidRead::new(raw.as_slice(), work, cancelled);
    let result = serde_cbor::from_reader::<Index, _>(&mut reader);
    if let Some(first) = reader.take_first_error() {
        return Err(first);
    }
    let value = result.map_err(|error| invalid(error.to_string()))?;
    reader.check_cancelled()?;
    let strings = match &value {
        Index::Leaf { request, entry } => request.capacity().checked_add(entry.capacity()),
        Index::Fork { left, right, .. } => left.capacity().checked_add(right.capacity()),
    }
    .ok_or_else(|| invalid("bounded Index retained layout"))?;
    let bytes = strings
        .checked_add(std::mem::size_of::<Index>())
        .ok_or_else(|| invalid("bounded Index retained layout"))?;
    let retained = fee.split(bytes).map_err(invalid)?;
    Ok(retained.retain(value))
}
pub(super) fn copy_paid(
    value: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<String>, FormalError> {
    work.consume_read_bytes(value.len() as u64, cancelled)?;
    let bytes = value
        .len()
        .checked_add(std::mem::size_of::<String>())
        .ok_or_else(|| invalid("bounded Index string layout"))?;
    let fee = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded Index string admission"))?;
    let mut string = String::with_capacity(value.len());
    if string.capacity() > value.len() {
        return Err(invalid("bounded Index string capacity"));
    }
    string.push_str(value);
    work.consume_steps(1, cancelled)?;
    Ok(fee.retain(string))
}
pub(super) fn lookup_bounded_v1(
    store: &Store,
    request: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<ReservedValue<String>>, FormalError> {
    work.consume_steps(1, cancelled)?;
    hash_hex(request)?;
    let mut current = store
        .body
        .index
        .as_deref()
        .map(|s| copy_paid(s, memory, work, cancelled))
        .transpose()?;
    let mut previous = None;
    while let Some(reference) = current {
        work.consume_steps(1, cancelled)?;
        let raw = store
            .raw_object_bounded(&reference, LIMIT, memory, work, cancelled)?
            .ok_or_else(|| invalid("referenced formal archive object missing"))?;
        let node = decode_index_bounded_v1(&raw, memory, work, cancelled)?;
        match &*node {
            Index::Leaf {
                request: key,
                entry,
            } => {
                work.consume_read_bytes(key.len() as u64, cancelled)?;
                hash_hex(key)?;
                return if key == request {
                    copy_paid(entry, memory, work, cancelled).map(Some)
                } else {
                    Ok(None)
                };
            }
            Index::Fork { bit, left, right } => {
                if *bit >= 256 || previous.is_some_and(|old| *bit <= old) {
                    return Err(invalid("formal index path order"));
                }
                previous = Some(*bit);
                let _bit_fee = memory
                    .reserve(32 + std::mem::size_of::<Vec<u8>>(), MemoryClass::Ordinary)
                    .map_err(|_| invalid("bounded Index bit admission"))?;
                work.consume_steps(1, cancelled)?;
                let chosen = if Store::bit(request, *bit)? {
                    right
                } else {
                    left
                };
                current = Some(copy_paid(chosen, memory, work, cancelled)?);
            }
        }
    }
    work.consume_steps(1, cancelled)?;
    Ok(None)
}
#[cfg(all(test, unix))]
#[path = "endpoint_store_bounded_index_tests.rs"]
mod tests;
