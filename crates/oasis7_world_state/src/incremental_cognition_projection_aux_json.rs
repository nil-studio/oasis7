//! Original JSON node codec to changed typed paths. Never a family proof.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_projection_delta::{Processing, spend},
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
use oasis7_proto::memory_budget::{MemoryReservation, ReservedValue};
use std::mem::size_of;
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("Cognition projection JSON path")
}
fn limit() -> ValueTreeError {
    ValueTreeError::ResourceLimited("Cognition projection JSON path")
}
pub enum CognitionAuxPathStepV1 {
    Name(String),
    Index(u64),
}
impl CognitionAuxPathStepV1 {
    pub fn borrowed(&self) -> JsonPathSegmentV1<'_> {
        match self {
            Self::Name(v) => JsonPathSegmentV1::Name(v),
            Self::Index(v) => JsonPathSegmentV1::Index(*v),
        }
    }
}
pub enum CognitionAuxJsonPatchValueV1 {
    Scalar(ReservedValue<Vec<u8>>),
    Container {
        array: bool,
        length: u64,
        reset: bool,
    },
    Remove,
}
pub struct CognitionAuxJsonPatchV1 {
    pub(crate) path: Vec<CognitionAuxPathStepV1>,
    pub(crate) value: CognitionAuxJsonPatchValueV1,
    _fee: MemoryReservation,
}
impl CognitionAuxJsonPatchV1 {
    pub fn path(&self) -> &[CognitionAuxPathStepV1] {
        &self.path
    }
    pub fn value(&self) -> &CognitionAuxJsonPatchValueV1 {
        &self.value
    }
}
pub(crate) struct JsonAuxBuildV1 {
    pub(crate) patches: Vec<CognitionAuxJsonPatchV1>,
    slots: MemoryReservation,
    memory: oasis7_proto::memory_budget::MemoryBudget,
}
impl JsonAuxBuildV1 {
    pub(crate) fn new(
        memory: &oasis7_proto::memory_budget::MemoryBudget,
    ) -> Result<Self, ValueTreeError> {
        Ok(Self {
            patches: Vec::new(),
            slots: memory.reserve(0, MemoryClass::Ordinary)?,
            memory: memory.clone(),
        })
    }
    fn order<C: FnMut() -> bool>(
        &mut self,
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<(), ValueTreeError> {
        fn cmp(
            left: &CognitionAuxJsonPatchV1,
            right: &CognitionAuxJsonPatchV1,
        ) -> Result<std::cmp::Ordering, ValueTreeError> {
            for (a, b) in left.path.iter().zip(&right.path) {
                let order = match (a, b) {
                    (CognitionAuxPathStepV1::Name(a), CognitionAuxPathStepV1::Name(b)) => a.cmp(b),
                    (CognitionAuxPathStepV1::Index(a), CognitionAuxPathStepV1::Index(b)) => {
                        if matches!(&left.value, CognitionAuxJsonPatchValueV1::Remove)
                            && matches!(&right.value, CognitionAuxJsonPatchValueV1::Remove)
                        {
                            b.cmp(a)
                        } else {
                            a.cmp(b)
                        }
                    }
                    _ => return Err(bad()),
                };
                if order != std::cmp::Ordering::Equal {
                    return Ok(order);
                }
            }
            Ok(left.path.len().cmp(&right.path.len()))
        }
        for i in 1..self.patches.len() {
            let mut j = i;
            while j > 0 {
                let bytes = self.patches[j - 1]
                    .path
                    .iter()
                    .chain(&self.patches[j].path)
                    .try_fold(128usize, |n, step| {
                        n.checked_add(size_of::<CognitionAuxPathStepV1>())
                            .and_then(|n| {
                                n.checked_add(match step {
                                    CognitionAuxPathStepV1::Name(s) => s.len(),
                                    _ => 0,
                                })
                            })
                            .ok_or_else(limit)
                    })?;
                spend(p, bytes, stop)?;
                match cmp(&self.patches[j - 1], &self.patches[j])? {
                    std::cmp::Ordering::Less => break,
                    std::cmp::Ordering::Equal => return Err(bad()),
                    std::cmp::Ordering::Greater => {
                        spend(p, 2 * size_of::<CognitionAuxJsonPatchV1>(), stop)?;
                        self.patches.swap(j - 1, j);
                        j -= 1
                    }
                }
            }
        }
        Ok(())
    }
    fn push<C: FnMut() -> bool>(
        &mut self,
        path: &[CognitionAuxPathStepV1],
        value: CognitionAuxJsonPatchValueV1,
        p: &Processing<'_>,
        stop: &mut C,
    ) -> Result<(), ValueTreeError> {
        let n = path
            .iter()
            .try_fold(size_of::<CognitionAuxJsonPatchV1>(), |n, s| {
                n.checked_add(size_of::<CognitionAuxPathStepV1>())
                    .and_then(|n| {
                        n.checked_add(match s {
                            CognitionAuxPathStepV1::Name(v) => v.len(),
                            _ => 0,
                        })
                    })
                    .ok_or_else(limit)
            })?;
        spend(p, n, stop)?;
        let fee = self.memory.reserve(n, MemoryClass::Ordinary)?;
        let mut copy = Vec::new();
        copy.try_reserve_exact(path.len()).map_err(|_| limit())?;
        if copy.capacity() > path.len() {
            return Err(limit());
        }
        for step in path {
            copy.push(match step {
                CognitionAuxPathStepV1::Name(v) => CognitionAuxPathStepV1::Name(v.clone()),
                CognitionAuxPathStepV1::Index(v) => CognitionAuxPathStepV1::Index(*v),
            })
        }
        if self.patches.len() == self.patches.capacity() {
            let cap = self
                .patches
                .capacity()
                .checked_mul(2)
                .ok_or_else(limit)?
                .max(4);
            let n = cap
                .checked_mul(size_of::<CognitionAuxJsonPatchV1>())
                .ok_or_else(limit)?;
            spend(p, n, stop)?;
            let mut slots = self.memory.reserve(n, MemoryClass::Ordinary)?;
            self.patches
                .try_reserve_exact(cap - self.patches.len())
                .map_err(|_| limit())?;
            if self.patches.capacity() > cap {
                return Err(limit());
            }
            self.slots
                .merge(&mut slots)
                .map_err(ValueTreeError::Invalid)?
        }
        self.patches.push(CognitionAuxJsonPatchV1 {
            path: copy,
            value,
            _fee: fee,
        });
        Ok(())
    }
}
fn member<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    entry: &StateEntryV1,
    array: bool,
    length: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &Processing<'_>,
) -> Result<(StateRootV1, CognitionAuxPathStepV1, MemoryReservation), ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(profile.field, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        MemberRecordLimitsV1 {
            semantic_bytes: records.semantic_bytes,
            payload_bytes: 165,
            record_bytes: records.record_bytes,
        },
        &mut *records.cancelled,
    )?;
    let n = usize::try_from(info.semantic_bytes).map_err(|_| limit())?;
    spend(
        p,
        n.checked_mul(4)
            .and_then(|n| n.checked_add(256))
            .ok_or_else(limit)?,
        &mut *records.cancelled,
    )?;
    let fee = records.values.memory.reserve(
        n.checked_add(size_of::<String>() + size_of::<CognitionAuxPathStepV1>())
            .ok_or_else(limit)?,
        records.values.class,
    )?;
    let mut text = Vec::new();
    text.try_reserve_exact(n).map_err(|_| limit())?;
    if text.capacity() > n {
        return Err(limit());
    }
    let (root, ordinal) =
        read::child_with_output(profile, entry, array, length, records, &mut text)?;
    let step = if array {
        if !text.is_empty() {
            return Err(bad());
        }
        CognitionAuxPathStepV1::Index(ordinal.ok_or_else(bad)?)
    } else {
        if ordinal.is_some() {
            return Err(bad());
        }
        CognitionAuxPathStepV1::Name(String::from_utf8(text).map_err(|_| bad())?)
    };
    Ok((root, step, fee))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_json_aux_paths_v1<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
    diff_cells: &mut u64,
    p: &Processing<'_>,
    build: &mut JsonAuxBuildV1,
) -> Result<(), ValueTreeError> {
    let mut path = Vec::new();
    let mut path_slots = records.values.memory.reserve(0, records.values.class)?;
    walk(
        profile,
        before,
        after,
        records,
        work,
        diff_cells,
        p,
        build,
        &mut path,
        &mut path_slots,
        0,
    )?;
    build.order(p, &mut *records.cancelled)
}
fn grow_path<R: ValueNodeReader, C: FnMut() -> bool>(
    path: &mut Vec<CognitionAuxPathStepV1>,
    slots: &mut MemoryReservation,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    p: &Processing<'_>,
) -> Result<(), ValueTreeError> {
    if path.len() < path.capacity() {
        return Ok(());
    }
    let cap = path.capacity().checked_mul(2).ok_or_else(limit)?.max(4);
    let bytes = cap
        .checked_mul(size_of::<CognitionAuxPathStepV1>())
        .ok_or_else(limit)?;
    spend(p, bytes, &mut *records.cancelled)?;
    let mut fee = records.values.memory.reserve(bytes, records.values.class)?;
    path.try_reserve_exact(cap - path.len())
        .map_err(|_| limit())?;
    if path.capacity() > cap {
        return Err(limit());
    }
    slots.merge(&mut fee).map_err(ValueTreeError::Invalid)
}
#[allow(clippy::too_many_arguments)]
fn walk<R: ValueNodeReader, C: FnMut() -> bool>(
    profile: JsonRecordProfileV1,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    work: &mut JsonReadWorkV1,
    diff_cells: &mut u64,
    p: &Processing<'_>,
    build: &mut JsonAuxBuildV1,
    path: &mut Vec<CognitionAuxPathStepV1>,
    path_slots: &mut MemoryReservation,
    depth: usize,
) -> Result<(), ValueTreeError> {
    spend(p, 0, &mut *records.cancelled)?;
    if before == after {
        return Ok(());
    }
    if depth > work.max_depth {
        return Err(limit());
    }
    let Some(after) = after else {
        return build.push(
            path,
            CognitionAuxJsonPatchValueV1::Remove,
            p,
            &mut *records.cancelled,
        );
    };
    let mut new = read::frame(profile, after, records, work)?;
    let old = before
        .map(|root| read::frame(profile, root, records, work))
        .transpose()?;
    let same = old.as_ref().is_some_and(|old| old.kind == new.kind);
    if !new.kind.container() {
        let key = read::cell_key(profile, "json.value", records)?;
        let info = read_member_record_header_v1(
            &new.ids[3],
            &key.0,
            records.values,
            MemberRecordLimitsV1 {
                semantic_bytes: records.semantic_bytes,
                payload_bytes: work.max_payload_bytes,
                record_bytes: records.record_bytes,
            },
            &mut *records.cancelled,
        )?;
        let mut heap = usize::try_from(info.payload_bytes)
            .map_err(|_| limit())?
            .checked_add(4096)
            .ok_or_else(limit)?;
        spend(p, heap, &mut *records.cancelled)?;
        let _graph = records.values.memory.reserve(heap, records.values.class)?;
        let value = materialize::node(profile, after, records, work, &mut heap, 0)?;
        let json = crate::incremental_world_rule_projection_delta::json(
            &value,
            records.values.memory,
            p,
            &mut *records.cancelled,
        )?;
        return build.push(
            path,
            CognitionAuxJsonPatchValueV1::Scalar(json),
            p,
            &mut *records.cancelled,
        );
    }
    let array = new.kind == read::Kind::Array;
    if !same || old.as_ref().is_some_and(|old| old.length != new.length) {
        build.push(
            path,
            CognitionAuxJsonPatchValueV1::Container {
                array,
                length: new.length,
                reset: !same,
            },
            p,
            &mut *records.cancelled,
        )?
    }
    if same {
        let old = old.as_ref().ok_or_else(bad)?;
        let before = before.ok_or_else(bad)?;
        crate::incremental_record_diff::visit_with_cells(
            before,
            after,
            records,
            diff_cells,
            crate::incremental_record_diff::RecordDiffLabelsV1 {
                invalid: "Cognition JSON projection changed paths",
                cells: "Cognition JSON projection changed cells",
                stack: "Cognition JSON projection changed stack",
                tree: LABELS,
            },
            |old_entry, new_entry, records, diff_cells| {
                let selected = new_entry.or(old_entry).ok_or_else(bad)?;
                if new.keys.iter().any(|key| key.as_bytes() == selected.key) {
                    return Ok(());
                }
                let a = old_entry
                    .map(|entry| member(profile, entry, array, old.length, records, p))
                    .transpose()?;
                let b = new_entry
                    .map(|entry| member(profile, entry, array, new.length, records, p))
                    .transpose()?;
                if let (Some((_, left, _)), Some((_, right, _))) = (&a, &b) {
                    match (left, right) {
                        (CognitionAuxPathStepV1::Name(a), CognitionAuxPathStepV1::Name(b))
                            if a == b => {}
                        (CognitionAuxPathStepV1::Index(a), CognitionAuxPathStepV1::Index(b))
                            if a == b => {}
                        _ => return Err(bad()),
                    }
                }
                if let Some((_, CognitionAuxPathStepV1::Index(index), _)) = &a
                    && b.is_none()
                {
                    if *index >= new.length {
                        return Ok(());
                    }
                    return Err(bad());
                }
                let step = if let Some((_, step, _)) = &b {
                    step
                } else {
                    &a.as_ref().ok_or_else(bad)?.1
                };
                let _copy = records.values.memory.reserve(
                    size_of::<CognitionAuxPathStepV1>()
                        + match step {
                            CognitionAuxPathStepV1::Name(v) => v.len(),
                            _ => 0,
                        },
                    records.values.class,
                )?;
                spend(
                    p,
                    size_of::<CognitionAuxPathStepV1>()
                        + match step {
                            CognitionAuxPathStepV1::Name(v) => v.len(),
                            _ => 0,
                        },
                    &mut *records.cancelled,
                )?;
                grow_path(path, path_slots, records, p)?;
                path.push(match step {
                    CognitionAuxPathStepV1::Name(v) => CognitionAuxPathStepV1::Name(v.clone()),
                    CognitionAuxPathStepV1::Index(v) => CognitionAuxPathStepV1::Index(*v),
                });
                let result = walk(
                    profile,
                    a.as_ref().map(|v| &v.0),
                    b.as_ref().map(|v| &v.0),
                    records,
                    work,
                    diff_cells,
                    p,
                    build,
                    path,
                    path_slots,
                    depth + 1,
                );
                path.pop();
                result
            },
        )?;
    } else {
        while let Some(entry) = new.cursor.next(records)? {
            spend(p, entry.key.len(), &mut *records.cancelled)?;
            if new.keys.iter().any(|key| key.as_bytes() == entry.key) {
                continue;
            }
            let (child, step, _fee) = member(profile, &entry, array, new.length, records, p)?;
            grow_path(path, path_slots, records, p)?;
            path.push(step);
            let result = walk(
                profile,
                None,
                Some(&child),
                records,
                work,
                diff_cells,
                p,
                build,
                path,
                path_slots,
                depth + 1,
            );
            path.pop();
            result?
        }
    }
    Ok(())
}
