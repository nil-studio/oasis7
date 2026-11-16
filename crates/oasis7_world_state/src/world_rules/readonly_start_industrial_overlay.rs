//! Borrow the exact supplied industrial rows over the original Legacy facts.
//! This replaces the old NewRecipe World snapshot without constructing a World.
use super::{
    IndustrialOwnerReadonlyFactsV1, IndustrialProductReadonlyFactsV1,
    IndustrialReceiptReadonlyFactsV1, IndustrialScratchReadonlyFactsV1,
    IndustrialStartEnvelopeReadonlyFactsV1, IndustrialStartReadonlyFactsV1,
    ManufacturingQuoteReadonlyFactsV1, RecipeStartedReadonlyFactsV1,
};
use crate::*;
use oasis7_agent_api::world_action::*;
use std::{collections::BTreeMap, ops::ControlFlow};
pub(super) struct IndustrialInputOverlayFactsV1<'a> {
    pub source: &'a WorldState,
    pub substitution: &'a SubstitutionStateV1,
    pub industrial: &'a oasis7_agent_api::world_action::IndustrialWorkStateV1,
}
impl IndustrialOwnerReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    type Error = std::convert::Infallible;
    fn owner_work(&self, s: u64, b: usize) -> Result<(), std::convert::Infallible> {
        self.source.owner_work(s, b)
    }
    fn owner_before_owned(&self, b: usize) -> Result<(), std::convert::Infallible> {
        self.source.owner_before_owned(b)
    }
    fn owner_agent_present(&self, id: &str) -> Result<bool, std::convert::Infallible> {
        self.source.owner_agent_present(id)
    }
    fn owner_visit_orders(
        &self,
        v: impl FnMut(
            &super::AgentIndustrialOrderV1,
        ) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        self.source.owner_visit_orders(v)
    }
    fn owner_with_claim<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&AgentClaimState>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.owner_with_claim(id, v)
    }
    fn owner_with_starter<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&StarterOcClaimState>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.owner_with_starter(id, v)
    }
    fn owner_acquisition_sequence(
        &self,
        id: &str,
    ) -> Result<Option<u64>, std::convert::Infallible> {
        self.source.owner_acquisition_sequence(id)
    }
}
impl ManufacturingQuoteReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    fn quote_with_factory<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&FactoryState>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.quote_with_factory(id, v)
    }
    fn quote_with_site<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&FactorySiteAuthorityV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.quote_with_site(id, v)
    }
    fn quote_with_anchor<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&LocationAnchorV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.quote_with_anchor(id, v)
    }
    fn quote_with_policy<T>(
        &self,
        v: impl FnOnce(&GameplayPolicyState) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.quote_with_policy(v)
    }
    fn quote_factory_retired(&self, id: &str) -> Result<bool, std::convert::Infallible> {
        self.source.quote_factory_retired(id)
    }
    fn quote_visit_claims(
        &self,
        v: impl FnMut(&AgentClaimState) -> Result<(), std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        self.source.quote_visit_claims(v)
    }
    fn quote_visit_jobs(
        &self,
        v: impl FnMut(&RecipeJobState) -> Result<(), std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        self.source.quote_visit_jobs(v)
    }
    fn quote_with_ledger<T>(
        &self,
        id: &MaterialLedgerId,
        v: impl FnOnce(Option<&BTreeMap<String, i64>>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.quote_with_ledger(id, v)
    }
    fn quote_agent_electricity(&self, id: &str) -> Result<Option<i64>, std::convert::Infallible> {
        self.source.quote_agent_electricity(id)
    }
    fn quote_with_account<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&MainTokenAccountBalance>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.quote_with_account(id, v)
    }
    fn quote_reputation(&self, id: &str) -> Result<Option<i64>, std::convert::Infallible> {
        self.source.quote_reputation(id)
    }
}
impl RecipeStartedReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    fn started_pending(&self, id: u64) -> Result<bool, std::convert::Infallible> {
        self.source.started_pending(id)
    }
    fn started_settled(&self, id: u64) -> Result<bool, std::convert::Infallible> {
        self.source.started_settled(id)
    }
    fn started_visit_jobs(
        &self,
        v: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        self.source.started_visit_jobs(v)
    }
    fn started_material(
        &self,
        ledger: &MaterialLedgerId,
        kind: &str,
    ) -> Result<i64, std::convert::Infallible> {
        self.source.started_material(ledger, kind)
    }

    fn started_with_path<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&LogisticsPathAuthorityV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.started_with_path(id, v)
    }
}

