//! Fixed original scalar controls. This never interprets their member payloads.
use super::member::Text;
use super::*;
use crate::incremental_runtime_controls::{
    RuntimeControlFieldV1 as Field, RuntimeControlReadV1 as Read, read_runtime_control_overlay_v1,
};
pub(super) fn capture<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    selected: Selection<'_>,
    index: usize,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut ReadWork<'_>,
    p: &Processing<'_>,
    build: &mut Build,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let field = match index {
        12 => Field::GovernanceEmergencyBrakeUntilTick,
        13 => Field::TickConsensusAuthoritySource,
        _ => return Err(bad().into()),
    };
    let parsed = WorldFieldKeyV1::from_bytes(selected.key()).map_err(ValueTreeError::Invalid)?;
    if selected.namespace() != field.namespace() {
        return Err(bad().into());
    }
    if parsed.kind == WorldFieldRecordKindV1::Metadata {
        return Ok(());
    }
    let key = WorldFieldKeyV1::new(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != selected.key()
        || parsed.kind != WorldFieldRecordKindV1::Entry
        || selected.after().is_none()
    {
        return Err(bad().into());
    }
    *work.controls_cells = work.controls_cells.checked_sub(1).ok_or_else(limit)?;
    let memory = build.memory.clone();
    let mut output = Text {
        bytes: Vec::new(),
        fee: memory
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?,
        memory: &memory,
        p,
        first: None,
    };
    let result = read_runtime_control_overlay_v1(
        field,
        ctx,
        work.selected_heap.try_into().map_err(|_| limit())?,
        &mut output,
    );
    if let Some(error) = output.first {
        return Err(error.into());
    }
    let value = result?;
    match value {
        Read::Unit if field == Field::GovernanceEmergencyBrakeUntilTick => build.value(
            FIELDS[index],
            &[],
            Some(&Option::<u64>::None),
            p,
            &mut ctx.cancelled,
        )?,
        Read::Unsigned(value) if field == Field::GovernanceEmergencyBrakeUntilTick => build.value(
            FIELDS[index],
            &[],
            Some(&Some(value)),
            p,
            &mut ctx.cancelled,
        )?,
        Read::Text { utf8_bytes } if field == Field::TickConsensusAuthoritySource => {
            if utf8_bytes != output.bytes.len() as u64 {
                return Err(bad().into());
            }
            let text = std::str::from_utf8(&output.bytes).map_err(|_| bad())?;
            build.value(FIELDS[index], &[], Some(text), p, &mut ctx.cancelled)?;
        }
        _ => return Err(bad().into()),
    }
    Ok(())
}
