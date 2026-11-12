//! Native industrial commitments keep their finite destination through settlement.
use super::{industrial_work_admission::industrial_owner_binding, invalid};
use crate::{RecipeJobState, WorldError, WorldState};

pub fn validate_industrial_completion(
    state: &WorldState,
    job: &RecipeJobState,
) -> Result<(), WorldError> {
    let Some(root) = validate_industrial_completion_root_v1(
        state
            .world_rules
            .as_ref()
            .map(|rules| &rules.industrial_work),
        job,
    )?
    else {
        return Ok(());
    };
    let binding = industrial_owner_binding(state, &job.requester_agent_id)?;
    let graph = &state
        .world_rules
        .as_ref()
        .ok_or_else(|| invalid("industrial_completion_graph_missing"))?
        .industrial_work;
    validate_industrial_completion_owner_v1(graph, root, &binding)?;
    super::industrial_work_admission::industrial_ledger_access(
        state,
        &root.owner_id,
        &root.output_ledger,
        state.time,
    )?;
    validate_industrial_completion_destination_v1(
        root,
        state.factories.get(&root.factory_id),
        state.retired_factory_ids.contains(&root.factory_id),
    )?;
    let plan = crate::builtin_recipe_plan(&root.pinned_recipe_id, root.accepted_batches)
        .ok_or_else(|| invalid("industrial_completion_recipe_unavailable"))?;
    if plan.produce != job.produce || plan.byproducts != job.byproducts {
        return Err(invalid("industrial_completion_plan_conflict"));
    }
    super::industrial_work_start::validate_industrial_output_capacity(
        state,
        root,
        &plan,
        Some(job.job_id),
    )
}

/// Fixed pre-owner predicates from the original completion validator. The
/// returned reference is data, not a qualified graph/terminal capability.
/// Its caller must bind the supplied graph and job to actual Current holders.
pub fn validate_industrial_completion_root_v1<'a>(
    graph: Option<&'a oasis7_agent_api::world_action::IndustrialWorkStateV1>,
    job: &RecipeJobState,
) -> Result<Option<&'a oasis7_agent_api::world_action::RootOperationV1>, WorldError> {
    let Some(root_ref) = &job.industrial_root_ref else {
        if graph.is_some_and(|graph| {
            graph
                .roots
                .values()
                .any(|root| root.started_job_id == Some(job.job_id))
        }) {
            return Err(invalid("industrial_completion_job_link_missing"));
        }
        return Ok(None);
    };
    let graph = graph.ok_or_else(|| invalid("industrial_completion_graph_missing"))?;
    let root = graph
        .roots
        .get(&root_ref.id)
        .filter(|root| root.root_ref == *root_ref && root.started_job_id == Some(job.job_id))
        .ok_or_else(|| invalid("industrial_completion_root_missing"))?;
    if graph
        .roots
        .values()
        .any(|other| other.started_job_id == Some(job.job_id) && other.root_ref != *root_ref)
    {
        return Err(invalid("industrial_completion_job_identity_conflict"));
    }
    validate_industrial_completion_selected_fields_v1(root, job)?;
    Ok(Some(root))
}
/// The owner hash remains computed by the original caller at the original
/// short-circuit point; this fixed predicate cannot be replaced by an Ops impl.
pub fn validate_industrial_completion_owner_v1(
    graph: &oasis7_agent_api::world_action::IndustrialWorkStateV1,
    root: &oasis7_agent_api::world_action::RootOperationV1,
    binding: &str,
) -> Result<(), WorldError> {
    if root.owner_binding_digest != binding || graph.dispositions.contains_key(&root.root_ref.id) {
        return Err(invalid("industrial_completion_authority_unavailable"));
    }
    Ok(())
}

/// Original destination check over one actual selected factory. It is pure
/// data validation; the caller must retain its Current factory/Q provenance.
pub fn validate_industrial_completion_destination_v1(
    root: &oasis7_agent_api::world_action::RootOperationV1,
    factory: Option<&crate::FactoryState>,
    retired: bool,
) -> Result<(), WorldError> {
    let factory = factory.ok_or_else(|| invalid("industrial_completion_factory_missing"))?;
    if factory.builder_agent_id != root.owner_id
        || factory.output_ledger != root.output_ledger
        || retired
    {
        return Err(invalid("industrial_completion_destination_changed"));
    }
    Ok(())
}

/// Sole original selected-root business predicate. The actual directory and
/// selected Current reader remain responsible for lookup/link provenance.
pub fn validate_industrial_completion_selected_fields_v1(
    root: &oasis7_agent_api::world_action::RootOperationV1,
    job: &RecipeJobState,
) -> Result<(), WorldError> {
    if !root.first_sink
        || root.slots.is_empty()
        || root.slots.values().any(|slot| !slot.consumed)
        || root.accepted_batches != job.accepted_batches
        || root.owner_id != job.requester_agent_id
        || root.factory_id != job.factory_id
        || root.pinned_recipe_id != job.recipe_id
        || root.output_ledger != job.output_ledger
    {
        return Err(invalid("industrial_completion_authority_unavailable"));
    }
    Ok(())
}
