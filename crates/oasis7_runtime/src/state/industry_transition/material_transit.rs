use super::*;

impl PreparedMaterialTransit {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        match event {
            DomainEvent::MaterialTransitStarted { .. } => Self::prepare_started(state, event, now),
            DomainEvent::MaterialTransitCompleted { .. } => {
                Self::prepare_completed(state, event, now)
            }
            _ => Err(invalid(
                "material transit preparation requires a supported event",
            )),
        }
    }

    fn prepare_started(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::MaterialTransitStarted {
            job_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            amount,
            distance_km,
            loss_bps,
            priority,
            route_id,
            path_id,
            route_ids,
            tariff_electricity_total,
            reroute_count,
            ready_at,
        } = event
        else {
            unreachable!()
        };
        if state.pending_material_transits.contains_key(job_id)
            || state.settled_logistics_transit_ids.contains(job_id)
        {
            return Err(invalid(format!(
                "material transit job already settled or pending: job_id={job_id}"
            )));
        }
        let mut route_updates = BTreeMap::new();
        for route_id in route_ids {
            let mut route = route_updates
                .get(route_id)
                .or_else(|| state.logistics_routes.get(route_id))
                .cloned()
                .ok_or_else(|| {
                    invalid(format!(
                        "logistics route not found for transit: route_id={route_id}"
                    ))
                })?;
            if !route.available
                || route.capacity_units <= 0
                || route.reserved_capacity_units.saturating_add(*amount) > route.capacity_units
            {
                return Err(invalid(format!(
                    "logistics route capacity unavailable for transit: route_id={route_id}"
                )));
            }
            route.reserved_capacity_units = route.reserved_capacity_units.saturating_add(*amount);
            route_updates.insert(route_id.clone(), route);
        }
        let mut agents = BTreeMap::new();
        if *tariff_electricity_total > 0 {
            let mut payer = state
                .agents
                .get(requester_agent_id)
                .cloned()
                .ok_or_else(|| WorldError::AgentNotFound {
                    agent_id: requester_agent_id.clone(),
                })?;
            let available = payer.state.resources.get(ResourceKind::Electricity);
            if available < *tariff_electricity_total {
                return Err(invalid(format!(
                    "logistics tariff escrow insufficient: requested={} available={} agent={requester_agent_id}",
                    tariff_electricity_total, available
                )));
            }
            payer
                .state
                .resources
                .remove(ResourceKind::Electricity, *tariff_electricity_total)
                .map_err(|err| invalid(format!("logistics tariff escrow failed: {err:?}")))?;
            payer.last_active = now;
            agents.insert(requester_agent_id.clone(), payer);
        } else if let Some(mut requester) = state.agents.get(requester_agent_id).cloned() {
            requester.last_active = now;
            agents.insert(requester_agent_id.clone(), requester);
        }
        let (_, world) = normalized_materials(state);
        let world_id = MaterialLedgerId::world();
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if from_ledger != &world_id {
            ledgers.insert(
                from_ledger.clone(),
                state
                    .material_ledgers
                    .get(from_ledger)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        remove_material_balance_for_ledger(&mut ledgers, from_ledger, kind, *amount)
            .map_err(|reason| invalid(format!("material transit reserve failed: {reason}")))?;
        let materials = ledgers.get(&world_id).cloned().unwrap_or_default();
        Ok(Self {
            event: event.clone(),
            ledgers,
            materials,
            pending: Some((
                *job_id,
                Some(MaterialTransitJobState {
                    job_id: *job_id,
                    requester_agent_id: requester_agent_id.clone(),
                    from_ledger: from_ledger.clone(),
                    to_ledger: to_ledger.clone(),
                    kind: kind.clone(),
                    amount: *amount,
                    distance_km: *distance_km,
                    loss_bps: *loss_bps,
                    priority: *priority,
                    route_id: route_id.clone(),
                    path_id: path_id.clone(),
                    route_ids: route_ids.clone(),
                    tariff_electricity_total: *tariff_electricity_total,
                    reroute_count: *reroute_count,
                    ready_at: *ready_at,
                }),
            )),
            route_updates,
            completed_route_ids: BTreeSet::new(),
            path: None,
            settled_id: None,
            receipt: None,
            agents,
            progress: None,
        })
    }

    fn prepare_completed(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::MaterialTransitCompleted {
            job_id,
            requester_agent_id,
            from_ledger,
            to_ledger,
            kind,
            sent_amount,
            received_amount,
            loss_amount,
            distance_km,
            priority,
            route_id,
            path_id,
            route_ids,
            tariff_electricity_total,
            reroute_count,
        } = event
        else {
            unreachable!()
        };
        let (materials, world) = normalized_materials(state);
        if state.settled_logistics_transit_ids.contains(job_id) {
            let agents = state
                .agents
                .get(requester_agent_id)
                .cloned()
                .map(|cell| BTreeMap::from([(requester_agent_id.clone(), cell)]))
                .unwrap_or_default();
            return Ok(Self::no_op(event, materials, world, agents));
        }
        let pending = state
            .pending_material_transits
            .get(job_id)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "material transit completion has no pending job: job_id={job_id}"
                ))
            })?;
        if now < pending.ready_at {
            return Err(invalid(format!(
                "material transit completion is early: job_id={job_id} ready_at={} now={now}",
                pending.ready_at
            )));
        }
        let expected_loss = {
            let amount = pending.amount.max(0);
            ((amount as i128)
                .saturating_mul(pending.distance_km as i128)
                .saturating_mul(pending.loss_bps as i128)
                / 10_000)
                .clamp(0, amount as i128) as i64
        };
        let conservation_valid = *sent_amount >= 0
            && *received_amount >= 0
            && *loss_amount >= 0
            && *received_amount <= *sent_amount
            && sent_amount.checked_sub(*received_amount) == Some(*loss_amount)
            && *loss_amount == expected_loss
            && *received_amount == pending.amount.saturating_sub(expected_loss);
        let identity_valid = pending.requester_agent_id == *requester_agent_id
            && pending.from_ledger == *from_ledger
            && pending.to_ledger == *to_ledger
            && pending.kind == *kind
            && pending.amount == *sent_amount
            && pending.distance_km == *distance_km
            && pending.priority == *priority
            && pending.route_id == *route_id
            && pending.path_id == *path_id
            && pending.route_ids == *route_ids
            && pending.tariff_electricity_total == *tariff_electricity_total
            && pending.reroute_count == *reroute_count;
        if !identity_valid || !conservation_valid {
            return Err(invalid(format!(
                "material transit completion does not match pending commitment: job_id={job_id}"
            )));
        }
        let world_id = MaterialLedgerId::world();
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if to_ledger != &world_id {
            ledgers.insert(
                to_ledger.clone(),
                state
                    .material_ledgers
                    .get(to_ledger)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        if *received_amount > 0 {
            preflight_material_balance_additions(
                &ledgers,
                to_ledger,
                &[MaterialStack::new(kind.clone(), *received_amount)],
            )
            .map_err(|reason| {
                invalid(format!(
                    "material transit completion preflight failed: {reason}"
                ))
            })?;
        }
        let reservation_routes = pending.route_ids.clone();
        let completed_path_id = pending.path_id.clone().or_else(|| path_id.clone());
        let path = prepare_path_authority(
            state,
            &pending,
            completed_path_id,
            (*received_amount).max(0),
        )?;
        if *received_amount > 0 {
            add_material_balance_for_ledger(&mut ledgers, to_ledger, kind, *received_amount)
                .map_err(|reason| {
                    invalid(format!("material transit completion failed: {reason}"))
                })?;
        }
        let materials = ledgers.get(&world_id).cloned().unwrap_or_default();
        let mut completed_route_ids = reservation_routes.iter().cloned().collect::<BTreeSet<_>>();
        if let Some(route_id) = route_id {
            completed_route_ids.insert(route_id.clone());
        }
        let mut route_updates = BTreeMap::new();
        for id in &reservation_routes {
            if let Some(mut route) = route_updates
                .get(id)
                .or_else(|| state.logistics_routes.get(id))
                .cloned()
            {
                route.reserved_capacity_units =
                    route.reserved_capacity_units.saturating_sub(pending.amount);
                route_updates.insert(id.clone(), route);
            }
        }
        let tariff_total = pending.tariff_electricity_total.max(0);
        let mut owner_fees = BTreeMap::<String, i64>::new();
        for id in &reservation_routes {
            if let Some(route) = state.logistics_routes.get(id) {
                let edge_fee = route
                    .tariff_electricity_per_unit
                    .saturating_mul(pending.amount)
                    .max(0);
                if edge_fee > 0 {
                    *owner_fees.entry(route.owner_agent_id.clone()).or_default() = owner_fees
                        .get(&route.owner_agent_id)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(edge_fee);
                }
            }
        }
        let mut agents = BTreeMap::new();
        let mut remaining = tariff_total;
        for (owner, owner_fee) in &owner_fees {
            if remaining == 0 {
                break;
            }
            let credit = (*owner_fee).min(remaining);
            if let Some(mut cell) = state.agents.get(owner).cloned() {
                cell.state
                    .resources
                    .add(ResourceKind::Electricity, credit)
                    .map_err(|err| invalid(format!("logistics tariff credit failed: {err:?}")))?;
                agents.insert(owner.clone(), cell);
            }
            remaining = remaining.saturating_sub(credit);
        }
        let requester = agents
            .remove(requester_agent_id)
            .or_else(|| state.agents.get(requester_agent_id).cloned());
        if let Some(mut requester) = requester {
            requester.last_active = now;
            agents.insert(requester_agent_id.clone(), requester);
        }
        let mut progress = state.industry_progress.clone();
        progress.completed_material_transits =
            progress.completed_material_transits.saturating_add(1);
        refresh_progress_stage(state, &mut progress, now);
        Ok(Self {
            event: event.clone(),
            ledgers,
            materials,
            pending: Some((*job_id, None)),
            route_updates,
            completed_route_ids,
            path: path.map(|value| (value.path_id.clone(), value)),
            settled_id: Some(*job_id),
            receipt: Some((
                *job_id,
                LogisticsSettlementReceiptV1 {
                    job_id: *job_id,
                    path_id: pending.path_id.clone().or_else(|| path_id.clone()),
                    route_ids: reservation_routes,
                    tariff_electricity_total: tariff_total,
                    owner_payouts: owner_fees,
                    governance_tax_electricity: 0,
                },
            )),
            agents,
            progress: Some(progress),
        })
    }

    fn no_op(
        event: &DomainEvent,
        materials: BTreeMap<String, i64>,
        world: BTreeMap<String, i64>,
        agents: BTreeMap<String, AgentCell>,
    ) -> Self {
        Self {
            event: event.clone(),
            ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
            materials,
            pending: None,
            route_updates: BTreeMap::new(),
            completed_route_ids: BTreeSet::new(),
            path: None,
            settled_id: None,
            receipt: None,
            agents,
            progress: None,
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
                    state.pending_material_transits.insert(id, value);
                }
                None => {
                    state.pending_material_transits.remove(&id);
                }
            }
        }
        state.logistics_routes.extend(self.route_updates);
        state
            .completed_logistics_route_ids
            .extend(self.completed_route_ids);
        if let Some((id, path)) = self.path {
            state.completed_logistics_paths.insert(id, path);
        }
        if let Some(id) = self.settled_id {
            state.settled_logistics_transit_ids.insert(id);
        }
        if let Some((id, receipt)) = self.receipt {
            state.logistics_settlement_receipts.insert(id, receipt);
        }
        state.agents.extend(self.agents);
        if let Some(progress) = self.progress {
            state.industry_progress = progress;
        }
    }

    pub(super) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut updates = self.agents.clone();
        if let Some(actor) = self.event.agent_id()
            && let Some(cell) = updates.get_mut(actor)
        {
            cell.mailbox.push_back(self.event.clone());
        }
        out.serialize_field(
            "agents",
            &SparseOverlay {
                base: &state.agents,
                updates: &updates,
            },
        )
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

    pub(super) fn serialize_logistics<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "logistics_routes",
            &SparseOverlay {
                base: &state.logistics_routes,
                updates: &self.route_updates,
            },
        )?;
        out.serialize_field(
            "completed_logistics_route_ids",
            &SetInsertOverlay {
                base: &state.completed_logistics_route_ids,
                inserts: &self.completed_route_ids,
            },
        )?;
        let paths = self.path.clone().into_iter().collect();
        out.serialize_field(
            "completed_logistics_paths",
            &SparseOverlay {
                base: &state.completed_logistics_paths,
                updates: &paths,
            },
        )?;
        out.serialize_field(
            "settled_logistics_transit_ids",
            &SetInsertOneOverlay {
                base: &state.settled_logistics_transit_ids,
                insert: self.settled_id.as_ref(),
            },
        )?;
        let receipts = self.receipt.clone().into_iter().collect();
        out.serialize_field(
            "logistics_settlement_receipts",
            &SparseOverlay {
                base: &state.logistics_settlement_receipts,
                updates: &receipts,
            },
        )?;
        out.serialize_field(
            "direct_material_transfer_receipts",
            &state.direct_material_transfer_receipts,
        )
    }

    pub(super) fn serialize_pending_and_progress<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let changes = self.pending.clone().into_iter().collect();
        out.serialize_field(
            "pending_material_transits",
            &SparseOptionalOverlay {
                base: &state.pending_material_transits,
                changes: &changes,
            },
        )?;
        out.serialize_field(
            "industry_progress",
            self.progress.as_ref().unwrap_or(&state.industry_progress),
        )
    }
}

