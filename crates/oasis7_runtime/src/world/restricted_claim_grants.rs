#[cfg(test)]
use super::super::DomainEvent;
use super::super::{
    AgentClaimState, RestrictedStarterClaimGrantStatus, RestrictedStarterClaimRefundSink,
    WorldError, WorldEvent, WorldEventBody,
};
use super::World;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RestrictedStarterClaimRefundDecision {
    pub sink: RestrictedStarterClaimRefundSink,
    pub treasury_bucket_id: Option<String>,
}

impl World {
    pub(super) fn restricted_starter_claim_locked_amount(&self, account_id: &str) -> u64 {
        self.state
            .agent_claims
            .values()
            .filter(|claim| claim.claim_owner_id == account_id)
            .fold(0_u64, |acc, claim| {
                acc.saturating_add(claim.claim_bond_locked_restricted_amount)
            })
    }

    pub(super) fn restricted_starter_claim_grant_can_be_reissued(&self, account_id: &str) -> bool {
        let Some(grant) = self.state.restricted_starter_claim_grants.get(account_id) else {
            return true;
        };
        if grant.status == RestrictedStarterClaimGrantStatus::Issued {
            return false;
        }
        self.main_token_restricted_starter_claim_balance(account_id) == 0
            && self.restricted_starter_claim_locked_amount(account_id) == 0
    }

    pub(super) fn restricted_starter_claim_refund_sink_for_claim(
        &self,
        claim: &AgentClaimState,
    ) -> RestrictedStarterClaimRefundDecision {
        super::incremental_normal_claim_epochs::evaluate::refund_sink_v1(claim,
            self.state.restricted_starter_claim_grants.get(&claim.claim_owner_id))
    }

    fn prepare_restricted_starter_claim_grant_expiry_bodies_at_epoch(
        &self,
        current_epoch: u64,
    ) -> Vec<WorldEventBody> {
        self.state
            .restricted_starter_claim_grants
            .iter()
            .filter(|(_, grant)| {
                grant.status == RestrictedStarterClaimGrantStatus::Issued
                    && current_epoch >= grant.expires_at_epoch
            })
            .map(|(account_id, grant)| {
                let expired_amount =
                    self.main_token_restricted_starter_claim_balance(account_id.as_str());
                WorldEventBody::Domain(super::incremental_normal_claim_epochs::evaluate::grant_expiry_v1(account_id,grant,current_epoch,expired_amount))
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn prepared_restricted_starter_claim_grant_expiry_accounts_for_test(
        &self,
        current_epoch: u64,
    ) -> Vec<String> {
        self.prepare_restricted_starter_claim_grant_expiry_bodies_at_epoch(current_epoch)
            .into_iter()
            .filter_map(|body| match body {
                WorldEventBody::Domain(DomainEvent::RestrictedStarterClaimGrantExpired {
                    beneficiary_account_id,
                    ..
                }) => Some(beneficiary_account_id),
                _ => None,
            })
            .collect()
    }

    pub(super) fn process_restricted_starter_claim_grant_epochs(
        &mut self,
    ) -> Result<Vec<WorldEvent>, WorldError> {
        let current_epoch = self.current_governance_epoch();
        let prepared =
            self.prepare_restricted_starter_claim_grant_expiry_bodies_at_epoch(current_epoch);
        let mut emitted = Vec::with_capacity(prepared.len());
        for body in prepared {
            self.append_event(body, None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }

        Ok(emitted)
    }
}
