//! The original RecipeStarted admission prefix over selected readonly facts.
//! No event is applied and no World, staged transition or permission is made.
use super::{
    IndustrialOwnerReadonlyFactsV1, ManufacturingQuoteReadonlyFactsV1,
    format_readonly_projection_data_v1,
};
use crate::{
    DomainEvent, LogisticsPathAuthorityV1, MaterialLedgerId, MaterialStack, RecipeJobState,
    WorldError,
};
use std::{collections::BTreeMap, mem::size_of, ops::ControlFlow};
pub trait RecipeStartedReadonlyFactsV1: ManufacturingQuoteReadonlyFactsV1 {
    fn started_pending(&self, id: u64) -> Result<bool, Self::Error>;
    fn started_settled(&self, id: u64) -> Result<bool, Self::Error>;
    fn started_visit_jobs(
        &self,
        visit: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn started_material(&self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error>;
    fn started_with_path<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&LogisticsPathAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error>;
}
fn reject<S: IndustrialOwnerReadonlyFactsV1, T>(
    s: &S,
    args: std::fmt::Arguments<'_>,
) -> Result<Result<T, WorldError>, S::Error> {
    let reason = format_readonly_projection_data_v1(args, &mut |steps, bytes, heap| {
        s.owner_work(steps, bytes)?;
        if heap == 0 {
            Ok(())
        } else {
            s.owner_before_owned(heap)
        }
    })?;
    Ok(Err(WorldError::ResourceBalanceInvalid { reason }))
}
macro_rules! domain {
    ($v:expr) => {
        match $v {
            Ok(value) => value,
            Err(error) => return Ok(Err(error)),
        }
    };
}
/// The shared prefix ends at the original emergency/ordinary power split.
/// The caller must execute that split before treating this DATA as a preview.
pub(crate) struct StartedPrefixDataV1 {
    pub allocations: BTreeMap<String, i64>,
    pub electricity: i64,
}
pub(crate) fn admission_prefix<S: RecipeStartedReadonlyFactsV1>(
    s: &S,
    event: &DomainEvent,
    now: u64,
) -> Result<Result<StartedPrefixDataV1, WorldError>, S::Error> {
    let DomainEvent::RecipeStarted {
        job_id,
        requester_agent_id,
        factory_id,
        recipe_id,
        accepted_batches,
        consume,
        produce,
        byproducts,
        power_required,
        power_owner_agent_id,
        duration_ticks,
        consume_ledger,
        output_ledger,
        logistics_route_ids,
        logistics_path_ids,
        ready_at,
        committed_recipe_origin,
        ..
    } = event
    else {
        return reject(
            s,
            format_args!("recipe lifecycle preparation requires supported event"),
        );
    };
    s.owner_work(
        1,
        requester_agent_id.len() + factory_id.len() + recipe_id.len(),
    )?;
    s.owner_before_owned(size_of::<WorldError>() + "recipe start committed origin mismatch".len())?;
    domain!(
        crate::state::industry_transition::validate_recipe_started_origin_v1(
            committed_recipe_origin.as_ref(),
            requester_agent_id,
            factory_id,
            recipe_id
        )
    );
    if s.started_pending(*job_id)? {
        return reject(
            s,
            format_args!("recipe start job is already pending: job_id={job_id}"),
        );
    }
    if s.started_settled(*job_id)? {
        return reject(
            s,
            format_args!("recipe start job identity is already settled: job_id={job_id}"),
        );
    }
    if s.quote_factory_retired(factory_id)? {
        return reject(
            s,
            format_args!("recipe scheduled for retired factory: factory_id={factory_id}"),
        );
    }
    s.quote_with_factory(factory_id,|factory|{
  let Some(factory)=factory else{return reject(s,format_args!("recipe factory not found: factory_id={factory_id}"))};
  let Some(power_owner)=power_owner_agent_id.as_deref()else{return reject(s,format_args!("recipe power owner is missing: job_id={job_id} factory_id={factory_id}"))};
  s.owner_work(1,power_owner.len()*2+factory.builder_agent_id.len()+requester_agent_id.len())?;
  if power_owner!=factory.builder_agent_id||power_owner!=requester_agent_id{return reject(s,format_args!("recipe power owner mismatch: job_id={job_id} payer={power_owner} requester={requester_agent_id} builder={} ",factory.builder_agent_id))}
  let Some(electricity)=s.quote_agent_electricity(power_owner)?else{return reject(s,format_args!("recipe power owner agent not found: job_id={job_id} payer={power_owner}"))};
  s.owner_work(1,factory.builder_agent_id.len()+requester_agent_id.len())?;
  if factory.builder_agent_id!=*requester_agent_id{return reject(s,format_args!("recipe requester is not factory builder: factory_id={factory_id} requester={requester_agent_id} builder={}",factory.builder_agent_id))}
  if factory.spec.recipe_slots==0{return reject(s,format_args!("recipe factory has no execution slots: factory_id={factory_id}"))}
  let mut pending=0usize;s.started_visit_jobs(|job|{s.owner_work(1,job.factory_id.len()+factory_id.len())?;if job.factory_id==*factory_id{pending=pending.checked_add(1).expect("resident recipe job count")}Ok(ControlFlow::Continue(()))})?;
  if pending>=usize::from(factory.spec.recipe_slots)||usize::from(factory.production.active_jobs)>=usize::from(factory.spec.recipe_slots){return reject(s,format_args!("recipe factory has no free execution slot: factory_id={factory_id} active_jobs={} pending_jobs={} recipe_slots={}",factory.production.active_jobs,pending,factory.spec.recipe_slots))}
  let expected=if *consume_ledger==MaterialLedgerId::World{&MaterialLedgerId::World}else if *consume_ledger==factory.input_ledger{&factory.output_ledger}else{return reject(s,format_args!("recipe consume ledger does not match factory input or world fallback: factory_id={factory_id} consume_ledger={consume_ledger} input_ledger={}",factory.input_ledger))};
  s.owner_work(1,size_of::<MaterialLedgerId>()*2)?;if output_ledger!=expected{return reject(s,format_args!("recipe output ledger does not match committed factory route: factory_id={factory_id} output_ledger={output_ledger} expected={expected}"))}
  if *accepted_batches==0{return reject(s,format_args!("recipe accepted_batches must be positive: job_id={job_id}"))}
  if *duration_ticks==0||*ready_at<=now{return reject(s,format_args!("recipe timing commitment is invalid: job_id={job_id} duration_ticks={duration_ticks} ready_at={ready_at} now={now}"))}
  let mut charge=|steps,bytes,heap|{s.owner_work(steps,bytes)?;if heap==0{Ok(())}else{s.owner_before_owned(heap)}};
  for(label,stacks)in[("consume",consume),("produce",produce),("byproduct",byproducts)]{
   if let Err(reason)=crate::state::apply_domain_event_industry_helpers::validate_recipe_material_stacks_with_work_v1(label,stacks,&mut charge)?{return reject(s,format_args!("recipe {label} stack invalid: {reason}"))}
  }
  let required=match crate::state::apply_domain_event_industry_helpers::aggregate_recipe_material_stacks_with_work_v1(consume,&mut charge)?{Ok(value)=>value,Err(reason)=>return reject(s,format_args!("recipe consume aggregation failed: {reason}"))};
  let allocations=domain!(allocate_paths(s,consume_ledger,logistics_route_ids,logistics_path_ids,consume)?);
  for(kind,amount)in &required{let available=s.started_material(consume_ledger,kind)?;if available<*amount{return reject(s,format_args!("recipe consume failed: insufficient material {kind}: requested={amount} available={available}"))}}
  if *power_required<0{return reject(s,format_args!("recipe power_required must be non-negative: job_id={job_id} power_required={power_required}"))}
  Ok(Ok(StartedPrefixDataV1{allocations,electricity}))
 })
}
fn allocate_paths<S: RecipeStartedReadonlyFactsV1>(
    s: &S,
    ledger: &MaterialLedgerId,
    routes: &[String],
    ids: &[String],
    consume: &[MaterialStack],
) -> Result<Result<BTreeMap<String, i64>, WorldError>, S::Error> {
    // The original allocator exits here before observing any unrelated path row.
    if ids.is_empty() {
        return Ok(Ok(BTreeMap::new()));
    }
    let mut paths = BTreeMap::new();
    let (mut strings, mut entries) = (0usize, ids.len() + routes.len() + consume.len());
    for id in ids {
        s.owner_work(1, id.len())?;
        strings = strings.saturating_add(id.len());
        if !paths.contains_key(id) {
            s.started_with_path(id, |value| {
                if let Some(value) = value {
                    super::readonly_industrial_data::tree_slot::<_, LogisticsPathAuthorityV1>(
                        s,
                        id,
                        paths.len(),
                    )?;
                    s.owner_before_owned(
                        value.route_ids.len().saturating_mul(size_of::<String>()),
                    )?;
                    let mut route_ids = Vec::with_capacity(value.route_ids.len());
                    for route in &value.route_ids {
                        s.owner_work(1, 0)?;
                        strings = strings.saturating_add(route.len());
                        entries = entries.saturating_add(1);
                        route_ids.push(super::readonly_industrial_data::text(s, route)?);
                    }
                    let copy = LogisticsPathAuthorityV1 {
                        path_id: super::readonly_industrial_data::text(s, &value.path_id)?,
                        route_ids,
                        from_ledger: super::readonly_industrial_data::ledger(
                            s,
                            &value.from_ledger,
                        )?,
                        to_ledger: super::readonly_industrial_data::ledger(s, &value.to_ledger)?,
                        kind: super::readonly_industrial_data::text(s, &value.kind)?,
                        settled_amount: value.settled_amount,
                        remaining_recipe_amount: value.remaining_recipe_amount,
                    };
                    strings = strings.saturating_add(value.kind.len());
                    paths.insert(super::readonly_industrial_data::text(s, id)?, copy);
                }
                Ok(())
            })?;
        }
    }
    for text in routes {
        s.owner_work(1, 0)?;
        strings = strings.saturating_add(text.len())
    }
    for stack in consume {
        s.owner_work(1, 0)?;
        strings = strings.saturating_add(stack.kind.len())
    }
    // Fixed allocator data structures: required kinds, sorted submitted IDs,
    // two sets, per-kind (path,quantity) vectors and the final allocation map.
    // Charge the actual submitted graph and a quadratic comparison upper bound
    // before its standard-library sort; every original allocator callback still
    // debits this same source, preserving a typed first cause.
    let levels = entries.saturating_add(1).ilog2() as usize + 2;
    let node = 11 * (size_of::<String>() + size_of::<Vec<(String, i64)>>())
        + 13 * size_of::<usize>()
        + 128;
    let heap = entries
        .saturating_mul(node.saturating_mul(6))
        .saturating_add(strings.saturating_mul(8))
        .saturating_add(4096);
    s.owner_before_owned(heap)?;
    s.owner_work(
        (ids.len() as u64)
            .saturating_mul(ids.len() as u64)
            .saturating_add(entries as u64),
        (strings + 1).saturating_mul(levels.saturating_mul(8)),
    )?;
    let mut first = None;
    let result = crate::allocate_recipe_path_amounts_from_records_v1(
        &paths,
        ledger,
        routes,
        ids,
        consume,
        &mut |bytes| match s.owner_work(1, bytes) {
            Ok(()) => Ok(()),
            Err(error) => {
                if first.is_none() {
                    first = Some(error)
                }
                Err(String::new())
            }
        },
    );
    if let Some(error) = first {
        return Err(error);
    }
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(reason) => reject(
            s,
            format_args!("recipe logistics path authority failed: {reason}"),
        ),
    }
}
impl RecipeStartedReadonlyFactsV1 for crate::WorldState {
    fn started_pending(&self, id: u64) -> Result<bool, Self::Error> {
        Ok(self.pending_recipe_jobs.contains_key(&id))
    }
    fn started_settled(&self, id: u64) -> Result<bool, Self::Error> {
        Ok(self.settled_recipe_job_ids.contains(&id))
    }
    fn started_visit_jobs(
        &self,
        mut visit: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        for job in self.pending_recipe_jobs.values() {
            if visit(job)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn started_material(&self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        let values = if *ledger == MaterialLedgerId::World {
            self.material_ledgers
                .get(ledger)
                .filter(|v| !v.is_empty())
                .unwrap_or(&self.materials)
        } else {
            match self.material_ledgers.get(ledger) {
                Some(v) => v,
                None => return Ok(0),
            }
        };
        Ok(values.get(kind).copied().unwrap_or(0))
    }
    fn started_with_path<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&LogisticsPathAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        visit(self.completed_logistics_paths.get(id))
    }
}

pub(crate) fn ordinary_power<S: IndustrialOwnerReadonlyFactsV1>(
    s: &S,
    available: i64,
    required: i64,
) -> Result<Result<(), WorldError>, S::Error> {
    s.owner_work(1, 0)?;
    if available < required {
        return reject(s, format_args!("insufficient electricity"));
    }
    Ok(Ok(()))
}

/// Original outer PreparedIndustryEvent reservation gate, before the recipe
/// lifecycle prefix. Counts preserve the original usize count -> u32 cast.
pub(crate) fn factory_capacity_reservation_gate_v1<S: RecipeStartedReadonlyFactsV1>(
    s: &S,
    event: &DomainEvent,
) -> Result<Result<(), WorldError>, S::Error> {
    let factory = match event {
        DomainEvent::RecipeStarted { factory_id, .. }
        | DomainEvent::FactoryRecycled { factory_id, .. } => factory_id,
        _ => return Ok(Ok(())),
    };
    let mut reserved = 0usize;
    s.owner_visit_orders(|order| {
        s.owner_work(1, order.factory_id.len() + factory.len())?;
        if super::industrial::manufacturing_order_reserves_slot_v1(order, factory) {
            reserved = reserved.checked_add(1).expect("resident order count")
        }
        Ok(ControlFlow::Continue(()))
    })?;
    let reserved = (reserved as u32) as usize;
    let mut jobs = 0usize;
    s.started_visit_jobs(|job| {
        s.owner_work(1, job.factory_id.len() + factory.len())?;
        if job.factory_id == *factory {
            jobs = jobs.checked_add(1).expect("resident recipe job count")
        }
        Ok(ControlFlow::Continue(()))
    })?;
    if reserved == 0 {
        return Ok(Ok(()));
    }
    if matches!(event, DomainEvent::FactoryRecycled { .. }) {
        return reject(s, format_args!("factory_capacity_reserved_by_agent_order"));
    }
    s.quote_with_factory(factory, |f| {
        s.owner_work(1, 0)?;
        if f.is_none_or(|f| jobs + reserved >= usize::from(f.spec.recipe_slots)) {
            return reject(s, format_args!("factory_capacity_reserved_by_agent_order"));
        }
        Ok(Ok(()))
    })
}
