//! Actual changed governance-cell output. The original relative-key parser,
//! primitive decoder and after-row lookups remain the sole wire kernels.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader, incremental_record_tree,
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
};
use oasis7_formal_protocol::{
    incremental_state::{StateEntryV1, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use std::{
    io::{self, Write},
    mem::size_of,
};
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("governance projection cell")
}
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("governance projection cell")
}
fn pay(p: &mut BusinessDiffProcessingWorkV1, n: usize) -> Result<(), ValueTreeError> {
    let n = u64::try_from(n).map_err(|_| limited())?;
    p.steps_remaining = p
        .steps_remaining
        .checked_sub(n.checked_add(1).ok_or_else(limited)?)
        .ok_or_else(limited)?;
    p.bytes_remaining = p.bytes_remaining.checked_sub(n).ok_or_else(limited)?;
    Ok(())
}
fn reserve<R: ValueNodeReader, C: FnMut() -> bool>(
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    n: usize,
) -> Result<MemoryReservation, ValueTreeError> {
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    pay(p, n)?;
    Ok(records.values.memory.reserve(n, records.values.class)?)
}
fn identity(value: u64) -> Result<GovernanceIdentityStatus, ValueTreeError> {
    match value {
        0 => Ok(GovernanceIdentityStatus::Active),
        1 => Ok(GovernanceIdentityStatus::Frozen),
        2 => Ok(GovernanceIdentityStatus::Expelled),
        _ => Err(bad()),
    }
}
fn proposal(value: u64) -> Result<GovernanceProposalStatus, ValueTreeError> {
    match value {
        0 => Ok(GovernanceProposalStatus::Open),
        1 => Ok(GovernanceProposalStatus::Passed),
        2 => Ok(GovernanceProposalStatus::Rejected),
        _ => Err(bad()),
    }
}
fn at<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<((WorldFieldKeyV1, u64), [u8; 32]), ValueTreeError> {
    let key = read::cell_key(kind, cell, records)?;
    let value =
        incremental_record_tree::lookup(root, &key.0, records, genesis::LABELS)?.ok_or_else(bad)?;
    Ok((key, value))
}
fn uint<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
) -> Result<u64, ValueTreeError> {
    pay(p, 0)?;
    let (key, value) = at(kind, cell, root, records)?;
    match read::value(cell, &value, &key, records, 8, &mut io::sink())? {
        GovernanceValueV1::Unsigned(v) => Ok(v),
        _ => Err(bad()),
    }
}
fn text<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
    held: &mut MemoryReservation,
) -> Result<String, ValueTreeError> {
    let (key, value) = at(kind, cell, root, records)?;
    let GovernanceValueV1::Text { utf8_bytes } =
        read::value(cell, &value, &key, records, max, &mut io::sink())?
    else {
        return Err(bad());
    };
    let n = usize::try_from(utf8_bytes).map_err(|_| limited())?;
    let mut fee = reserve(
        records,
        p,
        n.checked_add(size_of::<String>()).ok_or_else(limited)?,
    )?;
    struct Out {
        bytes: Vec<u8>,
        bound: usize,
    }
    impl Write for Out {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if self
                .bytes
                .len()
                .checked_add(b.len())
                .filter(|n| *n <= self.bound)
                .is_none()
            {
                return Err(io::Error::other("governance projection string bound"));
            }
            self.bytes.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Out {
        bytes: Vec::with_capacity(n),
        bound: n,
    };
    if out.bytes.capacity() != n {
        return Err(limited());
    }
    let actual = read::value(cell, &value, &key, records, max, &mut out)?;
    if actual != (GovernanceValueV1::Text { utf8_bytes }) || out.bytes.len() != n {
        return Err(bad());
    }
    pay(p, n)?;
    let value = String::from_utf8(out.bytes);
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    let value = value.map_err(|_| bad())?;
    held.merge(&mut fee).map_err(ValueTreeError::Invalid)?;
    Ok(value)
}
/// Scalar JSON is emitted with its actual original Rust enum representation;
/// storage integer tags are never serialized directly as governance statuses.
pub(super) fn scalar_json<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    cell: GovernanceCellV1<'_>,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let key = read::cell_key(kind, cell, records)?;
    if key.0.as_bytes() != entry.key {
        return Err(bad());
    }
    pay(p, 0)?;
    let value = read::value(cell, &entry.value, &key, records, max, &mut io::sink())?;
    let memory = records.values.memory.clone();
    macro_rules! json {
        ($v:expr) => {
            crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
                &$v,
                &memory,
                p,
                &mut *records.cancelled,
            )
        };
    }
    match value {
        GovernanceValueV1::Unit => json!(()),
        GovernanceValueV1::Unsigned(value) => match cell {
            GovernanceCellV1::Column(GovernanceColumnV1::IdentityStatus)
            | GovernanceCellV1::Snapshot(_, GovernanceSnapshotColumnV1::Status) => {
                json!(identity(value)?)
            }
            GovernanceCellV1::Column(GovernanceColumnV1::ProposalStatus) => json!(proposal(value)?),
            _ => json!(value),
        },
        GovernanceValueV1::Signed(value) => json!(value),
        GovernanceValueV1::Text { utf8_bytes } => {
            let n = usize::try_from(utf8_bytes).map_err(|_| limited())?;
            let _fee = reserve(
                records,
                p,
                n.checked_add(size_of::<String>()).ok_or_else(limited)?,
            )?;
            struct Out {
                bytes: Vec<u8>,
                bound: usize,
            }
            impl Write for Out {
                fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                    if self
                        .bytes
                        .len()
                        .checked_add(b.len())
                        .filter(|n| *n <= self.bound)
                        .is_none()
                    {
                        return Err(io::Error::other("governance scalar bound"));
                    }
                    self.bytes.extend_from_slice(b);
                    Ok(b.len())
                }
                fn flush(&mut self) -> io::Result<()> {
                    Ok(())
                }
            }
            let mut out = Out {
                bytes: Vec::with_capacity(n),
                bound: n,
            };
            if out.bytes.capacity() != n {
                return Err(limited());
            }
            let after = read::value(cell, &entry.value, &key, records, max, &mut out)?;
            if after != value || out.bytes.len() != n {
                return Err(bad());
            }
            pay(p, n)?;
            let string = String::from_utf8(out.bytes);
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled);
            }
            let string = string.map_err(|_| bad())?;
            json!(string)
        }
        _ => Err(bad()),
    }
}
/// A changed snapshot/ballot member is a fixed five/three-cell row. This reads
/// only that exact member, preserving embedded IDs distinct from map keys.
pub(super) fn member_json<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: GovernanceRecordKindV1,
    snapshot: bool,
    id: &str,
    root: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &mut BusinessDiffProcessingWorkV1,
    max: u64,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    let mut held = reserve(
        records,
        p,
        size_of::<GovernanceVoteWeightSnapshotState>().max(size_of::<GovernanceVoteBallotState>()),
    )?;
    let memory = records.values.memory.clone();
    if snapshot {
        use GovernanceSnapshotColumnV1 as F;
        let cell = |field| GovernanceCellV1::Snapshot(id, field);
        let agent_id = text(kind, cell(F::AgentId), root, records, p, max, &mut held)?;
        let (key, value) = at(kind, cell(F::ReputationScore), root, records)?;
        pay(p, 0)?;
        let reputation_score = match read::value(
            cell(F::ReputationScore),
            &value,
            &key,
            records,
            8,
            &mut io::sink(),
        )? {
            GovernanceValueV1::Signed(v) => v,
            _ => return Err(bad()),
        };
        let stake_locked = uint(kind, cell(F::StakeLocked), root, records, p)?;
        let status = identity(uint(kind, cell(F::Status), root, records, p)?)?;
        let vote_weight_cap = u32::try_from(uint(kind, cell(F::VoteWeightCap), root, records, p)?)
            .map_err(|_| bad())?;
        let value = GovernanceVoteWeightSnapshotState {
            agent_id,
            reputation_score,
            stake_locked,
            status,
            vote_weight_cap,
        };
        crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
            &value,
            &memory,
            p,
            &mut *records.cancelled,
        )
    } else {
        use GovernanceBallotColumnV1 as F;
        let cell = |field| GovernanceCellV1::Ballot(id, field);
        let option = text(kind, cell(F::Option), root, records, p, max, &mut held)?;
        let weight =
            u32::try_from(uint(kind, cell(F::Weight), root, records, p)?).map_err(|_| bad())?;
        let voted_at = uint(kind, cell(F::VotedAt), root, records, p)?;
        let value = GovernanceVoteBallotState {
            option,
            weight,
            voted_at,
        };
        crate::incremental_world_rule_projection_delta::encode_original_projection_json_data_v1(
            &value,
            &memory,
            p,
            &mut *records.cancelled,
        )
    }
}
