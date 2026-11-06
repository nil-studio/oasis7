use super::*;
use crate::{
    incremental_record_diff::{self, RecordDiffLabelsV1},
    incremental_record_tree::{self, RecordTreeLabelsV1},
};
use std::{cell::Cell, sync::Arc};
const TREE: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "resource receiver read",
    pending: "resource receiver pending",
    tree: "resource receiver tree",
    digest: "resource receiver digest",
    read_allowance: "resource receiver allowance",
    admission: "resource receiver admission",
    bound: "resource receiver bound",
    producer_pool: "resource receiver pool",
    page: "resource receiver page",
    write_allowance: "resource receiver write",
};
/// State hops debit the caller's existing scan at this exact nesting depth.
/// Payload reads retain the original shared value IO but do not become scans.
struct ScopedStateReader<'a, 'b> {
    source: &'a dyn ValueNodeReader,
    scan: RefCell<&'b mut StateScanBudget>,
    state_reads: Cell<bool>,
}
impl ValueNodeReader for ScopedStateReader<'_, '_> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if !self.state_reads.get() {
            return self.source.read_node(id, max, memory, class);
        }
        let admitted = {
            let mut scan = self.scan.borrow_mut();
            scan.nodes_remaining =
                scan.nodes_remaining
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "resource changed scan nodes",
                    ))?;
            let admitted = max;
            if admitted as u128 > u128::from(scan.read_bytes_remaining) {
                return Err(ValueTreeError::ResourceLimited(
                    "resource changed scan bytes",
                ));
            }
            scan.read_bytes_remaining -= admitted as u64;
            admitted
        };
        let page = self.source.read_node(id, admitted, memory, class)?;
        if page.len() > admitted
            || !page.uses_budget(memory)
            || page.reserved_bytes() < page.capacity()
        {
            return Err(invalid());
        }
        let mut scan = self.scan.borrow_mut();
        scan.read_bytes_remaining = scan
            .read_bytes_remaining
            .checked_add((admitted - page.len()) as u64)
            .ok_or_else(invalid)?;
        Ok(page)
    }
}
impl ScopedStateReader<'_, '_> {
    fn payload<T>(
        &self,
        f: impl FnOnce() -> Result<T, ValueTreeError>,
    ) -> Result<T, ValueTreeError> {
        self.state_reads.set(false);
        let result = f();
        self.state_reads.set(true);
        result
    }
}
type BoundColumnKeyV1 = (WorldFieldKeyV1, u64);
type ColumnLookupV1 = (BoundColumnKeyV1, Option<[u8; 32]>);
#[derive(Clone, Copy)]
struct Checkpoint {
    counts: [u64; 3],
    present: [Option<bool>; 3],
}
impl<M, R, E, C> FamilyVerifier<'_, '_, '_, M, R, E, C>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    fn lookup<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        record: Record,
        root: &StateRootV1,
        semantic: &K,
    ) -> Result<ColumnLookupV1, ValueTreeError> {
        self.spend_cell()?;
        let key = self.key(record.descriptor(), WorldFieldRecordKindV1::Entry, semantic)?;
        let cancel = self.cancelled;
        let mut stopped = || (cancel.borrow_mut())();
        let mut reader = InventoryValueReader {
            values: self.values,
            key_work: self.key_work,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            cancelled: &mut stopped,
        };
        let id = incremental_record_tree::lookup(root, &key.0, &mut reader, TREE)?;
        Ok((key, id))
    }
    fn checkpoint(
        &mut self,
        record: Record,
        root: &StateRootV1,
        reader: &ScopedStateReader<'_, '_>,
    ) -> Result<Checkpoint, ValueTreeError> {
        let mut result = Checkpoint {
            counts: [0; 3],
            present: [None; 3],
        };
        let mut omitted = 0u128;
        for (group, key) in schema::counts(record).into_iter().enumerate() {
            if let Some(key) = key {
                let (path, id) = match key {
                    schema::CheckpointKey::Simple(s) => self.lookup(record, root, &s)?,
                    schema::CheckpointKey::Pair(a, b) => self.lookup(record, root, &(a, b))?,
                };
                let id = id.ok_or_else(invalid)?;
                let owner = WorldFieldKeyV1::from_bytes(path.0.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key
                    .ok_or_else(invalid)?;
                let (cell, _, decoded, child) =
                    reader.payload(|| self.read_cell(record, &id, &path.0, owner))?;
                if child.is_some() {
                    return Err(invalid());
                }
                match (cell, decoded) {
                    (
                        schema::Cell::Fixed {
                            count_group: Some(actual),
                            ..
                        },
                        Decoded::Unsigned(n),
                    ) if usize::from(actual) == group => result.counts[group] = n,
                    _ => return Err(invalid()),
                }
            }
        }
        for (id, option) in schema::options(record).into_iter().enumerate() {
            if let Some((label, value_slot)) = option {
                let (path, value) = self.lookup(record, root, &(label, "present"))?;
                let value = value.ok_or_else(invalid)?;
                let owner = WorldFieldKeyV1::from_bytes(path.0.as_bytes())
                    .map_err(ValueTreeError::Invalid)?
                    .semantic_key
                    .ok_or_else(invalid)?;
                let (cell, _, decoded, child) =
                    reader.payload(|| self.read_cell(record, &value, &path.0, owner))?;
                if child.is_some() {
                    return Err(invalid());
                }
                let present = match (cell, decoded) {
                    (
                        schema::Cell::Fixed {
                            option: Some((actual, schema::OptionPart::Present)),
                            ..
                        },
                        Decoded::Bool(present),
                    ) if usize::from(actual) == id => present,
                    _ => return Err(invalid()),
                };
                let (_, value) = self.lookup(record, root, &(label, "value"))?;
                if value.is_some() != present {
                    return Err(invalid());
                }
                result.present[id] = Some(present);
                if !present {
                    omitted |= 1u128 << value_slot
                }
            }
        }
        let layout = schema::layout(record);
        let fixed = u64::from(layout.fixed_fields)
            .checked_sub(u64::from(omitted.count_ones()))
            .ok_or_else(invalid)?;
        let expected = result.counts.into_iter().try_fold(
            1u64 + u64::from(layout.groups.count_ones()) + fixed,
            |sum, n| sum.checked_add(n).ok_or_else(invalid),
        )?;
        if root.namespace != StateNamespaceV1::Artifacts || root.leaf_count != expected {
            return Err(invalid());
        }
        Ok(result)
    }
    fn key_only(
        &mut self,
        record: Record,
        entry: &StateEntryV1,
    ) -> Result<(schema::Cell, key::ParsedKey), ValueTreeError> {
        let binding = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
        let expected =
            WorldFieldKeyV1::new(record.descriptor(), binding.kind, binding.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
        if entry.namespace != StateNamespaceV1::Artifacts
            || entry.key != expected.as_bytes()
            || binding.kind != WorldFieldRecordKindV1::Entry
        {
            return Err(invalid());
        }
        let mut decoder = key::KeyDecoder::new();
        let cancel = self.cancelled;
        let mut stop = || (cancel.borrow_mut())();
        read_member_record_key_v1(
            &entry.value,
            &expected,
            self.values,
            MemberRecordLimitsV1 {
                semantic_bytes: self.semantic_bytes,
                payload_bytes: u64::MAX,
                record_bytes: self.record_bytes,
            },
            &mut decoder,
            &mut stop,
        )?;
        let parsed = decoder.finish().map_err(ValueTreeError::Invalid)?;
        Ok((
            schema::classify(record, WorldFieldRecordKindV1::Entry as u8, parsed)
                .ok_or_else(invalid)?,
            parsed,
        ))
    }
    pub(super) fn advance_record(
        &mut self,
        record: Record,
        before: &StateRootV1,
        after: &StateRootV1,
        scans: &mut [StateScanBudget],
    ) -> Result<(), ValueTreeError> {
        if self.cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if before == after {
            return Ok(());
        }
        let (scan, children) = scans.split_first_mut().ok_or_else(invalid)?;
        let source = ScopedStateReader {
            source: self.values.source,
            scan: RefCell::new(scan),
            state_reads: Cell::new(true),
        };
        let mut values = ValueReadContext {
            source: &source,
            memory: self.values.memory,
            class: self.values.class,
            io: &mut *self.values.io,
        };
        let mut local = FamilyVerifier {
            state: self.state,
            values: &mut values,
            key_work: &mut *self.key_work,
            semantic_bytes: self.semantic_bytes,
            record_bytes: self.record_bytes,
            max_payload_bytes: self.max_payload_bytes,
            take_state_error: &mut *self.take_state_error,
            cancelled: self.cancelled,
            work: &mut *self.work,
        };
        let old = local.checkpoint(record, before, &source)?;
        let new = local.checkpoint(record, after, &source)?;
        let mut actual = old.counts;
        // This guard is derived from the remaining real scan; every physical hop
        // additionally debits that same scan. It grants no new caller allowance.
        let mut frames = source.scan.borrow().nodes_remaining;
        let cancel = local.cancelled;
        let mut stop = || (cancel.borrow_mut())();
        let mut records = InventoryValueReader {
            values: &mut *local.values,
            key_work: &mut *local.key_work,
            semantic_bytes: local.semantic_bytes,
            record_bytes: local.record_bytes,
            cancelled: &mut stop,
        };
        let result = incremental_record_diff::visit(
            before,
            after,
            &mut records,
            &mut frames,
            RecordDiffLabelsV1 {
                invalid: "resource changed record",
                cells: "resource changed frames",
                stack: "resource changed stack",
                tree: TREE,
            },
            |old_entry, new_entry, records| {
                {
                    let mut scan = source.scan.borrow_mut();
                    let n = u64::from(old_entry.is_some()) + u64::from(new_entry.is_some());
                    scan.leaves_remaining = scan.leaves_remaining.checked_sub(n).ok_or(
                        ValueTreeError::ResourceLimited("resource changed scan leaves"),
                    )?;
                }
                source.payload(|| {
                    let mut row = FamilyVerifier {
                        state: local.state,
                        values: &mut *records.values,
                        key_work: &mut *records.key_work,
                        semantic_bytes: local.semantic_bytes,
                        record_bytes: local.record_bytes,
                        max_payload_bytes: local.max_payload_bytes,
                        take_state_error: &mut *local.take_state_error,
                        cancelled: local.cancelled,
                        work: &mut *local.work,
                    };
                    row.spend_cell()?;
                    let entry = new_entry.or(old_entry).ok_or_else(invalid)?;
                    let binding =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    let expected = WorldFieldKeyV1::new(
                        record.descriptor(),
                        binding.kind,
                        binding.semantic_key,
                    )
                    .map_err(ValueTreeError::Invalid)?;
                    if entry.namespace != StateNamespaceV1::Artifacts
                        || entry.key != expected.as_bytes()
                    {
                        return Err(invalid());
                    }
                    if matches!(
                        binding.kind,
                        WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Group
                    ) {
                        if old_entry.map(|r| r.value) != new_entry.map(|r| r.value) {
                            return Err(invalid());
                        }
                        return Ok(());
                    }
                    let (cell, parsed) = row.key_only(record, entry)?;
                    match cell {
                        schema::Cell::Fixed {
                            option: Some((id, schema::OptionPart::Value)),
                            ..
                        } => {
                            let id = usize::from(id);
                            if old_entry.is_some() != old.present[id].ok_or_else(invalid)?
                                || new_entry.is_some() != new.present[id].ok_or_else(invalid)?
                            {
                                return Err(invalid());
                            }
                        }
                        schema::Cell::Fixed { .. } => {
                            if old_entry.is_none() || new_entry.is_none() {
                                return Err(invalid());
                            }
                        }
                        schema::Cell::Dynamic { group, ordinal, .. } => {
                            let g = usize::from(group);
                            if ordinal {
                                let key::KeyValue::Unsigned(index) = parsed.value else {
                                    return Err(invalid());
                                };
                                match (old_entry.is_some(), new_entry.is_some()) {
                                    (false, true)
                                        if index >= old.counts[g] && index < new.counts[g] => {}
                                    (true, false) if index >= new.counts[g] => {}
                                    (true, true) if index < new.counts[g] => {}
                                    _ => return Err(invalid()),
                                }
                            }
                            match (old_entry.is_some(), new_entry.is_some()) {
                                (false, true) => {
                                    actual[g] = actual[g].checked_add(1).ok_or_else(invalid)?
                                }
                                (true, false) => {
                                    actual[g] = actual[g].checked_sub(1).ok_or_else(invalid)?
                                }
                                _ => {}
                            }
                        }
                        schema::Cell::Group(_) => return Err(invalid()),
                    }
                    if let Some(after) = new_entry {
                        let owner = binding.semantic_key.ok_or_else(invalid)?;
                        let (_, _, _, child) =
                            row.read_cell(record, &after.value, &expected, owner)?;
                        if let Some((kind, child)) = child {
                            let previous = if let Some(before) = old_entry {
                                row.read_cell(record, &before.value, &expected, owner)?.3
                            } else {
                                None
                            };
                            match previous {
                                Some((old_kind, old_root)) if old_kind == kind => {
                                    row.advance_record(kind, &old_root, &child, children)?
                                }
                                _ => row.verify_record(kind, &child, children)?,
                            }
                        }
                    }
                    Ok(())
                })
            },
        );
        let original = (local.take_state_error)();
        if local.cancelled() {
            return Err(ValueTreeError::Cancelled);
        }
        if let Some(original) = original {
            return Err(original);
        }
        result?;
        if actual != new.counts {
            return Err(invalid());
        }
        Ok(())
    }
}
