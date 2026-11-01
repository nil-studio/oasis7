use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RewardRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != kind.namespace() || after.namespace != kind.namespace() {
        return Err(read::invalid());
    }
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "reward changed closure",
            cells: "reward changed cells",
            stack: "reward changed stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(read::invalid)?;
            let meta =
                WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            if entry.key == meta.as_bytes() {
                if old.is_none() || new.is_none() {
                    return Err(read::invalid());
                }
                records.presence(&entry.value, &meta, 0)?;
                return Ok(());
            }
            match closure::cell(kind, entry, records, max)? {
                key_read::Parsed::Member(c) => {
                    let marker = read::cell_key(kind, RewardCellV1::Collection(c), records)?;
                    let root = crate::incremental_record_tree::lookup(
                        after,
                        &marker.0,
                        records,
                        genesis::LABELS,
                    )?
                    .ok_or_else(read::invalid)?;
                    read::primitive(
                        MemberPayloadCodecV1::Unit,
                        &root,
                        &marker,
                        records,
                        0,
                        &mut std::io::sink(),
                    )?;
                }
                _ => {
                    if old.is_none() || new.is_none() {
                        return Err(read::invalid());
                    }
                }
            }
            Ok(())
        },
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
#[derive(Default)]
struct OrderChanges {
    added: u64,
    removed: u64,
    old_max: Option<u64>,
    new_max: Option<u64>,
    added_min: Option<u64>,
    removed_min: Option<u64>,
}
impl OrderChanges {
    fn observe(&mut self, index: u64, old: bool, new: bool) -> Result<(), ValueTreeError> {
        if old {
            self.old_max = Some(self.old_max.map_or(index, |n| n.max(index)));
        }
        if new {
            self.new_max = Some(self.new_max.map_or(index, |n| n.max(index)));
        }
        if !old {
            self.added = self.added.checked_add(1).ok_or_else(read::invalid)?;
            self.added_min = Some(self.added_min.map_or(index, |n| n.min(index)));
        }
        if !new {
            self.removed = self.removed.checked_add(1).ok_or_else(read::invalid)?;
            self.removed_min = Some(self.removed_min.map_or(index, |n| n.min(index)));
        }
        Ok(())
    }
    fn check(&self, old: u64, new: u64) -> Result<(), ValueTreeError> {
        if self.old_max.is_some_and(|i| i >= old)
            || self.new_max.is_some_and(|i| i >= new)
            || self.added_min.is_some_and(|i| i < old)
            || self.removed_min.is_some_and(|i| i < new)
            || old
                .checked_add(self.added)
                .and_then(|n| n.checked_sub(self.removed))
                != Some(new)
        {
            Err(read::invalid())
        } else {
            Ok(())
        }
    }
}
/// Generic replay plus owner-specific changed paths; unchanged records are inherited.
pub fn accept_reward_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RewardFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<RewardFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(read::invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| read::invalid())?)?;
    let verified = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = error {
        return Err(error.into());
    }
    let verified = verified?;
    if !verified
        .record
        .after
        .state
        .iter()
        .any(|root| root == ctx.root)
    {
        return Err(read::invalid().into());
    }
    let mut counts = parent.record_counts();
    let mut length = parent.mint_length();
    let mut changes = OrderChanges::default();
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "reward outer changed cells",
        ))?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if !RewardRecordKindV1::ALL
            .into_iter()
            .any(|kind| kind.name() == binding.name)
        {
            continue;
        }
        let edge = |value: Option<[u8; 32]>, ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>| {
            value
                .map(|value| {
                    family::outer(
                        &StateEntryV1 {
                            namespace: write.namespace,
                            key: write.mutation.key.clone(),
                            value,
                        },
                        ctx,
                    )
                })
                .transpose()
                .map(Option::flatten)
        };
        let old = edge(write.mutation.before, ctx)?;
        let new = edge(write.mutation.after, ctx)?;
        match (old, new) {
            (Some(family::Outer::Metadata(a)), Some(family::Outer::Metadata(b)))
            | (Some(family::Outer::Format(a)), Some(family::Outer::Format(b)))
                if a == b => {}
            (Some(family::Outer::Length(old)), Some(family::Outer::Length(new))) => {
                if old != parent.mint_length() {
                    return Err(read::invalid().into());
                }
                length = new;
            }
            (
                Some(family::Outer::Record(old_key, old)),
                Some(family::Outer::Record(new_key, new)),
            ) => {
                if old_key != new_key {
                    return Err(read::invalid().into());
                }
                child(new_key.kind, &old, &new, &mut ctx.records(), cells, max)?;
                if new_key.kind == RewardRecordKindV1::Mint {
                    changes.observe(new_key.index, true, true)?;
                }
            }
            (None, Some(family::Outer::Record(key, root))) => {
                let mut body = WorldFieldReadContextV1 {
                    root: &root,
                    state: ctx.state,
                    values: ValueReadContext {
                        source: ctx.values.source,
                        memory: ctx.values.memory,
                        class: ctx.values.class,
                        io: &mut *ctx.values.io,
                    },
                    key_work: &mut *ctx.key_work,
                    semantic_bytes: ctx.semantic_bytes,
                    record_bytes: ctx.record_bytes,
                    take_state_error: &mut ctx.take_state_error,
                    cancelled: &mut ctx.cancelled,
                };
                qualify_reward_body_v1(key, &mut body, inner_scan, cells, max)?;
                let i = family::index(key.kind);
                counts[i] = counts[i].checked_add(1).ok_or_else(read::invalid)?;
                if key.kind == RewardRecordKindV1::Mint {
                    changes.observe(key.index, false, true)?;
                }
            }
            (Some(family::Outer::Record(key, _)), None) => {
                let i = family::index(key.kind);
                counts[i] = counts[i].checked_sub(1).ok_or_else(read::invalid)?;
                if key.kind == RewardRecordKindV1::Mint {
                    changes.observe(key.index, true, false)?;
                }
            }
            _ => return Err(read::invalid().into()),
        }
        ctx.check()?;
    }
    changes.check(parent.mint_length(), length)?;
    if counts[1] != length {
        return Err(read::invalid().into());
    }
    let result = family::mint(
        record.after.digest().map_err(|_| read::invalid())?,
        counts,
        length,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}
