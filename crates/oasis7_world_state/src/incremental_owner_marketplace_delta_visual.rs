//! Conditional visual presence is closed without reading inherited large text.
use super::*;
const FIELDS: [&str; 11] = [
    "entity_id",
    "module_id",
    "kind",
    "label_present",
    "anchor_kind",
    "label_text",
    "anchor_agent_id",
    "anchor_location_id",
    "anchor_x_cm",
    "anchor_y_cm",
    "anchor_z_cm",
];
fn profile(sem: ChildSemantic, value: ChildValue) -> bool {
    matches!(
        (sem, value),
        (
            ChildSemantic::Field(
                "entity_id"
                    | "module_id"
                    | "kind"
                    | "label_text"
                    | "anchor_agent_id"
                    | "anchor_location_id"
            ),
            ChildValue::Text
        ) | (
            ChildSemantic::Field("anchor_x_cm" | "anchor_y_cm" | "anchor_z_cm"),
            ChildValue::I64(_)
        ) | (ChildSemantic::Field("label_present"), ChildValue::Bool(_))
    ) || matches!(
        (sem, value),
        (ChildSemantic::Field("anchor_kind"), ChildValue::U64(0..=2))
    )
}
pub(crate) fn visual_delta<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    G: FnMut() -> bool,
    C: FnMut() -> bool,
>(
    field: FieldDescriptor,
    before: &StateRootV1,
    after: &StateRootV1,
    guard: &StateScanReader<'_, M, G>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), WorldStateOwnerError> {
    if field_from_descriptor(field) != Some(ModuleMarketplaceFieldV1::VisualEntities) {
        return Err(bad());
    }
    let mut mask = 0u16;
    let mut label = None;
    let mut anchor = None;
    for (i, name) in FIELDS.into_iter().enumerate() {
        let key = make_text_key(
            field,
            name,
            records.values,
            records.key_work,
            max,
            records.cancelled,
        )?;
        if guard
            .get(after, key.as_bytes())
            .map_err(map_scan_lookup_error)?
            .is_some()
        {
            mask |= 1 << i;
            if matches!(name, "label_present" | "anchor_kind") {
                let value = read_value_at_key(
                    after,
                    guard,
                    field,
                    &key,
                    ChildSemantic::Field(name),
                    None,
                    records.key_work,
                    records.values,
                    max,
                    records.cancelled,
                )?;
                match (name, value) {
                    ("label_present", ChildValue::Bool(v)) => label = Some(v),
                    ("anchor_kind", ChildValue::U64(v)) if v <= 2 => anchor = Some(v),
                    _ => return Err(bad()),
                }
            }
        }
    }
    let mut expected = 0b1_1111;
    match label {
        Some(true) => expected |= 1 << 5,
        Some(false) => (),
        None => return Err(bad()),
    };
    match anchor {
        Some(0) => expected |= 1 << 6,
        Some(1) => expected |= 1 << 7,
        Some(2) => expected |= (1 << 8) | (1 << 9) | (1 << 10),
        _ => return Err(bad()),
    };
    if mask != expected || after.leaf_count != u64::from(mask.count_ones()) {
        return Err(bad());
    }
    let mut original = None;
    let result = record_diff::visit(before, after, records, cells, LABELS, |_, new, records| {
        if let Some(new) = new {
            let checked = (|| {
                let (semantic, value) =
                    read_child_member(new, field, records.values, max, records.cancelled)?;
                if !profile(semantic, value) {
                    return Err(bad());
                }
                Ok(())
            })();
            if let Err(e) = checked {
                original = Some(e);
                return Err(ValueTreeError::Invalid("market visual changed row"));
            }
        }
        Ok(())
    });
    if (records.cancelled)() {
        return Err(WorldStateOwnerError::Value(ValueTreeError::Cancelled));
    }
    if let Some(e) = original {
        return Err(e);
    }
    result.map_err(WorldStateOwnerError::Value)
}
