use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyAuthorizationV1 {
    pub id: String,
    pub provider_id: String,
    pub crisis_id: String,
    pub crisis_evidence: crate::CrisisState,
    pub material: String,
    pub beneficiaries: BTreeSet<String>,
    pub policy_version: u32,
    pub starts_at: u64,
    pub expires_at: u64,
    pub review_proposal: String,
    pub remaining_escrow: i64,
    pub requests: BTreeMap<String, i64>,
    pub revision: u64,
    pub active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SupplyLineageV1 {
    pub target: i64,
    pub fulfilled: i64,
    pub reserved: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyBatchV1 {
    pub id: String,
    pub authorization_id: String,
    pub frozen_at: u64,
    pub authorization_revision: u64,
    pub frozen_escrow: i64,
    pub frozen_lineages: BTreeMap<String, SupplyLineageV1>,
    pub demands: BTreeMap<String, i64>,
    pub allocation: BTreeMap<String, i64>,
    pub allocated: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyShipmentV1 {
    pub authorization_id: String,
    pub batch_id: String,
    pub beneficiary_id: String,
    pub lineage_id: String,
    pub input_amount: i64,
    pub delivered_amount: i64,
    pub settled: bool,
}
pub fn lineage(auth: &SupplyAuthorizationV1, beneficiary: &str) -> String {
    supply_lineage_from_fields_v1(&auth.crisis_id,&auth.material,beneficiary)
}
/// Exact original length-delimited lineage DATA for selected scalar sources.
pub fn supply_lineage_from_fields_v1(crisis_id:&str,material:&str,beneficiary:&str)->String {
    format!("{}:{}{}:{}{}",crisis_id.len(),crisis_id,material.len(),material,beneficiary)
}

fn valid(_state: &WorldState, auth: &SupplyAuthorizationV1, now: u64) -> bool {
    auth.is_valid_current_material_contract_v1(now)
}

#[path = "supply_policy.rs"]
mod policy;
pub use policy::*;

impl PreparedWorldRule {
    pub fn supply(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &WorldRuleCommandV1,
        now: u64,
    ) -> Result<String, WorldError> {
        match command {
            WorldRuleCommandV1::AuthorizeSupply { material, .. } => {
                // GAME-018 emergency-supply-policy-v1 only admits Electricity delivered
                // into a registered minimum-consumption obligation. MaterialTransit
                // credits transferable material inventory and cannot implement that
                // contract. Fail before governance consumption or any resource debit.
                if material != "Electricity" {
                    return Err(invalid("scope_mismatch"));
                }
                Err(invalid("unsupported_resource_path"))
            }
            WorldRuleCommandV1::RequestSupply {
                authorization_id,
                cumulative_demand,
            } => {
                let auth = self
                    .rules
                    .supply_authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_missing"))?
                    .clone();
                if !valid(state, &auth, now)
                    || !auth.beneficiaries.contains(actor)
                    || now >= auth.starts_at + 10
                {
                    return Err(invalid("authorization_or_collection_closed"));
                }
                if *cumulative_demand <= 0 || *cumulative_demand > 1_000_000_000 {
                    return Err(invalid("demand_out_of_bounds"));
                }
                let id = lineage(&auth, actor);
                let value = self.rules.supply_lineages.entry(id).or_default();
                if *cumulative_demand < value.target {
                    return Err(invalid("cumulative_demand_cannot_shrink"));
                }
                value.target = *cumulative_demand;
                let auth = self
                    .rules
                    .supply_authorizations
                    .get_mut(authorization_id)
                    .expect("validated");
                auth.requests.insert(actor.into(), *cumulative_demand);
                auth.revision += 1;
                Ok(authorization_id.clone())
            }
            WorldRuleCommandV1::FreezeSupply { authorization_id } => {
                let auth = self
                    .rules
                    .supply_authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_missing"))?;
                if actor != auth.provider_id
                    || !valid(state, auth, now)
                    || now < auth.starts_at + 10
                    || self
                        .rules
                        .supply_batches
                        .values()
                        .any(|b| b.authorization_id == *authorization_id)
                {
                    return Err(invalid("batch_not_ready_or_already_frozen"));
                }
                let frozen_lineages = auth
                    .requests
                    .keys()
                    .map(|id| {
                        let l = lineage(auth, id);
                        (l.clone(), self.rules.supply_lineages[&l].clone())
                    })
                    .collect::<BTreeMap<_, _>>();
                let demands = auth
                    .requests
                    .iter()
                    .map(|(id, target)| {
                        let value = &frozen_lineages[&lineage(auth, id)];
                        (
                            id.clone(),
                            (*target - value.fulfilled - value.reserved).max(0),
                        )
                    })
                    .collect();
                self.rules.supply_batches.insert(
                    key.into(),
                    SupplyBatchV1 {
                        id: key.into(),
                        authorization_id: authorization_id.clone(),
                        frozen_at: now,
                        authorization_revision: auth.revision,
                        frozen_escrow: auth.remaining_escrow,
                        frozen_lineages,
                        demands,
                        allocation: BTreeMap::new(),
                        allocated: false,
                    },
                );
                Ok(key.into())
            }
            WorldRuleCommandV1::AllocateSupply { batch_id } => {
                let batch = self
                    .rules
                    .supply_batches
                    .get(batch_id)
                    .ok_or_else(|| invalid("batch_missing"))?
                    .clone();
                let auth = self
                    .rules
                    .supply_authorizations
                    .get(&batch.authorization_id)
                    .ok_or_else(|| invalid("authorization_missing"))?
                    .clone();
                if actor != auth.provider_id || !valid(state, &auth, now) || batch.allocated {
                    return Err(invalid("batch_not_executable"));
                }
                if auth.revision != batch.authorization_revision
                    || auth.remaining_escrow != batch.frozen_escrow
                    || batch
                        .frozen_lineages
                        .iter()
                        .any(|(id, v)| self.rules.supply_lineages.get(id) != Some(v))
                    || auth.beneficiaries.iter().any(|id| {
                        !state.agents.contains_key(id)
                            || state.gameplay_policy.blocked_agents.contains(id)
                    })
                {
                    return Err(invalid("frozen_dependency_drift"));
                }
                let total: i64 = batch.demands.values().sum();
                let budget = total.min(auth.remaining_escrow);
                let mut vector = BTreeMap::new();
                if total > 0 {
                    for (id, demand) in &batch.demands {
                        vector.insert(
                            id.clone(),
                            (i128::from(*demand) * i128::from(budget) / i128::from(total)) as i64,
                        );
                    }
                    let mut remainder = budget - vector.values().sum::<i64>();
                    for (id, amount) in &mut vector {
                        if remainder > 0 && *amount < batch.demands[id] {
                            *amount += 1;
                            remainder -= 1;
                        }
                    }
                }
                for (id, amount) in &vector {
                    self.rules
                        .supply_lineages
                        .get_mut(&lineage(&auth, id))
                        .expect("frozen lineage")
                        .reserved += amount;
                }
                let batch = self
                    .rules
                    .supply_batches
                    .get_mut(batch_id)
                    .expect("validated");
                batch.allocation = vector;
                batch.allocated = true;
                Ok(batch_id.clone())
            }
            WorldRuleCommandV1::DispatchSupply {
                batch_id,
                beneficiary_id,
                amount,
                route_id,
            } => {
                let job_id = match &self.event {
                    DomainEvent::WorldRuleCommitted { action_id, .. } => action_id,
                    _ => unreachable!(),
                };
                let batch = self
                    .rules
                    .supply_batches
                    .get(batch_id)
                    .ok_or_else(|| invalid("batch_missing"))?
                    .clone();
                let auth = self
                    .rules
                    .supply_authorizations
                    .get(&batch.authorization_id)
                    .ok_or_else(|| invalid("authorization_missing"))?
                    .clone();
                if actor != auth.provider_id
                    || !valid(state, &auth, now)
                    || !batch.allocated
                    || *amount <= 0
                    || *amount > batch.allocation.get(beneficiary_id).copied().unwrap_or(0)
                    || *amount > auth.remaining_escrow
                    || *job_id == 0
                    || self.rules.supply_shipments.contains_key(job_id)
                {
                    return Err(invalid("allocation_or_dispatch_invalid"));
                }
                if !crate::world::logistics::material_transit_has_capacity(state) {
                    return Err(invalid("material_transit_capacity_exceeded"));
                }
                let route = state
                    .logistics_routes
                    .get(route_id)
                    .ok_or_else(|| invalid("logistics_route_missing"))?;
                if !route.available
                    || route.kind != auth.material
                    || route.from_ledger != MaterialLedgerId::agent(actor)
                    || route.to_ledger != MaterialLedgerId::agent(beneficiary_id)
                    || route.distance_km <= 0
                    || route.distance_km > 1_000_000
                {
                    return Err(invalid("logistics_route_scope_invalid"));
                }
                let tariff = route
                    .tariff_electricity_per_unit
                    .checked_mul(*amount)
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| invalid("logistics_tariff_invalid"))?;
                let loss = crate::world::logistics::material_transit_loss_bps_for_state(
                    state,
                    &auth.material,
                );
                let ready = now
                    .checked_add(crate::world::logistics::material_transit_ticks(
                        route.distance_km,
                    ))
                    .ok_or_else(|| invalid("time_overflow"))?;
                let event = DomainEvent::MaterialTransitStarted {
                    job_id: *job_id,
                    requester_agent_id: actor.into(),
                    from_ledger: route.from_ledger.clone(),
                    to_ledger: route.to_ledger.clone(),
                    kind: auth.material.clone(),
                    amount: *amount,
                    distance_km: route.distance_km,
                    loss_bps: loss,
                    ready_at: ready,
                    priority: route.priority,
                    route_id: Some(route_id.clone()),
                    path_id: None,
                    route_ids: vec![route_id.clone()],
                    tariff_electricity_total: tariff,
                    reroute_count: 0,
                };
                self.industry=Some(crate::state::industry_transition::PreparedIndustryEvent::prepare_reserved_transit(state,&event,&self.event,now)?);
                self.rules
                    .supply_authorizations
                    .get_mut(&auth.id)
                    .expect("validated")
                    .remaining_escrow -= amount;
                *self
                    .rules
                    .supply_batches
                    .get_mut(batch_id)
                    .expect("validated")
                    .allocation
                    .get_mut(beneficiary_id)
                    .expect("validated") -= amount;
                let loss_bps = loss.saturating_mul(route.distance_km).clamp(0, 10_000);
                let lost = (*amount as i128 * loss_bps as i128 / 10_000) as i64;
                self.rules.supply_shipments.insert(
                    *job_id,
                    SupplyShipmentV1 {
                        authorization_id: auth.id.clone(),
                        batch_id: batch_id.clone(),
                        beneficiary_id: beneficiary_id.clone(),
                        lineage_id: lineage(&auth, beneficiary_id),
                        input_amount: *amount,
                        delivered_amount: *amount - lost,
                        settled: false,
                    },
                );
                Ok(job_id.to_string())
            }
            WorldRuleCommandV1::SettleSupply { job_id } => {
                let shipment = self
                    .rules
                    .supply_shipments
                    .get(job_id)
                    .ok_or_else(|| invalid("shipment_missing"))?
                    .clone();
                validate_supply_settlement_ready_v1(&shipment,state.logistics_settlement_receipts.contains_key(job_id),state.settled_logistics_transit_ids.contains(job_id))?;
                let value = self
                    .rules
                    .supply_lineages
                    .get_mut(&shipment.lineage_id)
                    .ok_or_else(|| invalid("lineage_missing"))?;
                settle_supply_lineage_v1(value,&shipment)?;
                self.rules
                    .supply_shipments
                    .get_mut(job_id)
                    .expect("validated")
                    .settled = true;
                Ok(job_id.to_string())
            }
            WorldRuleCommandV1::CloseSupply { authorization_id } => {
                let auth = self
                    .rules
                    .supply_authorizations
                    .get(authorization_id)
                    .ok_or_else(|| invalid("authorization_missing"))?
                    .clone();
                if !auth.active || (actor != auth.provider_id && valid(state, &auth, now)) {
                    return Err(invalid("revocation_not_authorized"));
                }
                for batch in self
                    .rules
                    .supply_batches
                    .values_mut()
                    .filter(|b| b.authorization_id == auth.id && b.allocated)
                {
                    for (id, reserved) in &mut batch.allocation {
                        release_supply_close_lineage_v1(self.rules.supply_lineages.get_mut(&lineage(&auth,id)).expect("allocated lineage"),reserved);
                    }
                }
                self.credit(
                    state,
                    &MaterialLedgerId::agent(&auth.provider_id),
                    &auth.material,
                    auth.remaining_escrow,
                )?;
                let value = self
                    .rules
                    .supply_authorizations
                    .get_mut(authorization_id)
                    .expect("validated");
                finish_supply_close_v1(value);
                Ok(authorization_id.clone())
            }
            _ => Err(invalid("unsupported_supply_command")),
        }
    }
}

impl SupplyAuthorizationV1 {
    /// Sole original validity rule for the deliberately unactivated draft
    /// material contract. This is not EmergencySupply Electricity authority
    /// and not a statement about a record's Current/source provenance.
    pub fn is_valid_current_material_contract_v1(&self, _now:u64)->bool{false}
    pub fn is_valid(&self, state: &WorldState, now: u64) -> bool {
        valid(state, self, now)
    }
}

#[path = "supply_minimum_service.rs"]
mod minimum_service;
pub use minimum_service::*;

#[path = "supply_observation.rs"]
mod observation;
pub use observation::{project_emergency_observation_coverage_with_work_v1,project_emergency_observation_coverage_v1,validate_observation_location_from_records_v1};

#[path = "supply_read.rs"]
mod read;

/// Sole original settlement readiness predicate. Input observations are data;
/// they do not authorize a Current write or grant a lifecycle event capability.
pub fn validate_supply_settlement_ready_v1(shipment:&SupplyShipmentV1,receipt_present:bool,settled_present:bool)->Result<(),WorldError>{
    if shipment.settled||!receipt_present||!settled_present{return Err(invalid("await_logistics_receipt_or_already_settled"));}Ok(())
}
/// Original arithmetic and first-error sequence over one selected lineage.
pub fn settle_supply_lineage_v1(value:&mut SupplyLineageV1,shipment:&SupplyShipmentV1)->Result<(),WorldError>{
    value.reserved=value.reserved.checked_sub(shipment.input_amount).filter(|v|*v>=0).ok_or_else(||invalid("lineage_reservation_mismatch"))?;
    value.fulfilled=value.fulfilled.checked_add(shipment.delivered_amount).ok_or_else(||invalid("lineage_overflow"))?;Ok(())
}
/// Keep original ordinary subtraction, including its build-profile overflow
/// behavior, and the original reservation-before-allocation-zero ordering.
pub fn release_supply_close_lineage_v1(lineage:&mut SupplyLineageV1,reserved:&mut i64){lineage.reserved-=*reserved;*reserved=0;}
pub fn finish_supply_close_v1(authorization:&mut SupplyAuthorizationV1){authorization.active=false;authorization.remaining_escrow=0;}

pub use observation::snapshot_key as emergency_observation_snapshot_key_v1;
