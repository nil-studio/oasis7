//! Decode only this owner's closed relative tuples; original Text members borrow.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::{incremental_state::StateEntryV1, incremental_value::ValueNodeReader};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("governance relative key")
}
fn pair(
    value: SemanticKeyRefV1<'_>,
) -> Result<(SemanticKeyRefV1<'_>, SemanticKeyRefV1<'_>), ValueTreeError> {
    if let SemanticKeyRefV1::Pair { left, right } = value {
        Ok((
            left.decode().map_err(ValueTreeError::Invalid)?,
            right.decode().map_err(ValueTreeError::Invalid)?,
        ))
    } else {
        Err(invalid())
    }
}
fn record(value: SemanticKeyRefV1<'_>) -> bool {
    matches!(
        pair(value),
        Ok((SemanticKeyRefV1::Unit, SemanticKeyRefV1::Text("record")))
    )
}
fn collection(value: SemanticKeyRefV1<'_>) -> Result<GovernanceCollectionV1, ValueTreeError> {
    let (base, SemanticKeyRefV1::Text(name)) = pair(value)? else {
        return Err(invalid());
    };
    if !record(base) {
        return Err(invalid());
    }
    GovernanceCollectionV1::ALL
        .into_iter()
        .find(|c| c.name() == name)
        .ok_or_else(invalid)
}
pub(super) fn parse(
    bytes: &[u8],
    kind: GovernanceRecordKindV1,
) -> Result<GovernanceCellV1<'_>, ValueTreeError> {
    let root = read_semantic_key_v1(bytes).map_err(ValueTreeError::Invalid)?;
    if root == SemanticKeyRefV1::Unit {
        return Ok(GovernanceCellV1::Record);
    }
    let (head, tail) = pair(root)?;
    let cell = if record(head) {
        let SemanticKeyRefV1::Text(name) = tail else {
            return Err(invalid());
        };
        GovernanceCellV1::Column(
            GovernanceColumnV1::ALL
                .into_iter()
                .find(|c| c.accepts(kind) && c.name() == name)
                .ok_or_else(invalid)?,
        )
    } else if let Ok(c) = collection(head) {
        match (c, tail) {
            (_, SemanticKeyRefV1::Unit) => GovernanceCellV1::Count(c),
            (GovernanceCollectionV1::Options, SemanticKeyRefV1::Unsigned(index)) => {
                GovernanceCellV1::Option(index)
            }
            (GovernanceCollectionV1::Tallies, SemanticKeyRefV1::Text(id)) => {
                GovernanceCellV1::Tally(id)
            }
            _ => return Err(invalid()),
        }
    } else {
        let (base, SemanticKeyRefV1::Text(id)) = pair(head)? else {
            return Err(invalid());
        };
        let SemanticKeyRefV1::Text(name) = tail else {
            return Err(invalid());
        };
        match collection(base)? {
            GovernanceCollectionV1::Snapshot => GovernanceCellV1::Snapshot(
                id,
                GovernanceSnapshotColumnV1::ALL
                    .into_iter()
                    .find(|c| c.name() == name)
                    .ok_or_else(invalid)?,
            ),
            GovernanceCollectionV1::Ballots => GovernanceCellV1::Ballot(
                id,
                GovernanceBallotColumnV1::ALL
                    .into_iter()
                    .find(|c| c.name() == name)
                    .ok_or_else(invalid)?,
            ),
            _ => return Err(invalid()),
        }
    };
    if !cell.accepts(kind) {
        return Err(invalid());
    }
    Ok(cell)
}
struct KeyBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for KeyBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("governance key bound"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Key/framing only: new values need separate closure and profile validation.
/// Only a qualified parent permits skipping an inherited old payload. Semantic bytes
/// are precharged and capped; no record payload or collection is aggregated.
pub(super) fn with_cell<R: ValueNodeReader, C: FnMut() -> bool, T>(
    kind: GovernanceRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
    inherited_old_payload: bool,
    visit: impl FnOnce(
        GovernanceCellV1<'_>,
        &(WorldFieldKeyV1, u64),
        &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<T, ValueTreeError>,
) -> Result<T, ValueTreeError> {
    let limit = usize::try_from(records.semantic_bytes).map_err(|_| invalid())?;
    let allowance = limit.checked_add(8192).ok_or_else(invalid)?;
    let _charge = records
        .values
        .memory
        .reserve(allowance, records.values.class)?;
    let mut semantic = KeyBytes {
        bytes: Vec::with_capacity(limit),
        limit,
    };
    if semantic.bytes.capacity() > allowance {
        return Err(invalid());
    }
    let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        key.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key || entry.namespace != kind.namespace() {
        return Err(invalid());
    }
    let info = read_member_record_key_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: if inherited_old_payload { u64::MAX } else { max },
            record_bytes: if inherited_old_payload {
                u64::MAX
            } else {
                records.record_bytes
            },
        },
        &mut semantic,
        &mut *records.cancelled,
    )?;
    let cell = parse(&semantic.bytes, kind)?;
    if info.codec != cell.codec() && !(cell.nullable() && info.codec == MemberPayloadCodecV1::Unit)
    {
        return Err(invalid());
    }
    let expected = read::cell_key(kind, cell, records)?;
    if expected.0 != key || expected.1 != info.semantic_bytes {
        return Err(invalid());
    }
    visit(cell, &expected, records)
}
