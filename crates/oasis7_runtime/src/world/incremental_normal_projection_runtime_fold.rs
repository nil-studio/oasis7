//! Fixed actual Runtime revocation source fold. Container creation comes only
//! from the original codec's presence patch, never a guessed missing baseline.
use super::*;
use oasis7_world_projection::current_commitment::{
    ProjectionColumnDataV1 as Column, ProjectionColumnErrorV1, ProjectionDataPathRefV1 as Path,
};
use oasis7_world_state::incremental_runtime_revocation::RevocationProjectionContainerV1 as Container;

pub(super) fn graph(
    e: ProjectionColumnErrorV1<oasis7_formal_protocol::controlled_authority::formal::FormalError>,
) -> BodyMutationError {
    match e {
        ProjectionColumnErrorV1::Work(e) => error(e),
        ProjectionColumnErrorV1::Memory(e) => ValueTreeError::Memory(e).into(),
        ProjectionColumnErrorV1::Invalid(e) => BodyMutationError::Invalid(e),
    }
}
pub(super) fn select(
    columns: &Column,
    path: &[Path<'_>],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<Column>, BodyMutationError> {
    let memory = common.memory().clone();
    columns
        .select_path_data_v1(path, &memory, &mut |s, b| common.charge(s, b))
        .map_err(graph)
}

pub(super) fn value(
    json: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Column, BodyMutationError> {
    let bytes = json
        .len()
        .checked_mul(128)
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(bad)?;
    common
        .charge(json.len() as u64, bytes as u64)
        .map_err(error)?;
    let memory = common.memory().clone();
    let _phase = memory
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let value = serde_json::from_slice(json).map_err(error)?;
    Column::from_json_data(value, &memory, &mut |s, b| common.charge(s, b)).map_err(graph)
}
fn path<'a>(fields: &[&'a str], slots: &mut [Path<'a>; 8]) -> Result<usize, BodyMutationError> {
    if fields.len() + 1 > slots.len() {
        return Err(bad());
    }
    slots[0] = Path::Field("capability_revocation_state");
    for (i, field) in fields.iter().enumerate() {
        slots[i + 1] = Path::Field(field);
    }
    Ok(fields.len() + 1)
}

/// Normalize the original fixed set exactly once while constructing GEN/cold
/// columns. Ordinary ticks never scan a set to repair a missing prerequisite.
pub(in crate::world) fn initialize_revocation_set_projection_v1(
    columns: &Column,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Column, BodyMutationError> {
    let path = [Path::Field("capability_revocation_state"), Path::Field("revoked_grant_ids")];
    let selected = select(columns, &path, common)?.ok_or_else(bad)?;
    let memory = common.memory().clone();
    let selected = selected.as_ordered_text_set_data_v1(&memory, &mut |s,b| common.charge(s,b)).map_err(graph)?;
    columns.with_path_data_v1(&path, Some(&selected), &memory, &mut |s,b| common.charge(s,b)).map_err(graph)
}

pub(in crate::world) fn fold_current_revocation_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    source: &ActualCurrentRevocationProjectionSourceV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    source.validate(tx, common, live)?;
    let memory = common.memory().clone();
    let mut columns = before
        .share_data(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph)?;
    // The original fixed record contains fourteen roots, participant six and
    // task-index two. This stack inventory records their actual final headers.
    let mut headers: [Option<(&'static [&'static str], Container, bool, u64)>; 32] = [None; 32];
    let mut count = 0usize;
    source.visit(tx, common, live, |patch, common| {
        let mut slots = [Path::Field(""); 8];
        match patch {
            RevocationProjectionPatchRefV1::Container {
                path: fields,
                kind,
                present,
                length,
            } => {
                let n = path(fields, &mut slots)?;
                let slot = headers.get_mut(count).ok_or_else(bad)?;
                *slot = Some((fields, kind, present, length));
                count += 1;
                if !present {
                    columns = columns
                        .with_path_data_v1(&slots[..n], None, &memory, &mut |s, b| {
                            common.charge(s, b)
                        })
                        .map_err(graph)?;
                    return Ok(());
                }
                let existing = select(&columns, &slots[..n], common)?;
                let mut selected = match existing {
                    Some(existing) => existing,
                    None => value(
                        match kind {
                            Container::Object | Container::Map => b"{}",
                            Container::Set | Container::Sequence => b"[]",
                        },
                        common,
                    )?,
                };
                if matches!(kind, Container::Set)
                    && selected
                        .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
                        .map_err(graph)?
                        == 0
                {
                    selected = selected
                        .as_ordered_text_set_data_v1(&memory, &mut |s, b| common.charge(s, b))
                        .map_err(graph)?;
                }
                if matches!(kind, Container::Sequence) {
                    let mut len = selected
                        .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
                        .map_err(graph)?;
                    let after = usize::try_from(length).map_err(|_| bad())?;
                    while len > after {
                        selected = selected
                            .with_path_data_v1(
                                &[Path::Index(len - 1)],
                                None,
                                &memory,
                                &mut |s, b| common.charge(s, b),
                            )
                            .map_err(graph)?;
                        len -= 1;
                    }
                }
                columns = columns
                    .with_path_data_v1(&slots[..n], Some(&selected), &memory, &mut |s, b| {
                        common.charge(s, b)
                    })
                    .map_err(graph)?;
            }
            RevocationProjectionPatchRefV1::Value { path: fields, json } => {
                let n = path(fields, &mut slots)?;
                let selected = json.map(|json| value(json, common)).transpose()?;
                columns = columns
                    .with_path_data_v1(&slots[..n], selected.as_ref(), &memory, &mut |s, b| {
                        common.charge(s, b)
                    })
                    .map_err(graph)?;
            }
            RevocationProjectionPatchRefV1::Member {
                path: fields,
                key,
                json,
            } => {
                let n = path(fields, &mut slots)?;
                *slots.get_mut(n).ok_or_else(bad)? = Path::Field(key);
                let selected = json.map(|json| value(json, common)).transpose()?;
                columns = columns
                    .with_path_data_v1(&slots[..n + 1], selected.as_ref(), &memory, &mut |s, b| {
                        common.charge(s, b)
                    })
                    .map_err(graph)?;
            }
            RevocationProjectionPatchRefV1::SetMember {
                path: fields,
                key,
                present,
            } => {
                let n = path(fields, &mut slots)?;
                let selected = select(&columns, &slots[..n], common)?.ok_or_else(bad)?;
                let selected = selected
                    .with_text_set_member_data_v1(key, present, &memory, &mut |s, b| {
                        common.charge(s, b)
                    })
                    .map_err(graph)?;
                columns = columns
                    .with_path_data_v1(&slots[..n], Some(&selected), &memory, &mut |s, b| {
                        common.charge(s, b)
                    })
                    .map_err(graph)?;
            }
            RevocationProjectionPatchRefV1::SequenceMember {
                path: fields,
                index,
                json,
            } => {
                if let Some(json) = json {
                    let n = path(fields, &mut slots)?;
                    *slots.get_mut(n).ok_or_else(bad)? =
                        Path::Index(usize::try_from(index).map_err(|_| bad())?);
                    let selected = value(json, common)?;
                    columns = columns
                        .with_path_data_v1(
                            &slots[..n + 1],
                            Some(&selected),
                            &memory,
                            &mut |s, b| common.charge(s, b),
                        )
                        .map_err(graph)?;
                }
                // Actual removed ordinals were already truncated from the tail
                // using the authenticated final container header above.
            }
        }
        Ok(())
    })?;
    for (fields, kind, present, length) in headers.into_iter().flatten() {
        let mut slots = [Path::Field(""); 8];
        let n = path(fields, &mut slots)?;
        let selected = select(&columns, &slots[..n], common)?;
        if !present {
            if selected.is_some() {
                return Err(bad());
            }
            continue;
        }
        let selected = selected.ok_or_else(bad)?;
        let actual = match kind {
            Container::Map => selected
                .object_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
                .map_err(graph)?,
            Container::Set | Container::Sequence => selected
                .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
                .map_err(graph)?,
            Container::Object => continue, // optional original child emission is checked by its own patch
        };
        if u64::try_from(actual).map_err(|_| bad())? != length {
            return Err(bad());
        }
    }
    source.validate(tx, common, live)?;
    Ok(columns)
}
