use super::*;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_action::{
    EmergencyEvidenceV1 as Evidence, EmergencyServiceRegistrationV1 as Service,
    EmergencySupplyCommandV1 as C,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmergencySupplyStateV1 {
    pub world_id: Option<String>,
    pub minimum_services: BTreeMap<String, MinimumServiceContractV1>,
    pub region_lineages: BTreeMap<String, String>,
    pub observations: BTreeMap<String, oasis7_agent_api::world_action::EmergencyObservationV1>,
    pub observation_interval_coverage: BTreeMap<String, BTreeMap<String, i64>>,
    pub overflowed_windows: BTreeSet<(String, u64)>,
    pub services: BTreeMap<String, Service>,
    pub evidence: BTreeMap<String, Evidence>,
    pub authorizations: BTreeMap<String, EmergencyAuthorizationV1>,
    pub batches: BTreeMap<String, EmergencyBatchV1>,
    pub needs: BTreeMap<String, EmergencyNeedLedgerV1>,
    pub reviews: BTreeMap<String, EmergencyReviewV1>,
    pub restricted_power: BTreeMap<String, EmergencyRestrictedPowerV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyAuthorizationV1 {
    pub id: String,
    pub evidence_id: String,
    pub provider_id: String,
    pub reviewer_id: String,
    pub reviewer_controller_id: String,
    pub reviewer_owner_source_id: String,
    pub provider_controller_id: String,
    pub region_id: String,
    pub starts_at: u64,
    pub expires_at: u64,
    pub remaining: i64,
    pub active: bool,
    pub requested: BTreeSet<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyBatchV1 {
    pub authorization_id: String,
    pub frozen_at: u64,
    pub frozen_pool: i64,
    pub evidence_id: String,
    pub demands: BTreeMap<String, i64>,
    pub frozen_ledger: BTreeMap<String, EmergencyNeedLedgerV1>,
    pub allocation: BTreeMap<String, i64>,
    pub allocated: bool,
    pub committed_at: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyNeedLedgerV1 {
    pub obligation_id: String,
    pub controller_id: String,
    pub location_id: String,
    pub beneficiary_id: String,
    pub interval_start: u64,
    pub minimum: i64,
    pub normal_coverage: i64,
    pub fulfilled: i64,
    pub reserved: i64,
    pub public_critical: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyRestrictedPowerV1 {
    pub beneficiary_id: String,
    pub obligation_id: String,
    pub interval_start: u64,
    pub remaining: i64,
    pub delivered: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyReviewV1 {
    pub batch_id: String,
    pub need_id: String,
    pub requester_id: String,
    pub reason_ref: String,
    pub opened_at: u64,
    pub deadline: u64,
    pub decision: Option<bool>,
}
pub(super) fn need_key(obligation: &str, start: u64) -> String {
    format!(
        "need:{}",
        blake3::hash(format!("{}:{obligation}:{start}", obligation.len()).as_bytes()).to_hex()
    )
}
fn require_text(s: &str) -> Result<(), WorldError> {
    if s.is_empty() || s.len() > 128 {
        Err(invalid("invalid_evidence"))
    } else {
        Ok(())
    }
}
pub(super) fn evidence_end(e: &Evidence) -> Option<u64> {
    e.observations.last()?.window_start.checked_add(60)
}
pub(super) fn fresh(e: &Evidence, now: u64) -> bool {
    evidence_end(e).is_some_and(|end| end <= now && now - end <= 60)
}
impl EmergencySupplyStateV1 {
    pub fn validate_evidence(&self, e: &Evidence, now: u64) -> Result<(), WorldError> {
        self.validate_observations(e, now, true).map(|_| ())
    }
    pub(super) fn validate_observations(
        &self,
        e: &Evidence,
        now: u64,
        require_shortage: bool,
    ) -> Result<bool, WorldError> {
        require_text(&e.region_id)?;
        require_text(&e.region_budget_lineage)?;
        require_text(&e.source_ref)?;
        if e.observations.len() != 3 || !fresh(e, now) {
            return Err(invalid("evidence_stale"));
        }
        let first = e.observations[0].window_start;
        let eligible: BTreeSet<_> = self
            .services
            .values()
            .filter(|s| {
                s.region_id == e.region_id
                    && s.registered_at.checked_add(600).is_some_and(|t| t <= first)
            })
            .map(|s| s.obligation_id.clone())
            .collect();
        if eligible.is_empty() || eligible.len() > 4096 {
            return Err(invalid("invalid_evidence"));
        }
        let mut recovered = true;
        for (i, w) in e.observations.iter().enumerate() {
            if w.window_start % 60 != 0
                || w.window_start
                    != first
                        .checked_add(i as u64 * 60)
                        .ok_or_else(|| invalid("numeric_overflow"))?
                || w.obligation_ids != eligible
                || w.normal_coverage.keys().cloned().collect::<BTreeSet<_>>() != eligible
            {
                return Err(invalid("invalid_evidence"));
            }
            require_text(&w.world_root)?;
            let mut total = 0i128;
            let mut missing = 0i128;
            let mut all = BTreeSet::new();
            let mut affected = BTreeSet::new();
            let mut places = BTreeSet::new();
            for id in &eligible {
                let s = &self.services[id];
                let normal = w.normal_coverage[id];
                if normal < 0
                    || normal
                        > s.electricity_per_interval
                            .checked_mul(10)
                            .ok_or_else(|| invalid("numeric_overflow"))?
                {
                    return Err(invalid("invalid_evidence"));
                }
                total += i128::from(s.electricity_per_interval) * 10;
                missing += i128::from(s.electricity_per_interval) * 10 - i128::from(normal);
                all.insert(&s.controller_id);
                if i128::from(normal) < i128::from(s.electricity_per_interval) * 10 {
                    affected.insert(&s.controller_id);
                    places.insert(&s.location_id);
                }
            }
            recovered &= total > 0 && missing * 100 < total * 10;
            if total == 0
                || require_shortage
                    && (missing * 100 < total * 30
                        || affected.len() < 3
                        || places.len() < 2
                        || affected.len() * 100 < all.len() * 30)
            {
                return Err(invalid("invalid_evidence"));
            }
        }
        Ok(recovered)
    }
    /// A schedule may consume only registered recipes in this exact interval. This
    /// returns a prepared successor; callers install it with the recipe transition.
    pub fn prepare_consumption(
        &self,
        actor: &str,
        obligation: &str,
        recipe: &str,
        now: u64,
        quantity: i64,
    ) -> Result<Self, WorldError> {
        let service = self
            .services
            .get(obligation)
            .ok_or_else(|| invalid("need_source_unverifiable"))?;
        if service.beneficiary_id != actor || !service.recipe_ids.contains(recipe) || quantity <= 0
        {
            return Err(invalid("scope_mismatch"));
        }
        let key = need_key(obligation, now / 60 * 60);
        let mut next = self.clone();
        let power = next
            .restricted_power
            .get_mut(&key)
            .ok_or_else(|| invalid("resource_blocked"))?;
        if power.beneficiary_id != actor || power.remaining < quantity {
            return Err(invalid("resource_blocked"));
        }
        power.remaining -= quantity;
        Ok(next)
    }
}
/// Hamilton allocation: exact integer floors followed by descending fractional
/// remainders, then canonical controller identity. No request-order dependence.
pub fn emergency_proportional_allocation(
    demands: &BTreeMap<String, i64>,
    available: i64,
) -> Result<BTreeMap<String, i64>, WorldError> {
    if available < 0 || demands.values().any(|n| *n < 0) {
        return Err(invalid("numeric_overflow"));
    }
    let total = demands
        .values()
        .try_fold(0i128, |sum, n| sum.checked_add(i128::from(*n)))
        .ok_or_else(|| invalid("numeric_overflow"))?;
    if total == 0 {
        return Ok(demands.keys().map(|id| (id.clone(), 0)).collect());
    }
    let budget = i128::from(available).min(total);
    let mut result = BTreeMap::new();
    let mut fractions = Vec::new();
    let mut used = 0i128;
    for (id, n) in demands {
        let product = i128::from(*n)
            .checked_mul(budget)
            .ok_or_else(|| invalid("numeric_overflow"))?;
        let floor = product / total;
        result.insert(
            id.clone(),
            i64::try_from(floor).map_err(|_| invalid("numeric_overflow"))?,
        );
        used += floor;
        fractions.push((product % total, id));
    }
    fractions.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    for (_, id) in fractions
        .into_iter()
        .take(usize::try_from(budget - used).map_err(|_| invalid("numeric_overflow"))?)
    {
        *result.get_mut(id).unwrap() += 1;
    }
    Ok(result)
}

impl PreparedWorldRule {
    pub(crate) fn emergency_supply(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &C,
        now: u64,
    ) -> Result<String, WorldError> {
        if let Some(proposal) = command.governance_proposal() {
            require_governance(
                state,
                actor,
                proposal,
                &WorldRuleCommandV1::EmergencySupply {
                    command: command.clone(),
                },
            )?;
        }
        match command {
            C::Observe { region_id } => self.observe_supply(state, region_id, now),
            C::RegisterService { registration, .. } => {
                let source = self
                    .rules
                    .emergency_supply
                    .minimum_services
                    .get(&registration.obligation_id)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?;
                if registration != &source.registration
                    || source
                        .registered_at
                        .checked_add(600)
                        .is_none_or(|time| time > now)
                    || (actor != source.operator_agent_id && actor != registration.beneficiary_id)
                {
                    return Err(invalid("need_source_unverifiable"));
                }
                if !super::minimum_service::source_owner_is_current(state, source)
                    || !super::observation::validate_location(
                        state,
                        &registration.beneficiary_id,
                        &registration.location_id,
                        now,
                    )
                {
                    return Err(invalid("need_source_unverifiable"));
                }
                self.rules
                    .emergency_supply
                    .services
                    .insert(registration.obligation_id.clone(), registration.clone());
                Ok(registration.obligation_id.clone())
            }
            C::PublishEvidence { evidence, .. } => {
                if self
                    .rules
                    .emergency_supply
                    .region_lineages
                    .get(&evidence.region_id)
                    != Some(&evidence.region_budget_lineage)
                    || evidence.source_ref
                        != format!(
                            "observations:{}",
                            crate::util::hash_json(&evidence.observations)?
                        )
                {
                    return Err(invalid("invalid_evidence"));
                }
                self.rules
                    .emergency_supply
                    .validate_observations(evidence, now, false)?;
                for observation in &evidence.observations {
                    let key = super::observation::snapshot_key(
                        &evidence.region_id,
                        observation.window_start,
                    );
                    if self.rules.emergency_supply.observations.get(&key) != Some(observation) {
                        return Err(invalid("invalid_evidence"));
                    }
                }
                self.rules
                    .emergency_supply
                    .evidence
                    .insert(key.into(), evidence.clone());
                Ok(key.into())
            }
            C::Authorize {
                evidence_id,
                reviewer_id,
                starts_at,
                expires_at,
                quantity,
                ..
            } => {
                let e = self
                    .rules
                    .emergency_supply
                    .evidence
                    .get(evidence_id)
                    .ok_or_else(|| invalid("invalid_evidence"))?
                    .clone();
                self.rules.emergency_supply.validate_evidence(&e, now)?;
                self.rules
                    .emergency_supply
                    .validate_current_sources(state, &e, now)?;
                if self
                    .rules
                    .emergency_supply
                    .evidence
                    .values()
                    .filter(|candidate| {
                        candidate.region_id == e.region_id
                            && candidate.region_budget_lineage == e.region_budget_lineage
                            && evidence_end(candidate) >= evidence_end(&e)
                    })
                    .any(|candidate| {
                        self.rules
                            .emergency_supply
                            .validate_observations(candidate, now, false)
                            == Ok(true)
                    })
                {
                    return Err(invalid("authorization_inactive"));
                }

                if *starts_at < now
                    || starts_at % 60 != 0
                    || *expires_at
                        <= starts_at
                            .checked_add(60)
                            .ok_or_else(|| invalid("numeric_overflow"))?
                    || *expires_at
                        > starts_at
                            .checked_add(600)
                            .ok_or_else(|| invalid("numeric_overflow"))?
                    || *quantity <= 0
                {
                    return Err(invalid("window_closed"));
                }
                if self
                    .rules
                    .emergency_supply
                    .authorizations
                    .values()
                    .any(|a| a.active && a.expires_at > now && a.region_id == e.region_id)
                {
                    return Err(invalid("region_slot_conflict"));
                }
                let provider = self
                    .rules
                    .emergency_supply
                    .services
                    .values()
                    .find(|s| {
                        s.beneficiary_id == actor
                            && s.region_id == e.region_id
                            && s.registered_at
                                .checked_add(600)
                                .is_some_and(|t| t <= e.observations[0].window_start)
                    })
                    .ok_or_else(|| invalid("need_source_unverifiable"))?
                    .clone();
                let reviewer = self
                    .rules
                    .emergency_supply
                    .services
                    .values()
                    .find(|s| {
                        s.beneficiary_id == *reviewer_id
                            && s.registered_at
                                .checked_add(600)
                                .is_some_and(|at| at <= e.observations[0].window_start)
                    })
                    .ok_or_else(|| invalid("missing_authority"))?;
                if reviewer.controller_id == provider.controller_id {
                    return Err(invalid("missing_authority"));
                }
                let reviewer_controller_id = reviewer.controller_id.clone();
                let reviewer_owner_source_id = reviewer.source_ref.clone();
                let agent = self
                    .agents
                    .entry(actor.into())
                    .or_insert_with(|| state.agents[actor].clone());
                agent
                    .state
                    .resources
                    .remove(ResourceKind::Electricity, *quantity)
                    .map_err(|_| invalid("resource_blocked"))?;
                self.rules.emergency_supply.authorizations.insert(
                    key.into(),
                    EmergencyAuthorizationV1 {
                        id: key.into(),
                        evidence_id: evidence_id.clone(),
                        provider_id: actor.into(),
                        reviewer_id: reviewer_id.clone(),
                        reviewer_controller_id,
                        reviewer_owner_source_id,
                        provider_controller_id: provider.controller_id,
                        region_id: e.region_id,
                        starts_at: *starts_at,
                        expires_at: *expires_at,
                        remaining: *quantity,
                        active: true,
                        requested: BTreeSet::new(),
                    },
                );
                Ok(key.into())
            }
            C::RefreshEvidence {
                authorization_id,
                evidence_id,
            } => {
                let a = self
                    .rules
                    .emergency_supply
                    .authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_inactive"))?
                    .clone();
                if actor != a.provider_id || !a.active || now >= a.expires_at {
                    return Err(invalid("missing_authority"));
                }
                let e = self
                    .rules
                    .emergency_supply
                    .evidence
                    .get(evidence_id)
                    .ok_or_else(|| invalid("invalid_evidence"))?;
                self.rules
                    .emergency_supply
                    .validate_observations(e, now, false)?;
                let old = &self.rules.emergency_supply.evidence[&a.evidence_id];
                if e.region_id != a.region_id
                    || e.region_budget_lineage != old.region_budget_lineage
                    || evidence_end(e) <= evidence_end(old)
                {
                    return Err(invalid("scope_mismatch"));
                }
                self.rules
                    .emergency_supply
                    .authorizations
                    .get_mut(authorization_id)
                    .unwrap()
                    .evidence_id = evidence_id.clone();
                Ok(authorization_id.clone())
            }
            C::Request {
                authorization_id,
                needs,
            } => {
                let a = self
                    .rules
                    .emergency_supply
                    .authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_inactive"))?
                    .clone();
                let e = self.emergency_active(state, &a, now)?;
                if now < a.starts_at || now >= a.starts_at + 60 || needs.is_empty() {
                    return Err(invalid("window_closed"));
                }
                let latest = e.observations.last().unwrap();
                let observation_key =
                    super::observation::snapshot_key(&e.region_id, latest.window_start);
                let normal = self
                    .rules
                    .emergency_supply
                    .observation_interval_coverage
                    .get(&observation_key)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?
                    .clone();
                for n in needs {
                    let service = self
                        .rules
                        .emergency_supply
                        .services
                        .get(&n.obligation_id)
                        .ok_or_else(|| invalid("need_source_unverifiable"))?
                        .clone();
                    if service.beneficiary_id != actor
                        || service.region_id != a.region_id
                        || service.controller_id == a.provider_controller_id
                        || service.controller_id == a.reviewer_controller_id
                        || !normal.contains_key(&need_key(&n.obligation_id, n.interval_start))
                        || n.interval_start % 60 != 0
                        || n.interval_start < a.starts_at + 60
                        || n.interval_start
                            .checked_add(60)
                            .is_none_or(|end| end > a.expires_at)
                    {
                        return Err(invalid("scope_mismatch"));
                    }
                }
                let mut requested = a.requested.clone();
                for n in needs {
                    requested.insert(need_key(&n.obligation_id, n.interval_start));
                }
                let obligations: BTreeSet<_> = a
                    .requested
                    .iter()
                    .filter_map(|id| {
                        self.rules
                            .emergency_supply
                            .needs
                            .get(id)
                            .map(|n| n.obligation_id.as_str())
                    })
                    .chain(needs.iter().map(|n| n.obligation_id.as_str()))
                    .collect();
                let controllers: BTreeSet<_> = obligations
                    .iter()
                    .filter_map(|id| {
                        self.rules
                            .emergency_supply
                            .services
                            .get(*id)
                            .map(|s| s.controller_id.as_str())
                    })
                    .collect();
                let locations: BTreeSet<_> = obligations
                    .iter()
                    .filter_map(|id| {
                        self.rules
                            .emergency_supply
                            .services
                            .get(*id)
                            .map(|s| s.location_id.as_str())
                    })
                    .collect();
                if requested.len() > 4096 || controllers.len() > 1024 || locations.len() > 64 {
                    self.rules
                        .emergency_supply
                        .overflowed_windows
                        .insert((a.region_id.clone(), a.starts_at));
                    return Ok(format!("overflowed:{}", authorization_id));
                }
                for n in needs {
                    let service = self
                        .rules
                        .emergency_supply
                        .services
                        .get(&n.obligation_id)
                        .ok_or_else(|| invalid("need_source_unverifiable"))?
                        .clone();
                    if service.beneficiary_id != actor
                        || service.region_id != a.region_id
                        || service.controller_id == a.provider_controller_id
                        || service.controller_id == a.reviewer_controller_id
                        || !normal.contains_key(&need_key(&n.obligation_id, n.interval_start))
                        || n.interval_start % 60 != 0
                        || n.interval_start < a.starts_at + 60
                        || n.interval_start
                            .checked_add(60)
                            .is_none_or(|end| end > a.expires_at)
                    {
                        return Err(invalid("scope_mismatch"));
                    }
                    let id = need_key(&n.obligation_id, n.interval_start);
                    if let Some(existing) = self.rules.emergency_supply.needs.get_mut(&id) {
                        if existing.minimum != service.electricity_per_interval {
                            return Err(invalid("snapshot_conflict"));
                        }
                        existing.normal_coverage =
                            normal[&need_key(&n.obligation_id, n.interval_start)];
                    } else {
                        self.rules.emergency_supply.needs.insert(
                            id.clone(),
                            EmergencyNeedLedgerV1 {
                                obligation_id: service.obligation_id,
                                controller_id: service.controller_id,
                                location_id: service.location_id,
                                beneficiary_id: service.beneficiary_id,
                                interval_start: n.interval_start,
                                minimum: service.electricity_per_interval,
                                normal_coverage: normal
                                    [&need_key(&n.obligation_id, n.interval_start)],
                                fulfilled: 0,
                                reserved: 0,
                                public_critical: service.public_critical,
                            },
                        );
                    }
                    self.rules
                        .emergency_supply
                        .authorizations
                        .get_mut(authorization_id)
                        .unwrap()
                        .requested
                        .insert(id);
                }
                Ok(authorization_id.clone())
            }
            C::Freeze { authorization_id } => {
                let a = self
                    .rules
                    .emergency_supply
                    .authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_inactive"))?
                    .clone();
                let evidence = self.emergency_active(state, &a, now)?;
                if self.rules.emergency_supply.evidence.values().any(|newer| {
                    newer.region_id == evidence.region_id
                        && newer.region_budget_lineage == evidence.region_budget_lineage
                        && evidence_end(newer) > evidence_end(&evidence)
                }) {
                    return Err(invalid("snapshot_conflict"));
                }
                let last = evidence
                    .observations
                    .last()
                    .ok_or_else(|| invalid("invalid_evidence"))?;
                let coverage = self
                    .rules
                    .emergency_supply
                    .observation_interval_coverage
                    .get(&super::observation::snapshot_key(
                        &evidence.region_id,
                        last.window_start,
                    ))
                    .ok_or_else(|| invalid("need_source_unverifiable"))?;

                if actor != a.provider_id
                    || now < a.starts_at + 60
                    || self
                        .rules
                        .emergency_supply
                        .batches
                        .values()
                        .any(|b| b.authorization_id == *authorization_id)
                {
                    return Err(invalid("window_closed"));
                }
                let provider = self
                    .rules
                    .emergency_supply
                    .services
                    .values()
                    .find(|s| s.beneficiary_id == actor && s.region_id == a.region_id)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?;
                let mut demands = BTreeMap::new();
                let mut frozen = BTreeMap::new();
                for id in &a.requested {
                    let n = &self.rules.emergency_supply.needs[id];
                    if n.interval_start < now {
                        continue;
                    }
                    if coverage.get(id) != Some(&n.normal_coverage) {
                        return Err(invalid("snapshot_conflict"));
                    }
                    let remaining = n
                        .minimum
                        .checked_sub(n.normal_coverage)
                        .and_then(|v| v.checked_sub(n.fulfilled))
                        .and_then(|v| v.checked_sub(n.reserved))
                        .map(|v| v.max(0))
                        .ok_or_else(|| invalid("numeric_overflow"))?;
                    if remaining > 0 && n.location_id != provider.location_id {
                        return Err(invalid("unsupported_resource_path"));
                    }
                    demands.insert(id.clone(), remaining);
                    frozen.insert(id.clone(), n.clone());
                }
                self.rules.emergency_supply.batches.insert(
                    key.into(),
                    EmergencyBatchV1 {
                        authorization_id: authorization_id.clone(),
                        frozen_at: now,
                        frozen_pool: a.remaining,
                        evidence_id: a.evidence_id,
                        demands,
                        frozen_ledger: frozen,
                        allocation: BTreeMap::new(),
                        allocated: false,
                        committed_at: None,
                    },
                );
                Ok(key.into())
            }
            C::Allocate { batch_id } => {
                let b = self
                    .rules
                    .emergency_supply
                    .batches
                    .get(batch_id)
                    .ok_or_else(|| invalid("window_closed"))?
                    .clone();
                let a = self.rules.emergency_supply.authorizations[&b.authorization_id].clone();
                self.emergency_active(state, &a, now)?;
                if actor != a.provider_id
                    || b.frozen_pool != a.remaining
                    || b.evidence_id != a.evidence_id
                    || b.allocated
                    || b.frozen_ledger
                        .iter()
                        .any(|(id, n)| self.rules.emergency_supply.needs.get(id) != Some(n))
                {
                    return Err(invalid("snapshot_conflict"));
                }
                let mut remaining = a.remaining;
                let mut vector = BTreeMap::new();
                // Current interval precedes all later intervals; P0 precedes P1 only
                // inside one interval. Controller aggregation precedes obligation splitting.
                let mut groups: BTreeMap<(u64, bool), Vec<String>> = BTreeMap::new();
                for id in b.demands.keys() {
                    let n = &b.frozen_ledger[id];
                    groups
                        .entry((n.interval_start, !n.public_critical))
                        .or_default()
                        .push(id.clone());
                }
                for (_, ids) in groups {
                    let mut controllers: BTreeMap<String, i64> = BTreeMap::new();
                    for id in &ids {
                        let n = &b.frozen_ledger[id];
                        let value = controllers.entry(n.controller_id.clone()).or_default();
                        *value = value
                            .checked_add(b.demands[id])
                            .ok_or_else(|| invalid("numeric_overflow"))?;
                    }
                    let apportioned = emergency_proportional_allocation(&controllers, remaining)?;
                    for (controller, mut available) in apportioned {
                        remaining -= available;
                        let mut sorted: Vec<_> = ids
                            .iter()
                            .filter(|id| b.frozen_ledger[*id].controller_id == controller)
                            .collect();
                        sorted.sort_by(|a, bid| {
                            b.frozen_ledger[*a]
                                .obligation_id
                                .cmp(&b.frozen_ledger[*bid].obligation_id)
                        });
                        for id in sorted {
                            let amount = b.demands[id].min(available);
                            available -= amount;
                            vector.insert(id.clone(), amount);
                        }
                    }
                }
                for (id, q) in &vector {
                    let n = self.rules.emergency_supply.needs.get_mut(id).unwrap();
                    n.reserved = n
                        .reserved
                        .checked_add(*q)
                        .ok_or_else(|| invalid("numeric_overflow"))?;
                }
                let target = self
                    .rules
                    .emergency_supply
                    .batches
                    .get_mut(batch_id)
                    .unwrap();
                target.allocation = vector;
                target.allocated = true;
                target.committed_at = Some(now);
                Ok(batch_id.clone())
            }
            C::Deliver {
                batch_id,
                need_id,
                quantity,
            } => {
                let b = self
                    .rules
                    .emergency_supply
                    .batches
                    .get(batch_id)
                    .ok_or_else(|| invalid("window_closed"))?
                    .clone();
                let a = self.rules.emergency_supply.authorizations[&b.authorization_id].clone();
                self.emergency_active(state, &a, now)?;
                let n = self
                    .rules
                    .emergency_supply
                    .needs
                    .get(need_id)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?
                    .clone();
                if actor != a.provider_id
                    || !b.allocated
                    || *quantity <= 0
                    || *quantity > b.allocation.get(need_id).copied().unwrap_or(0)
                    || *quantity > a.remaining
                    || now > n.interval_start
                {
                    return Err(invalid("resource_blocked"));
                }
                let source = self
                    .rules
                    .emergency_supply
                    .minimum_services
                    .get(&n.obligation_id)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?;
                if source.registration.beneficiary_id != n.beneficiary_id
                    || source.registration.location_id != n.location_id
                    || !super::observation::validate_location(state, actor, &n.location_id, now)
                    || !super::observation::validate_location(
                        state,
                        &n.beneficiary_id,
                        &n.location_id,
                        now,
                    )
                {
                    return Err(invalid("unsupported_resource_path"));
                }
                if self.rules.emergency_supply.reviews.values().any(|r| {
                    r.batch_id == *batch_id
                        && r.need_id == *need_id
                        && r.decision.is_none()
                        && now >= r.deadline
                }) {
                    return Err(invalid("review_blocked"));
                }
                let power = self
                    .rules
                    .emergency_supply
                    .restricted_power
                    .entry(need_id.clone())
                    .or_insert(EmergencyRestrictedPowerV1 {
                        beneficiary_id: n.beneficiary_id,
                        obligation_id: n.obligation_id,
                        interval_start: n.interval_start,
                        remaining: 0,
                        delivered: 0,
                    });
                power.remaining = power
                    .remaining
                    .checked_add(*quantity)
                    .ok_or_else(|| invalid("numeric_overflow"))?;
                power.delivered = power
                    .delivered
                    .checked_add(*quantity)
                    .ok_or_else(|| invalid("numeric_overflow"))?;
                let n = self.rules.emergency_supply.needs.get_mut(need_id).unwrap();
                n.reserved = n
                    .reserved
                    .checked_sub(*quantity)
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| invalid("numeric_overflow"))?;
                n.fulfilled = n
                    .fulfilled
                    .checked_add(*quantity)
                    .ok_or_else(|| invalid("numeric_overflow"))?;
                self.rules
                    .emergency_supply
                    .authorizations
                    .get_mut(&a.id)
                    .unwrap()
                    .remaining -= quantity;
                *self
                    .rules
                    .emergency_supply
                    .batches
                    .get_mut(batch_id)
                    .unwrap()
                    .allocation
                    .get_mut(need_id)
                    .unwrap() -= quantity;
                Ok(need_id.clone())
            }
            C::OpenReview {
                batch_id,
                need_id,
                reason_ref,
            } => {
                require_text(reason_ref)?;
                let b = self
                    .rules
                    .emergency_supply
                    .batches
                    .get(batch_id)
                    .ok_or_else(|| invalid("window_closed"))?;
                let a = &self.rules.emergency_supply.authorizations[&b.authorization_id];
                let n = self
                    .rules
                    .emergency_supply
                    .needs
                    .get(need_id)
                    .ok_or_else(|| invalid("need_source_unverifiable"))?;
                if !b.allocated
                    || !b.demands.contains_key(need_id)
                    || (actor != n.beneficiary_id && actor != a.provider_id)
                    || now
                        > b.committed_at
                            .ok_or_else(|| invalid("review_blocked"))?
                            .checked_add(120)
                            .ok_or_else(|| invalid("numeric_overflow"))?
                    || self.rules.emergency_supply.reviews.values().any(|r| {
                        r.batch_id == *batch_id && r.need_id == *need_id && r.decision.is_none()
                    })
                {
                    return Err(invalid("window_closed"));
                }
                self.rules.emergency_supply.reviews.insert(
                    key.into(),
                    EmergencyReviewV1 {
                        batch_id: batch_id.clone(),
                        need_id: need_id.clone(),
                        requester_id: actor.into(),
                        reason_ref: reason_ref.clone(),
                        opened_at: now,
                        deadline: now
                            .checked_add(120)
                            .ok_or_else(|| invalid("numeric_overflow"))?,
                        decision: None,
                    },
                );
                Ok(key.into())
            }
            C::DecideReview {
                review_id,
                cancel_unsettled,
                ..
            } => {
                let review = self
                    .rules
                    .emergency_supply
                    .reviews
                    .get(review_id)
                    .ok_or_else(|| invalid("review_blocked"))?
                    .clone();
                let b = &self.rules.emergency_supply.batches[&review.batch_id];
                let a = &self.rules.emergency_supply.authorizations[&b.authorization_id];
                let n = &self.rules.emergency_supply.needs[&review.need_id];
                if review.decision.is_some()
                    || actor != a.reviewer_id
                    || a.reviewer_controller_id == a.provider_controller_id
                    || a.reviewer_controller_id == n.controller_id
                    || state.resolved_agent_owner(actor).is_none_or(|owner| {
                        owner.player_id != a.reviewer_controller_id
                            || owner.source_id != a.reviewer_owner_source_id
                    })
                {
                    return Err(invalid("missing_authority"));
                }
                if *cancel_unsettled {
                    let b = self
                        .rules
                        .emergency_supply
                        .batches
                        .get_mut(&review.batch_id)
                        .unwrap();
                    let amount = b
                        .allocation
                        .get_mut(&review.need_id)
                        .ok_or_else(|| invalid("review_blocked"))?;
                    let need = self
                        .rules
                        .emergency_supply
                        .needs
                        .get_mut(&review.need_id)
                        .unwrap();
                    need.reserved = need
                        .reserved
                        .checked_sub(*amount)
                        .filter(|v| *v >= 0)
                        .ok_or_else(|| invalid("numeric_overflow"))?;
                    *amount = 0;
                }
                self.rules
                    .emergency_supply
                    .reviews
                    .get_mut(review_id)
                    .unwrap()
                    .decision = Some(*cancel_unsettled);
                Ok(review_id.clone())
            }
            C::Close { authorization_id } => {
                let a = self
                    .rules
                    .emergency_supply
                    .authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_inactive"))?
                    .clone();
                if !a.active
                    || (actor != a.provider_id && self.emergency_active(state, &a, now).is_ok())
                {
                    return Err(invalid("missing_authority"));
                }
                for b in self
                    .rules
                    .emergency_supply
                    .batches
                    .values_mut()
                    .filter(|b| b.authorization_id == *authorization_id)
                {
                    for (id, q) in &mut b.allocation {
                        self.rules
                            .emergency_supply
                            .needs
                            .get_mut(id)
                            .unwrap()
                            .reserved -= *q;
                        *q = 0;
                    }
                }
                let agent = self
                    .agents
                    .entry(a.provider_id.clone())
                    .or_insert_with(|| state.agents[&a.provider_id].clone());
                agent
                    .state
                    .resources
                    .add(ResourceKind::Electricity, a.remaining)
                    .map_err(|_| invalid("numeric_overflow"))?;
                let a = self
                    .rules
                    .emergency_supply
                    .authorizations
                    .get_mut(authorization_id)
                    .unwrap();
                a.active = false;
                a.remaining = 0;
                Ok(authorization_id.clone())
            }
        }
    }
    fn emergency_active(
        &self,
        state: &WorldState,
        a: &EmergencyAuthorizationV1,
        now: u64,
    ) -> Result<Evidence, WorldError> {
        let e = self
            .rules
            .emergency_supply
            .evidence
            .get(&a.evidence_id)
            .ok_or_else(|| invalid("invalid_evidence"))?;
        if self
            .rules
            .emergency_supply
            .overflowed_windows
            .contains(&(a.region_id.clone(), a.starts_at))
        {
            return Err(invalid("batch_overflow"));
        }
        if !a.active || now < a.starts_at || now >= a.expires_at {
            return Err(invalid("authorization_inactive"));
        }
        if self
            .rules
            .emergency_supply
            .evidence
            .values()
            .filter(|candidate| {
                candidate.region_id == e.region_id
                    && candidate.region_budget_lineage == e.region_budget_lineage
                    && evidence_end(candidate) >= evidence_end(e)
            })
            .any(|candidate| {
                self.rules
                    .emergency_supply
                    .validate_observations(candidate, now, false)
                    == Ok(true)
            })
        {
            return Err(invalid("authorization_inactive"));
        }
        self.rules
            .emergency_supply
            .validate_current_sources(state, e, now)?;
        if !fresh(e, now) {
            return Err(invalid("evidence_stale"));
        }
        Ok(e.clone())
    }
}