impl IndustrialProductReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    fn industrial_with_product<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_wasm_abi::ProductProfileV1>,
        ) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.industrial_with_product(id, visit)
    }
}
impl IndustrialScratchReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    fn industrial_with_lot<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialLotV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.lots.get(id))
    }
    fn industrial_with_lot_authority<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialLotAuthorityV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.imported_lot_authorities.get(id))
    }
    fn industrial_with_receipt<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialReceiptV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.receipts.get(id))
    }
    fn industrial_with_disposition<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&DispositionPlanV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.dispositions.get(id))
    }
    fn industrial_sink_present(&self, key: &str) -> Result<bool, std::convert::Infallible> {
        Ok(self.industrial.sink_keys.contains(key))
    }
}
impl IndustrialStartReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    fn start_with_root<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&RootOperationV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.roots.get(id))
    }
    fn start_visit_jobs(
        &self,
        visit: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        self.source.start_visit_jobs(visit)
    }
    fn start_with_recipe<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_wasm_abi::RecipeProfileV1>,
        ) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.start_with_recipe(id, visit)
    }
    fn start_with_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.start_with_progress(visit)
    }
}
impl IndustrialStartEnvelopeReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    fn start_with_request<T>(
        &self,
        key: &str,
        visit: impl FnOnce(Option<&super::WorldRuleReceiptV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        self.source.start_with_request(key, visit)
    }
}
impl IndustrialReceiptReadonlyFactsV1 for IndustrialInputOverlayFactsV1<'_> {
    type Error = std::convert::Infallible;
    fn receipt_metered(&self) -> bool {
        false
    }
    fn receipt_work(&self, _: u64, _: usize, _: usize) -> Result<(), std::convert::Infallible> {
        Ok(())
    }
    fn receipt_count(&self) -> Result<usize, std::convert::Infallible> {
        Ok(self.industrial.receipts.len())
    }
    fn receipt_with_root<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&RootOperationV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.roots.get(id))
    }
    fn receipt_with_existing<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&IndustrialReceiptV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        visit(self.industrial.receipts.get(id))
    }
    fn receipt_visit(
        &self,
        mut visit: impl FnMut(&IndustrialReceiptV1) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        for value in self.industrial.receipts.values() {
            if visit(value)?.is_break() {
                break;
            }
        }
        Ok(())
    }
}

impl super::readonly_substitution::SubstitutionReadonlyFactsV1
    for IndustrialInputOverlayFactsV1<'_>
{
    fn substitution_metered(&self) -> bool {
        false
    }
    fn substitution_rules_present(&self) -> Result<bool, std::convert::Infallible> {
        Ok(true)
    }
    fn substitution_with_authority<T>(
        &self,
        id: &str,
        v: impl FnOnce(Option<&SubstitutionExecutionAuthorityV1>) -> Result<T, std::convert::Infallible>,
    ) -> Result<T, std::convert::Infallible> {
        v(self.substitution.authorities.get(id))
    }
    fn substitution_visit_authorities(
        &self,
        mut v: impl FnMut(
            &SubstitutionExecutionAuthorityV1,
        ) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        for row in self.substitution.authorities.values() {
            if v(row)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn substitution_visit_roots(
        &self,
        mut v: impl FnMut(&RootOperationV1) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        for row in self.industrial.roots.values() {
            if v(row)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn substitution_visit_lots(
        &self,
        mut v: impl FnMut(&IndustrialLotV1) -> Result<ControlFlow<()>, std::convert::Infallible>,
    ) -> Result<(), std::convert::Infallible> {
        for row in self.industrial.lots.values() {
            if v(row)?.is_break() {
                break;
            }
        }
        Ok(())
    }
}
