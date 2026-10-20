//! Native industrial commitments keep their finite destination through settlement.
use super::{industrial_work_admission::industrial_owner_binding, invalid};
use crate::{RecipeJobState, WorldError, WorldState};

pub fn validate_industrial_completion(
    state: &WorldState,
    job: &RecipeJobState,
) -> Result<(), WorldError> {
    let Some(root_ref) = &job.industrial_root_ref else {
        if state.world_rules.as_ref().is_some_and(|rules| {
            rules
                .industrial_work
                .roots
                .values()
                .any(|root| root.started_job_id == Some(job.job_id))
        }) {
            return Err(invalid("industrial_completion_job_link_missing"));
        }
        return Ok(());
    };
    let rules = state
        .world_rules
        .as_ref()
        .ok_or_else(|| invalid("industrial_completion_graph_missing"))?;
    let root = rules
        .industrial_work
        .roots
        .get(&root_ref.id)
        .filter(|root| root.root_ref == *root_ref && root.started_job_id == Some(job.job_id))
        .ok_or_else(|| invalid("industrial_completion_root_missing"))?;
    if rules
        .industrial_work
        .roots
        .values()
        .any(|other| other.started_job_id == Some(job.job_id) && other.root_ref != *root_ref)
    {
        return Err(invalid("industrial_completion_job_identity_conflict"));
    }
    if !root.first_sink
        || root.slots.is_empty()
        || root.slots.values().any(|slot| !slot.consumed)
        || root.accepted_batches != job.accepted_batches
        || root.owner_id != job.requester_agent_id
        || root.factory_id != job.factory_id
        || root.pinned_recipe_id != job.recipe_id
        || root.output_ledger != job.output_ledger
        || root.owner_binding_digest != industrial_owner_binding(state, &job.requester_agent_id)?
        || rules
            .industrial_work
            .dispositions
            .contains_key(&root.root_ref.id)
    {
        return Err(invalid("industrial_completion_authority_unavailable"));
    }
    super::industrial_work_admission::industrial_ledger_access(
        state,
        &root.owner_id,
        &root.output_ledger,
        state.time,
    )?;
    let factory = state
        .factories
        .get(&root.factory_id)
        .ok_or_else(|| invalid("industrial_completion_factory_missing"))?;
    if factory.builder_agent_id != root.owner_id
        || factory.output_ledger != root.output_ledger
        || state.retired_factory_ids.contains(&root.factory_id)
    {
        return Err(invalid("industrial_completion_destination_changed"));
    }
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
