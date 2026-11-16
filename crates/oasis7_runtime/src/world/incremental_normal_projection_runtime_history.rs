//! Append the four actual Cognition History originals to their cached arrays.
//! Missing arrays are created only from an authenticated present empty prefix.
use super::fold::{graph, select, value};
use super::*;
use crate::world::incremental_cognition_projection_aux_state::PaidCognitionHistoryProjectionAuxV1;
use oasis7_world_state::incremental_cognition_history::CognitionHistoryFieldV1 as Field;
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1 as Column, ProjectionDataPathRefV1 as Path,
};
use std::cell::RefCell;
fn path(field: Field) -> ([Path<'static>; 3], usize) {
    match field {
        Field::JournalEvents => (
            [
                Path::Field("cognition"),
                Path::Field("cognition_journal"),
                Path::Field("events"),
            ],
            3,
        ),
        Field::Responses => (
            [
                Path::Field("cognition"),
                Path::Field("responses"),
                Path::Field(""),
            ],
            2,
        ),
        Field::EconomyJournal => (
            [
                Path::Field("cognition"),
                Path::Field("cognition_economy"),
                Path::Field("journal"),
            ],
            3,
        ),
        Field::EconomyProvisionJournal => (
            [
                Path::Field("cognition"),
                Path::Field("cognition_economy"),
                Path::Field("provision_journal"),
            ],
            3,
        ),
    }
}
pub(in crate::world) fn fold_current_cognition_history_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    source: &PaidCognitionHistoryProjectionAuxV1,
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
    source.visit_windows_with_work_v1(pay, |field, before, _, present| {
        let (slots, n) = path(field);
        let selected = select(&columns, &slots[..n], &mut common.borrow_mut())?;
        if !present {
            if before != 0 {
                return Err(bad());
            }
            if selected.is_some() {
                columns = columns
                    .with_path_data_v1(&slots[..n], None, &memory, &mut |s, b| {
                        common.borrow_mut().charge(s, b)
                    })
                    .map_err(graph)?;
            }
            return Ok(());
        }
        if let Some(selected) = selected {
            if u64::try_from(
                selected
                    .array_len_data_v1(&memory, &mut |s, b| common.borrow_mut().charge(s, b))
                    .map_err(graph)?,
            )
            .map_err(|_| bad())?
                != before
            {
                return Err(bad());
            }
        } else {
            if before != 0 {
                return Err(bad());
            }
            if n == 3 && select(&columns, &slots[..2], &mut common.borrow_mut())?.is_none() {
                let envelope = value(b"{}", &mut common.borrow_mut())?;
                columns = columns
                    .with_path_data_v1(&slots[..2], Some(&envelope), &memory, &mut |s, b| {
                        common.borrow_mut().charge(s, b)
                    })
                    .map_err(graph)?;
            }
            let empty = value(b"[]", &mut common.borrow_mut())?;
            columns = columns
                .with_path_data_v1(&slots[..n], Some(&empty), &memory, &mut |s, b| {
                    common.borrow_mut().charge(s, b)
                })
                .map_err(graph)?;
        }
        Ok(())
    })?;
    source.visit_records_with_work_v1(pay, |field, ordinal, json| {
        let (slots, n) = path(field);
        let selected = select(&columns, &slots[..n], &mut common.borrow_mut())?.ok_or_else(bad)?;
        let len = selected
            .array_len_data_v1(&memory, &mut |s, b| common.borrow_mut().charge(s, b))
            .map_err(graph)?;
        if u64::try_from(len).map_err(|_| bad())? != ordinal {
            return Err(bad());
        }
        let row = value(json, &mut common.borrow_mut())?;
        let selected = selected
            .with_array_append_data_v1(&row, &memory, &mut |s, b| common.borrow_mut().charge(s, b))
            .map_err(graph)?;
        columns = columns
            .with_path_data_v1(&slots[..n], Some(&selected), &memory, &mut |s, b| {
                common.borrow_mut().charge(s, b)
            })
            .map_err(graph)?;
        Ok(())
    })?;
    source.visit_windows_with_work_v1(pay, |field, _, after, present| {
        let (slots, n) = path(field);
        let selected = select(&columns, &slots[..n], &mut common.borrow_mut())?;
        if !present {
            return if selected.is_none() && after == 0 {
                Ok(())
            } else {
                Err(bad())
            };
        }
        let selected = selected.ok_or_else(bad)?;
        let len = selected
            .array_len_data_v1(&memory, &mut |s, b| common.borrow_mut().charge(s, b))
            .map_err(graph)?;
        if u64::try_from(len).map_err(|_| bad())? != after {
            return Err(bad());
        }
        Ok(())
    })?;
    source.validate(tx, &mut common.borrow_mut(), live)?;
    Ok(columns)
}