fn prepare_path_authority(
    state: &WorldState,
    pending: &MaterialTransitJobState,
    path_id: Option<String>,
    settled_amount: i64,
) -> Result<Option<LogisticsPathAuthorityV1>, WorldError> {
    let Some(path_id) = path_id else {
        return Ok(None);
    };
    let mut authority = if let Some(existing) = state.completed_logistics_paths.get(&path_id) {
        if existing.route_ids != pending.route_ids
            || existing.from_ledger != pending.from_ledger
            || existing.to_ledger != pending.to_ledger
            || existing.kind != pending.kind
            || existing.settled_amount < 0
            || existing.remaining_recipe_amount < 0
            || existing.remaining_recipe_amount > existing.settled_amount
        {
            return Err(invalid(format!(
                "material transit path authority does not match settlement: path_id={path_id}"
            )));
        }
        existing.clone()
    } else {
        LogisticsPathAuthorityV1 {
            path_id: path_id.clone(),
            route_ids: pending.route_ids.clone(),
            from_ledger: pending.from_ledger.clone(),
            to_ledger: pending.to_ledger.clone(),
            kind: pending.kind.clone(),
            settled_amount: 0,
            remaining_recipe_amount: 0,
        }
    };
    authority.settled_amount = authority
        .settled_amount
        .checked_add(settled_amount)
        .ok_or_else(|| {
            invalid(format!(
                "material transit path settled quantity overflow: path_id={path_id}"
            ))
        })?;
    authority.remaining_recipe_amount = authority
        .remaining_recipe_amount
        .checked_add(settled_amount)
        .ok_or_else(|| {
            invalid(format!(
                "material transit path recipe quantity overflow: path_id={path_id}"
            ))
        })?;
    Ok(Some(authority))
}

pub(super) fn refresh_progress_stage(
    state: &WorldState,
    progress: &mut IndustryProgressState,
    now: WorldTime,
) {
    let current = progress.stage;
    let active_completed_jobs = state
        .factories
        .values()
        .map(|factory| factory.production.completed_jobs)
        .sum::<u64>();
    let mut next = IndustryStage::Bootstrap;
    if state
        .factories
        .values()
        .any(factory_has_canonical_stable_line)
    {
        next = IndustryStage::ScaleOut;
    }
    let governance_enabled =
        state.gameplay_policy.electricity_tax_bps > 0 || state.gameplay_policy.data_tax_bps > 0;
    if next == IndustryStage::ScaleOut
        && governance_enabled
        && (active_completed_jobs >= 6 || progress.completed_material_transits >= 3)
    {
        next = IndustryStage::Governance;
    }
    if next != current {
        progress.stage = next;
        progress.stage_updated_at = now;
    }
}
