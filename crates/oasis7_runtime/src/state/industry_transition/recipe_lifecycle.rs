use super::super::apply_domain_event_industry_helpers::{
    aggregate_recipe_material_stacks, validate_recipe_material_stacks,
    validate_recipe_output_capacity,
};
use super::*;
use crate::AgentActivityV1;

impl PreparedRecipeLifecycle {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        match event {
            DomainEvent::RecipeStarted { .. } => Self::prepare_started(state, event, now),
            DomainEvent::RecipeCompleted { .. } => Self::prepare_completed(state, event, now),
            DomainEvent::FactoryProductionBlocked { .. } => {
                Self::prepare_blocked(state, event, now)
            }
            DomainEvent::FactoryProductionResumed {
                requester_agent_id,
                factory_id,
                ..
            } => {
                let factory = state.factories.get(factory_id).cloned().map(|mut value| {
                    value.production.status = if value.production.active_jobs > 0 {
                        FactoryProductionStatus::Running
                    } else {
                        FactoryProductionStatus::Idle
                    };
                    value.production.last_resumed_at = Some(now);
                    value.production.current_blocker_kind = None;
                    value.production.current_blocker_detail = None;
                    (factory_id.clone(), value)
                });
                Ok(Self::simple(
                    state,
                    event,
                    requester_agent_id,
                    now,
                    factory,
                    None,
                ))
            }
            DomainEvent::FactoryProductionPaused {
                requester_agent_id,
                factory_id,
                ..
            } => {
                let factory = state.factories.get(factory_id).cloned().map(|mut value| {
                    value.production.status = FactoryProductionStatus::Paused;
                    value.production.current_job_id = None;
                    value.production.current_recipe_id = None;
                    value.production.current_blocker_kind = None;
                    value.production.current_blocker_detail = None;
                    value.production.last_completed_recipe_id = None;
                    value.production.same_recipe_repeat_count = 0;
                    value.production.last_completed_canonical_snapshot = None;
                    (factory_id.clone(), value)
                });
                let mut progress = state.industry_progress.clone();
                refresh_progress(state, factory.as_ref(), &mut progress, now);
                Ok(Self::simple(
                    state,
                    event,
                    requester_agent_id,
                    now,
                    factory,
                    Some(progress),
                ))
            }
            _ => Err(invalid(
                "recipe lifecycle preparation requires supported event",
            )),
        }
    }

    fn simple(
        state: &WorldState,
        event: &DomainEvent,
        requester: &str,
        now: WorldTime,
        factory: Option<(String, FactoryState)>,
        progress: Option<IndustryProgressState>,
    ) -> Self {
        let (materials, world) = normalized_materials(state);
        let agent = state.agents.get(requester).cloned().map(|mut cell| {
            cell.last_active = now;
            (requester.to_string(), cell)
        });
        Self {
            event: event.clone(),
            pending: None,
            settled_id: None,
            factory,
            ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
            materials,
            paths: BTreeMap::new(),
            agent,
            progress,
            failure_disposition: None,
            settlement_order: None,
            next_settlement_order: None,
            completion_receipt: None,
        }
    }

    fn prepare_started(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::RecipeStarted {
            committed_recipe_origin,
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
            bottleneck_tags,
            market_quotes,
            logistics_route_ids,
            logistics_path_ids,
            ready_at,
        } = event
        else {
            unreachable!()
        };
        if committed_recipe_origin
            .as_ref()
            .is_some_and(|origin| !origin.matches_recipe(requester_agent_id, factory_id, recipe_id))
        {
            return Err(invalid("recipe start committed origin mismatch"));
        }
        if state.pending_recipe_jobs.contains_key(job_id) {
            return Err(invalid(format!(
                "recipe start job is already pending: job_id={job_id}"
            )));
        }
        if state.settled_recipe_job_ids.contains(job_id) {
            return Err(invalid(format!(
                "recipe start job identity is already settled: job_id={job_id}"
            )));
        }
        if state.retired_factory_ids.contains(factory_id) {
            return Err(invalid(format!(
                "recipe scheduled for retired factory: factory_id={factory_id}"
            )));
        }
        let mut factory =
            state.factories.get(factory_id).cloned().ok_or_else(|| {
                invalid(format!("recipe factory not found: factory_id={factory_id}"))
            })?;
        let power_owner = power_owner_agent_id.as_deref().ok_or_else(|| {
            invalid(format!(
                "recipe power owner is missing: job_id={job_id} factory_id={factory_id}"
            ))
        })?;
        if power_owner != factory.builder_agent_id || power_owner != requester_agent_id {
            return Err(invalid(format!(
                "recipe power owner mismatch: job_id={job_id} payer={power_owner} requester={requester_agent_id} builder={} ",
                factory.builder_agent_id
            )));
        }
        let mut agent = state.agents.get(power_owner).cloned().ok_or_else(|| {
            invalid(format!(
                "recipe power owner agent not found: job_id={job_id} payer={power_owner}"
            ))
        })?;
        if factory.builder_agent_id != *requester_agent_id {
            return Err(invalid(format!(
                "recipe requester is not factory builder: factory_id={factory_id} requester={requester_agent_id} builder={}",
                factory.builder_agent_id
            )));
        }
        if factory.spec.recipe_slots == 0 {
            return Err(invalid(format!(
                "recipe factory has no execution slots: factory_id={factory_id}"
            )));
        }
        let pending_count = state
            .pending_recipe_jobs
            .values()
            .filter(|job| job.factory_id == *factory_id)
            .count();
        if pending_count >= usize::from(factory.spec.recipe_slots)
            || usize::from(factory.production.active_jobs) >= usize::from(factory.spec.recipe_slots)
        {
            return Err(invalid(format!(
                "recipe factory has no free execution slot: factory_id={factory_id} active_jobs={} pending_jobs={} recipe_slots={}",
                factory.production.active_jobs, pending_count, factory.spec.recipe_slots
            )));
        }
        let expected_output = if *consume_ledger == MaterialLedgerId::world() {
            MaterialLedgerId::world()
        } else if *consume_ledger == factory.input_ledger {
            factory.output_ledger.clone()
        } else {
            return Err(invalid(format!(
                "recipe consume ledger does not match factory input or world fallback: factory_id={factory_id} consume_ledger={consume_ledger} input_ledger={}",
                factory.input_ledger
            )));
        };
        if *output_ledger != expected_output {
            return Err(invalid(format!(
                "recipe output ledger does not match committed factory route: factory_id={factory_id} output_ledger={output_ledger} expected={expected_output}"
            )));
        }
        if *accepted_batches == 0 {
            return Err(invalid(format!(
                "recipe accepted_batches must be positive: job_id={job_id}"
            )));
        }
        if *duration_ticks == 0 || *ready_at <= now {
            return Err(invalid(format!(
                "recipe timing commitment is invalid: job_id={job_id} duration_ticks={duration_ticks} ready_at={ready_at} now={now}"
            )));
        }
        validate_recipe_material_stacks("consume", consume)
            .map_err(|r| invalid(format!("recipe consume stack invalid: {r}")))?;
        validate_recipe_material_stacks("produce", produce)
            .map_err(|r| invalid(format!("recipe produce stack invalid: {r}")))?;
        validate_recipe_material_stacks("byproduct", byproducts)
            .map_err(|r| invalid(format!("recipe byproduct stack invalid: {r}")))?;
        let required = aggregate_recipe_material_stacks(consume)
            .map_err(|r| invalid(format!("recipe consume aggregation failed: {r}")))?;
        let allocations = state
            .allocate_recipe_path_amounts(
                consume_ledger,
                logistics_route_ids,
                logistics_path_ids,
                consume,
            )
            .map_err(|r| invalid(format!("recipe logistics path authority failed: {r}")))?;
        let (mut materials, world) = normalized_materials(state);
        let world_id = MaterialLedgerId::world();
        for (kind, amount) in required {
            let available = (if consume_ledger == &world_id {
                Some(&world)
            } else {
                state.material_ledgers.get(consume_ledger)
            })
            .and_then(|ledger| ledger.get(&kind))
            .copied()
            .unwrap_or(0);
            if available < amount {
                return Err(invalid(format!(
                    "recipe consume failed: insufficient material {kind}: requested={amount} available={available}"
                )));
            }
        }
        if *power_required < 0 {
            return Err(invalid(format!(
                "recipe power_required must be non-negative: job_id={job_id} power_required={power_required}"
            )));
        }
        let available = agent.state.resources.get(ResourceKind::Electricity);
        if available < *power_required {
            return Err(invalid(format!(
                "recipe power consume failed: insufficient electricity: payer={power_owner} requested={power_required} available={available}"
            )));
        }
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if consume_ledger != &world_id {
            ledgers.insert(
                consume_ledger.clone(),
                state
                    .material_ledgers
                    .get(consume_ledger)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        for stack in consume {
            remove_material_balance_for_ledger(
                &mut ledgers,
                consume_ledger,
                &stack.kind,
                stack.amount,
            )
            .map_err(|r| invalid(format!("recipe consume failed: {r}")))?;
        }
        if consume_ledger == &world_id {
            materials = ledgers[&world_id].clone();
        }
        agent
            .state
            .resources
            .remove(ResourceKind::Electricity, *power_required)
            .map_err(|r| {
                invalid(format!(
                    "recipe power consume failed for payer {power_owner}: {r:?}"
                ))
            })?;
        let mut paths = BTreeMap::new();
        for (path_id, amount) in allocations {
            let mut path = state.completed_logistics_paths[&path_id].clone();
            path.remaining_recipe_amount = path
                .remaining_recipe_amount
                .checked_sub(amount)
                .expect("path allocation prevalidated");
            paths.insert(path_id, path);
        }
        factory.production.active_jobs = factory.production.active_jobs.saturating_add(1);
        factory.production.current_job_id = Some(*job_id);
        factory.production.current_recipe_id = Some(recipe_id.clone());
        factory.production.last_started_at = Some(now);
        factory.production.status = FactoryProductionStatus::Running;
        agent.activity = Some(AgentActivityV1::executing(
            "recipe",
            *job_id,
            factory_id.clone(),
            now,
        ));
        agent.last_active = now;
        let mut progress = state.industry_progress.clone();
        for quote in market_quotes {
            progress
                .latest_market_quotes
                .insert(quote.kind.clone(), quote.clone());
        }
        Ok(Self {
            event: event.clone(),
            pending: Some((
                *job_id,
                Some(RecipeJobState {
                    committed_recipe_origin: committed_recipe_origin.clone(),
                    job_id: *job_id,
                    requester_agent_id: requester_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recipe_id: recipe_id.clone(),
                    accepted_batches: *accepted_batches,
                    consume: consume.clone(),
                    produce: produce.clone(),
                    byproducts: byproducts.clone(),
                    power_required: *power_required,
                    power_owner_agent_id: Some(power_owner.to_string()),
                    duration_ticks: *duration_ticks,
                    consume_ledger: consume_ledger.clone(),
                    output_ledger: output_ledger.clone(),
                    bottleneck_tags: bottleneck_tags.clone(),
                    logistics_route_ids: logistics_route_ids.clone(),
                    logistics_path_ids: logistics_path_ids.clone(),
                    ready_at: *ready_at,
                }),
            )),
            settled_id: None,
            factory: Some((factory_id.clone(), factory)),
            ledgers,
            materials,
            paths,
            agent: Some((requester_agent_id.clone(), agent)),
            progress: Some(progress),
            failure_disposition: None,
            settlement_order: None,
            next_settlement_order: None,
            completion_receipt: None,
        })
    }

    fn prepare_completed(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::RecipeCompleted {
            committed_recipe_origin,
            job_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            accepted_batches,
            produce,
            byproducts,
            output_ledger,
            bottleneck_tags,
            logistics_route_ids,
            logistics_path_ids,
            ..
        } = event
        else {
            unreachable!()
        };
        let (mut materials, world) = normalized_materials(state);
        let mut completion_receipt = RecipeCompletionReceiptV1 {
            committed_recipe_origin: committed_recipe_origin.clone(),
            job_id: *job_id,
            requester_agent_id: requester_agent_id.clone(),
            factory_id: factory_id.clone(),
            recipe_id: recipe_id.clone(),
            accepted_batches: *accepted_batches,
            produce: produce.clone(),
            byproducts: byproducts.clone(),
            output_ledger: output_ledger.clone(),
            bottleneck_tags: bottleneck_tags.clone(),
            logistics_route_ids: logistics_route_ids.clone(),
            logistics_path_ids: logistics_path_ids.clone(),
        };
        if let Some(existing) = state.recipe_completion_receipts.get(job_id)
            && existing != &completion_receipt
        {
            return Err(invalid(format!(
                "recipe completion conflicts with persisted receipt: job_id={job_id}"
            )));
        }
        if state.settled_recipe_job_ids.contains(job_id) {
            return Ok(Self::idempotent(
                state,
                event,
                requester_agent_id,
                materials,
                world,
            ));
        }
        let pending = state
            .pending_recipe_jobs
            .get(job_id)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "recipe completion has no pending job: job_id={job_id}"
                ))
            })?;
        if now < pending.ready_at {
            return Err(invalid(format!(
                "recipe completion is early: job_id={job_id} ready_at={} now={now}",
                pending.ready_at
            )));
        }
        if pending.committed_recipe_origin != *committed_recipe_origin
            || pending.requester_agent_id != *requester_agent_id
            || pending.factory_id != *factory_id
            || pending.recipe_id != *recipe_id
            || pending.accepted_batches != *accepted_batches
            || pending.produce != *produce
            || pending.byproducts != *byproducts
            || pending.output_ledger != *output_ledger
            || pending.bottleneck_tags != *bottleneck_tags
            || pending.logistics_route_ids != *logistics_route_ids
            || pending.logistics_path_ids != *logistics_path_ids
        {
            return Err(invalid(format!(
                "recipe completion does not match pending commitment: job_id={job_id}"
            )));
        }
        completion_receipt.committed_recipe_origin = pending.committed_recipe_origin.clone();
        validate_recipe_output_capacity(
            &state.material_ledgers,
            output_ledger,
            produce,
            byproducts,
        )
        .map_err(|r| invalid(format!("recipe output preflight failed: {r}")))?;
        let world_id = MaterialLedgerId::world();
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if output_ledger != &world_id {
            ledgers.insert(
                output_ledger.clone(),
                state
                    .material_ledgers
                    .get(output_ledger)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        for stack in produce.iter().chain(byproducts) {
            add_material_balance_for_ledger(&mut ledgers, output_ledger, &stack.kind, stack.amount)
                .map_err(|r| invalid(format!("recipe produce failed: {r}")))?;
        }
        if output_ledger == &world_id {
            materials = ledgers[&world_id].clone();
        }
        let snapshot = FactoryProductionSnapshot::from_recipe_job(&pending);
        let mut factory = state.factories.get(factory_id).cloned();
        if let Some(value) = factory.as_mut() {
            value.production.active_jobs = value.production.active_jobs.saturating_sub(1);
            if value.production.current_job_id == Some(*job_id) {
                value.production.current_job_id = None;
            }
            value.production.current_recipe_id = None;
            value.production.last_completed_at = Some(now);
            value.production.completed_jobs = value.production.completed_jobs.saturating_add(1);
            let same =
                value.production.last_completed_canonical_snapshot.as_ref() == Some(&snapshot);
            value.production.same_recipe_repeat_count = if same {
                value.production.same_recipe_repeat_count.saturating_add(1)
            } else {
                1
            };
            value.production.last_completed_recipe_id = Some(recipe_id.clone());
            value.production.last_completed_canonical_snapshot = Some(snapshot);
            if value.production.active_jobs == 0 {
                value.production.status = FactoryProductionStatus::Idle;
            }
        }
        let mut progress = state.industry_progress.clone();
        progress.completed_recipe_jobs = progress.completed_recipe_jobs.saturating_add(1);
        if progress.starter_industrial_milestone.is_none()
            && *accepted_batches > 0
            && factory_id == STARTER_SMELTER_FACTORY_ID
            && recipe_id == STARTER_SMELTER_RECIPE_ID
            && state
                .factories
                .get(factory_id)
                .is_some_and(|factory| factory.output_ledger == *output_ledger)
            && produce
                .iter()
                .any(|stack| stack.kind == "iron_ingot" && stack.amount > 0)
        {
            progress.starter_industrial_milestone = Some(StarterIndustrialMilestoneV1 {
                settlement_summary: pending.committed_recipe_origin.as_ref().map(|_| {
                    StarterIndustrialSettlementSummaryV1 {
                        requester_agent_id: pending.requester_agent_id.clone(),
                        accepted_batches: pending.accepted_batches,
                        consume: pending.consume.clone(),
                        power_required: pending.power_required,
                        produce: pending.produce.clone(),
                    }
                }),
                committed_recipe_origin: pending.committed_recipe_origin.clone(),
                profile_id: STARTER_INDUSTRIAL_PROFILE_ID.to_string(),
                profile_revision: STARTER_INDUSTRIAL_PROFILE_REVISION,
                factory_id: factory_id.clone(),
                recipe_id: recipe_id.clone(),
                output_ledger: output_ledger.clone(),
                settlement_job_id: *job_id,
                settled_at: now,
            });
        }
        let factory_update = factory.map(|value| (factory_id.clone(), value));
        refresh_progress(state, factory_update.as_ref(), &mut progress, now);
        let no_active = factory_update
            .as_ref()
            .is_some_and(|(_, value)| value.production.active_jobs == 0);
        let agent = state
            .agents
            .get(requester_agent_id)
            .cloned()
            .map(|mut cell| {
                if no_active {
                    cell.activity = Some(AgentActivityV1::idle(now));
                }
                cell.last_active = now;
                (requester_agent_id.clone(), cell)
            });
        Ok(Self {
            event: event.clone(),
            pending: Some((*job_id, None)),
            settled_id: Some(*job_id),
            factory: factory_update,
            ledgers,
            materials,
            paths: BTreeMap::new(),
            agent,
            progress: Some(progress),
            failure_disposition: None,
            settlement_order: Some((
                *job_id,
                state
                    .industry_settlement_orders
                    .get(job_id)
                    .copied()
                    .unwrap_or(state.next_industry_settlement_order),
            )),
            next_settlement_order: (!state.industry_settlement_orders.contains_key(job_id))
                .then(|| state.next_industry_settlement_order.saturating_add(1)),
            completion_receipt: Some((*job_id, completion_receipt)),
        })
    }

    fn prepare_blocked(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::FactoryProductionBlocked {
            action_id,
            requester_agent_id,
            factory_id,
            recipe_id,
            blocker_kind,
            blocker_detail,
            ..
        } = event
        else {
            unreachable!()
        };
        let terminal = blocker_kind == "product_validation";
        let pending = state.pending_recipe_jobs.get(action_id).cloned();
        if terminal
            && let Some(existing) = state.factory_production_failure_dispositions.get(action_id)
        {
            if existing.requester_agent_id == *requester_agent_id
                && existing.factory_id == *factory_id
                && existing.recipe_id == *recipe_id
                && existing.blocker_kind == *blocker_kind
                && existing.blocker_detail == *blocker_detail
            {
                let (materials, world) = normalized_materials(state);
                return Ok(Self::idempotent(
                    state,
                    event,
                    requester_agent_id,
                    materials,
                    world,
                ));
            }
            return Err(invalid(format!(
                "product-validation blocker conflicts with persisted disposition: job_id={action_id}"
            )));
        }
        if terminal && pending.is_none() {
            let (materials, world) = normalized_materials(state);
            return Ok(Self::idempotent(
                state,
                event,
                requester_agent_id,
                materials,
                world,
            ));
        }
        if let Some(pending) = pending.as_ref()
            && terminal
            && (pending.requester_agent_id != *requester_agent_id
                || pending.factory_id != *factory_id
                || pending.recipe_id != *recipe_id)
        {
            return Err(invalid(format!(
                "product-validation blocker does not match pending commitment: job_id={action_id}"
            )));
        }
        let mut factory = state.factories.get(factory_id).cloned();
        if let Some(value) = factory.as_mut() {
            if terminal && pending.is_some() {
                value.production.active_jobs = value.production.active_jobs.saturating_sub(1);
            }
            value.production.status = FactoryProductionStatus::Blocked;
            value.production.last_blocked_at = Some(now);
            value.production.current_blocker_kind = Some(blocker_kind.clone());
            value.production.current_blocker_detail = Some(blocker_detail.clone());
            value.production.current_job_id = None;
            value.production.current_recipe_id = None;
            value.production.last_completed_recipe_id = None;
            value.production.same_recipe_repeat_count = 0;
            value.production.last_completed_canonical_snapshot = None;
        }
        let factory = factory.map(|value| (factory_id.clone(), value));
        let mut progress = state.industry_progress.clone();
        if !terminal {
            refresh_progress(state, factory.as_ref(), &mut progress, now);
        }
        let mut agent = state.agents.get(requester_agent_id).cloned();
        if let Some(cell) = agent.as_mut() {
            cell.activity = Some(AgentActivityV1::blocked(
                "recipe",
                *action_id,
                factory_id.clone(),
                blocker_kind.clone(),
                blocker_detail.clone(),
                now,
            ));
            cell.last_active = now;
        }
        let (materials, world) = normalized_materials(state);
        let failure_disposition = terminal.then(|| {
            let pending = pending.as_ref().expect("terminal pending validated");
            (
                *action_id,
                FactoryProductionFailureDispositionV1 {
                    action_id: *action_id,
                    requester_agent_id: pending.requester_agent_id.clone(),
                    factory_id: pending.factory_id.clone(),
                    recipe_id: pending.recipe_id.clone(),
                    blocker_kind: blocker_kind.clone(),
                    blocker_detail: blocker_detail.clone(),
                    disposition_kind: "consumed_lost".into(),
                    consumed_inputs: pending.consume.clone(),
                    lost_inputs: pending.consume.clone(),
                    consumed_power: pending.power_required,
                    lost_power: pending.power_required,
                    next_action: "inspect_product_validation_and_reschedule".into(),
                    next_recheck: None,
                },
            )
        });
        let settlement_order = terminal.then(|| {
            (
                *action_id,
                state
                    .industry_settlement_orders
                    .get(action_id)
                    .copied()
                    .unwrap_or(state.next_industry_settlement_order),
            )
        });
        let next_settlement_order = (terminal
            && !state.industry_settlement_orders.contains_key(action_id))
        .then(|| state.next_industry_settlement_order.saturating_add(1));
        Ok(Self {
            event: event.clone(),
            pending: terminal.then_some((*action_id, None)),
            settled_id: terminal.then_some(*action_id),
            factory,
            ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
            materials,
            paths: BTreeMap::new(),
            agent: agent.map(|cell| (requester_agent_id.clone(), cell)),
            progress: (!terminal).then_some(progress),
            failure_disposition,
            settlement_order,
            next_settlement_order,
            completion_receipt: None,
        })
    }

    fn idempotent(
        state: &WorldState,
        event: &DomainEvent,
        requester: &str,
        materials: BTreeMap<String, i64>,
        world: BTreeMap<String, i64>,
    ) -> Self {
        Self {
            event: event.clone(),
            pending: None,
            settled_id: None,
            factory: None,
            ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
            materials,
            paths: BTreeMap::new(),
            agent: state
                .agents
                .get(requester)
                .cloned()
                .map(|cell| (requester.to_string(), cell)),
            progress: None,
            failure_disposition: None,
            settlement_order: None,
            next_settlement_order: None,
            completion_receipt: None,
        }
    }

    pub(crate) fn matches(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install(self, state: &mut WorldState) {
        state.material_ledgers.extend(self.ledgers);
        state.materials = self.materials;
        if let Some((id, pending)) = self.pending {
            match pending {
                Some(value) => {
                    state.pending_recipe_jobs.insert(id, value);
                }
                None => {
                    state.pending_recipe_jobs.remove(&id);
                }
            }
        }
        if let Some(id) = self.settled_id {
            state.settled_recipe_job_ids.insert(id);
        }
        if let Some((id, factory)) = self.factory {
            state.factories.insert(id, factory);
        }
        state.completed_logistics_paths.extend(self.paths);
        if let Some((id, agent)) = self.agent {
            state.agents.insert(id, agent);
        }
        if let Some(progress) = self.progress {
            state.industry_progress = progress;
        }
        if let Some((id, disposition)) = self.failure_disposition {
            state
                .factory_production_failure_dispositions
                .insert(id, disposition);
        }
        if let Some((id, order)) = self.settlement_order {
            state.industry_settlement_orders.insert(id, order);
        }
        if let Some(next) = self.next_settlement_order {
            state.next_industry_settlement_order = next;
        }
        if let Some((id, receipt)) = self.completion_receipt {
            state.recipe_completion_receipts.insert(id, receipt);
        }
    }

    pub(super) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        serialize_agents(&self.event, self.agent.as_ref(), state, out)
    }

    pub(super) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.materials)?;
        out.serialize_field(
            "material_ledgers",
            &SparseOverlay {
                base: &state.material_ledgers,
                updates: &self.ledgers,
            },
        )
    }

    pub(super) fn serialize_failure_dispositions<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Some((id, value)) = &self.failure_disposition {
            out.serialize_field(
                "factory_production_failure_dispositions",
                &SparseOverlay {
                    base: &state.factory_production_failure_dispositions,
                    updates: &BTreeMap::from([(*id, value.clone())]),
                },
            )
        } else {
            out.serialize_field(
                "factory_production_failure_dispositions",
                &state.factory_production_failure_dispositions,
            )
        }
    }

    pub(super) fn serialize_settlement_history<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "next_industry_settlement_order",
            &self
                .next_settlement_order
                .unwrap_or(state.next_industry_settlement_order),
        )?;
        if let Some((id, order)) = self.settlement_order {
            out.serialize_field(
                "industry_settlement_orders",
                &SparseOverlay {
                    base: &state.industry_settlement_orders,
                    updates: &BTreeMap::from([(id, order)]),
                },
            )
        } else {
            out.serialize_field(
                "industry_settlement_orders",
                &state.industry_settlement_orders,
            )
        }
    }

    pub(super) fn serialize_terminal_receipts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Some((id, receipt)) = &self.completion_receipt {
            out.serialize_field(
                "recipe_completion_receipts",
                &SparseOverlay {
                    base: &state.recipe_completion_receipts,
                    updates: &BTreeMap::from([(*id, receipt.clone())]),
                },
            )?;
        } else if !state.recipe_completion_receipts.is_empty() {
            out.serialize_field(
                "recipe_completion_receipts",
                &state.recipe_completion_receipts,
            )?;
        }
        if !state.factory_recycle_receipts.is_empty() {
            out.serialize_field("factory_recycle_receipts", &state.factory_recycle_receipts)?;
        }
        Ok(())
    }

    pub(super) fn serialize_logistics<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("logistics_routes", &state.logistics_routes)?;
        out.serialize_field(
            "completed_logistics_route_ids",
            &state.completed_logistics_route_ids,
        )?;
        out.serialize_field(
            "completed_logistics_paths",
            &SparseOverlay {
                base: &state.completed_logistics_paths,
                updates: &self.paths,
            },
        )?;
        out.serialize_field(
            "settled_logistics_transit_ids",
            &state.settled_logistics_transit_ids,
        )?;
        out.serialize_field(
            "logistics_settlement_receipts",
            &state.logistics_settlement_receipts,
        )?;
        out.serialize_field(
            "direct_material_transfer_receipts",
            &state.direct_material_transfer_receipts,
        )
    }

    pub(super) fn serialize_factory_fields<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let factories = self.factory.clone().into_iter().collect();
        out.serialize_field(
            "factories",
            &SparseOverlay {
                base: &state.factories,
                updates: &factories,
            },
        )?;
        out.serialize_field("retired_factory_ids", &state.retired_factory_ids)?;
        out.serialize_field(
            "settled_factory_build_ids",
            &state.settled_factory_build_ids,
        )?;
        out.serialize_field("pending_factory_builds", &state.pending_factory_builds)?;
        let pending = self.pending.clone().into_iter().collect();
        out.serialize_field(
            "pending_recipe_jobs",
            &SparseOptionalOverlay {
                base: &state.pending_recipe_jobs,
                changes: &pending,
            },
        )?;
        out.serialize_field(
            "settled_recipe_job_ids",
            &SetInsertOneOverlay {
                base: &state.settled_recipe_job_ids,
                insert: self.settled_id.as_ref(),
            },
        )
    }

    pub(super) fn serialize_pending_and_progress<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "pending_material_transits",
            &state.pending_material_transits,
        )?;
        out.serialize_field(
            "industry_progress",
            self.progress.as_ref().unwrap_or(&state.industry_progress),
        )
    }
}

fn refresh_progress(
    state: &WorldState,
    factory_update: Option<&(String, FactoryState)>,
    progress: &mut IndustryProgressState,
    now: WorldTime,
) {
    let factories = state.factories.iter().map(|(id, factory)| {
        factory_update
            .filter(|(update_id, _)| update_id == id)
            .map(|(_, update)| update)
            .unwrap_or(factory)
    });
    let collected = factories.collect::<Vec<_>>();
    let jobs = collected
        .iter()
        .map(|factory| factory.production.completed_jobs)
        .sum::<u64>();
    let stable = collected
        .iter()
        .any(|factory| factory_has_canonical_stable_line(factory));
    let current = progress.stage;
    let mut next = if stable {
        IndustryStage::ScaleOut
    } else {
        IndustryStage::Bootstrap
    };
    let governed =
        state.gameplay_policy.electricity_tax_bps > 0 || state.gameplay_policy.data_tax_bps > 0;
    if next == IndustryStage::ScaleOut
        && governed
        && (jobs >= 6 || progress.completed_material_transits >= 3)
    {
        next = IndustryStage::Governance;
    }
    if next != current {
        progress.stage = next;
        progress.stage_updated_at = now;
    }
}
