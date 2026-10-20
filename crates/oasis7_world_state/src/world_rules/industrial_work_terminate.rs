//! Explicit declared termination of an already committed native production job.
use super::industrial_work::{apply_disposition, industrial_ref, record_receipt};
use super::industrial_work_admission::industrial_owner_binding;
use super::{PreparedWorldRule, invalid};
use crate::{DomainEvent, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::*;

impl PreparedWorldRule {
    pub fn industrial_terminate_root(
        &mut self,
        state: &WorldState,
        actor: &str,
        root_ref: &IndustrialRefV1,
        now: WorldTime,
        world: &str,
    ) -> Result<String, WorldError> {
        let owner_binding = industrial_owner_binding(state, actor)?;
        let root = self
            .rules
            .industrial_work
            .roots
            .get(&root_ref.id)
            .filter(|root| {
                root.root_ref == *root_ref
                    && root_ref.world_binding == world
                    && root.owner_id == actor
                    && root.owner_binding_digest == owner_binding
                    && root.first_sink
                    && root
                        .allowed_dispositions
                        .contains(&IndustrialDispositionV1::Terminate)
            })
            .ok_or_else(|| invalid("industrial_termination_not_authorized"))?
            .clone();
        let job_id = root
            .started_job_id
            .ok_or_else(|| invalid("industrial_termination_job_missing"))?;
        let pending = state
            .pending_recipe_jobs
            .get(&job_id)
            .filter(|job| {
                job.industrial_root_ref.as_ref() == Some(root_ref)
                    && job.requester_agent_id == actor
                    && job.factory_id == root.factory_id
                    && job.recipe_id == root.pinned_recipe_id
                    && job.accepted_batches == root.accepted_batches
            })
            .ok_or_else(|| invalid("industrial_termination_job_not_pending"))?;
        if self
            .rules
            .industrial_work
            .dispositions
            .contains_key(&root_ref.id)
        {
            return Err(invalid("industrial_termination_already_disposed"));
        }
        let plan_ref = industrial_ref(
            world,
            "industrial",
            "industrial-disposition/v1",
            &format!("terminate:{}", root_ref.id),
            1,
            &(root_ref, job_id, &root.allowed_dispositions),
        )?;
        let plan = DispositionPlanV1 {
            root_ref: root_ref.clone(),
            plan_ref: plan_ref.clone(),
            expected_causal_digest: root.causal_digest.clone(),
            disposition: IndustrialDispositionV1::Terminate,
        };
        apply_disposition(&mut self.rules.industrial_work, plan, world)?;
        let event = DomainEvent::FactoryProductionBlocked {
            action_id: job_id,
            requester_agent_id: actor.into(),
            factory_id: root.factory_id.clone(),
            recipe_id: root.pinned_recipe_id.clone(),
            blocker_kind: "industrial_termination".into(),
            blocker_detail: format!("declared root termination: {}", plan_ref.digest),
        };
        let prepared =
            crate::state::industry_transition::PreparedIndustryEvent::prepare(state, &event, now)?;
        let receipt_ref = industrial_ref(
            world,
            "industrial",
            "industrial-receipt/v1",
            &format!("terminate:{}", root_ref.id),
            1,
            &(root_ref, &event, &pending.consume, pending.power_required),
        )?;
        record_receipt(
            &mut self.rules.industrial_work,
            IndustrialReceiptV1 {
                receipt_ref,
                root_ref: root_ref.clone(),
                child_ref: plan_ref,
                effect_role: "production_termination".into(),
                payload_digest: crate::util::hash_json(&event)?,
                input: root
                    .slots
                    .values()
                    .flat_map(|slot| slot.parents.clone())
                    .collect(),
                output: vec![],
            },
            world,
        )?;
        self.industry = Some(prepared);
        Ok(root_ref.id.clone())
    }
}
