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
        let mut visible: BTreeSet<String> = self
            .needs
            .iter()
            .filter(|(_, n)| reader == Some(n.beneficiary_id.as_str()))
            .map(|(id, _)| id.clone())
            .collect();
        for a in self
            .authorizations
            .values()
            .filter(|a| reader == Some(a.provider_id.as_str()))
        {
            visible.extend(a.requested.iter().cloned());
        }
        for review in self.reviews.values() {
            if self
                .batches
                .get(&review.batch_id)
                .and_then(|b| self.authorizations.get(&b.authorization_id))
                .is_some_and(|a| reader == Some(a.reviewer_id.as_str()))
            {
                visible.insert(review.need_id.clone());
            }
        }
        let mut result = self.clone();
        result.needs.retain(|id, _| visible.contains(id));
        let obligations: BTreeSet<_> = result
            .needs
            .values()
            .map(|n| n.obligation_id.clone())
            .chain(
                self.services
                    .values()
                    .filter(|s| reader == Some(s.beneficiary_id.as_str()))
                    .map(|s| s.obligation_id.clone()),
            )
            .collect();
        result.services.retain(|id, _| obligations.contains(id));
        for e in result.evidence.values_mut() {
            for w in &mut e.observations {
                w.obligation_ids.retain(|id| obligations.contains(id));
                w.normal_coverage.retain(|id, _| obligations.contains(id));
            }
        }
        for w in result.observations.values_mut() {
            w.obligation_ids.retain(|id| obligations.contains(id));
            w.normal_coverage.retain(|id, _| obligations.contains(id));
        }
        for coverage in result.observation_interval_coverage.values_mut() {
            coverage.retain(|id, _| visible.contains(id));
        }
        for a in result.authorizations.values_mut() {
            a.requested.retain(|id| visible.contains(id));
        }
        for b in result.batches.values_mut() {
            b.demands.retain(|id, _| visible.contains(id));
            b.frozen_ledger.retain(|id, _| visible.contains(id));
            b.allocation.retain(|id, _| visible.contains(id));
        }
        result
            .reviews
            .retain(|_, r| visible.contains(&r.need_id) || reader == Some(r.requester_id.as_str()));
        result
            .restricted_power
            .retain(|_, p| reader == Some(p.beneficiary_id.as_str()));
        result
    }
}
