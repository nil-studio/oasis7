//! Owner-neutral, bounded decoding of the closed field-member value profiles.
//! Callers choose an exact typed codec; no schema, qualification or authority is inferred here.
use crate::{
    incremental_field_member::*,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_ledger_value::{LEDGER_VALUE_HEADER_BYTES, LedgerValueInfoV1, header_v1},
    incremental_utf8_output::Utf8Output,
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueTreeError};
use std::io::{self, Write};
#[derive(Clone, Copy)]
pub(crate) struct RecordReadLabelsV1 {
    pub utf8: &'static str,
    pub ledger_utf8: &'static str,
    pub ledger_frame: &'static str,
    pub boolean: &'static str,
    pub fixed: &'static str,
    pub profile: &'static str,
    pub integer: &'static str,
}
#[derive(Clone, Copy)]
pub(crate) struct RecordReadProfileV1 {
    pub codec: MemberPayloadCodecV1,
    pub nullable: bool,
    pub integer_max: u64,
    pub labels: RecordReadLabelsV1,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordValueV1 {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text { utf8_bytes: u64 },
    Ledger(LedgerValueInfoV1),
}
struct LedgerOutput<'a, W> {
    header: [u8; LEDGER_VALUE_HEADER_BYTES],
    used: usize,
    text: Utf8Output<'a, W>,
}
impl<W: Write> Write for LedgerOutput<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let take = bytes.len().min(self.header.len() - self.used);
        self.header[self.used..self.used + take].copy_from_slice(&bytes[..take]);
        self.used += take;
        // Report consumed header bytes before delegating to a fallible sink.
        // Interrupted then retries only the ID slice, never an eaten header.
        if take != 0 {
            return Ok(take);
        }
        self.text.write(&bytes[take..]).map(|n| n + take)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.text.flush()
    }
}
pub(crate) fn read_record_value_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
    profile: RecordReadProfileV1,
) -> Result<RecordValueV1, ValueTreeError> {
    let (codec, nullable) = (profile.codec, profile.nullable);
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: records.semantic_bytes,
        payload_bytes: max_payload_bytes,
        record_bytes: records.record_bytes,
    };
    let mut fixed = [0u8; 8];
    let (info, decoded) = match codec {
        MemberPayloadCodecV1::Text => {
            let mut checked = Utf8Output::new(output, profile.labels.utf8);
            let result = read_member_record_v1(
                root,
                &key.0,
                records.values,
                limits,
                &mut io::sink(),
                &mut checked,
                &mut *records.cancelled,
            );
            if checked.is_invalid() {
                return Err(ValueTreeError::Invalid(profile.labels.utf8));
            }
            let info = result?;
            if !checked.is_complete() {
                return Err(ValueTreeError::Invalid(profile.labels.utf8));
            }
            (
                info,
                RecordValueV1::Text {
                    utf8_bytes: info.payload_bytes,
                },
            )
        }
        MemberPayloadCodecV1::Ledger => {
            let mut checked = LedgerOutput {
                header: [0; LEDGER_VALUE_HEADER_BYTES],
                used: 0,
                text: Utf8Output::new(output, profile.labels.ledger_utf8),
            };
            let result = read_member_record_v1(
                root,
                &key.0,
                records.values,
                limits,
                &mut io::sink(),
                &mut checked,
                &mut *records.cancelled,
            );
            if checked.text.is_invalid() {
                return Err(ValueTreeError::Invalid(profile.labels.ledger_utf8));
            }
            let info = result?;
            let decoded = if info.codec == MemberPayloadCodecV1::Unit {
                RecordValueV1::Unit
            } else {
                if !checked.text.is_complete() || checked.used != LEDGER_VALUE_HEADER_BYTES {
                    return Err(ValueTreeError::Invalid(profile.labels.ledger_frame));
                }
                RecordValueV1::Ledger(
                    header_v1(&checked.header, info.payload_bytes)
                        .map_err(ValueTreeError::Invalid)?,
                )
            };
            (info, decoded)
        }
        _ => {
            let mut cursor = io::Cursor::new(fixed.as_mut_slice());
            let info = read_member_record_v1(
                root,
                &key.0,
                records.values,
                MemberRecordLimitsV1 {
                    payload_bytes: max_payload_bytes.min(8),
                    ..limits
                },
                &mut io::sink(),
                &mut cursor,
                &mut *records.cancelled,
            )?;
            let decoded = match info.codec {
                MemberPayloadCodecV1::Unit => RecordValueV1::Unit,
                MemberPayloadCodecV1::U64 => RecordValueV1::Unsigned(u64::from_be_bytes(fixed)),
                MemberPayloadCodecV1::I64 => RecordValueV1::Signed(i64::from_be_bytes(fixed)),
                MemberPayloadCodecV1::Bool => match fixed[0] {
                    0 => RecordValueV1::Bool(false),
                    1 => RecordValueV1::Bool(true),
                    _ => return Err(ValueTreeError::Invalid(profile.labels.boolean)),
                },
                _ => return Err(ValueTreeError::Invalid(profile.labels.fixed)),
            };
            (info, decoded)
        }
    };
    if info.semantic_bytes != key.1
        || !(info.codec == codec || nullable && info.codec == MemberPayloadCodecV1::Unit)
    {
        return Err(ValueTreeError::Invalid(profile.labels.profile));
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    if info.codec == MemberPayloadCodecV1::Unit {
        return Ok(RecordValueV1::Unit);
    }
    if matches!(decoded, RecordValueV1::Unsigned(v) if v > profile.integer_max) {
        return Err(ValueTreeError::Invalid(profile.labels.integer));
    }
    Ok(decoded)
}
