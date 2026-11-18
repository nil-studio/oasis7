use super::super::agent_claims::{AgentClaimCostQuote, agent_claim_quote};
use super::super::{AgentClaimState, DomainEvent, WorldError, WorldEvent, WorldEventBody};
use super::World;

impl World {
    pub fn agent_claim(&self, target_agent_id: &str) -> Option<&AgentClaimState> {
        self.state.agent_claims.get(target_agent_id)
    }

    pub fn claimed_agent_count(&self, claimer_agent_id: &str) -> usize {
        self.state
            .agent_claims
            .values()
            .filter(|claim| claim.claim_owner_id == claimer_agent_id)
            .count()
    }

    pub(super) fn agent_claim_quote_for_owner(
        &self,
        claimer_agent_id: &str,
    ) -> Result<AgentClaimCostQuote, WorldError> {
        let owned_claim_count = if self.state.starter_oc_claims.contains_key(claimer_agent_id) {
            let mut targets =
                crate::world_rules::manufacturing_existing_targets(&self.state, claimer_agent_id)?;
            targets.extend(crate::world_rules::manufacturing_reserved_targets(
                &self.state,
                claimer_agent_id,
            ));
            targets.len()
        } else {
            self.claimed_agent_count(claimer_agent_id)
        };
        let reputation_score = self
            .state
            .reputation_scores
            .get(claimer_agent_id)
            .copied()
            .unwrap_or(0);
        agent_claim_quote(reputation_score, owned_claim_count).map_err(|reason| {
            WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "agent claim quote unavailable: claimer={} reason={reason}",
                    claimer_agent_id
                ),
            }
        })
    }

    pub(super) fn process_agent_claim_epochs(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let current_epoch = self.current_agent_claim_epoch();
        if current_epoch <= self.state.agent_claim_last_processed_epoch {
            return Ok(Vec::new());
        }

        let mut emitted = Vec::new();
        let claims = self
            .state
            .agent_claims
            .values()
            .cloned()
            .collect::<Vec<_>>();

        for claim in claims {
            self.process_agent_claim_epoch(current_epoch, &claim, &mut emitted)?;
        }

        self.state.agent_claim_last_processed_epoch = current_epoch;
        Ok(emitted)
    }

    fn process_agent_claim_epoch(
        &mut self,
        current_epoch: u64,
        claim: &AgentClaimState,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        let Some(event) = self.prepare_agent_claim_epoch_event(current_epoch, claim)? else {
            return Ok(());
        };
        let target_agent_id = claim.target_agent_id.clone();
        let upkeep_settled = matches!(event, DomainEvent::AgentClaimUpkeepSettled { .. });
        self.append_agent_claim_event(event, emitted)?;

        if upkeep_settled {
            let Some(refreshed_claim) = self.state.agent_claims.get(&target_agent_id) else {
                return Ok(());
            };
            if let Some(followup) =
                self.prepare_agent_claim_release_or_idle_event(current_epoch, refreshed_claim)?
            {
                self.append_agent_claim_event(followup, emitted)?;
            }
        }
        Ok(())
    }

    fn prepare_agent_claim_epoch_event(
        &self,
        current_epoch: u64,
        claim: &AgentClaimState,
    ) -> Result<Option<DomainEvent>, WorldError> {
        let Some(latest_claim) = self.state.agent_claims.get(&claim.target_agent_id).cloned()
        else {
            return Ok(None);
        };

        let refund = self.restricted_starter_claim_refund_sink_for_claim(&latest_claim);
        super::incremental_normal_claim_epochs::evaluate::prepare_epoch_v1(
            current_epoch,&latest_claim,
            self.main_token_liquid_balance(&latest_claim.claim_owner_id),
            self.main_token_restricted_starter_claim_balance(&latest_claim.claim_owner_id),
            self.agent_last_control_epoch(&latest_claim.target_agent_id),&refund)
    }

    fn prepare_agent_claim_release_or_idle_event(
        &self,
        current_epoch: u64,
        claim: &AgentClaimState,
    ) -> Result<Option<DomainEvent>, WorldError> {
        let refund = self.restricted_starter_claim_refund_sink_for_claim(claim);
        super::incremental_normal_claim_epochs::evaluate::prepare_release_or_idle_v1(
            current_epoch,claim,self.agent_last_control_epoch(&claim.target_agent_id),&refund)
    }

    #[cfg(test)]
    pub(crate) fn prepared_next_agent_claim_event_for_test(
        &self,
        target_agent_id: &str,
        at_tick: u64,
    ) -> Result<Option<DomainEvent>, WorldError> {
        let Some(claim) = self.state.agent_claims.get(target_agent_id) else {
            return Ok(None);
        };
        self.prepare_agent_claim_epoch_event(self.agent_claim_epoch_for_tick(at_tick), claim)
    }

    fn current_agent_claim_epoch(&self) -> u64 {
        self.agent_claim_epoch_for_tick(self.state.time)
    }

    fn agent_last_control_epoch(&self, target_agent_id: &str) -> u64 {
        self.state
            .agents
            .get(target_agent_id)
            .map(|cell| self.agent_claim_epoch_for_tick(cell.last_active))
            .unwrap_or_else(|| self.current_agent_claim_epoch())
    }

    fn agent_claim_epoch_for_tick(&self, tick: u64) -> u64 {
        let epoch_length = self.governance_execution_policy().epoch_length_ticks.max(1);
        tick / epoch_length
    }

    fn append_agent_claim_event(
        &mut self,
        event: DomainEvent,
        emitted: &mut Vec<WorldEvent>,
    ) -> Result<(), WorldError> {
        self.append_event(WorldEventBody::Domain(event), None)?;
        if let Some(event) = self.journal.events.last() {
            emitted.push(event.clone());
        }
        Ok(())
    }
}

// Historical v0 acquisition fixture: upkeep/replay regressions must continue
// covering worlds with pre-industrial claims, without reopening the v1 action.
#[cfg(test)]
impl World {
    pub(crate) fn append_legacy_claim_fixture(&mut self, owner: &str, target: &str) {
        let q = agent_claim_quote(
            self.state
                .reputation_scores
                .get(owner)
                .copied()
                .unwrap_or(0),
            self.claimed_agent_count(owner),
        )
        .unwrap();
        let mut data = serde_json::to_value(&q)
            .unwrap()
            .as_object()
            .unwrap()
            .clone();
        data.remove("claim_cap");
        data.extend(serde_json::json!({
            "claimer_agent_id":owner,"target_agent_id":target,
            "upfront_restricted_spent_amount":0,
            "upfront_liquid_spent_amount":q.activation_fee_amount+q.claim_bond_amount+q.upkeep_per_epoch,
            "auto_issued_restricted_amount":0,
            "auto_issued_restricted_source_treasury_bucket_id":null,
            "claim_bond_locked_restricted_amount":0,
            "claim_bond_locked_liquid_amount":q.claim_bond_amount,
            "claimed_at_epoch":self.current_governance_epoch(),
            "upkeep_paid_through_epoch":self.current_governance_epoch()
        }).as_object().unwrap().clone());
        let event: DomainEvent =
            serde_json::from_value(serde_json::json!({"type":"AgentClaimed","data":data})).unwrap();
        self.append_event(WorldEventBody::Domain(event), None)
            .unwrap();
    }
}
