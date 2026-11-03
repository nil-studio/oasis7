//! A complete explicit record scope, bound to the current transaction/root.
use super::*;
use crate::{
    incremental_expected_bytes::Compare, incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_state::{StateRootV1, StateScanContext, StateScanError},
    incremental_value::ValueNodeReader,
};
use std::cell::RefCell;
#[derive(Debug)]
pub struct FactoryRecordProofV1 {
    pub(super) parent_roots: [u8; 32],
    pub(super) schema: [u8; 32],
    pub(super) owner: [u8; 32],
    pub(super) root: StateRootV1,
    pub(super) value_root: [u8; 32],
}
pub(super) fn verify<R: ValueNodeReader, C: FnMut() -> bool>(
    key: FactoryRecordKeyV1<'_>,
    expected: FactoryRecordRefV1<'_>,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max_payload_bytes: u64,
    work: &mut FactoryWorkBudgetV1,
) -> Result<(), WorldFieldOverlayErrorV1> {
    // The complete expected schema already includes every parent flag/count.
    // Authenticate Metadata once, then compare each distinct target directly;
    // per-cell point ancestor lookups would repeatedly walk the same paths.
    let field = key.descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let keys = std::array::from_fn(|index| {
        if index == 0 {
            Some((metadata.clone(), 0))
        } else {
            None
        }
    });
    let root = tree::roots(child, &keys, records)?[0]
        .ok_or(ValueTreeError::Invalid("factory expected metadata"))?;
    records.presence(&root, &metadata, 0)?;
    let mut expected_cells = 1u64; // authenticated record-local Metadata
    visit_factory_record_v1(key, expected, &mut |cell,
                                                 payload|
     -> Result<
        (),
        WorldFieldOverlayErrorV1,
    > {
        work.cell()?;
        expected_cells = expected_cells
            .checked_add(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "factory record cardinality",
            ))?;
        let expected = overlay::FactoryValueRefV1::from_payload(payload)?;
        let mut compare = Compare::new(expected.text().as_bytes());
        let target = with_factory_cell_key!(key,cell,encoded=>records.key(field,WorldFieldRecordKindV1::Entry,encoded))?;
        let keys = std::array::from_fn(|index| {
            if index == 0 {
                Some(target.clone())
            } else {
                None
            }
        });
        let root = tree::roots(child, &keys, records)?[0]
            .ok_or(ValueTreeError::Invalid("factory expected cell"))?;
        let value = read::value(
            cell,
            &root,
            &target,
            records,
            max_payload_bytes,
            &mut compare,
        )?;
        if !expected.matches(value) || !compare.is_equal() {
            return Err(ValueTreeError::Invalid("factory expected record").into());
        }
        Ok(())
    })?;
    // Source traversal authenticates every actual entry and actual leaf_count.
    // Distinct closed expected keys already proved present above; equal counts
    // therefore exclude every undeclared child, not only a supplied subset.
    let source = tree::Source {
        source: records.values.source,
        memory: records.values.memory,
        class: records.values.class,
        io: RefCell::new(&mut *records.values.io),
        cancelled: RefCell::new(&mut *records.cancelled),
        error: RefCell::new(None),
    };
    let result = StateScanContext {
        source: &source,
        memory: source.memory,
        class: source.class,
        io: &mut work.scan,
    }
    .visit_entries(
        child,
        |_| Ok::<_, ValueTreeError>(()),
        || (source.cancelled.borrow_mut())(),
    );
    let count = match result {
        Ok(count) => source.finish(Ok(count))?,
        Err(StateScanError::Memory(e)) => return Err(ValueTreeError::Memory(e).into()),
        Err(StateScanError::ResourceLimited) => {
            return Err(ValueTreeError::ResourceLimited("factory record closure scan").into());
        }
        Err(StateScanError::Cancelled) => return Err(ValueTreeError::Cancelled.into()),
        Err(StateScanError::Visitor(e)) => return Err(e.into()),
        Err(StateScanError::Invalid(_)) => {
            return Err(ValueTreeError::Invalid("factory record closure").into());
        }
        Err(StateScanError::Tree(e)) => {
            return Err(source.finish::<()>(Err(e)).unwrap_err().into());
        }
    };
    if count != expected_cells {
        return Err(ValueTreeError::Invalid("factory undeclared record child").into());
    }
    Ok(())
}

impl FactoryRecordProofV1 {
    /// Exposes only the already checked record relation at this exact binding.
    pub fn validate_binding(
        &self,
        parent_roots: &[u8; 32],
        schema: &[u8; 32],
        owner: &[u8; 32],
        value_root: &[u8; 32],
        namespace: oasis7_formal_protocol::incremental_state::StateNamespaceV1,
    ) -> Result<&StateRootV1, ValueTreeError> {
        if &self.parent_roots != parent_roots
            || &self.schema != schema
            || &self.owner != owner
            || &self.value_root != value_root
            || self.root.namespace != namespace
        {
            return Err(ValueTreeError::Invalid("factory record proof binding"));
        }
        Ok(&self.root)
    }
}
