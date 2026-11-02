//! Same-transaction controls with exact expected values and failure isolation.
use super::*;
use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, required_member,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::MemoryClass;
struct MatchText<'a> {
    expected: &'a [u8],
    offset: usize,
    equal: bool,
}
impl Write for MatchText<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or(std::io::ErrorKind::InvalidData)?;
        self.equal &= self.expected.get(self.offset..end) == Some(bytes);
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn read_runtime_control_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: RuntimeControlFieldV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_text_bytes: u64,
    output: &mut impl Write,
) -> Result<RuntimeControlReadV1, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        if records.values.class != MemoryClass::Ordinary || !tx.uses_budget(records.values.memory) {
            return Err(invalid().into());
        }
        let (key, id, length) = required_member(field.descriptor(), tx, &mut records, error)?;
        let value = read_value(
            field,
            &id,
            &(key, length),
            &mut records,
            max_text_bytes,
            output,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(value.try_into()?)
    })
}
pub struct RuntimeControlChangeV1<'a> {
    pub field: RuntimeControlFieldV1,
    pub expected: RuntimeControlValueRefV1<'a>,
    pub after: RuntimeControlValueRefV1<'a>,
    pub max_text_bytes: u64,
}
pub fn change_runtime_control_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeControlChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let RuntimeControlChangeV1 {
        field,
        expected,
        after,
        max_text_bytes,
    } = change;
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        field.validate(expected)?;
        field.validate(after)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !tx.uses_budget(write.memory)
        {
            return Err(invalid().into());
        }
        let _scratch = write
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let expected_text = if let RuntimeControlValueRefV1::Text(text) = expected {
            text.as_bytes()
        } else {
            &[]
        };
        let mut comparison = MatchText {
            expected: expected_text,
            offset: 0,
            equal: true,
        };
        let (key, current) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            let (key, id, length) = required_member(field.descriptor(), tx, &mut records, error)?;
            let value = read_value(
                field,
                &id,
                &(key.clone(), length),
                &mut records,
                max_text_bytes,
                &mut comparison,
            )?;
            (key, value)
        };
        let matches = match (expected, current) {
            (RuntimeControlValueRefV1::Absent, RecordValueV1::Unit) => true,
            (RuntimeControlValueRefV1::Unsigned(a), RecordValueV1::Unsigned(b)) => a == b,
            (RuntimeControlValueRefV1::Text(a), RecordValueV1::Text { utf8_bytes }) => {
                comparison.equal && comparison.offset == a.len() && utf8_bytes == a.len() as u64
            }
            _ => false,
        };
        if !matches {
            return Err(invalid().into());
        }
        let unchanged = expected == after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(false);
        }
        let output = write_member_record_v1(
            field.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(),
            after.payload(),
            &mut MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            },
        )?;
        if output.field_key != key {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(field.namespace(), key.as_bytes(), Some(output.value_root))?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
