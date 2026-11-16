//! Fixed visible-state composition over actual readonly columns. This module
//! deliberately cannot construct a qualified projection commitment or a World.
use super::*;
use oasis7_world_projection::current_commitment::{
    ProjectionColumnDataV1, ProjectionColumnErrorV1, project_agent_cells_data_v1,
    project_claim_sequences_data_v1, project_location_authorities_data_v1,
    project_pending_builds_data_v1, project_site_authorities_data_v1,
};
fn column_error<E: std::error::Error + Send + Sync + 'static>(
    error: ProjectionColumnErrorV1<E>,
) -> BodyMutationError {
    match error {
        ProjectionColumnErrorV1::Work(error) => paid(error),
        ProjectionColumnErrorV1::Memory(error) => ValueTreeError::Memory(error).into(),
        ProjectionColumnErrorV1::Invalid(note) => invalid(note),
    }
}
/// The state serializer input is observation DATA. Source binding, permission,
/// terminal-task selection and root publication remain in the actual producer.
/// `actual` must preserve the original top-level emitter's absent/skip bits;
/// field existence in WORLD_STATE_FIELDS is not an emitted-value substitute.
pub(super) fn compose_non_rule_visible_state_data_v1(
    actual: &ProjectionColumnDataV1,
    static_default_schema: &ProjectionColumnDataV1,
    reader: Option<&str>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ProjectionColumnDataV1, BodyMutationError> {
    let memory = common.memory().clone();
    let mut work = |steps, bytes| common.charge(steps, bytes);
    let mut visible = static_default_schema
        .share_data(&memory, &mut work)
        .map_err(column_error)?;
    for field in VISIBLE_STATE_COLUMNS_V1 {
        if *field == "world_rules" {
            continue;
        }
        let Some(value) = actual
            .object_member_data(field, &memory, &mut work)
            .map_err(column_error)?
        else {
            visible = visible
                .with_object_member_data(field, None, &memory, &mut work)
                .map_err(column_error)?;
            continue;
        };
        let value = value.share_data(&memory, &mut work).map_err(column_error)?;
        let after = match *field {
            "agents" => project_agent_cells_data_v1(&value, reader, &memory, &mut work),
            "agent_location_authorities" => {
                project_location_authorities_data_v1(&value, reader, &memory, &mut work)
            }
            "factory_site_authorities" => {
                project_site_authorities_data_v1(&value, reader, &memory, &mut work)
            }
            "pending_factory_builds" => {
                project_pending_builds_data_v1(&value, reader, &memory, &mut work)
            }
            "agent_claim_acquisition_sequences" => {
                let claims = actual
                    .object_member_data("agent_claims", &memory, &mut work)
                    .map_err(column_error)?
                    .ok_or_else(|| invalid("projection actual claims emitter missing"))?
                    .share_data(&memory, &mut work)
                    .map_err(column_error)?;
                project_claim_sequences_data_v1(&value, &claims, &memory, &mut work)
            }
            _ => Ok(value),
        }
        .map_err(column_error)?;
        // Unlike the other masked maps, the original State serializer omits
        // acquisition sequences after the formatter's claims-membership retain.
        // Inspect actual resulting DATA; never reuse the unfiltered emission bit.
        if *field == "agent_claim_acquisition_sequences" {
            let mut any = false;
            after
                .root_data()
                .object_members_data(&memory, &mut work, &mut |_, _, work| {
                    work(1, 0).map_err(ProjectionColumnErrorV1::Work)?;
                    any = true;
                    Ok(())
                })
                .map_err(column_error)?;
            if !any {
                visible = visible
                    .with_object_member_data(field, None, &memory, &mut work)
                    .map_err(column_error)?;
                continue;
            }
        }
        visible = visible
            .with_object_member_data(field, Some(&after), &memory, &mut work)
            .map_err(column_error)?;
    }
    Ok(visible)
}
