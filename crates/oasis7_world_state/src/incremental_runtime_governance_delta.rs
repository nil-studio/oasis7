//! Closed, changed-column receiver; equal qualified subtrees are never hydrated.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_semantic_key::{CanonicalSemanticKeyRefV1, SemanticKeyRefV1, read_semantic_key_v1},
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("Runtime governance changed columns")
}
pub(super) fn key<R: ValueNodeReader, C: FnMut() -> bool>(
    column: RuntimeGovernanceColumnV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<(WorldFieldKeyV1, u64), ValueTreeError> {
    with_gov_key!(column,key=>records.key(column.kind().columns(),WorldFieldRecordKindV1::Entry,key))
}
pub(super) fn value<R: ValueNodeReader, C: FnMut() -> bool>(
    column: RuntimeGovernanceColumnV1<'_>,
    id: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    records: &mut InventoryValueReader<'_, '_, R, C>,
    output: &mut impl std::io::Write,
) -> Result<RecordValueV1, ValueTreeError> {
    let max = records.record_bytes;
    read_record_value_v1(
        id,
        key,
        records,
        max,
        output,
        RecordReadProfileV1 {
            codec: column.codec(),
            nullable: false,
            integer_max: column.integer_max(),
            labels: RecordReadLabelsV1 {
                utf8: "governance column UTF8",
                ledger_utf8: "governance unsupported ledger",
                ledger_frame: "governance unsupported ledger",
                boolean: "governance column bool",
                fixed: "governance column fixed",
                profile: "governance column profile",
                integer: "governance column integer",
            },
        },
    )
}
pub(super) fn read<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    column: RuntimeGovernanceColumnV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    output: &mut impl std::io::Write,
) -> Result<Option<RecordValueV1>, ValueTreeError> {
    let key = key(column, records)?;
    let Some(id) = incremental_record_tree::lookup(root, &key.0, records, LABELS)? else {
        return Ok(None);
    };
    value(column, &id, &key, records, output).map(Some)
}
fn unsigned<R: ValueNodeReader, C: FnMut() -> bool>(
    root: &StateRootV1,
    column: RuntimeGovernanceColumnV1<'_>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<u64, ValueTreeError> {
    match read(root, column, records, &mut std::io::sink())? {
        Some(RecordValueV1::Unsigned(v)) => Ok(v),
        _ => Err(invalid()),
    }
}
fn option_index(option: GovernanceOptionV1) -> usize {
    GovernanceOptionV1::ALL
        .iter()
        .position(|c| *c == option)
        .unwrap()
}
fn parse<'a>(
    kind: RuntimeGovernanceKindV1,
    semantic: SemanticKeyRefV1<'a>,
) -> Result<RuntimeGovernanceColumnV1<'a>, ValueTreeError> {
    use RuntimeGovernanceColumnV1 as C;
    match semantic {
        SemanticKeyRefV1::Text(name) => C::scalar(kind, name).map_err(|_| invalid()),
        SemanticKeyRefV1::Pair { left, right } => {
            let SemanticKeyRefV1::Text(name) = left.decode().map_err(ValueTreeError::Invalid)?
            else {
                return Err(invalid());
            };
            let right = right.decode().map_err(ValueTreeError::Invalid)?;
            if kind == RuntimeGovernanceKindV1::EpochSnapshot {
                match (name, right) {
                    ("signer_node_ids", SemanticKeyRefV1::Text("count")) => Ok(C::SignerCount),
                    ("signer_node_ids", SemanticKeyRefV1::Unsigned(i)) => Ok(C::Signer(i)),
                    ("validator_stakes", SemanticKeyRefV1::Text("count")) => Ok(C::StakeCount),
                    ("validator_stakes", SemanticKeyRefV1::Pair { left, right }) => {
                        if left.decode().map_err(ValueTreeError::Invalid)?
                            != SemanticKeyRefV1::Text("member")
                        {
                            return Err(invalid());
                        }
                        let SemanticKeyRefV1::Text(id) =
                            right.decode().map_err(ValueTreeError::Invalid)?
                        else {
                            return Err(invalid());
                        };
                        Ok(C::Stake(id))
                    }
                    _ => Err(invalid()),
                }
            } else if kind == RuntimeGovernanceKindV1::IdentityPenalty {
                let option = GovernanceOptionV1::ALL
                    .into_iter()
                    .find(|c| c.name() == name)
                    .ok_or_else(invalid)?;
                match right {
                    SemanticKeyRefV1::Text("present") => Ok(C::OptionPresence(option)),
                    SemanticKeyRefV1::Text("value") => Ok(C::OptionValue(option)),
                    _ => Err(invalid()),
                }
            } else {
                Err(invalid())
            }
        }
        _ => Err(invalid()),
    }
}
fn with_column<R: ValueNodeReader, C: FnMut() -> bool, T>(
    kind: RuntimeGovernanceKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    mut visit: impl FnMut(
        RuntimeGovernanceColumnV1<'_>,
        &(WorldFieldKeyV1, u64),
        &mut InventoryValueReader<'_, '_, R, C>,
    ) -> Result<T, ValueTreeError>,
) -> Result<T, ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    if parsed.kind != WorldFieldRecordKindV1::Entry
        || entry.namespace != StateNamespaceV1::Authorization
    {
        return Err(invalid());
    }
    let field_key = WorldFieldKeyV1::new(kind.columns(), parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if field_key.as_bytes() != entry.key {
        return Err(invalid());
    }
    let info = read_member_record_header_v1(
        &entry.value,
        &field_key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: records.record_bytes,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    let length = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let _charge = records.values.memory.reserve(
        length.checked_add(8192).ok_or_else(invalid)?,
        records.values.class,
    )?;
    let mut semantic = vec![0; length];
    let root = records.values.load_root(&entry.value)?;
    let start = FIELD_MEMBER_HEADER_BYTES
        .checked_add(entry.key.len())
        .ok_or_else(invalid)?;
    records.values.read_range(
        &root,
        start as u64,
        info.semantic_bytes,
        &mut std::io::Cursor::new(semantic.as_mut_slice()),
        &mut *records.cancelled,
    )?;
    let original = CanonicalSemanticKeyRefV1::new(&semantic).map_err(ValueTreeError::Invalid)?;
    let expected = records.key(kind.columns(), WorldFieldRecordKindV1::Entry, &original)?;
    if expected.0 != field_key {
        return Err(invalid());
    }
    let column = parse(
        kind,
        read_semantic_key_v1(&semantic).map_err(ValueTreeError::Invalid)?,
    )?;
    visit(column, &expected, records)
}
/// Inherit complete parent shape while checking changed leaves and cross-column
/// cardinality/presence. No unchanged text, signer or stake payload is decoded.
pub(super) fn changed<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeGovernanceKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut RuntimeGovernanceWorkV1,
) -> Result<(), ValueTreeError> {
    if before.namespace != StateNamespaceV1::Authorization || after.namespace != before.namespace {
        return Err(invalid());
    }
    let mut signer = [0u64; 2];
    let mut stake = [0u64; 2];
    if kind == RuntimeGovernanceKindV1::EpochSnapshot {
        signer = [
            unsigned(before, RuntimeGovernanceColumnV1::SignerCount, records)?,
            unsigned(after, RuntimeGovernanceColumnV1::SignerCount, records)?,
        ];
        stake = [
            unsigned(before, RuntimeGovernanceColumnV1::StakeCount, records)?,
            unsigned(after, RuntimeGovernanceColumnV1::StakeCount, records)?,
        ];
    }
    let mut signer_delta = 0i128;
    let mut stake_delta = 0i128;
    let mut options = [false; 7];
    let meta = WorldFieldKeyV1::new(kind.columns(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    incremental_record_diff::visit(
        before,
        after,
        records,
        &mut work.cells_remaining,
        RecordDiffLabelsV1 {
            invalid: "Runtime governance changed path",
            cells: "Runtime governance changed paths",
            stack: "Runtime governance changed stack",
            tree: LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            if entry.key == meta.as_bytes() {
                if old.is_none() || new.is_none() || old.map(|e| e.value) != new.map(|e| e.value) {
                    return Err(invalid());
                }
                return Ok(());
            }
            with_column(kind, entry, records, |column, key, records| {
                if !column.nullable() && (old.is_none() || new.is_none()) {
                    return Err(invalid());
                }
                if let Some(new) = new {
                    records.values.verify_closure(
                        &new.value,
                        records.record_bytes,
                        &mut *records.cancelled,
                    )?;
                    value(column, &new.value, key, records, &mut std::io::sink())?;
                }
                use RuntimeGovernanceColumnV1 as K;
                match column {
                    K::Signer(ordinal) => {
                        if old.is_some() && ordinal >= signer[0]
                            || new.is_some() && ordinal >= signer[1]
                        {
                            return Err(invalid());
                        }
                        if old.is_none() {
                            if ordinal < signer[0] {
                                return Err(invalid());
                            }
                            signer_delta += 1;
                        }
                        if new.is_none() {
                            if ordinal < signer[1] {
                                return Err(invalid());
                            }
                            signer_delta -= 1;
                        }
                    }
                    K::Stake(_) => {
                        stake_delta += i128::from(new.is_some()) - i128::from(old.is_some());
                    }
                    K::OptionPresence(option) | K::OptionValue(option) => {
                        options[option_index(option)] = true;
                    }
                    _ => {}
                }
                Ok(())
            })
        },
    )?;
    if kind == RuntimeGovernanceKindV1::EpochSnapshot
        && (i128::from(signer[0]) + signer_delta != i128::from(signer[1])
            || i128::from(stake[0]) + stake_delta != i128::from(stake[1]))
    {
        return Err(invalid());
    }
    for (option, touched) in GovernanceOptionV1::ALL.into_iter().zip(options) {
        if !touched {
            continue;
        }
        let present = match read(
            after,
            RuntimeGovernanceColumnV1::OptionPresence(option),
            records,
            &mut std::io::sink(),
        )? {
            Some(RecordValueV1::Bool(value)) => value,
            _ => return Err(invalid()),
        };
        let value = read(
            after,
            RuntimeGovernanceColumnV1::OptionValue(option),
            records,
            &mut std::io::sink(),
        )?;
        if value.is_some() != present {
            return Err(invalid());
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
