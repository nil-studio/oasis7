//! Original canonical-record ID under one continuing budget; data only.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::CANONICAL_RECORD_ID_DOMAIN_V1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded record ID layout"))
}
// Original validation diagnostics contain no interpolated input. Each listed
// literal is a possible original validator result; sum safely bounds any one.
fn validation_layout() -> Result<usize, FormalError> {
    const MESSAGES: &[&str] = &[
        "formal world identity bounds",
        "formal digest/key requires canonical lowercase hex32",
        "formal record version/position/closure roots",
        "incremental record original input root",
        "incremental authority typed root set",
        "legacy record cannot carry incremental roots",
        "durable set transition cannot combine profile handoff",
        "formal initial record boundary",
        "formal execution requires original frozen reservation",
        "formal handoff target/boundary",
    ];
    let mut n = add(size_of::<FormalError>(), size_of::<String>())?;
    for message in MESSAGES {
        n = add(n, message.len())?;
    }
    Ok(n)
}
pub(in crate::controlled_authority::formal::endpoint) struct PaidCanonicalRecordIdV1 {
    value: ReservedValue<String>,
}
impl std::fmt::Debug for PaidCanonicalRecordIdV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidCanonicalRecordIdV1")
            .finish_non_exhaustive()
    }
}
impl PaidCanonicalRecordIdV1 {
    pub(in crate::controlled_authority::formal::endpoint) fn id(&self) -> &str {
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
pub(super) fn hash_canonical_record_bounded_v1(
    record: &CanonicalRecordV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidCanonicalRecordIdV1, FormalError> {
    work.consume_steps(1, stop)?;
    let _diagnostics = memory
        .reserve(validation_layout()?, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded record ID validation admission"))?;
    record.validate_with_work(&mut |n| {
        work.charge(
            1,
            u64::try_from(n).map_err(|_| invalid("bounded record ID layout"))?,
            stop,
        )
    })?;
    let mut ctx = work.bounded_context(memory, stop);
    let encoded = super::context_wire::encode_paid(
        record,
        LIMIT,
        &mut ctx,
        "formal encoding exceeds byte bound",
        "bounded record ID encoding",
    )?;
    let domain = CANONICAL_RECORD_ID_DOMAIN_V1.as_bytes();
    let frame_len = add(add(8, domain.len())?, encoded.len())?;
    // The streaming state, finalized Hash and hex ArrayString (64 data
    // bytes plus a pointer-sized header allowance; locked length is u32) remain live with the charged frame.
    let hash_stack = add(
        add(size_of::<blake3::Hasher>(), size_of::<blake3::Hash>())?,
        add(64, size_of::<usize>())?,
    )?;
    let frame_layout = add(
        add(size_of::<ReservedValue<Vec<u8>>>(), frame_len)?,
        hash_stack,
    )?;
    let frame_fee = memory
        .reserve(frame_layout, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded record ID frame admission"))?;
    ctx.charge(1, 0)?;
    let mut frame = Vec::with_capacity(frame_len);
    if frame.capacity() != frame_len {
        return Err(invalid("bounded record ID frame capacity"));
    }
    for bytes in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain,
        encoded.as_slice(),
    ] {
        for chunk in bytes.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            frame.extend_from_slice(chunk);
        }
    }
    let frame = frame_fee.retain(frame);
    let output_fee = memory
        .reserve(
            add(size_of::<PaidCanonicalRecordIdV1>(), 64)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded record ID output admission"))?;
    // Streaming BLAKE3 has the same digest as the original one-shot function,
    // with a cancellation/work point before every actual input chunk.
    let mut hasher = blake3::Hasher::new();
    for chunk in frame.chunks(8192) {
        ctx.charge(1, chunk.len() as u64)?;
        hasher.update(chunk);
    }
    ctx.charge(1, 64)?;
    let hex = hasher.finalize().to_hex();
    let mut id = String::with_capacity(64);
    if id.capacity() != 64 {
        return Err(invalid("bounded record ID output capacity"));
    }
    id.push_str(hex.as_str());
    Ok(PaidCanonicalRecordIdV1 {
        value: output_fee.retain(id),
    })
}
pub(super) fn read_qualified_history_record_id_bounded_v1(
    store: &Store,
    history_id: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidCanonicalRecordIdV1, FormalError> {
    let history = super::bounded_history_read::read_history_object_bounded_v1(
        store, history_id, memory, work, stop,
    )?;
    let raw = store
        .raw_object_bounded(&history.node().entry, LIMIT, memory, work, stop)?
        .ok_or_else(|| invalid("referenced formal archive object missing"))?;
    let state = super::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
    work.consume_steps(1, stop)?;
    let FormalRequestStateV1::Qualified { proof, .. } = state.state() else {
        return Err(invalid("formal record ID requires Qualified history entry"));
    };
    hash_canonical_record_bounded_v1(&proof.decision.record, memory, work, stop)
}
#[cfg(test)]
#[path = "endpoint_store_bounded_record_id_tests.rs"]
mod tests;
