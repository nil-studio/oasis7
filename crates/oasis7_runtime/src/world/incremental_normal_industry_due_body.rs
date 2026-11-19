//! The original completion body's exact move projection. This is correlation
//! data only; the Current completion committer must independently retain and
//! validate its pending source before minting the counted publication owner.
use crate::{DomainEvent, RecipeJobState, WorldEventBody};

pub(in crate::world) fn recipe_completion_body_from_job_v1(job: RecipeJobState) -> WorldEventBody {
    WorldEventBody::Domain(DomainEvent::RecipeCompleted {
        committed_recipe_origin: job.committed_recipe_origin,
        job_id: job.job_id,
        requester_agent_id: job.requester_agent_id,
        factory_id: job.factory_id,
        recipe_id: job.recipe_id,
        accepted_batches: job.accepted_batches,
        produce: job.produce,
        byproducts: job.byproducts,
        output_ledger: job.output_ledger,
        bottleneck_tags: job.bottleneck_tags,
        logistics_route_ids: job.logistics_route_ids,
        logistics_path_ids: job.logistics_path_ids,
    })
}
