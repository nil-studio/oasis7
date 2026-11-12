//! Complete original queue-priority observations over the actual industrial
//! Current sources. These scalar priorities are DATA, never a due permit.
use super::*;
use crate::world::economy::{
    ProductionQueuePriority,
    due_priority::{self, DuePriorityFactsV1},
};
use crate::world::event_processing::factory_schedule_fixed::FactoryRecipeFactsV1;
use current::{CurrentFactoryScheduleFactsV1, EvaluationErrorV1};
impl<M, P, R, V, E, C, PE, PC> due_priority::sealed::Sealed
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
}
impl<M, P, R, V, E, C, PE, PC> DuePriorityFactsV1
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    type Error = EvaluationErrorV1;
    fn reserve_tokens(&mut self, payload: usize, slots: usize) -> Result<(), Self::Error> {
        // Original vec![one/two] + extend uses geometric slot growth. Admit the
        // entire overlap of the original tiny vector and its final grown buffer.
        let n = slots
            .checked_mul(2)
            .and_then(|n| n.checked_add(8))
            .and_then(|n| n.checked_mul(size_of::<String>()))
            .and_then(|n| n.checked_add(payload))
            .ok_or_else(limited)?;
        self.fees.hold(n, self.common)?;
        Ok(())
    }
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.charge(n)
    }
    fn electricity_critical(&mut self) -> Result<bool, Self::Error> {
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, _work| {
                let guard = ctx.transaction.guard();
                Ok(oasis7_world_state::incremental_inventory_resource_overlay::read_current_world_resource_v1(oasis7_agent_api::world_action::ResourceKind::Electricity,ctx,&guard,live)?.unwrap_or(0))
            },
        )?;
        Ok(value <= crate::world::economy::critical_electricity_floor_for_due_v1())
    }
    fn hardware_critical(&mut self) -> Result<bool, Self::Error> {
        Ok(self.quantity(
            &oasis7_world_state::MaterialLedgerId::World,
            "hardware_part",
        )? <= crate::world::economy::critical_hardware_floor_for_due_v1())
    }
    fn product_role_priority(
        &mut self,
        kind: &str,
    ) -> Result<Option<ProductionQueuePriority>, Self::Error> {
        let profile = FactoryRecipeFactsV1::product_profile(self, kind)?;
        let Some(profile) = profile else {
            return Ok(None);
        };
        self.charge(profile.role_tag.len())?;
        self.fees.hold(profile.role_tag.len(), self.common)?;
        Ok(due_priority::priority_from_actual_role_tag_v1(
            &profile.role_tag,
        ))
    }
    fn bottleneck_pressure(&mut self, tags: &[String]) -> Result<bool, Self::Error> {
        for tag in tags {
            self.charge(tag.len())?;
            let sources = self.sources;
            let live = self.live;
            let total = super::io::paid(
                sources.roots,
                self.context,
                self.read,
                self.work,
                self.common,
                |ctx, parent, work| {
                    let guard = ctx.transaction.guard();
                    Ok(oasis7_world_state::incremental_inventory::sum_current_inventory_material_v1(tag,sources.inventory_directory,sources.lifecycle.inventory,sources.roots,ctx,parent,&mut work.inventory_family,&mut work.inventory_directory,&guard,live)?)
                },
            )?;
            if *total <= due_priority::actual_bottleneck_threshold_v1(tag) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub(in crate::world) enum DuePriorityJobV1<'a> {
    Build(&'a crate::FactoryBuildJobState),
    Recipe(&'a crate::RecipeJobState),
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn current_due_priority_v1<M, P, R, V, E, C, PE, PC>(
    job: DuePriorityJobV1<'_>,
    sources: &FactoryScheduleSourcesV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<ProductionQueuePriority, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    source_binding(sources, context.transaction, common)?;
    let mut fees = current::SourceFeesV1::new(work.max_source_records, common)?;
    let mut facts = CurrentFactoryScheduleFactsV1 {
        sources,
        context,
        read: parent,
        work,
        common,
        now,
        live,
        fees: &mut fees,
    };
    let result = match job {
        DuePriorityJobV1::Build(job) => {
            due_priority::factory_priority_with_facts_v1(&mut facts, job)
        }
        DuePriorityJobV1::Recipe(job) => {
            due_priority::recipe_priority_with_facts_v1(&mut facts, job)
        }
    };
    drop(facts);
    result.map_err(|error| evaluation_error(error, fees))
}
