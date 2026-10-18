use super::*;

fn refresh_progress_without_factory(
    state: &WorldState,
    progress: &mut IndustryProgressState,
    now: WorldTime,
    removed: &str,
) {
    let current = progress.stage;
    let active_completed_jobs = state
        .factories
        .iter()
        .filter(|(id, _)| id.as_str() != removed)
        .map(|(_, factory)| factory.production.completed_jobs)
        .sum::<u64>();
    let stable = state
        .factories
        .iter()
        .filter(|(id, _)| id.as_str() != removed)
        .any(|(_, factory)| factory_has_canonical_stable_line(factory));
    let mut next = if stable {
        IndustryStage::ScaleOut
    } else {
        IndustryStage::Bootstrap
    };
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

impl PreparedFactoryLifecycle {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        match event {
            DomainEvent::FactoryBuildStarted {
                job_id,
                builder_agent_id,
                site_id,
                spec,
                consume_ledger,
                ready_at,
                contract_version,
                site_authority_revision,
                site_location_id,
                location_anchor_revision,
                construction_power_obligation,
            } => Self::prepare_started(
                state,
                event,
                now,
                *job_id,
                builder_agent_id,
                site_id,
                spec,
                consume_ledger,
                *ready_at,
                *contract_version,
                *site_authority_revision,
                site_location_id,
                *location_anchor_revision,
                construction_power_obligation,
            ),
            DomainEvent::FactoryBuilt {
                job_id,
                builder_agent_id,
                site_id,
                spec,
            } => Self::prepare_built(state, event, now, *job_id, builder_agent_id, site_id, spec),
            DomainEvent::FactoryDurabilityChanged {
                factory_id,
                durability_ppm,
                ..
            } => {
                let (materials, world) = normalized_materials(state);
                let factory = state.factories.get(factory_id).cloned().map(|mut value| {
                    value.durability_ppm = (*durability_ppm).clamp(0, 1_000_000);
                    (factory_id.clone(), Some(value))
                });
                Ok(Self {
                    event: event.clone(),
                    pending: None,
                    factory,
                    settled_id: None,
                    retired_insert: None,
                    pending_recipe_deletions: BTreeSet::new(),
                    ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
                    materials,
                    agent: None,
                    progress: None,
                    construction_receipt: None,
                    recycle_receipt: None,
                })
            }
            DomainEvent::FactoryMaintained {
                operator_agent_id,
                factory_id,
                consume_ledger,
                consumed_parts,
                durability_ppm,
            } => Self::prepare_maintained(
                state,
                event,
                now,
                operator_agent_id,
                factory_id,
                consume_ledger,
                *consumed_parts,
                *durability_ppm,
            ),
            DomainEvent::FactoryRecycled {
                operator_agent_id,
                factory_id,
                recycle_ledger,
                recovered,
                durability_ppm,
            } => {
                let receipt = FactoryRecycleReceiptV1 {
                    operator_agent_id: operator_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recycle_ledger: recycle_ledger.clone(),
                    recovered: recovered.clone(),
                    durability_ppm: *durability_ppm,
                };
                if let Some(existing) = state.factory_recycle_receipts.get(factory_id)
                    && existing != &receipt
                {
                    return Err(invalid(format!(
                        "factory recycle conflicts with persisted receipt: factory_id={factory_id}"
                    )));
                }
                Self::prepare_recycled(
                    state,
                    event,
                    now,
                    operator_agent_id,
                    factory_id,
                    recycle_ledger,
                    recovered,
                )
                .map(|mut prepared| {
                    if !state.retired_factory_ids.contains(factory_id) {
                        prepared.recycle_receipt = Some((factory_id.clone(), receipt));
                    }
                    prepared
                })
            }
            _ => Err(invalid(
                "factory lifecycle preparation requires supported event",
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_started(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
        job_id: ActionId,
        builder: &str,
        site_id: &str,
        spec: &FactoryModuleSpec,
        ledger_id: &MaterialLedgerId,
        ready_at: WorldTime,
        contract_version: Option<u8>,
        site_authority_revision: Option<u64>,
        site_location_id: &Option<String>,
        location_anchor_revision: Option<u64>,
        construction_power_obligation: &Option<FactoryBuildPowerObligationV1>,
    ) -> Result<Self, WorldError> {
        if state.pending_factory_builds.contains_key(&job_id)
            || state.settled_factory_build_ids.contains(&job_id)
        {
            return Err(invalid(format!(
                "factory build job already settled or pending: job_id={job_id}"
            )));
        }
        if builder.trim().is_empty() || !state.agents.contains_key(builder) {
            return Err(invalid(format!(
                "factory build builder agent is missing or empty: builder_agent_id={builder}"
            )));
        }
        if site_id.trim().is_empty() {
            return Err(invalid("factory build site_id cannot be empty"));
        }
        if spec.factory_id.trim().is_empty() {
            return Err(invalid("factory build factory_id cannot be empty"));
        }
        if ready_at <= now {
            return Err(invalid(format!(
                "factory build ready_at must be in the future: now={now} ready_at={ready_at}"
            )));
        }
        if state.retired_factory_ids.contains(&spec.factory_id) {
            return Err(invalid(format!(
                "factory build started for retired identity: factory_id={}",
                spec.factory_id
            )));
        }
        if state.factories.contains_key(&spec.factory_id)
            || state
                .pending_factory_builds
                .values()
                .any(|job| job.spec.factory_id == spec.factory_id)
        {
            return Err(invalid(format!(
                "factory build identity is already active: factory_id={}",
                spec.factory_id
            )));
        }
        let has_modern_facts = site_authority_revision.is_some()
            || site_location_id.is_some()
            || location_anchor_revision.is_some()
            || construction_power_obligation.is_some();
        let modern = match contract_version {
            None if has_modern_facts => {
                return Err(invalid(format!(
                    "modern factory build event is missing its contract discriminator: factory_id={}",
                    spec.factory_id
                )));
            }
            None | Some(0) => false,
            Some(FACTORY_BUILD_STARTED_MODERN_VERSION) => true,
            Some(version) => {
                return Err(invalid(format!(
                    "unsupported factory build event contract version: {version}"
                )));
            }
        };
        if modern
            && (site_authority_revision.is_none()
                || site_location_id.is_none()
                || location_anchor_revision.is_none()
                || construction_power_obligation.is_none())
        {
            return Err(invalid(format!(
                "modern factory build event is missing authority or construction facts: factory_id={}",
                spec.factory_id
            )));
        }
        if let Some(obligation) = construction_power_obligation {
            let site_revision = site_authority_revision.ok_or_else(|| {
                invalid("factory build power obligation requires site authority revision")
            })?;
            let site_location = site_location_id
                .as_deref()
                .ok_or_else(|| invalid("factory build power obligation requires site location"))?;
            let site = state.factory_site_authorities.get(site_id).ok_or_else(|| {
                invalid(format!(
                    "factory build site authority missing: site_id={site_id}"
                ))
            })?;
            let location = state.agent_location_authorities.get(builder).ok_or_else(|| invalid(format!("factory build builder location authority missing: builder_agent_id={builder}")))?;
            let anchor_matches = location_anchor_revision.map_or(!modern, |revision| {
                state
                    .location_anchors
                    .get(site_location)
                    .is_some_and(|anchor| {
                        anchor.location_id == site_location
                            && anchor.authority_revision == revision
                            && anchor.active
                            && anchor.effective_at <= now
                    })
            });
            if site.site_id != site_id
                || location.agent_id != builder
                || site_revision != site.authority_revision
                || site_location != site.location_id
                || !site.active
                || !site.chunk_ready
                || !location.active
                || location.location_id != site.location_id
                || location.effective_at > now
                || !anchor_matches
                || (site.owner_agent_id != builder
                    && !site.authorized_agent_ids.iter().any(|id| id == builder))
            {
                return Err(invalid(format!(
                    "factory build authority changed or is unavailable: site_id={site_id} builder_agent_id={builder}"
                )));
            }
            if obligation.payer_agent_id != builder
                || obligation.profile_key != spec.factory_id
                || obligation.profile_revision == 0
                || obligation.electricity_amount < 0
                || obligation.mode != FactoryConstructionPowerMode::StartOnlySink
            {
                return Err(invalid(format!(
                    "factory build power obligation is invalid: factory_id={} builder_agent_id={builder}",
                    spec.factory_id
                )));
            }
            if modern && obligation.material_ledger.as_ref() != Some(ledger_id) {
                return Err(invalid(format!(
                    "modern factory build construction ledger does not match consume ledger: factory_id={}",
                    spec.factory_id
                )));
            }
            if modern {
                let profile = state
                    .factory_profiles
                    .get(&spec.factory_id)
                    .ok_or_else(|| {
                        invalid(format!(
                            "modern factory build canonical profile missing: factory_id={}",
                            spec.factory_id
                        ))
                    })?;
                super::super::factory_authority::canonical_factory_profile_matches_spec(
                    profile, spec,
                )
                .map_err(invalid)?;
            }
            let profile = state
                .factory_construction_power_profiles
                .get(&spec.factory_id)
                .ok_or_else(|| {
                    invalid(format!(
                        "factory build construction power profile missing: factory_id={}",
                        spec.factory_id
                    ))
                })?;
            if !profile.active
                || profile.authority_revision != obligation.profile_revision
                || profile.factory_id != spec.factory_id
                || profile.electricity_amount != obligation.electricity_amount
                || profile.mode != obligation.mode
            {
                return Err(invalid(format!(
                    "factory build construction power profile changed: factory_id={}",
                    spec.factory_id
                )));
            }
        }
        let mut required = BTreeMap::<String, i64>::new();
        for stack in &spec.build_cost {
            if stack.kind.trim().is_empty() || stack.amount <= 0 {
                return Err(invalid(format!(
                    "factory build cost must be positive and named: {}={}",
                    stack.kind, stack.amount
                )));
            }
            let total = required.entry(stack.kind.clone()).or_default();
            *total = total.checked_add(stack.amount).ok_or_else(|| {
                invalid(format!(
                    "factory build cost overflow: kind={} amount={}",
                    stack.kind, stack.amount
                ))
            })?;
        }
        let (mut materials, world) = normalized_materials(state);
        let world_id = MaterialLedgerId::world();
        for (kind, amount) in &required {
            let available = (if ledger_id == &world_id {
                Some(&world)
            } else {
                state.material_ledgers.get(ledger_id)
            })
            .and_then(|ledger| ledger.get(kind))
            .copied()
            .unwrap_or(0);
            if available < *amount {
                return Err(invalid(format!(
                    "factory build consume failed: insufficient material {kind}: requested={amount} available={available}"
                )));
            }
        }
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if ledger_id != &world_id {
            ledgers.insert(
                ledger_id.clone(),
                state
                    .material_ledgers
                    .get(ledger_id)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        for stack in &spec.build_cost {
            remove_material_balance_for_ledger(&mut ledgers, ledger_id, &stack.kind, stack.amount)
                .map_err(|reason| invalid(format!("factory build consume failed: {reason}")))?;
        }
        if ledger_id == &world_id {
            materials = ledgers.get(&world_id).cloned().unwrap_or_default();
        }
        let mut agent = state
            .agents
            .get(builder)
            .cloned()
            .expect("validated builder");
        if let Some(obligation) = construction_power_obligation {
            agent
                .state
                .resources
                .remove(ResourceKind::Electricity, obligation.electricity_amount)
                .map_err(|error| invalid(format!("factory build consume failed: {error:?}")))?;
        }
        agent.last_active = now;
        Ok(Self {
            event: event.clone(),
            pending: Some((
                job_id,
                Some(FactoryBuildJobState {
                    job_id,
                    builder_agent_id: builder.into(),
                    site_id: site_id.into(),
                    spec: spec.clone(),
                    consume_ledger: ledger_id.clone(),
                    ready_at,
                    contract_version: contract_version.unwrap_or_default(),
                    site_authority_revision,
                    site_location_id: site_location_id.clone(),
                    location_anchor_revision,
                    construction_power_obligation: construction_power_obligation.clone(),
                }),
            )),
            factory: None,
            settled_id: None,
            retired_insert: None,
            pending_recipe_deletions: BTreeSet::new(),
            ledgers,
            materials,
            agent: Some((builder.into(), agent)),
            progress: None,
            construction_receipt: None,
            recycle_receipt: None,
        })
    }

    fn prepare_built(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
        job_id: ActionId,
        builder: &str,
        site_id: &str,
        spec: &FactoryModuleSpec,
    ) -> Result<Self, WorldError> {
        let (materials, world) = normalized_materials(state);
        if state.settled_factory_build_ids.contains(&job_id) {
            if state
                .factories
                .get(&spec.factory_id)
                .is_some_and(|factory| {
                    factory.builder_agent_id == builder
                        && factory.site_id == site_id
                        && factory.spec == *spec
                })
            {
                return Ok(Self::no_op(state, event, builder, materials, world));
            }
            return Err(invalid(format!(
                "settled factory build completion does not match receipt: job_id={job_id}"
            )));
        }
        let pending = state.pending_factory_builds.get(&job_id).ok_or_else(|| {
            invalid(format!(
                "factory build completion has no pending job: job_id={job_id}"
            ))
        })?;
        if state.retired_factory_ids.contains(&spec.factory_id) {
            return Err(invalid(format!(
                "factory built for retired identity: factory_id={}",
                spec.factory_id
            )));
        }
        if pending.builder_agent_id != builder
            || pending.site_id != site_id
            || pending.spec != *spec
        {
            return Err(invalid(format!(
                "factory build completion does not match pending commitment: job_id={job_id}"
            )));
        }
        if now < pending.ready_at {
            return Err(invalid(format!(
                "factory build completion is early: job_id={job_id} ready_at={} now={now}",
                pending.ready_at
            )));
        }
        if state.factories.contains_key(&spec.factory_id) {
            return Err(invalid(format!(
                "factory build completion would overwrite active factory: factory_id={}",
                spec.factory_id
            )));
        }
        let site_ledger = MaterialLedgerId::site(site_id.to_string());
        let factory = FactoryState {
            factory_id: spec.factory_id.clone(),
            site_id: site_id.into(),
            builder_agent_id: builder.into(),
            spec: spec.clone(),
            input_ledger: site_ledger.clone(),
            output_ledger: site_ledger,
            durability_ppm: 1_000_000,
            production: FactoryProductionState::default(),
            site_authority_revision: pending.site_authority_revision,
            site_location_id: pending.site_location_id.clone(),
            location_anchor_revision: pending.location_anchor_revision,
            construction_power_profile_key: pending
                .construction_power_obligation
                .as_ref()
                .map(|value| value.profile_key.clone()),
            construction_power_profile_revision: pending
                .construction_power_obligation
                .as_ref()
                .map(|value| value.profile_revision),
            built_at: now,
        };
        let agent = state.agents.get(builder).cloned().map(|mut cell| {
            cell.last_active = now;
            (builder.to_string(), cell)
        });
        let mut progress = state.industry_progress.clone();
        material_transit::refresh_progress_stage(state, &mut progress, now);
        Ok(Self {
            event: event.clone(),
            pending: Some((job_id, None)),
            factory: Some((spec.factory_id.clone(), Some(factory))),
            settled_id: Some(job_id),
            retired_insert: None,
            pending_recipe_deletions: BTreeSet::new(),
            ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
            materials,
            agent,
            progress: Some(progress),
            construction_receipt: pending
                .construction_power_obligation
                .clone()
                .map(|value| (spec.factory_id.clone(), value)),
            recycle_receipt: None,
        })
    }

    fn no_op(
        state: &WorldState,
        event: &DomainEvent,
        builder: &str,
        materials: BTreeMap<String, i64>,
        world: BTreeMap<String, i64>,
    ) -> Self {
        Self {
            event: event.clone(),
            pending: None,
            factory: None,
            settled_id: None,
            retired_insert: None,
            pending_recipe_deletions: BTreeSet::new(),
            ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
            materials,
            agent: state
                .agents
                .get(builder)
                .cloned()
                .map(|cell| (builder.to_string(), cell)),
            progress: None,
            construction_receipt: None,
            recycle_receipt: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_maintained(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
        operator: &str,
        factory_id: &str,
        ledger_id: &MaterialLedgerId,
        parts: i64,
        durability: i64,
    ) -> Result<Self, WorldError> {
        if state.retired_factory_ids.contains(factory_id) {
            return Err(invalid(format!(
                "factory maintenance targets retired identity: factory_id={factory_id}"
            )));
        }
        let mut factory = state.factories.get(factory_id).cloned().ok_or_else(|| {
            invalid(format!(
                "factory maintenance targets unknown factory: factory_id={factory_id}"
            ))
        })?;
        if factory.builder_agent_id != operator {
            return Err(invalid(format!(
                "factory maintenance operator mismatch: factory_id={factory_id} operator={operator} builder={} ",
                factory.builder_agent_id
            )));
        }
        if parts <= 0 {
            return Err(invalid(format!(
                "factory maintenance consumed_parts must be positive: factory_id={factory_id} consumed_parts={parts}"
            )));
        }
        if !(0..=1_000_000).contains(&durability) {
            return Err(invalid(format!(
                "factory maintenance durability is out of range: factory_id={factory_id} durability_ppm={durability}"
            )));
        }
        let (mut materials, world) = normalized_materials(state);
        let world_id = MaterialLedgerId::world();
        let available = (if ledger_id == &world_id {
            Some(&world)
        } else {
            state.material_ledgers.get(ledger_id)
        })
        .and_then(|v| v.get("hardware_part"))
        .copied()
        .unwrap_or(0);
        if available < parts {
            return Err(invalid(format!(
                "factory maintenance consume failed: insufficient material hardware_part: requested={parts} available={available}"
            )));
        }
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if ledger_id != &world_id {
            ledgers.insert(
                ledger_id.clone(),
                state
                    .material_ledgers
                    .get(ledger_id)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        remove_material_balance_for_ledger(&mut ledgers, ledger_id, "hardware_part", parts)
            .map_err(|reason| invalid(format!("factory maintenance consume failed: {reason}")))?;
        if ledger_id == &world_id {
            materials = ledgers[&world_id].clone();
        }
        factory.durability_ppm = durability;
        let agent = state.agents.get(operator).cloned().map(|mut cell| {
            cell.last_active = now;
            (operator.to_string(), cell)
        });
        Ok(Self {
            event: event.clone(),
            pending: None,
            factory: Some((factory_id.to_string(), Some(factory))),
            settled_id: None,
            retired_insert: None,
            pending_recipe_deletions: BTreeSet::new(),
            ledgers,
            materials,
            agent,
            progress: None,
            construction_receipt: None,
            recycle_receipt: None,
        })
    }

    fn prepare_recycled(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
        operator: &str,
        factory_id: &str,
        ledger_id: &MaterialLedgerId,
        recovered: &[MaterialStack],
    ) -> Result<Self, WorldError> {
        let (mut materials, world) = normalized_materials(state);
        if state.retired_factory_ids.contains(factory_id) {
            return Ok(Self {
                event: event.clone(),
                pending: None,
                factory: None,
                settled_id: None,
                retired_insert: None,
                pending_recipe_deletions: BTreeSet::new(),
                ledgers: BTreeMap::from([(MaterialLedgerId::world(), world)]),
                materials,
                agent: state
                    .agents
                    .get(operator)
                    .cloned()
                    .map(|cell| (operator.to_string(), cell)),
                progress: None,
                construction_receipt: None,
                recycle_receipt: None,
            });
        }
        let factory = state.factories.get(factory_id).ok_or_else(|| {
            invalid(format!(
                "factory recycle targets unknown factory: factory_id={factory_id}"
            ))
        })?;
        if factory.builder_agent_id != operator {
            return Err(invalid(format!(
                "factory recycle operator mismatch: factory_id={factory_id} operator={operator} builder={} ",
                factory.builder_agent_id
            )));
        }
        if state
            .pending_recipe_jobs
            .values()
            .any(|job| job.factory_id == factory_id)
        {
            return Err(invalid(format!(
                "factory recycle targets factory with active recipe: factory_id={factory_id}"
            )));
        }
        preflight_material_balance_additions(&state.material_ledgers, ledger_id, recovered)
            .map_err(|reason| {
                invalid(format!(
                    "factory recycle material preflight failed: {reason}"
                ))
            })?;
        let world_id = MaterialLedgerId::world();
        let mut ledgers = BTreeMap::from([(world_id.clone(), world)]);
        if ledger_id != &world_id {
            ledgers.insert(
                ledger_id.clone(),
                state
                    .material_ledgers
                    .get(ledger_id)
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        for stack in recovered {
            add_material_balance_for_ledger(&mut ledgers, ledger_id, &stack.kind, stack.amount)
                .map_err(|reason| {
                    invalid(format!("factory recycle material add failed: {reason}"))
                })?;
        }
        if ledger_id == &world_id {
            materials = ledgers[&world_id].clone();
        }
        let agent = state.agents.get(operator).cloned().map(|mut cell| {
            cell.last_active = now;
            (operator.to_string(), cell)
        });
        let mut progress = state.industry_progress.clone();
        refresh_progress_without_factory(state, &mut progress, now, factory_id);
        let pending_recipe_deletions = state
            .pending_recipe_jobs
            .iter()
            .filter(|(_, job)| job.factory_id == factory_id)
            .map(|(id, _)| *id)
            .collect();
        Ok(Self {
            event: event.clone(),
            pending: None,
            factory: Some((factory_id.to_string(), None)),
            settled_id: None,
            retired_insert: Some(factory_id.to_string()),
            pending_recipe_deletions,
            ledgers,
            materials,
            agent,
            progress: Some(progress),
            construction_receipt: None,
            recycle_receipt: None,
        })
    }

    pub(crate) fn matches(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install(self, state: &mut WorldState) {
        state.material_ledgers.extend(self.ledgers);
        state.materials = self.materials;
        if let Some((id, pending)) = self.pending {
            match pending {
                Some(job) => {
                    state.pending_factory_builds.insert(id, job);
                }
                None => {
                    state.pending_factory_builds.remove(&id);
                }
            }
        }
        if let Some((id, factory)) = self.factory {
            match factory {
                Some(value) => {
                    state.factories.insert(id, value);
                }
                None => {
                    state.factories.remove(&id);
                }
            }
        }
        if let Some(id) = self.settled_id {
            state.settled_factory_build_ids.insert(id);
        }
        if let Some(id) = self.retired_insert {
            state.retired_factory_ids.insert(id);
        }
        for id in self.pending_recipe_deletions {
            state.pending_recipe_jobs.remove(&id);
        }
        if let Some((id, agent)) = self.agent {
            state.agents.insert(id, agent);
        }
        if let Some(progress) = self.progress {
            state.industry_progress = progress;
        }
        if let Some((id, receipt)) = self.construction_receipt {
            state.factory_construction_receipts.insert(id, receipt);
        }
        if let Some((id, receipt)) = self.recycle_receipt {
            state.factory_recycle_receipts.insert(id, receipt);
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

    pub(super) fn serialize_factory_fields<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let factories = self.factory.clone().into_iter().collect();
        out.serialize_field(
            "factories",
            &SparseOptionalOverlay {
                base: &state.factories,
                changes: &factories,
            },
        )?;
        out.serialize_field(
            "retired_factory_ids",
            &SetInsertOneOverlay {
                base: &state.retired_factory_ids,
                insert: self.retired_insert.as_ref(),
            },
        )?;
        out.serialize_field(
            "settled_factory_build_ids",
            &SetInsertOneOverlay {
                base: &state.settled_factory_build_ids,
                insert: self.settled_id.as_ref(),
            },
        )?;
        let pending = self.pending.clone().into_iter().collect();
        out.serialize_field(
            "pending_factory_builds",
            &SparseOptionalOverlay {
                base: &state.pending_factory_builds,
                changes: &pending,
            },
        )?;
        let recipe_deletions = self
            .pending_recipe_deletions
            .iter()
            .map(|id| (*id, None))
            .collect();
        out.serialize_field(
            "pending_recipe_jobs",
            &SparseOptionalOverlay {
                base: &state.pending_recipe_jobs,
                changes: &recipe_deletions,
            },
        )?;
        out.serialize_field("settled_recipe_job_ids", &state.settled_recipe_job_ids)
    }

    pub(super) fn serialize_transit_and_progress<S: SerializeStruct>(
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

    pub(super) fn serialize_construction_receipts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if let Some((id, receipt)) = &self.construction_receipt {
            out.serialize_field(
                "factory_construction_receipts",
                &SparseOverlay {
                    base: &state.factory_construction_receipts,
                    updates: &BTreeMap::from([(id.clone(), receipt.clone())]),
                },
            )
        } else if !state.factory_construction_receipts.is_empty() {
            out.serialize_field(
                "factory_construction_receipts",
                &state.factory_construction_receipts,
            )
        } else {
            Ok(())
        }
    }

    pub(super) fn serialize_terminal_receipts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if !state.recipe_completion_receipts.is_empty() {
            out.serialize_field(
                "recipe_completion_receipts",
                &state.recipe_completion_receipts,
            )?;
        }
        if let Some((id, receipt)) = &self.recycle_receipt {
            out.serialize_field(
                "factory_recycle_receipts",
                &SparseOverlay {
                    base: &state.factory_recycle_receipts,
                    updates: &BTreeMap::from([(id.clone(), receipt.clone())]),
                },
            )?;
        } else if !state.factory_recycle_receipts.is_empty() {
            out.serialize_field("factory_recycle_receipts", &state.factory_recycle_receipts)?;
        }
        Ok(())
    }
}
