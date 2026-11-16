//! Selected escrow-credit DATA for the ordinary RecipeStarted rehearsal. The
//! original material rows remain borrowed and no WorldState is reconstructed.
use super::{
    IndustrialOwnerReadonlyFactsV1, ManufacturingQuoteReadonlyFactsV1, RecipeStartedReadonlyFactsV1,
};
use crate::*;
use std::{collections::BTreeMap, ops::ControlFlow};
pub(super) struct EscrowCreditFactsV1<'a, S> {
    pub source: &'a S,
    pub ledger: &'a MaterialLedgerId,
    pub credits: &'a BTreeMap<String, i64>,
}
impl<S: RecipeStartedReadonlyFactsV1> IndustrialOwnerReadonlyFactsV1
    for EscrowCreditFactsV1<'_, S>
{
    type Error = S::Error;
    fn owner_work(&self, s: u64, b: usize) -> Result<(), Self::Error> {
        self.source.owner_work(s, b)
    }
    fn owner_before_owned(&self, b: usize) -> Result<(), Self::Error> {
        self.source.owner_before_owned(b)
    }
    fn owner_agent_present(&self, id: &str) -> Result<bool, Self::Error> {
        self.source.owner_agent_present(id)
    }
    fn owner_visit_orders(
        &self,
        v: impl FnMut(&super::AgentIndustrialOrderV1) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.owner_visit_orders(v)
    }
    fn owner_with_claim<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&AgentClaimState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.owner_with_claim(id, v)
    }
    fn owner_with_starter<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&StarterOcClaimState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.owner_with_starter(id, v)
    }
    fn owner_acquisition_sequence(&self, id: &str) -> Result<Option<u64>, Self::Error> {
        self.source.owner_acquisition_sequence(id)
    }
}
impl<S: RecipeStartedReadonlyFactsV1> ManufacturingQuoteReadonlyFactsV1
    for EscrowCreditFactsV1<'_, S>
{
    fn quote_with_factory<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&FactoryState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.quote_with_factory(id, v)
    }
    fn quote_with_site<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&FactorySiteAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.quote_with_site(id, v)
    }
    fn quote_with_anchor<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&LocationAnchorV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.quote_with_anchor(id, v)
    }
    fn quote_with_policy<T>(
        &self,
        v: impl FnOnce(&GameplayPolicyState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.quote_with_policy(v)
    }
    fn quote_factory_retired(&self, id: &str) -> Result<bool, Self::Error> {
        self.source.quote_factory_retired(id)
    }
    fn quote_visit_claims(
        &self,
        v: impl FnMut(&AgentClaimState) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.quote_visit_claims(v)
    }
    fn quote_visit_jobs(
        &self,
        v: impl FnMut(&RecipeJobState) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.quote_visit_jobs(v)
    }
    fn quote_with_ledger<T>(
        &self,
        id: &MaterialLedgerId,
        v: impl FnOnce(Option<&BTreeMap<String, i64>>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.quote_with_ledger(id, v)
    }
    fn quote_agent_electricity(&self, id: &str) -> Result<Option<i64>, Self::Error> {
        self.source.quote_agent_electricity(id)
    }
    fn quote_with_account<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&MainTokenAccountBalance>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.quote_with_account(id, v)
    }
    fn quote_reputation(&self, id: &str) -> Result<Option<i64>, Self::Error> {
        self.source.quote_reputation(id)
    }
}
impl<S: RecipeStartedReadonlyFactsV1> RecipeStartedReadonlyFactsV1 for EscrowCreditFactsV1<'_, S> {
    fn started_pending(&self, id: u64) -> Result<bool, Self::Error> {
        self.source.started_pending(id)
    }
    fn started_settled(&self, id: u64) -> Result<bool, Self::Error> {
        self.source.started_settled(id)
    }
    fn started_visit_jobs(
        &self,
        v: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.source.started_visit_jobs(v)
    }
    fn started_material(&self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        self.owner_work(1, std::mem::size_of::<MaterialLedgerId>() * 2)?;
        if ledger == self.ledger {
            self.owner_work(
                1,
                (kind.len() + 1).saturating_mul((self.credits.len() + 1).ilog2() as usize * 2 + 4),
            )?;
            if let Some(value) = self.credits.get(kind) {
                return Ok(*value);
            }
        }
        self.source.started_material(ledger, kind)
    }
    fn started_with_path<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&LogisticsPathAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.source.started_with_path(id, v)
    }
}
