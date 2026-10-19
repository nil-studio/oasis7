use super::*;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_action::{
    EmergencyObservationV1 as Observation, EmergencySupplyCommandV1 as C,
};
pub(super) fn snapshot_key(region: &str, window: u64) -> String {
    format!(
        "observation:{}",
        blake3::hash(format!("{}:{region}:{window}", region.len()).as_bytes()).to_hex()
    )
}
pub(super) fn validate_location(state: &WorldState, agent: &str, location: &str, now: u64) -> bool {
    state
        .agent_location_authorities
        .get(agent)
        .is_some_and(|a| {
            a.active && a.agent_id == agent && a.location_id == location && a.effective_at <= now
        })
        && state
            .location_anchors
            .get(location)
            .is_some_and(|a| a.active && a.location_id == location && a.effective_at <= now)
}
impl PreparedWorldRule {
    pub(super) fn observe_supply(
        &mut self,
        state: &WorldState,
        region: &str,
        now: u64,
    ) -> Result<String, WorldError> {
        if !matches!(
            &self.event,
            DomainEvent::WorldRuleCommitted {
                action_id: 0,
                command: WorldRuleCommandV1::EmergencySupply {
                    command: C::Observe { .. }
                },
                ..
            }
        ) || now == 0
            || now % 60 != 0
        {
            return Err(invalid("missing_authority"));
        }
        let window = now - 60;
        let key = snapshot_key(region, window);
        if self.rules.emergency_supply.observations.contains_key(&key) {
            return Ok(key);
        }
        let services: Vec<_> = self
            .rules
            .emergency_supply
            .minimum_services
            .values()
            .map(|contract| &contract.registration)
            .filter(|s| {
                s.region_id == region
                    && s.registered_at
                        .checked_add(600)
                        .is_some_and(|time| time <= window)
            })
            .cloned()
            .collect();
        if services.is_empty() || services.len() > 4096 {
            return Err(invalid("need_source_unverifiable"));
        }
        // The denominator is the complete region registry, including eligible
        // services that never called the emergency registration endpoint.
        for service in &services {
            self.rules
                .emergency_supply
                .services
                .insert(service.obligation_id.clone(), service.clone());
        }
        let mut balances = BTreeMap::new();
        for s in &services {
            let source = self
                .rules
                .emergency_supply
                .minimum_services
                .get(&s.obligation_id)
                .ok_or_else(|| invalid("need_source_unverifiable"))?;
            if !super::minimum_service::source_owner_is_current(state, source)
                || !validate_location(state, &s.beneficiary_id, &s.location_id, now)
                || state.agent_location_authorities[&s.beneficiary_id].effective_at
                    > s.registered_at
                || state.location_anchors[&s.location_id].effective_at > s.registered_at
                || state
                    .gameplay_policy
                    .blocked_agents
                    .contains(&s.beneficiary_id)
            {
                return Err(invalid("need_source_unverifiable"));
            }
            balances.entry(s.beneficiary_id.clone()).or_insert_with(|| {
                state
                    .agents
                    .get(&s.beneficiary_id)
                    .map(|a| a.state.resources.get(ResourceKind::Electricity))
                    .unwrap_or(0)
                    .max(0)
            });
        }
        let mut normal: BTreeMap<String, i64> = services
            .iter()
            .map(|s| (s.obligation_id.clone(), 0))
            .collect();
        let mut intervals = BTreeMap::new();
        for index in 0..10u64 {
            let start = now
                .checked_add(index * 60)
                .ok_or_else(|| invalid("numeric_overflow"))?;
            for s in &services {
                let balance = balances.get_mut(&s.beneficiary_id).unwrap();
                let coverage = (*balance).min(s.electricity_per_interval);
                *balance -= coverage;
                let total = normal.get_mut(&s.obligation_id).unwrap();
                *total = total
                    .checked_add(coverage)
                    .ok_or_else(|| invalid("numeric_overflow"))?;
                intervals.insert(super::policy::need_key(&s.obligation_id, start), coverage);
            }
        }
        let root = crate::util::hash_json(state)?;
        self.rules.emergency_supply.observations.insert(
            key.clone(),
            Observation {
                window_start: window,
                world_root: root,
                obligation_ids: services.iter().map(|s| s.obligation_id.clone()).collect(),
                normal_coverage: normal,
            },
        );
        self.rules
            .emergency_supply
            .observation_interval_coverage
            .insert(key.clone(), intervals);
        if let Some(first) = now.checked_sub(180) {
            let snapshots: Option<Vec<_>> = [first, first + 60, first + 120]
                .into_iter()
                .map(|start| {
                    self.rules
                        .emergency_supply
                        .observations
                        .get(&snapshot_key(region, start))
                        .cloned()
                })
                .collect();
            if let Some(observations) = snapshots {
                let lineage = self
                    .rules
                    .emergency_supply
                    .region_lineages
                    .get(region)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?;
                let evidence = oasis7_agent_api::world_action::EmergencyEvidenceV1 {
                    region_id: region.into(),
                    region_budget_lineage: lineage.clone(),
                    source_ref: format!("observations:{}", crate::util::hash_json(&observations)?),
                    observations,
                };
                if self
                    .rules
                    .emergency_supply
                    .validate_observations(&evidence, now, false)
                    .is_ok()
                {
                    let id = format!("evidence:{}", crate::util::hash_json(&evidence)?);
                    self.rules.emergency_supply.evidence.insert(id, evidence);
                }
            }
        }
        Ok(key)
    }
}

