use super::World;
use crate::{DomainEvent, RecipeJobState, WorldEventBody};

pub(super) enum IndustrialCompletionGuard {
    Ready,
    Blocked(Option<WorldEventBody>),
}

impl World {
    pub(super) fn recipe_completion_body(job: RecipeJobState) -> WorldEventBody {
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
    pub(super) fn industrial_completion_guard(&self, job: &RecipeJobState) -> IndustrialCompletionGuard {
        let Err(error) = crate::world_rules::validate_industrial_completion(self.state(), job) else {
            return IndustrialCompletionGuard::Ready;
        };
        let detail = format!("{error:?}");
        let unchanged = self.state().factories.get(&job.factory_id).is_some_and(|factory| {
            factory.production.current_blocker_kind.as_deref() == Some("industrial_terminal")
                && factory.production.current_blocker_detail.as_deref() == Some(&detail)
        });
        IndustrialCompletionGuard::Blocked((!unchanged).then(|| WorldEventBody::Domain(
            DomainEvent::FactoryProductionBlocked {
                action_id: job.job_id,
                requester_agent_id: job.requester_agent_id.clone(),
                factory_id: job.factory_id.clone(),
                recipe_id: job.recipe_id.clone(),
                blocker_kind: "industrial_terminal".into(),
                blocker_detail: detail,
            }
        )))
    }
}
