//! The remaining actual Runtime families use the same strict paid path kernel
//! as Cognition. The source chooses fixed original fields and typed paths; this
//! consumer cannot accept caller-provided permission or replacement roots.
use super::cognition::{apply_at_path_v1, check_at_path_v1, path_from_base_v1};
use super::fold::{graph, select, value};
use super::*;
use oasis7_world_state::observation_projection::current_commitment::{
    ProjectionColumnDataV1 as Column, ProjectionDataPathRefV1 as Path,
};
use oasis7_world_state::{
    incremental_cognition_projection_aux::CognitionAuxJsonPatchValueV1 as Change,
    incremental_runtime_projection_aux::{CurrentRuntimeProjectionAuxV1, RuntimeAuxPatchRefV1},
};
use std::cell::RefCell;

pub(super) fn fold_current_runtime_fields_projection_v1<M: MerkleNodeReader>(
    before: &Column,
    source: &CurrentRuntimeProjectionAuxV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<Column, BodyMutationError> {
    source
        .validate(tx, common.memory(), live)
        .map_err(|error| match error {
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                e,
            ) => e.into(),
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
                e.into()
            }
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e) => {
                BodyMutationError::Original(Box::new(e))
            }
        })?;
    let memory = common.memory().clone();
    let mut columns = before
        .share_data(&memory, &mut |s, b| common.charge(s, b))
        .map_err(graph)?;
    let common = RefCell::new(common);
    let pay = |s, b| common.borrow_mut().charge(s, b).map_err(error);
    source.visit_with_work_v1(pay, |field, prefix, patch| {
        let base = path_from_base_v1(&[Path::Field(field)], prefix, &mut common.borrow_mut())?;
        match patch {
            RuntimeAuxPatchRefV1::Json(json) => {
                let selected = json
                    .map(|json| value(json, &mut common.borrow_mut()))
                    .transpose()?;
                if selected.is_none()
                    && select(&columns, &base.items, &mut common.borrow_mut())?.is_none()
                {
                    return Ok(());
                }
                columns = columns
                    .with_path_data_v1(&base.items, selected.as_ref(), &memory, &mut |s, b| {
                        common.borrow_mut().charge(s, b)
                    })
                    .map_err(graph)?;
            }
            RuntimeAuxPatchRefV1::Container {
                array,
                length,
                reset,
            } => {
                apply_at_path_v1(
                    &mut columns,
                    &base.items,
                    &Change::Container {
                        array,
                        length,
                        reset,
                    },
                    &mut common.borrow_mut(),
                )?;
            }
            RuntimeAuxPatchRefV1::Deep(patch) => {
                let path = path_from_base_v1(&base.items, patch.path(), &mut common.borrow_mut())?;
                apply_at_path_v1(
                    &mut columns,
                    &path.items,
                    patch.value(),
                    &mut common.borrow_mut(),
                )?;
            }
        }
        Ok(())
    })?;
    // Headers are checked after every actual row/path change. A missing row,
    // ordinal gap or accidental stale member cannot silently preserve a root.
    source.visit_with_work_v1(pay, |field, prefix, patch| {
        let base = path_from_base_v1(&[Path::Field(field)], prefix, &mut common.borrow_mut())?;
        match patch {
            RuntimeAuxPatchRefV1::Container {
                array,
                length,
                reset,
            } => check_at_path_v1(
                &columns,
                &base.items,
                &Change::Container {
                    array,
                    length,
                    reset,
                },
                &mut common.borrow_mut(),
            ),
            RuntimeAuxPatchRefV1::Deep(patch) => {
                let path = path_from_base_v1(&base.items, patch.path(), &mut common.borrow_mut())?;
                check_at_path_v1(
                    &columns,
                    &path.items,
                    patch.value(),
                    &mut common.borrow_mut(),
                )
            }
            RuntimeAuxPatchRefV1::Json(_) => Ok(()),
        }
    })?;
    source
        .validate(tx, common.borrow().memory(), live)
        .map_err(|error| match error {
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                e,
            ) => e.into(),
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
                e.into()
            }
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e) => {
                BodyMutationError::Original(Box::new(e))
            }
        })?;
    Ok(columns)
}
