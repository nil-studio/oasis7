use super::*;
use oasis7_agent_api::world_action::{
    EmergencyServiceRegistrationV1, MinimumServiceCommandV1, minimum_service_authority_payload,
};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MinimumServiceContractV1 {
    pub registration: EmergencyServiceRegistrationV1,
    pub owner_source_id: String,
    pub operator_agent_id: String,
    pub region_budget_lineage: String,
    pub registered_at: u64,
    pub certificate: oasis7_agent_api::world_action::GovernanceFinalityCertificate,
}
impl PreparedWorldRule {
    pub fn minimum_service(
        &mut self,
        state: &WorldState,
        actor: &str,
        _key: &str,
        command: &MinimumServiceCommandV1,
        now: u64,
        world_binding: &str,
    ) -> Result<String, WorldError> {
        let MinimumServiceCommandV1::Register {
            registration: r,
            region_budget_lineage,
            certificate: c,
        } = command;
        super::super::validate_manufactured_world_scope(
            state,
            &r.beneficiary_id,
            Some(world_binding),
        )?;
        let owner = state
            .resolved_agent_owner(&r.beneficiary_id)
            .ok_or_else(|| invalid("need_source_unverifiable"))?;
        if (actor != r.beneficiary_id && actor != owner.primary_agent_id)
            || r.controller_id != owner.player_id
            || r.source_ref != owner.source_id
            || r.registered_at != now
            || r.electricity_per_interval <= 0
            || r.electricity_per_interval > 1_000_000_000
            || r.recipe_ids.is_empty()
            || r.recipe_ids.len() > 128
            || self
                .rules
                .emergency_supply
                .minimum_services
                .contains_key(&r.obligation_id)
        {
            return Err(invalid("need_source_unverifiable"));
        }
        for id in [
            &r.obligation_id,
            &r.region_id,
            &r.location_id,
            &r.controller_id,
            &r.beneficiary_id,
            region_budget_lineage,
        ] {
            if id.is_empty() || id.len() > 128 {
                return Err(invalid("need_source_unverifiable"));
            }
        }
        if r.source_ref.len() > 256 {
            return Err(invalid("need_source_unverifiable"));
        }
        if self
            .rules
            .emergency_supply
            .minimum_services
            .values()
            .any(|existing| {
                existing.registration.location_id == r.location_id
                    && existing.registration.region_id != r.region_id
            })
        {
            return Err(invalid("scope_mismatch"));
        }
        if self
            .rules
            .emergency_supply
            .minimum_services
            .values()
            .any(|source| {
                source.registration.beneficiary_id == r.beneficiary_id
                    && !source.registration.recipe_ids.is_disjoint(&r.recipe_ids)
            })
        {
            return Err(invalid("need_source_unverifiable"));
        }
        let location = state
            .agent_location_authorities
            .get(&r.beneficiary_id)
            .ok_or_else(|| invalid("need_source_unverifiable"))?;
        let anchor = state
            .location_anchors
            .get(&r.location_id)
            .ok_or_else(|| invalid("need_source_unverifiable"))?;
        if !location.active
            || !anchor.active
            || location.location_id != r.location_id
            || location.effective_at > now
            || anchor.effective_at > now
        {
            return Err(invalid("need_source_unverifiable"));
        }
        if world_binding.is_empty()
            || self
                .rules
                .emergency_supply
                .world_id
                .as_ref()
                .is_some_and(|id| id != world_binding)
        {
            return Err(invalid("scope_mismatch"));
        }
        let payload =
            minimum_service_authority_payload(actor, r, region_budget_lineage, world_binding, now)
                .map_err(|_| invalid("invalid_evidence"))?;
        let manifest = crate::util::sha256_hex(&payload);
        if c.proposal_id
            != u64::from_str_radix(&manifest[..16], 16).map_err(|_| invalid("invalid_evidence"))?
        {
            return Err(invalid("invalid_evidence"));
        }
        super::super::authority::verify_world_rule_authority(state, &manifest, now, c)?;
        if self
            .rules
            .emergency_supply
            .region_lineages
            .get(&r.region_id)
            .is_some_and(|lineage| lineage != region_budget_lineage)
        {
            return Err(invalid("scope_mismatch"));
        }
        self.rules.emergency_supply.world_id = Some(world_binding.into());
        self.rules
            .emergency_supply
            .region_lineages
            .insert(r.region_id.clone(), region_budget_lineage.clone());
        self.rules.emergency_supply.minimum_services.insert(
            r.obligation_id.clone(),
            MinimumServiceContractV1 {
                registration: r.clone(),
                owner_source_id: owner.source_id,
                operator_agent_id: owner.primary_agent_id,
                region_budget_lineage: region_budget_lineage.clone(),
                registered_at: now,
                certificate: c.clone(),
            },
        );
        Ok(r.obligation_id.clone())
    }
}

pub fn source_owner_is_current(state: &WorldState, source: &MinimumServiceContractV1) -> bool {
    state
        .resolved_agent_owner(&source.registration.beneficiary_id)
        .is_some_and(|owner| {
            owner.player_id == source.registration.controller_id
                && owner.source_id == source.owner_source_id
                && owner.source_id == source.registration.source_ref
                && owner.primary_agent_id == source.operator_agent_id
        })
}