impl EmergencySupplyStateV1 {
    pub(super) fn validate_current_sources(
        &self,
        state: &WorldState,
        e: &oasis7_agent_api::world_action::EmergencyEvidenceV1,
        now: u64,
    ) -> Result<(), WorldError> {
        for id in &e
            .observations
            .last()
            .ok_or_else(|| invalid("invalid_evidence"))?
            .obligation_ids
        {
            let service = self
                .services
                .get(id)
                .ok_or_else(|| invalid("need_source_unverifiable"))?;
            if !self.minimum_services.contains_key(id)
                || self.minimum_services.get(id).is_none_or(|source| {
                    !super::minimum_service::source_owner_is_current(state, source)
                })
                || !validate_location(state, &service.beneficiary_id, &service.location_id, now)
                || state
                    .gameplay_policy
                    .blocked_agents
                    .contains(&service.beneficiary_id)
            {
                return Err(invalid("need_source_unverifiable"));
            }
        }
        Ok(())
    }
}

impl EmergencySupplyStateV1 {
    pub fn due_observation_regions(&self, now: u64) -> Vec<String> {
        let Some(window) = now.checked_sub(60) else {
            return Vec::new();
        };
        if now % 60 != 0 {
            return Vec::new();
        }
        self.minimum_services
            .values()
            .filter(|source| {
                source
                    .registered_at
                    .checked_add(600)
                    .is_some_and(|time| time <= window)
            })
            .map(|source| source.registration.region_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|region| {
                !self
                    .observations
                    .contains_key(&snapshot_key(region, window))
            })
            .collect()
    }
}

impl EmergencySupplyStateV1 {
    pub fn prepare_consumption_for_world(
        &self,
        state: &WorldState,
        actor: &str,
        obligation: &str,
        recipe: &str,
        now: u64,
        quantity: i64,
    ) -> Result<Self, WorldError> {
        let source = self
            .minimum_services
            .get(obligation)
            .ok_or_else(|| invalid("need_source_unverifiable"))?;
        let service = self
            .services
            .get(obligation)
            .ok_or_else(|| invalid("need_source_unverifiable"))?;
        if service != &source.registration
            || service.beneficiary_id != actor
            || source
                .registered_at
                .checked_add(600)
                .is_none_or(|at| at > now)
            || !super::minimum_service::source_owner_is_current(state, source)
            || state
                .gameplay_policy
                .blocked_agents
                .contains(&actor.to_string())
        {
            return Err(invalid("need_source_unverifiable"));
        }
        if !validate_location(state, actor, &service.location_id, now) {
            return Err(invalid("unsupported_resource_path"));
        }
        self.prepare_consumption(actor, obligation, recipe, now, quantity)
    }
}
