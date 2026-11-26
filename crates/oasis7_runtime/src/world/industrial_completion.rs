use super::World;
use crate::{DomainEvent, RecipeJobState, WorldEventBody};

pub(super) enum IndustrialCompletionGuard {
    Ready,
    Blocked(Option<Box<WorldEventBody>>),
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
    pub(super) fn industrial_completion_guard(
        &self,
        job: &RecipeJobState,
    ) -> IndustrialCompletionGuard {
        let Err(error) = crate::world_rules::validate_industrial_completion(self.state(), job)
        else {
            return IndustrialCompletionGuard::Ready;
        };
        let detail = format!("{error:?}");
        let unchanged = self
            .state()
            .factories
            .get(&job.factory_id)
            .is_some_and(|factory| {
                factory.production.current_blocker_kind.as_deref() == Some("industrial_terminal")
                    && factory.production.current_blocker_detail.as_deref() == Some(&detail)
            });
        IndustrialCompletionGuard::Blocked((!unchanged).then(|| {
            Box::new(WorldEventBody::Domain(
                DomainEvent::FactoryProductionBlocked {
                    action_id: job.job_id,
                    requester_agent_id: job.requester_agent_id.clone(),
                    factory_id: job.factory_id.clone(),
                    recipe_id: job.recipe_id.clone(),
                    blocker_kind: "industrial_terminal".into(),
                    blocker_detail: detail,
                },
            ))
        }))
    }

    pub(super) fn install_staged_industrial<T: super::transition::StagedTransition>(
        &mut self,
        transition: &T,
        caused_by: Option<crate::CausedBy>,
    ) -> Result<(), crate::WorldError> {
        use super::transition::{
            BorrowedWorldView, DurableCommitConfirmation, TransitionContext, TransitionReject,
        };
        let commit = {
            let view = BorrowedWorldView::new(self).map_err(TransitionReject::into_world_error)?;
            let ctx = TransitionContext::new(self.state.time, caused_by);
            transition.prepare(&view, &ctx).map_err(TransitionReject::into_world_error)?
        };
        commit
            .install_confirmed_unchecked_head(self, DurableCommitConfirmation::Confirmed)
            .map_err(TransitionReject::into_world_error)
    }

    pub(super) fn install_staged_industrial_completion(
        &mut self,
        job: &RecipeJobState,
    ) -> Result<Option<crate::WorldEvent>, crate::WorldError> {
        let prev = self.next_event_id;
        self.install_staged_industrial(&super::industrial::staged_writes::StagedIndustrialCompletion { job }, None)?;
        Ok((self.next_event_id > prev).then(|| self.journal.events.last().cloned()).flatten())
    }

    pub(super) fn install_action_or_factory_event(
        &mut self,
        action_id: crate::ActionId,
        action: &crate::Action,
        event_body: WorldEventBody,
        caused_by: Option<crate::CausedBy>,
    ) -> Result<crate::WorldEventId, crate::WorldError> {
        if let crate::Action::BuildFactory { builder_agent_id, site_id, spec } = action {
            self.install_staged_industrial(
                &super::industrial::staged_writes::StagedFactoryConstruction {
                    action_id, builder_agent_id, site_id, spec, validate_materials: true, module_id: None,
                },
                caused_by,
            )?;
            return self.journal.events.last().map(|e| e.id).ok_or_else(|| {
                crate::WorldError::ResourceBalanceInvalid { reason: "missing factory construction event".into() }
            });
        }
        self.append_event(event_body, caused_by)
    }

    pub(super) fn process_due_industrial_completions(
        &mut self,
    ) -> Result<Vec<crate::WorldEvent>, crate::WorldError> {
        let now = self.state.time;
        let mut due_builds: Vec<_> = self.state.pending_factory_builds.values().filter(|j| j.ready_at <= now).cloned().collect();
        due_builds.sort_by_key(|j| (self.production_priority_for_factory_job(j) as u8, j.ready_at, j.job_id));
        let mut emitted = Vec::with_capacity(due_builds.len());
        for job in due_builds {
            self.append_event(WorldEventBody::Domain(DomainEvent::FactoryBuilt {
                job_id: job.job_id, builder_agent_id: job.builder_agent_id, site_id: job.site_id, spec: job.spec,
            }), None)?;
            if let Some(event) = self.journal.events.last() { emitted.push(event.clone()); }
        }
        let binding = self.world_rule_binding();
        let mut due_recipes: Vec<_> = self.state.pending_recipe_jobs.values().filter(|j| {
            j.ready_at <= now && j.emergency_power_world_binding.as_ref().is_none_or(|b| binding.as_ref() == Some(b))
        }).cloned().collect();
        due_recipes.sort_by_key(|j| (self.production_priority_for_recipe_job(j) as u8, j.ready_at, j.job_id));
        for job in due_recipes {
            if let Some(event) = self.install_staged_industrial_completion(&job)? { emitted.push(event); }
        }
        Ok(emitted)
    }
}
