//! Atomic bridge from qualified escrow parents to ordinary recipe production.
use super::industrial_work::{consume_bound_slot, industrial_ref, record_receipt};
use super::industrial_work_admission::{
    industrial_effective_consume, industrial_ledger_access, industrial_owner_binding,
};
use super::{PreparedWorldRule, invalid};
use crate::{DomainEvent, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::*;

/// Checks each material independently in its native integer unit. Pending
/// outputs on the exact destination ledger already occupy this finite runway.
pub fn validate_industrial_output_capacity(
    state: &WorldState,
    root: &RootOperationV1,
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    exclude_job: Option<u64>,
) -> Result<(), WorldError> {
    let result: Result<(), WorldError> =
        validate_industrial_output_capacity_from_records_with_work_v1(
            root,
            plan,
            exclude_job,
            state.material_ledgers.get(&root.output_ledger),
            state.pending_recipe_jobs.values(),
            &mut |_| Ok(()),
        );
    result
}

impl PreparedWorldRule {
    #[allow(clippy::too_many_arguments)]
    pub fn industrial_start_root(
        &mut self,
        state: &WorldState,
        actor: &str,
        root_ref: &IndustrialRefV1,
        expected_profile_digest: &str,
        route_ids: &[String],
        path_ids: &[String],
        now: WorldTime,
        world: &str,
    ) -> Result<String, WorldError> {
        let selected = match super::readonly_industrial_start::prepare_inputs(
            state,
            &super::readonly_industrial_start::ResidentStartRowsV1,
            &super::readonly_parent_consumption::LegacyParentWorkV1,
            &mut self.rules.industrial_work,
            actor,
            root_ref,
            expected_profile_digest,
            now,
            world,
        ) {
            Ok(result) => result?,
            Err(error) => match error {},
        };
        let super::readonly_industrial_start::StartInputDataV1 {
            root,
            plan,
            consume,
            parents,
        } = selected;
        let profile = state
            .recipe_profiles
            .get(&root.pinned_recipe_id)
            .expect("validated recipe profile");
        let factory = state
            .factories
            .get(&root.factory_id)
            .expect("validated factory");
        let DomainEvent::WorldRuleCommitted { action_id, .. } = self.event else {
            return Err(invalid("industrial_start_event_required"));
        };
        self.rules
            .industrial_work
            .roots
            .get_mut(&root_ref.id)
            .expect("validated root")
            .started_job_id = Some(action_id);
        // Credit exists only on this private preparation clone. The ordinary
        // reducer immediately consumes exactly these certified escrow inputs.
        let mut temporary = state.clone();
        temporary.world_rules = Some(self.rules.clone());
        let ledger = temporary
            .material_ledgers
            .entry(factory.input_ledger.clone())
            .or_default();
        for input in &consume {
            let old = ledger.get(&input.kind).copied().unwrap_or(0);
            ledger.insert(
                input.kind.clone(),
                old.checked_add(input.amount)
                    .ok_or_else(|| invalid("industrial_start_quantity_overflow"))?,
            );
        }
        let ready_at = now
            .checked_add(u64::from(plan.duration_ticks))
            .ok_or_else(|| invalid("industrial_start_time_overflow"))?;
        let event = DomainEvent::RecipeStarted {
            job_id: action_id,
            requester_agent_id: actor.into(),
            factory_id: root.factory_id.clone(),
            recipe_id: root.pinned_recipe_id.clone(),
            accepted_batches: plan.accepted_batches,
            consume,
            produce: plan.produce,
            byproducts: plan.byproducts,
            power_required: plan.power_required,
            power_owner_agent_id: Some(actor.into()),
            duration_ticks: plan.duration_ticks,
            consume_ledger: factory.input_ledger.clone(),
            output_ledger: factory.output_ledger.clone(),
            bottleneck_tags: profile.bottleneck_tags.clone(),
            market_quotes: vec![],
            logistics_route_ids: route_ids.to_vec(),
            logistics_path_ids: path_ids.to_vec(),
            ready_at,
            emergency_power_obligation: None,
            committed_recipe_origin: None,
        };
        let mut prepared = crate::state::industry_transition::PreparedIndustryEvent::prepare(
            &temporary, &event, now,
        )?;
        prepared.bind_industrial_root(root_ref)?;
        if let Some(updated) = prepared.world_rules_projection() {
            self.rules = updated.clone();
        }
        let child = industrial_ref(
            world,
            "industrial",
            "industrial-start/v1",
            &format!("start:{}", root_ref.id),
            1,
            &(root_ref, &event),
        )?;
        let receipt_ref = industrial_ref(
            world,
            "industrial",
            "industrial-receipt/v1",
            &format!("production-start:{}", root_ref.id),
            1,
            &event,
        )?;
        record_receipt(
            &mut self.rules.industrial_work,
            IndustrialReceiptV1 {
                receipt_ref,
                root_ref: root_ref.clone(),
                child_ref: child,
                effect_role: "production_start".into(),
                payload_digest: crate::util::hash_json(&event)?,
                input: parents,
                output: vec![],
            },
            world,
        )?;
        self.industry = Some(prepared);
        Ok(root_ref.id.clone())
    }
}

/// Original capacity predicates over selected ledger plus the complete actual
/// pending-job iterator. This is only a data validation kernel; it does not
/// authenticate either source or mint an industrial permission.
/// Borrowed original output-reservation fields. This data view has no
/// constructor-based authority; actual Current/cache owners bind its source.
#[derive(Clone, Copy)]
pub struct IndustrialOutputReservationRefV1<'a> {
    pub job_id: u64,
    pub output_ledger: &'a crate::MaterialLedgerId,
    pub produce: &'a [crate::MaterialStack],
    pub byproducts: &'a [crate::MaterialStack],
}
impl<'a> From<&'a crate::RecipeJobState> for IndustrialOutputReservationRefV1<'a> {
    fn from(job: &'a crate::RecipeJobState) -> Self {
        Self {
            job_id: job.job_id,
            output_ledger: &job.output_ledger,
            produce: &job.produce,
            byproducts: &job.byproducts,
        }
    }
}
/// Compatibility entry delegates the exact original fields, without copying a
/// RecipeJob or manufacturing irrelevant fields for a metadata projection.
pub fn validate_industrial_output_capacity_from_records_with_work_v1<'a, E, I>(
    root: &RootOperationV1,
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    exclude_job: Option<u64>,
    ledger: Option<&std::collections::BTreeMap<String, i64>>,
    pending: I,
    before_allocation: &mut impl FnMut(&oasis7_wasm_abi::RecipeExecutionPlan) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<WorldError>,
    I: Iterator<Item = &'a crate::RecipeJobState> + Clone,
{
    validate_industrial_output_capacity_from_reservations_with_work_v1(
        root,
        plan,
        exclude_job,
        ledger,
        pending.map(IndustrialOutputReservationRefV1::from),
        before_allocation,
    )
}
pub fn validate_industrial_output_capacity_from_reservations_with_work_v1<'a, E, I>(
    root: &RootOperationV1,
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    exclude_job: Option<u64>,
    ledger: Option<&std::collections::BTreeMap<String, i64>>,
    pending: I,
    before_allocation: &mut impl FnMut(&oasis7_wasm_abi::RecipeExecutionPlan) -> Result<(), E>,
) -> Result<(), E>
where
    E: From<WorldError>,
    I: Iterator<Item = IndustrialOutputReservationRefV1<'a>> + Clone,
{
    before_allocation(plan)?;
    let result = super::readonly_output_capacity::capacity(
        root,
        plan,
        exclude_job,
        ledger,
        &mut |visit| {
            for job in pending.clone() {
                if visit(job)?.is_break() {
                    break;
                }
            }
            Ok::<(), E>(())
        },
        &mut |_, _, _| Ok::<(), E>(()),
    );
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(E::from(error)),
        Err(error) => Err(error),
    }
}

/// Ordinary admission bound for the one concrete output-aggregation map. The
/// legacy NOOP path need not execute any resource/layout arithmetic.
pub fn industrial_completion_output_map_heap_v1(
    plan: &oasis7_wasm_abi::RecipeExecutionPlan,
) -> Result<usize, oasis7_formal_protocol::incremental_value::ValueTreeError> {
    use oasis7_formal_protocol::incremental_value::ValueTreeError;
    let limited = || ValueTreeError::ResourceLimited("industrial completion output map");
    let entries = plan
        .produce
        .len()
        .checked_add(plan.byproducts.len())
        .ok_or_else(limited)?;
    let mut layout = std::mem::size_of::<std::collections::BTreeMap<String, u64>>()
        .checked_add(crate::incremental_typed_record_codec::btree_output_heap::<
            String,
            u64,
        >(entries)?)
        .ok_or_else(limited)?;
    for stack in plan.produce.iter().chain(&plan.byproducts) {
        layout = layout.checked_add(stack.kind.len()).ok_or_else(limited)?;
    }
    Ok(layout)
}
