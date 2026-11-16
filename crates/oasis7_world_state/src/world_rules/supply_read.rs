use super::policy::{evidence_end, fresh};
use super::*;

impl EmergencySupplyStateV1 {
    pub fn authorization_is_valid(&self, id: &str, now: u64) -> bool {
        let Some(a) = self.authorizations.get(id) else {
            return false;
        };
        let Some(e) = self.evidence.get(&a.evidence_id) else {
            return false;
        };
        a.active
            && a.starts_at <= now
            && now < a.expires_at
            && fresh(e, now)
            && !self
                .evidence
                .values()
                .filter(|candidate| {
                    candidate.region_id == e.region_id
                        && candidate.region_budget_lineage == e.region_budget_lineage
                        && evidence_end(candidate) >= evidence_end(e)
                })
                .any(|candidate| self.validate_observations(candidate, now, false) == Ok(true))
    }
    pub fn authorization_is_valid_for_world(&self, state: &WorldState, id: &str, now: u64) -> bool {
        self.authorization_is_valid(id, now)
            && self
                .authorizations
                .get(id)
                .and_then(|a| self.evidence.get(&a.evidence_id))
                .is_some_and(|e| self.validate_current_sources(state, e, now).is_ok())
    }
    pub fn for_reader(&self, reader: Option<&str>) -> Self {
        let visibility_source=super::readonly_visibility::LegacyEmergencyVisibilityV1(self);
        let visibility=super::readonly_visibility::emergency_visibility_plan_v1(&visibility_source,reader).unwrap();
        let mut result=self.clone();
        result.needs.retain(|id,_|visibility.need(&visibility_source,id).unwrap());
        result.services.retain(|id, _| visibility.obligation(&visibility_source,id).unwrap());
        for e in result.evidence.values_mut() {
            for w in &mut e.observations {
                w.obligation_ids.retain(|id| visibility.obligation(&visibility_source,id).unwrap());
                w.normal_coverage.retain(|id, _| visibility.obligation(&visibility_source,id).unwrap());
            }
        }
        for w in result.observations.values_mut() {
            w.obligation_ids.retain(|id| visibility.obligation(&visibility_source,id).unwrap());
            w.normal_coverage.retain(|id, _| visibility.obligation(&visibility_source,id).unwrap());
        }
        for coverage in result.observation_interval_coverage.values_mut() {
            coverage.retain(|id, _| visibility.need(&visibility_source,id).unwrap());
        }
        for a in result.authorizations.values_mut() {
            a.requested.retain(|id| visibility.need(&visibility_source,id).unwrap());
        }
        for b in result.batches.values_mut() {
            b.demands.retain(|id, _| visibility.need(&visibility_source,id).unwrap());
            b.frozen_ledger.retain(|id, _| visibility.need(&visibility_source,id).unwrap());
            b.allocation.retain(|id, _| visibility.need(&visibility_source,id).unwrap());
        }
        result
            .reviews
            .retain(|_, r| visibility.review(&visibility_source,&r.need_id,&r.requester_id,reader).unwrap());
        result
            .restricted_power
            .retain(|_, p| visibility.restricted(&visibility_source,&p.beneficiary_id,reader).unwrap());
        result
    }
}
