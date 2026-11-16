//! Real Cognition State/Proof JSON patches, applied with original presence and
//! container ordering. This data fold does not resolve task status or authority.
use super::fold::{graph, select, value};
use super::*;
use crate::world::incremental_cognition_projection_aux_state::{
    PaidCognitionProofProjectionAuxV1, PaidCognitionStateProjectionAuxV1,
};
use oasis7_world_state::incremental_cognition_projection_aux::{
    CognitionAuxJsonPatchV1 as Patch, CognitionAuxJsonPatchValueV1 as Change,
    CognitionAuxPathStepV1 as Step,
};
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1 as Column, ProjectionDataPathRefV1 as Path,
};
use std::cell::RefCell;

pub(super) struct PaidPath<'a> {
    pub(super) items: Vec<Path<'a>>,
    _fee: MemoryReservation,
}
pub(super) fn path_from_base_v1<'a>(
    base: &[Path<'a>],
    suffix: &'a [Step],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidPath<'a>, BodyMutationError> {
    let count = base.len().checked_add(suffix.len()).ok_or_else(bad)?;
    let bytes = count
        .checked_mul(std::mem::size_of::<Path<'_>>())
        .and_then(|n| n.checked_add(std::mem::size_of::<Vec<Path<'_>>>()))
        .ok_or_else(bad)?;
    common.charge(count as u64, bytes as u64).map_err(error)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut items = Vec::new();
    items.try_reserve_exact(count).map_err(|_| bad())?;
    if items.capacity() > count {
        return Err(bad());
    }
    items.extend_from_slice(base);
    for step in suffix {
        items.push(match step {
            Step::Name(name) => Path::Field(name),
            Step::Index(index) => Path::Index(usize::try_from(*index).map_err(|_| bad())?),
        })
    }
    Ok(PaidPath { items, _fee: fee })
}
fn full_path<'a>(
    root: &'a str,
    member: Option<&'a str>,
    patch: Option<&'a Patch>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidPath<'a>, BodyMutationError> {
    let base = [
        Path::Field("cognition"),
        Path::Field(root),
        Path::Field(member.unwrap_or("")),
    ];
    path_from_base_v1(
        &base[..if member.is_some() { 3 } else { 2 }],
        patch.map_or(&[], |patch| patch.path()),
        common,
    )
}

pub(in crate::world) fn presence(
    columns: &mut Column,
    root: &str,
    member: Option<&str>,
    present: bool,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let memory = common.memory().clone();
    let path = full_path(root, member, None, common)?;
    if !present {
        if select(columns, &path.items, common)?.is_some() {
            *columns = columns
                .with_path_data_v1(&path.items, None, &memory, &mut |s, b| common.charge(s, b))
                .map_err(graph)?;
        }
    } else if member.is_some() && select(columns, &path.items[..2], common)?.is_none() {
        // An actual present child establishes its original envelope object.
        // This is not a missing-source/default data fallback.
        let object = value(b"{}", common)?;
        *columns = columns
            .with_path_data_v1(&path.items[..2], Some(&object), &memory, &mut |s, b| {
                common.charge(s, b)
            })
            .map_err(graph)?;
    }
    Ok(())
}
pub(in crate::world) fn apply(
    columns: &mut Column,
    root: &str,
    member: Option<&str>,
    patch: &Patch,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let path = full_path(root, member, Some(patch), common)?;
    apply_at_path_v1(columns, &path.items, patch.value(), common)
}
pub(super) fn apply_at_path_v1(
    columns: &mut Column,
    path: &[Path<'_>],
    change: &Change,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let memory = common.memory().clone();
    let selected = match change {
        Change::Remove => None,
        Change::Scalar(json) => Some(value(json, common)?),
        Change::Container {
            array,
            length,
            reset,
        } => {
            let previous = if *reset {
                None
            } else {
                select(columns, path, common)?
            };
            let mut selected = match previous {
                Some(previous) => previous,
                None => value(if *array { b"[]" } else { b"{}" }, common)?,
            };
            if *array {
                let mut before = selected
                    .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
                    .map_err(graph)?;
                let after = usize::try_from(*length).map_err(|_| bad())?;
                while before > after {
                    selected = selected
                        .with_path_data_v1(
                            &[Path::Index(before - 1)],
                            None,
                            &memory,
                            &mut |s, b| common.charge(s, b),
                        )
                        .map_err(graph)?;
                    before -= 1;
                }
            } else {
                selected
                    .object_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
                    .map_err(graph)?;
            }
            Some(selected)
        }
    };
    // A container header has already truncated removed array suffixes. The
    // original ordered remove patches may therefore refer to an absent tail.
    if selected.is_none() && select(columns, path, common)?.is_none() {
        return Ok(());
    }
    *columns = columns
        .with_path_data_v1(path, selected.as_ref(), &memory, &mut |s, b| {
            common.charge(s, b)
        })
        .map_err(graph)?;
    Ok(())
}
pub(in crate::world) fn check(
    columns: &Column,
    root: &str,
    member: Option<&str>,
    patch: &Patch,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let path = full_path(root, member, Some(patch), common)?;
    check_at_path_v1(columns, &path.items, patch.value(), common)
}
pub(super) fn check_at_path_v1(
    columns: &Column,
    path: &[Path<'_>],
    change: &Change,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let Change::Container { array, length, .. } = change else {
        return Ok(());
    };
    let selected = select(columns, path, common)?.ok_or_else(bad)?;
    let memory = common.memory().clone();
    let actual = if *array {
        selected
            .array_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
            .map_err(graph)?
    } else {
        selected
            .object_len_data_v1(&memory, &mut |s, b| common.charge(s, b))
            .map_err(graph)?
    };
    if u64::try_from(actual).map_err(|_| bad())? != *length {
        return Err(bad());
    }
    Ok(())
}
pub(in crate::world) fn fold_current_cognition_state_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    source: &PaidCognitionStateProjectionAuxV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    source.validate(tx, common, live)?;
    let memory = common.memory().clone();
    let mut columns = before
        .share_data(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph)?;
    let common = RefCell::new(common);
    let pay = |s, b| common.borrow_mut().charge(s, b).map_err(error);
    source
        .source()
        .visit_json_presence_with_work_v1(pay, |root, member, present| {
            presence(
                &mut columns,
                root,
                member,
                present,
                &mut common.borrow_mut(),
            )
        })?;
    source
        .source()
        .visit_json_paths_with_work_v1(pay, |root, member, patch| {
            apply(&mut columns, root, member, patch, &mut common.borrow_mut())
        })?;
    source
        .source()
        .visit_json_paths_with_work_v1(pay, |root, member, patch| {
            check(&columns, root, member, patch, &mut common.borrow_mut())
        })?;
    source
        .source()
        .visit_controls_with_work_v1(pay, |root, member, json| {
            presence(
                &mut columns,
                root,
                member,
                json.is_some(),
                &mut common.borrow_mut(),
            )?;
            if let Some(json) = json {
                let path = full_path(root, member, None, &mut common.borrow_mut())?;
                let selected = value(json, &mut common.borrow_mut())?;
                columns = columns
                    .with_path_data_v1(&path.items, Some(&selected), &memory, &mut |s, b| {
                        common.borrow_mut().charge(s, b)
                    })
                    .map_err(graph)?;
            }
            Ok(())
        })?;
    source.validate(tx, &mut common.borrow_mut(), live)?;
    Ok(columns)
}
fn proof_path(root: &str) -> (&str, Option<&str>) {
    match root {
        "delegation.receipts" => ("agent_delegation", Some("receipts")),
        "service_causal_proposals" => ("service_causal_proposals", None),
        "action_digests" => ("action_digests", None),
        _ => (root, None),
    }
}
pub(in crate::world) fn fold_current_cognition_proof_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    source: &PaidCognitionProofProjectionAuxV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    source.validate(tx, common, live)?;
    let memory = common.memory().clone();
    let mut columns = before
        .share_data(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph)?;
    let common = RefCell::new(common);
    let pay = |s, b| common.borrow_mut().charge(s, b).map_err(error);
    source.visit_presence_with_work_v1(pay, |root, present| {
        let (root, member) = proof_path(root);
        presence(
            &mut columns,
            root,
            member,
            present,
            &mut common.borrow_mut(),
        )
    })?;
    source.visit_paths_with_work_v1(pay, |root, patch| {
        let (root, member) = proof_path(root);
        apply(&mut columns, root, member, patch, &mut common.borrow_mut())
    })?;
    source.visit_paths_with_work_v1(pay, |root, patch| {
        let (root, member) = proof_path(root);
        check(&columns, root, member, patch, &mut common.borrow_mut())
    })?;
    source.validate(tx, &mut common.borrow_mut(), live)?;
    Ok(columns)
}
