//! Original fixed LocalClosure metadata point read; never continuity authority.
use super::super::incremental::local_closure::LocalDataClosureRecordV1;
use super::super::{FormalArchiveReadOwnerV1, FormalArchiveReadWorkV1};
use super::paid_read::PaidRead;
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded local closure layout"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("bounded local closure layout"))
}
fn diagnostic_constant() -> Result<usize, FormalError> {
    // Original Fixed visitor messages and serde_cbor_2 0.13 ErrorCode Display.
    const LABELS: &[&str] = &[
        "fixed local data closure v1 bytes",
        "local closure fixed domain/length",
        "local closure unknown version",
        "local closure generation option",
        "local closure fixed decode length",
        "invalid type: , expected ",
        "invalid value: , expected ",
        "invalid length , expected ",
        "a string",
        "byte array",
        "a sequence",
        "a map",
        "a tuple",
        "a boolean",
        "unit",
        "option",
        "integer",
        "floating point",
        " at offset ",
        "Scratch buffer too small",
        "EOF while parsing a value",
        "EOF while parsing an array",
        "EOF while parsing a map",
        "length out of range",
        "invalid UTF-8",
        "unassigned type",
        "unexpected code",
        "trailing data",
        "array too short",
        "array too long",
        "recursion limit exceeded",
        "wrong enum format",
        "wrong struct format",
    ];
    // Six u64/usize decimal slots (20 digits), 32 fixed delimiters, four <=26
    // byte primitive Debug descriptions. Input string escaping belongs to 6n.
    LABELS
        .iter()
        .try_fold(6 * 20 + 32 + 4 * 26, |n, s| add(n, s.len()))
}
fn decode_workspace(n: usize) -> Result<usize, FormalError> {
    let k = diagnostic_constant()?;
    // Fixed record has no owned DTO containers. IoRead scratch<=2n+16KiB;
    // first escaped/grown diagnostic<=12n+2K. Visitor creates only fixed fields.
    let parse = add(add(mul(14, n)?, 16 * 1024)?, mul(2, k)?)?;
    // Once from_reader returns Err, its scratch drops before Invalid bridge:
    // two diagnostic buffers may coexist, bounded by24n+4K+64.
    let bridge = add(add(mul(24, n)?, mul(4, k)?)?, 64)?;
    let headers = std::mem::size_of::<LocalDataClosureRecordV1>()
        + std::mem::size_of::<PaidRead<'_>>()
        + std::mem::size_of::<serde_cbor::Deserializer<serde_cbor::de::IoRead<&mut PaidRead<'_>>>>(
        )
        + std::mem::size_of::<serde_cbor::Error>()
        + std::mem::size_of::<FormalError>()
        + std::mem::size_of::<String>();
    add(parse.max(bridge), headers)
}
pub(in crate::controlled_authority::formal::endpoint) struct PaidLocalClosureRecordV1 {
    value: ReservedValue<LocalDataClosureRecordV1>,
}
impl std::fmt::Debug for PaidLocalClosureRecordV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidLocalClosureRecordV1")
            .finish_non_exhaustive()
    }
}
impl PaidLocalClosureRecordV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn record(
        &self,
    ) -> &LocalDataClosureRecordV1 {
        &self.value
    }
    pub(in crate::controlled_authority::formal::endpoint) fn uses_budget(
        &self,
        memory: &MemoryBudget,
    ) -> bool {
        self.value.uses_budget(memory)
    }
    pub(in crate::controlled_authority::formal::endpoint) fn reserved_bytes(&self) -> usize {
        self.value.reserved_bytes()
    }
}
fn decode_record(
    raw: &FormalArchiveReadOwnerV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidLocalClosureRecordV1, FormalError> {
    work.consume_steps(1, stop)?;
    if !raw.uses_budget(memory) || raw.as_slice().len() > LIMIT {
        return Err(invalid("bounded local closure raw pool/limit"));
    }
    let wrapper = std::mem::size_of::<PaidLocalClosureRecordV1>()
        .checked_sub(std::mem::size_of::<LocalDataClosureRecordV1>())
        .ok_or_else(|| invalid("bounded local closure owner layout"))?;
    let fee = memory
        .reserve(
            add(decode_workspace(raw.as_slice().len())?, wrapper)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded local closure heap admission"))?;
    let mut reader = PaidRead::new(raw.as_slice(), work, stop);
    let decoded = serde_cbor::from_reader::<LocalDataClosureRecordV1, _>(&mut reader);
    if let Some(first) = reader.take_first_error() {
        return Err(first);
    }
    let record = decoded.map_err(|e| invalid(e.to_string()))?;
    reader.check_cancelled()?;
    Ok(PaidLocalClosureRecordV1 {
        value: fee.retain(record),
    })
}
pub(super) fn read_local_closure_object_bounded_v1(
    store: &Store,
    id: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidLocalClosureRecordV1, FormalError> {
    work.consume_steps(1, stop)?;
    let raw = store
        .raw_object_bounded(id, LIMIT, memory, work, stop)?
        .ok_or_else(|| invalid("referenced formal archive object missing"))?;
    // Canonical encoder width is NOT the original byteString decoder grammar.
    decode_record(&raw, memory, work, stop)
}
#[cfg(test)]
#[path = "endpoint_store_bounded_local_closure_read_tests.rs"]
mod tests;
