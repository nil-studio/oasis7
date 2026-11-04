//! Complete proposal qualification streams original Text/JSON instead of DTO hydration.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnReadProfileV1, read_column_row_v1},
    incremental_record_value_read::RecordValueV1,
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
struct Probe<'a, 'b, 'v, R, C> {
    root: &'a StateRootV1,
    records: &'a mut InventoryValueReader<'b, 'v, R, C>,
    work: &'a mut RuntimeProposalReadWorkV1,
    expected: u64,
}
impl<R: ValueNodeReader, C: FnMut() -> bool> Probe<'_, '_, '_, R, C> {
    fn point<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<(WorldFieldKeyV1, u64, [u8; 32]), ValueTreeError> {
        let (key, n, value) = header::point(self.root, key, self.records, self.work)?;
        self.expected = self.expected.checked_add(1).ok_or_else(invalid)?;
        Ok((key, n, value.ok_or_else(invalid)?))
    }
    fn scalar<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
        codec: MemberPayloadCodecV1,
    ) -> Result<RecordValueV1, ValueTreeError> {
        let (key, n, id) = self.point(key)?;
        self.records.values.verify_closure(
            &id,
            self.records.record_bytes,
            &mut *self.records.cancelled,
        )?;
        header::value(
            &id,
            &(key, n),
            codec,
            self.records,
            self.work,
            &mut std::io::sink(),
        )
    }
    fn unsigned<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<u64, ValueTreeError> {
        match self.scalar(key, MemberPayloadCodecV1::U64)? {
            RecordValueV1::Unsigned(n) => Ok(n),
            _ => Err(invalid()),
        }
    }
    fn text<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<(), ValueTreeError> {
        match self.scalar(key, MemberPayloadCodecV1::Text)? {
            RecordValueV1::Text { .. } => Ok(()),
            _ => Err(invalid()),
        }
    }
    fn present(&mut self, name: &str) -> Result<bool, ValueTreeError> {
        match self.scalar(&(name, "present"), MemberPayloadCodecV1::Bool)? {
            RecordValueV1::Bool(v) => Ok(v),
            _ => Err(invalid()),
        }
    }
    fn optional(&mut self, name: &str) -> Result<(), ValueTreeError> {
        let present = self.present(name)?;
        if present {
            self.unsigned(&(name, "value"))?;
        } else {
            if header::point(self.root, &(name, "value"), self.records, self.work)?
                .2
                .is_some()
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn json<K: crate::incremental_semantic_key::SemanticKeyV1 + ?Sized>(
        &mut self,
        key: &K,
    ) -> Result<(), ValueTreeError> {
        let (key, _, id) = self.point(key)?;
        let entry = StateEntryV1 {
            namespace: self.root.namespace,
            key: key.as_bytes().to_vec(),
            value: id,
        };
        let row = read_column_row_v1(
            ColumnReadProfileV1 {
                field: columns(),
                schema: schema(),
                groups: &[],
                reference_schema: Some(recovery::reference_schema),
            },
            &entry,
            self.records,
            16 << 10,
        )?;
        let ColumnCellV1::Root(root) = &row.cell else {
            return Err(invalid());
        };
        crate::incremental_json_records::qualify_json_subtree_records_v1(
            json_profile(),
            root,
            self.records,
            &mut self.work.json,
        )?;
        Ok(())
    }
}
pub(super) fn qualify<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    root.validate().map_err(|_| invalid())?;
    if root.namespace != StateNamespaceV1::Authorization {
        return Err(invalid());
    }
    // Bounded temporary keys, status facts, point tuple and entry allocations.
    let _scratch = ctx.values.memory.reserve(32 << 10, ctx.values.class)?;
    let mut actual = 0u64;
    let cancel = RefCell::new(&mut ctx.cancelled);
    let result = StateScanContext {
        source: ctx.state,
        memory: ctx.values.memory,
        class: ctx.values.class,
        io: scan,
    }
    .visit_entries(
        root,
        |entry| {
            work.cells_remaining = work
                .cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited("proposal closed rows"))?;
            let key = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            let canonical = WorldFieldKeyV1::new(columns(), key.kind, key.semantic_key)
                .map_err(ValueTreeError::Invalid)?;
            if canonical.as_bytes() != entry.key
                || entry.namespace != root.namespace
                || !matches!(
                    key.kind,
                    WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry
                )
            {
                return Err(invalid());
            }
            actual = actual.checked_add(1).ok_or_else(invalid)?;
            Ok(())
        },
        || (cancel.borrow_mut())(),
    );
    if (cancel.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(e) = (ctx.take_state_error)() {
        return Err(e);
    }
    match result {
        Ok(_) => (),
        Err(StateScanError::Visitor(e)) => return Err(e),
        Err(StateScanError::Memory(e)) => return Err(e.into()),
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("proposal closed scan"));
        }
        Err(_) => return Err(invalid()),
    }
    drop(cancel);
    let mut records = ctx.records();
    let meta = WorldFieldKeyV1::new(columns(), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let id = crate::incremental_record_tree::lookup(root, &meta, &mut records, genesis::LABELS)?
        .ok_or_else(invalid)?;
    records
        .values
        .verify_closure(&id, records.record_bytes, &mut *records.cancelled)?;
    records.presence(&id, &meta, 0)?;
    let mut p = Probe {
        root,
        records: &mut records,
        work,
        expected: 1,
    };
    for name in ["id", "timelock_ticks", "manifest.version"] {
        p.unsigned(&name)?;
    }
    for name in ["author", "base_manifest_hash"] {
        p.text(&name)?;
    }
    p.json(&"manifest.content")?;
    for name in ["queued_at_tick", "not_before_tick", "activate_epoch"] {
        p.optional(name)?;
    }
    let status = header::validate_status(root, None, p.records, p.work)?;
    p.expected = p
        .expected
        .checked_add(status.cells.iter().filter(|x| x.2.is_some()).count() as u64)
        .ok_or_else(invalid)?;
    drop(status);
    if p.present("patch")? {
        p.text(&"patch.base_manifest_hash")?;
        p.optional("patch.new_version")?;
        let count = p.unsigned(&"patch.ops.count")?;
        if count > actual {
            return Err(invalid());
        }
        for i in 0..count {
            let kind = p.unsigned(&("patch.ops", (i, "kind")))?;
            match kind {
                0 => p.json(&("patch.ops", (i, "value")))?,
                1 => (),
                _ => return Err(invalid()),
            }
            let len = p.unsigned(&("patch.ops", (i, "path.count")))?;
            if len > actual {
                return Err(invalid());
            }
            for j in 0..len {
                p.text(&("patch.ops", (i, ("path", j))))?;
            }
        }
    }
    if p.expected != actual || actual != root.leaf_count {
        return Err(invalid());
    }
    ctx.check()
}
