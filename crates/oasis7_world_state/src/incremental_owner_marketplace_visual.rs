//! Closed visual-entity columns within an authenticated marketplace child.
use super::*;

pub(super) fn validate_visual<M, V, C>(
    child: &StateRootV1,
    reader: &StateScanReader<'_, M, C>,
    field: FieldDescriptor,
    values: &mut ValueReadContext<'_, V>,
    max_value_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), WorldStateOwnerError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    C: FnMut() -> bool,
{
    const ENTITY: u16 = 1 << 0;
    const MODULE: u16 = 1 << 1;
    const KIND: u16 = 1 << 2;
    const LABEL_PRESENT: u16 = 1 << 3;
    const ANCHOR_KIND: u16 = 1 << 4;
    const LABEL_TEXT: u16 = 1 << 5;
    const AGENT_ID: u16 = 1 << 6;
    const LOCATION_ID: u16 = 1 << 7;
    const X: u16 = 1 << 8;
    const Y: u16 = 1 << 9;
    const Z: u16 = 1 << 10;

    let mut seen = 0u16;
    let mut label_present = None;
    let mut anchor_kind = None;
    validate_child_entries(
        child,
        reader,
        field,
        values,
        max_value_bytes,
        cancelled,
        |semantic, value| {
            let ChildSemantic::Field(name) = semantic else {
                return Err(WorldStateOwnerError::Invalid(
                    "module visual unexpected pair key",
                ));
            };
            let (bit, expected_kind) = match name {
                "entity_id" => (ENTITY, ChildPayloadKind::Text),
                "module_id" => (MODULE, ChildPayloadKind::Text),
                "kind" => (KIND, ChildPayloadKind::Text),
                "label_present" => (LABEL_PRESENT, ChildPayloadKind::Bool),
                "anchor_kind" => (ANCHOR_KIND, ChildPayloadKind::U64),
                "label_text" => (LABEL_TEXT, ChildPayloadKind::Text),
                "anchor_agent_id" => (AGENT_ID, ChildPayloadKind::Text),
                "anchor_location_id" => (LOCATION_ID, ChildPayloadKind::Text),
                "anchor_x_cm" => (X, ChildPayloadKind::I64),
                "anchor_y_cm" => (Y, ChildPayloadKind::I64),
                "anchor_z_cm" => (Z, ChildPayloadKind::I64),
                _ => {
                    return Err(WorldStateOwnerError::Invalid(
                        "module visual unknown child field",
                    ));
                }
            };
            if seen & bit != 0 {
                return Err(WorldStateOwnerError::Invalid(
                    "module visual duplicate child field",
                ));
            }
            if child_payload_kind(name) != Some(expected_kind) {
                return Err(WorldStateOwnerError::Invalid(
                    "module visual field codec table",
                ));
            }
            match (name, value) {
                ("label_present", ChildValue::Bool(value)) => label_present = Some(value),
                ("anchor_kind", ChildValue::U64(value)) if value <= 2 => anchor_kind = Some(value),
                ("anchor_kind", _) => {
                    return Err(WorldStateOwnerError::Invalid("module visual anchor kind"));
                }
                (
                    "entity_id" | "module_id" | "kind" | "label_text" | "anchor_agent_id"
                    | "anchor_location_id",
                    ChildValue::Text,
                )
                | ("anchor_x_cm" | "anchor_y_cm" | "anchor_z_cm", ChildValue::I64(_)) => {}
                _ => {
                    return Err(WorldStateOwnerError::Invalid(
                        "module visual child value kind",
                    ));
                }
            }
            seen |= bit;
            Ok(())
        },
    )?;
    let mut expected = ENTITY | MODULE | KIND | LABEL_PRESENT | ANCHOR_KIND;
    if label_present == Some(true) {
        expected |= LABEL_TEXT;
    }
    match anchor_kind {
        Some(0) => expected |= AGENT_ID,
        Some(1) => expected |= LOCATION_ID,
        Some(2) => expected |= X | Y | Z,
        _ => {
            return Err(WorldStateOwnerError::Invalid(
                "module visual required anchor kind missing",
            ));
        }
    }
    if label_present.is_none() || seen != expected {
        return Err(WorldStateOwnerError::Invalid("module visual child closure"));
    }
    Ok(())
}
