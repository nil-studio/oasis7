//! Bounded point validation of the closed status/timing header.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree as tree,
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
};
pub(super) const STATUS_COLUMNS: [&str; 4] = [
    "status.kind",
    "status.manifest_hash",
    "status.approver",
    "status.reason",
];
pub(super) fn view(value: &crate::ProposalStatus) -> (u64, [Option<&str>; 3]) {
    use crate::ProposalStatus as S;
    match value {
        S::Proposed => (0, [None, None, None]),
        S::Shadowed { manifest_hash } => (1, [Some(manifest_hash), None, None]),
        S::Approved {
            manifest_hash,
            approver,
        } => (2, [Some(manifest_hash), Some(approver), None]),
        S::Rejected { reason } => (3, [None, None, Some(reason)]),
        S::Applied { manifest_hash } => (4, [Some(manifest_hash), None, None]),
    }
}
fn profile(codec: MemberPayloadCodecV1) -> RecordReadProfileV1 {
    RecordReadProfileV1 {
        codec,
        nullable: false,
        integer_max: u64::MAX,
        labels: RecordReadLabelsV1 {
            utf8: "proposal header UTF8",
            ledger_utf8: "proposal header ledger UTF8",
            ledger_frame: "proposal header ledger frame",
            boolean: "proposal header bool",
            fixed: "proposal header fixed",
            profile: "proposal header codec",
            integer: "proposal header integer",
        },
    }
}
pub(super) fn point<R: ValueNodeReader, C: FnMut() -> bool, K: SemanticKeyV1 + ?Sized>(
    root: &StateRootV1,
    key: &K,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<(WorldFieldKeyV1, u64, Option<[u8; 32]>), ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("proposal header points"))?;
    let (key, bytes) = records.key(columns(), WorldFieldRecordKindV1::Entry, key)?;
    let value = tree::lookup(root, &key, records, genesis::LABELS)?;
    Ok((key, bytes, value))
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    codec: MemberPayloadCodecV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &RuntimeProposalReadWorkV1,
    output: &mut impl std::io::Write,
) -> Result<RecordValueV1, ValueTreeError> {
    read_record_value_v1(
        id,
        key,
        records,
        if codec == MemberPayloadCodecV1::Text {
            work.max_text_bytes
        } else {
            8
        },
        output,
        profile(codec),
    )
}
struct Compare<'a> {
    expected: Option<&'a [u8]>,
    offset: usize,
    equal: bool,
}
impl std::io::Write for Compare<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or(std::io::ErrorKind::InvalidData)?;
        if let Some(expected) = self.expected {
            self.equal &= expected.get(self.offset..end) == Some(bytes);
        }
        self.offset = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) struct StatusFacts {
    pub cells: [(WorldFieldKeyV1, u64, Option<[u8; 32]>); 4],
}
pub(super) fn validate_status<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    expected: Option<&crate::ProposalStatus>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<ReservedValue<StatusFacts>, ValueTreeError> {
    let charge = records
        .values
        .memory
        .reserve(16 << 10, records.values.class)?;
    let mut cells = Vec::with_capacity(4);
    for name in STATUS_COLUMNS {
        cells.push(point(root, &name, records, work)?);
    }
    let tag = match value(
        &cells[0].2.ok_or_else(invalid)?,
        &(cells[0].0.clone(), cells[0].1),
        MemberPayloadCodecV1::U64,
        records,
        work,
        &mut std::io::sink(),
    )? {
        RecordValueV1::Unsigned(v) if v <= 4 => v,
        _ => return Err(invalid()),
    };
    let required = match tag {
        0 => [false, false, false],
        1 | 4 => [true, false, false],
        2 => [true, true, false],
        3 => [false, false, true],
        _ => return Err(invalid()),
    };
    let expected = expected.map(view);
    if expected.as_ref().is_some_and(|(kind, _)| *kind != tag) {
        return Err(invalid());
    }
    for index in 0..3 {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        match (required[index], cells[index + 1].2) {
            (false, None) => (),
            (true, Some(id)) => {
                let text = expected.as_ref().and_then(|(_, texts)| texts[index]);
                let mut compare = Compare {
                    expected: text.map(str::as_bytes),
                    offset: 0,
                    equal: true,
                };
                let observed = value(
                    &id,
                    &(cells[index + 1].0.clone(), cells[index + 1].1),
                    MemberPayloadCodecV1::Text,
                    records,
                    work,
                    &mut compare,
                )?;
                if !matches!(observed, RecordValueV1::Text { .. })
                    || !compare.equal
                    || text.is_some_and(|v| v.len() != compare.offset)
                {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(StatusFacts {
        cells: cells.try_into().map_err(|_| invalid())?,
    }))
}
pub(super) fn validate_optional<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    name: &str,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<(), ValueTreeError> {
    let present = point(root, &(name, "present"), records, work)?;
    let is_some = match value(
        &present.2.ok_or_else(invalid)?,
        &(present.0, present.1),
        MemberPayloadCodecV1::Bool,
        records,
        work,
        &mut std::io::sink(),
    )? {
        RecordValueV1::Bool(v) => v,
        _ => return Err(invalid()),
    };
    let data = point(root, &(name, "value"), records, work)?;
    match (is_some, data.2) {
        (false, None) => (),
        (true, Some(id)) => {
            if !matches!(
                value(
                    &id,
                    &(data.0, data.1),
                    MemberPayloadCodecV1::U64,
                    records,
                    work,
                    &mut std::io::sink()
                )?,
                RecordValueV1::Unsigned(_)
            ) {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
