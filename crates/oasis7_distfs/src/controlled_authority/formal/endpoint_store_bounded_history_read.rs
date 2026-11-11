//! Paid original HistoryNode point decoding. A node is data, not a history grant.
use super::super::{FormalArchiveReadOwnerV1, FormalArchiveReadWorkV1};
use super::paid_read::PaidRead;
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded history heap layout"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("bounded history heap layout"))
}
fn diagnostic_constant() -> Result<usize, FormalError> {
    const LABELS: &[&str] = &[
        "NativeHistoryNodeV1",
        "NativeHistoryLinkV1",
        "FormalHeadRefV1",
        "head",
        "request",
        "entry",
        "ancestors",
        "position",
        "record_id",
        "node",
        "struct NativeHistoryNodeV1",
        "struct NativeHistoryLinkV1",
        "struct FormalHeadRefV1",
        "field identifier",
        "a string",
        "a sequence",
        "u64",
        "option",
        "invalid type: , expected ",
        "invalid value: , expected ",
        "invalid length , expected ",
        "missing field ``",
        "duplicate field ``",
        "unknown field ``, expected ",
        "unknown variant ``, expected ",
        " at offset ",
        "one of ",
        " or ",
    ];
    // At most six length/offset/integer decimal slots (20 digits each), 32
    // fixed punctuation/list delimiters and four <=26-byte primitive Debug
    // descriptions. Hostile string bytes are covered by the separate 6n term.
    LABELS
        .iter()
        .try_fold(6 * 20 + 32 + 4 * 26, |n, s| add(n, s.len()))
}
fn decode_workspace(n: usize) -> Result<usize, FormalError> {
    // Original ancestors is a generic Vec<Link>. serde_core 1.0.229 cautious
    // preallocation is <=1MiB, then geometric slot capacity <=2*k*sizeof(Link).
    // Conservatively k<=actual input bytes n. No new 64-link grammar bound:
    // only the caller's original head-node validation applies that post-check.
    let slots = add(
        1 << 20,
        mul(mul(2, n)?, std::mem::size_of::<HistoryLink>())?,
    )?;
    // Three root Strings plus two per link: total owned string payload <=2n;
    // IoRead scratch <=2n+16KiB. First Debug-escaped diagnostic <=12n+2K.
    // Thus parse=slots+16n+16KiB+2K. On Err, partial DTO and scratch drop
    // before Invalid(error.to_string()); two error buffers <=24n+4K+64.
    let k = diagnostic_constant()?;
    let parse = add(add(add(slots, mul(16, n)?)?, 16 * 1024)?, mul(2, k)?)?;
    let bridge = add(add(mul(24, n)?, mul(4, k)?)?, 64)?;
    let headers = std::mem::size_of::<HistoryNode>()
        + std::mem::size_of::<HistoryLink>()
        + std::mem::size_of::<PaidRead<'_>>()
        + std::mem::size_of::<serde_cbor::Deserializer<serde_cbor::de::IoRead<&mut PaidRead<'_>>>>(
        )
        + std::mem::size_of::<serde_cbor::Error>()
        + std::mem::size_of::<FormalError>()
        + std::mem::size_of::<String>();
    add(parse.max(bridge), headers)
}
pub(in crate::controlled_authority::formal::endpoint) struct PaidHistoryNodeV1 {
    value: ReservedValue<HistoryNode>,
}
impl std::fmt::Debug for PaidHistoryNodeV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidHistoryNodeV1").finish_non_exhaustive()
    }
}
impl PaidHistoryNodeV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn node(&self) -> &HistoryNode {
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
fn decode_history(
    raw: &FormalArchiveReadOwnerV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidHistoryNodeV1, FormalError> {
    work.consume_steps(1, stop)?;
    if !raw.uses_budget(memory) || raw.as_slice().len() > LIMIT {
        return Err(invalid("bounded history raw pool/limit"));
    }
    // PaidHistoryNode's single ReservedValue wrapper is retained separately
    // from the Node inline header already included in the decoder phase.
    let wrapper = std::mem::size_of::<PaidHistoryNodeV1>()
        .checked_sub(std::mem::size_of::<HistoryNode>())
        .ok_or_else(|| invalid("bounded history owner layout"))?;
    let fee = memory
        .reserve(
            add(decode_workspace(raw.as_slice().len())?, wrapper)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded history heap admission"))?;
    let mut reader = PaidRead::new(raw.as_slice(), work, stop);
    let decoded = serde_cbor::from_reader::<HistoryNode, _>(&mut reader);
    if let Some(first) = reader.take_first_error() {
        return Err(first);
    }
    let node = decoded.map_err(|e| invalid(e.to_string()))?;
    reader.check_cancelled()?;
    Ok(PaidHistoryNodeV1 {
        value: fee.retain(node),
    })
}
pub(super) fn read_history_object_bounded_v1(
    store: &Store,
    id: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidHistoryNodeV1, FormalError> {
    work.consume_steps(1, stop)?;
    let raw = store
        .raw_object_bounded(id, LIMIT, memory, work, stop)?
        .ok_or_else(|| invalid("referenced formal archive object missing"))?;
    decode_history(&raw, memory, work, stop)
}
#[cfg(test)]
#[path = "endpoint_store_bounded_history_read_tests.rs"]
mod tests;
