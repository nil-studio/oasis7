//! Fixed facts over actual readonly Current columns. No WorldState is
//! reconstructed and no caller status, permission or JSON is accepted.
use super::*;
use super::{
    fact_decode,
    original_rows::{OriginalProjectionRowDataV1, OriginalProjectionRowsDataV1},
    state_columns::OriginalStateColumnsDataV1,
};
use oasis7_world_state::incremental_semantic_key::{SemanticKeyV1, write_semantic_key_v1};
use oasis7_world_state::{
    FactorySiteAuthorityV1, FactoryState, GameplayPolicyState, IndustryProgressState,
    MaterialLedgerId, RecipeJobState, gameplay_capability::GameplayCapabilityReadonlyFactsV1,
};
use std::{
    cell::RefCell,
    io::{self, Write},
    mem::size_of,
    ops::ControlFlow,
};

#[path = "incremental_normal_projection_commitment_read_source.rs"]
pub(super) mod read_source;
pub(in crate::world) use read_source::{CurrentProjectionReadSourceV1, ProjectionReadSourceV1};

pub(in crate::world) struct CurrentProjectionReadFactsV1<'a, 'c> {
    agent_intents: Option<&'a OriginalProjectionRowsDataV1>,
    columns: &'a OriginalStateColumnsDataV1,
    agent_electricity: &'a OriginalProjectionRowsDataV1,
    source: &'a dyn ProjectionReadSourceV1,
    common: RefCell<&'a mut FormalBoundedContextV1<'c>>,
    core: WorkIdentityV1,
    clock: u64,
    fees: RefCell<Vec<MemoryReservation>>,
    slots: RefCell<MemoryReservation>,
    _fee: MemoryReservation,
}
impl<'a, 'c> CurrentProjectionReadFactsV1<'a, 'c> {
    /// Only private, closed provenance adapters can enter this DATA kernel.
    pub(super) fn for_projection_data_v1(
        columns: &'a OriginalStateColumnsDataV1,
        agent_electricity: &'a OriginalProjectionRowsDataV1,
        metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
        source: &'a dyn ProjectionReadSourceV1,
        common: &'a mut FormalBoundedContextV1<'c>,
    ) -> Result<Self, BodyMutationError> {
        source.validate_metadata(metadata, common)?;
        columns.validate_context(common)?;
        agent_electricity.validate(common)?;
        common.charge(1, size_of::<Self>() as u64).map_err(paid)?;
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let slots = common
            .memory()
            .reserve(0, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        source.begin_facts(common)?;
        Ok(Self {
            agent_intents: None,
            columns,
            agent_electricity,
            source,
            core: common.work().identity(),
            clock: metadata.current_clock(),
            common: RefCell::new(common),
            fees: RefCell::new(Vec::new()),
            slots: RefCell::new(slots),
            _fee: fee,
        })
    }
    pub(super) fn with_security_data_v1(self) -> Result<Self, BodyMutationError> {
        self.source.process_policy(&mut self.common.borrow_mut())?;
        self.source.authorization(&mut self.common.borrow_mut())?;
        self.source
            .finality_context(&mut self.common.borrow_mut())?;
        Ok(self)
    }
    pub(super) fn verify_provider_audience_finality_v1(
        &self,
        epoch: u64,
    ) -> Result<
        (),
        crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityErrorV1,
    > {
        self.check()?;
        self.source.finality(epoch, &mut self.common.borrow_mut())
    }
    pub(super) fn verify_authorization_root_data_v1(&self) -> Result<(), BodyMutationError> {
        self.check()?;
        self.source.authorization(&mut self.common.borrow_mut())
    }
    pub(super) fn process_facts_v1(
        &self,
    ) -> Result<read_source::ProjectionProcessPolicyDataV1, BodyMutationError> {
        self.check()?;
        self.source.process_policy(&mut self.common.borrow_mut())
    }
    fn check(&self) -> Result<(), BodyMutationError> {
        self.source.check(&mut self.common.borrow_mut())?;
        let common = self.common.borrow();
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(invalid("Current projection facts context"));
        }
        Ok(())
    }
    fn rows(&self, name: &str) -> Result<&OriginalProjectionRowsDataV1, BodyMutationError> {
        self.check()?;
        self.columns
            .map_data_v1(name, &mut self.common.borrow_mut())?
            .map(std::sync::Arc::as_ref)
            .ok_or_else(|| invalid("Current projection fact map missing"))
    }
    fn key<K: SemanticKeyV1 + ?Sized>(
        &self,
        key: &K,
    ) -> Result<ReservedValue<Vec<u8>>, BodyMutationError> {
        struct Output<'a, 'b, 'c> {
            common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
            bytes: Option<Vec<u8>>,
            n: usize,
            bound: usize,
            first: Option<BodyMutationError>,
        }
        impl Write for Output<'_, '_, '_> {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                if self.first.is_some() {
                    return Err(io::ErrorKind::Other.into());
                }
                for part in b.chunks(8192) {
                    if let Err(e) = self.common.borrow_mut().charge(1, part.len() as u64) {
                        self.first = Some(paid(e));
                        return Err(io::ErrorKind::Other.into());
                    }
                    let Some(n) = self.n.checked_add(part.len()).filter(|n| *n <= self.bound)
                    else {
                        self.first = Some(invalid("projection fact key output bound"));
                        return Err(io::ErrorKind::Other.into());
                    };
                    if let Some(bytes) = &mut self.bytes {
                        bytes.extend_from_slice(part)
                    }
                    self.n = n
                }
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        self.check()?;
        let mut count = Output {
            common: &self.common,
            bytes: None,
            n: 0,
            bound: usize::MAX,
            first: None,
        };
        let result = write_semantic_key_v1(key, &mut count);
        if let Some(error) = count.first {
            return Err(error);
        }
        result.map_err(paid)?;
        let len = count.n;
        self.common
            .borrow_mut()
            .charge(1, (size_of::<Vec<u8>>() + len) as u64)
            .map_err(paid)?;
        let fee = self
            .common
            .borrow()
            .memory()
            .reserve(size_of::<Vec<u8>>() + len, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let bytes = Vec::with_capacity(len);
        if bytes.capacity() != len {
            return Err(invalid("projection fact key capacity"));
        }
        let mut output = Output {
            common: &self.common,
            bytes: Some(bytes),
            n: 0,
            bound: len,
            first: None,
        };
        let result = write_semantic_key_v1(key, &mut output);
        if let Some(error) = output.first {
            return Err(error);
        }
        result.map_err(paid)?;
        self.common.borrow_mut().consume_steps(0).map_err(paid)?;
        if output.n != len {
            return Err(invalid("projection fact key count changed"));
        }
        Ok(fee.retain(
            output
                .bytes
                .ok_or_else(|| invalid("projection fact key output"))?,
        ))
    }
    fn point<K: SemanticKeyV1 + ?Sized>(
        &self,
        field: &str,
        key: &K,
    ) -> Result<Option<&OriginalProjectionRowDataV1>, BodyMutationError> {
        let key = self.key(key)?;
        self.rows(field)?
            .row_data_v1(&key, &mut self.common.borrow_mut())
    }
    fn raw<'b>(&self, row: &'b OriginalProjectionRowDataV1) -> Result<&'b [u8], BodyMutationError> {
        row.original_value_json()
            .ok_or_else(|| invalid("Current projection fact row kind"))
    }
    pub(in crate::world) fn validate(&self) -> Result<(), BodyMutationError> {
        self.check()
    }
    fn visit_rows(
        &self,
        field: &str,
        mut visit: impl FnMut(
            &OriginalProjectionRowDataV1,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        let mut rows = self
            .rows(field)?
            .iter_data_v1(&mut self.common.borrow_mut())?;
        loop {
            let Some(row) = rows.next_data_v1(&mut self.common.borrow_mut())? else {
                break;
            };
            if visit(row)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn rule_rows(
        &self,
        column:oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1,
    ) -> Result<Option<&OriginalProjectionRowsDataV1>, BodyMutationError> {
        self.check()?;
        let mut common = self.common.borrow_mut();
        let Some(rules) = self.columns.world_rule_rows_data_v1(&mut common)? else {
            return Ok(None);
        };
        Ok(Some(rules.rows_v1(column, &mut common)?.as_ref()))
    }
    fn rule_point<K: SemanticKeyV1 + ?Sized>(
        &self,
        column:oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1,
        key: &K,
    ) -> Result<Option<&OriginalProjectionRowDataV1>, BodyMutationError> {
        let key = self.key(key)?;
        match self.rule_rows(column)? {
            None => Ok(None),
            Some(rows) => rows.row_data_v1(&key, &mut self.common.borrow_mut()),
        }
    }
    fn visit_rule_rows(
        &self,
        column:oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1,
        mut visit: impl FnMut(
            &OriginalProjectionRowDataV1,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        let Some(rows) = self.rule_rows(column)? else {
            return Ok(());
        };
        let mut rows = rows.iter_data_v1(&mut self.common.borrow_mut())?;
        loop {
            let Some(row) = rows.next_data_v1(&mut self.common.borrow_mut())? else {
                break;
            };
            if visit(row)?.is_break() {
                break;
            }
        }
        Ok(())
    }
    fn owner_agent_present_data(&self, id: &str) -> Result<bool, BodyMutationError> {
        Ok(self.point("agents", id)?.is_some())
    }
    fn owner_with_claim_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AgentClaimState>) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("agent_claims", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_agent_claim_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn owner_with_starter_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::StarterOcClaimState>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("starter_oc_claims", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_starter_claim_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn owner_acquisition_sequence_data(&self, id: &str) -> Result<Option<u64>, BodyMutationError> {
        match self.point("agent_claim_acquisition_sequences", id)? {
            None => Ok(None),
            Some(row) => fact_decode::with_u64_data_v1(self.raw(row)?, &self.common, |value| {
                Ok(Some(*value))
            }),
        }
    }
    fn quote_with_anchor_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::LocationAnchorV1>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("location_anchors", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_location_anchor_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn quote_with_account_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::MainTokenAccountBalance>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("main_token_balances", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_token_account_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn quote_with_ledger_data<T>(
        &self,
        id: &MaterialLedgerId,
        visit: impl FnOnce(
            Option<&std::collections::BTreeMap<String, i64>>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("material_ledgers", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_material_ledger_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn quote_reputation_data(&self, id: &str) -> Result<Option<i64>, BodyMutationError> {
        match self.point("reputation_scores", id)? {
            None => Ok(None),
            Some(row) => fact_decode::with_i64_data_v1(self.raw(row)?, &self.common, |value| {
                Ok(Some(*value))
            }),
        }
    }
    fn quote_factory_retired_data(&self, id: &str) -> Result<bool, BodyMutationError> {
        Ok(self.point("retired_factory_ids", id)?.is_some())
    }
    fn quote_agent_electricity_data(&self, id: &str) -> Result<Option<i64>, BodyMutationError> {
        self.check()?;
        let key = self.key(id)?;
        let row = {
            let mut common = self.common.borrow_mut();
            self.agent_electricity.row_data_v1(&key, &mut common)?
        };
        match row {
            None => Ok(None),
            Some(row) => fact_decode::with_i64_data_v1(self.raw(row)?, &self.common, |value| {
                Ok(Some(*value))
            }),
        }
    }
    fn quote_visit_claims_data(
        &self,
        mut visit: impl FnMut(&oasis7_world_state::AgentClaimState) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.visit_rows("agent_claims", |row| {
            fact_decode::with_agent_claim_data_v1(self.raw(row)?, &self.common, |value| {
                visit(value)
            })?;
            Ok(ControlFlow::Continue(()))
        })
    }
    fn quote_visit_jobs_data(
        &self,
        mut visit: impl FnMut(&RecipeJobState) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.visit_rows("pending_recipe_jobs", |row| {
            fact_decode::with_recipe_job_data_v1(self.raw(row)?, &self.common, |value| {
                visit(value)
            })?;
            Ok(ControlFlow::Continue(()))
        })
    }
    fn organization_with_alliance_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AllianceState>) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("alliances", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_alliance_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn organization_visit_contracts_data(
        &self,
        mut visit: impl FnMut(
            &oasis7_world_state::EconomicContractState,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.visit_rows("economic_contracts", |row| {
            fact_decode::with_economic_contract_data_v1(self.raw(row)?, &self.common, |value| {
                visit(value)
            })
        })
    }
    fn industrial_with_product_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_wasm_abi::ProductProfileV1>) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("product_profiles", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_product_profile_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn industrial_with_recipe_data<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_wasm_abi::RecipeProfileV1>) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.point("recipe_profiles", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_recipe_profile_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
}
impl GameplayCapabilityReadonlyFactsV1 for CurrentProjectionReadFactsV1<'_, '_> {
    type Error = BodyMutationError;
    fn work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error> {
        self.check()?;
        self.common
            .borrow_mut()
            .charge(
                steps,
                u64::try_from(bytes).map_err(|_| invalid("projection fact work bytes"))?,
            )
            .map_err(paid)
    }
    fn before_owned(&self, bytes: usize) -> Result<(), Self::Error> {
        self.work(1, bytes)?;
        let fee = self
            .common
            .borrow()
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut fees = self.fees.borrow_mut();
        if fees.len() == fees.capacity() {
            let capacity = fees.capacity().checked_mul(2).unwrap_or(usize::MAX).max(4);
            let n = capacity
                .checked_mul(size_of::<MemoryReservation>())
                .ok_or_else(|| invalid("projection fact fee slots"))?;
            self.work(1, n)?;
            let slots = self
                .common
                .borrow()
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let additional = capacity - fees.len();
            fees.try_reserve_exact(additional)
                .map_err(|_| invalid("projection fact fee capacity"))?;
            if fees.capacity() != capacity {
                return Err(invalid("projection fact fee capacity"));
            }
            *self.slots.borrow_mut() = slots
        }
        fees.push(fee);
        Ok(())
    }
    fn time(&self) -> Result<u64, Self::Error> {
        self.work(1, 0)?;
        Ok(self.clock)
    }
    fn visit_factories(
        &self,
        mut visit: impl FnMut(&str, &FactoryState) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        let mut rows = self
            .rows("factories")?
            .iter_data_v1(&mut self.common.borrow_mut())?;
        loop {
            let next = rows.next_data_v1(&mut self.common.borrow_mut())?;
            let Some(row) = next else { break };
            let serde_json::Value::String(id) = row.original_key_value() else {
                return Err(invalid("Current factory fact key"));
            };
            self.work(1, id.len())?;
            fact_decode::with_factory_data_v1(self.raw(row)?, &self.common, |value| {
                visit(id, value)
            })?
        }
        Ok(())
    }
    fn visit_recipe_jobs(
        &self,
        mut visit: impl FnMut(u64, &RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        let mut rows = self
            .rows("pending_recipe_jobs")?
            .iter_data_v1(&mut self.common.borrow_mut())?;
        loop {
            let next = rows.next_data_v1(&mut self.common.borrow_mut())?;
            let Some(row) = next else { break };
            let id = row
                .original_key_value()
                .as_u64()
                .ok_or_else(|| invalid("Current recipe fact key"))?;
            self.work(1, 0)?;
            if fact_decode::with_recipe_job_data_v1(self.raw(row)?, &self.common, |value| {
                visit(id, value)
            })?
            .is_break()
            {
                break;
            }
        }
        Ok(())
    }
    fn with_factory<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactoryState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.point("factories", id)? {
            None => visit(None),
            Some(row) => fact_decode::with_factory_data_v1(self.raw(row)?, &self.common, |value| {
                visit(Some(value))
            }),
        }
    }
    fn with_industry_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let bytes = self
            .columns
            .scalar_original_json_data_v1("industry_progress", &mut self.common.borrow_mut())?;
        fact_decode::with_industry_progress_data_v1(bytes, &self.common, visit)
    }
    fn with_gameplay_policy<T>(
        &self,
        visit: impl FnOnce(&GameplayPolicyState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        let bytes = self
            .columns
            .scalar_original_json_data_v1("gameplay_policy", &mut self.common.borrow_mut())?;
        fact_decode::with_gameplay_policy_data_v1(bytes, &self.common, visit)
    }
    fn with_site<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactorySiteAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.point("factory_site_authorities", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_site_authority_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
    fn material_ledger_present(&self, id: &MaterialLedgerId) -> Result<bool, Self::Error> {
        self.work(1, 0)?;
        Ok(self.point("material_ledgers", id)?.is_some())
    }
}

impl oasis7_world_state::world_rules::IndustrialOwnerReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    type Error = BodyMutationError;
    fn owner_work(&self, steps: u64, bytes: usize) -> Result<(), Self::Error> {
        self.work(steps, bytes)
    }
    fn owner_before_owned(&self, bytes: usize) -> Result<(), Self::Error> {
        self.before_owned(bytes)
    }
    fn owner_agent_present(&self, id: &str) -> Result<bool, Self::Error> {
        self.owner_agent_present_data(id)
    }
    fn owner_visit_orders(
        &self,
        mut visit: impl FnMut(
            &oasis7_world_state::world_rules::AgentIndustrialOrderV1,
        ) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::AgentOrders,|row|fact_decode::with_industrial_order_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
    fn owner_with_claim<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AgentClaimState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.owner_with_claim_data(id, visit)
    }
    fn owner_with_starter<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::StarterOcClaimState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.owner_with_starter_data(id, visit)
    }
    fn owner_acquisition_sequence(&self, id: &str) -> Result<Option<u64>, Self::Error> {
        self.owner_acquisition_sequence_data(id)
    }
}
impl oasis7_world_state::world_rules::ManufacturingQuoteReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn quote_with_factory<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactoryState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.with_factory(id, visit)
    }
    fn quote_with_site<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&FactorySiteAuthorityV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.with_site(id, visit)
    }
    fn quote_with_anchor<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::LocationAnchorV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.quote_with_anchor_data(id, visit)
    }
    fn quote_with_policy<T>(
        &self,
        visit: impl FnOnce(&GameplayPolicyState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.with_gameplay_policy(visit)
    }
    fn quote_factory_retired(&self, id: &str) -> Result<bool, Self::Error> {
        self.quote_factory_retired_data(id)
    }
    fn quote_visit_claims(
        &self,
        visit: impl FnMut(&oasis7_world_state::AgentClaimState) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.quote_visit_claims_data(visit)
    }
    fn quote_visit_jobs(
        &self,
        visit: impl FnMut(&RecipeJobState) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error> {
        self.quote_visit_jobs_data(visit)
    }
    fn quote_with_ledger<T>(
        &self,
        id: &MaterialLedgerId,
        visit: impl FnOnce(Option<&std::collections::BTreeMap<String, i64>>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.quote_with_ledger_data(id, visit)
    }
    fn quote_agent_electricity(&self, id: &str) -> Result<Option<i64>, Self::Error> {
        self.quote_agent_electricity_data(id)
    }
    fn quote_with_account<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::MainTokenAccountBalance>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.quote_with_account_data(id, visit)
    }
    fn quote_reputation(&self, id: &str) -> Result<Option<i64>, Self::Error> {
        self.quote_reputation_data(id)
    }
}
impl oasis7_world_state::world_rules::OrganizationActivityReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn organization_time(&self) -> Result<u64, Self::Error> {
        self.time()
    }
    fn organization_visit_registrations(
        &self,
        mut visit: impl FnMut(
            &oasis7_world_state::world_rules::OrganizationRegistrationV1,
        ) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::OrganizationRegistrations,|row|fact_decode::with_organization_registration_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
    fn organization_visit_cases(
        &self,
        mut visit: impl FnMut(
            &oasis7_world_state::world_rules::OrganizationContinuityCaseV1,
        ) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::OrganizationContinuityCases,|row|fact_decode::with_organization_case_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
    fn organization_with_alliance<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_world_state::AllianceState>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.organization_with_alliance_data(id, visit)
    }
    fn organization_visit_contracts(
        &self,
        visit: impl FnMut(
            &oasis7_world_state::EconomicContractState,
        ) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.organization_visit_contracts_data(visit)
    }
}

impl oasis7_world_state::world_rules::IndustrialProductReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn industrial_with_product<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_wasm_abi::ProductProfileV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.industrial_with_product_data(id, visit)
    }
}

impl oasis7_world_state::world_rules::IndustrialScratchReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn industrial_with_lot<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::IndustrialLotV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialLots,id)?{None=>visit(None),Some(row)=>fact_decode::with_industrial_lot_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
    fn industrial_with_lot_authority<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::IndustrialLotAuthorityV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::ImportedLotAuthorities,id)?{None=>visit(None),Some(row)=>fact_decode::with_industrial_lot_authority_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
    fn industrial_with_receipt<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::IndustrialReceiptV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialReceipts,id)?{None=>visit(None),Some(row)=>fact_decode::with_industrial_receipt_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
    fn industrial_with_disposition<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::DispositionPlanV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialDispositions,id)?{None=>visit(None),Some(row)=>fact_decode::with_industrial_disposition_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
    fn industrial_sink_present(&self, key: &str) -> Result<bool, Self::Error> {
        Ok(self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialSinkKeys,key)?.is_some())
    }
}

impl oasis7_world_state::world_rules::IndustrialStartReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn start_with_root<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::RootOperationV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialRoots,id)?{None=>visit(None),Some(row)=>fact_decode::with_industrial_root_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
    fn start_visit_jobs(
        &self,
        mut visit: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.visit_recipe_jobs(|_, job| visit(job))
    }
    fn start_with_recipe<T>(
        &self,
        id: &str,
        visit: impl FnOnce(Option<&oasis7_wasm_abi::RecipeProfileV1>) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.industrial_with_recipe_data(id, visit)
    }
    fn start_with_progress<T>(
        &self,
        visit: impl FnOnce(&IndustryProgressState) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        self.with_industry_progress(visit)
    }
}

/// Private fee continuation for values created by the fixed raw fact kernels.
/// The returned DTO must be declared before this field so its heap dies first.
pub(in crate::world) struct CurrentProjectionFactsFeesV1 {
    _fees: Vec<MemoryReservation>,
    _slots: MemoryReservation,
    _header: MemoryReservation,
}
impl CurrentProjectionReadFactsV1<'_, '_> {
    fn into_fees(self) -> CurrentProjectionFactsFeesV1 {
        CurrentProjectionFactsFeesV1 {
            _fees: self.fees.into_inner(),
            _slots: self.slots.into_inner(),
            _header: self._fee,
        }
    }
}
/// An original resolved-owner observation with its real allocation fees. It
/// grants no audience, action, source or commit permission.
pub(in crate::world) struct PaidCurrentProjectionOwnerDataV1 {
    owner: Option<oasis7_world_state::ResolvedAgentOwnerV1>,
    _fees: CurrentProjectionFactsFeesV1,
}
impl PaidCurrentProjectionOwnerDataV1 {
    pub(in crate::world) fn owner(&self) -> Option<&oasis7_world_state::ResolvedAgentOwnerV1> {
        self.owner.as_ref()
    }
}
pub(in crate::world) fn resolve_projection_owner_data_v1(
    state: &OriginalStateColumnsDataV1,
    electricity: &OriginalProjectionRowsDataV1,
    metadata:&crate::world::incremental_normal_business_commitment::OriginalProjectionReadMetadataV1<'_>,
    agent: &str,
    origin: &dyn crate::world::incremental_normal_projection_commitment::facts::ProjectionReadSourceV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidCurrentProjectionOwnerDataV1, BodyMutationError> {
    let facts = CurrentProjectionReadFactsV1::for_projection_data_v1(
        state,
        electricity,
        metadata,
        origin,
        common,
    )?;
    facts.before_owned(size_of::<PaidCurrentProjectionOwnerDataV1>())?;
    let owner =
        oasis7_world_state::world_rules::resolve_industrial_owner_readonly_v1(&facts, agent)?;
    facts.validate()?;
    Ok(PaidCurrentProjectionOwnerDataV1 {
        owner,
        _fees: facts.into_fees(),
    })
}

#[path = "incremental_normal_projection_commitment_aux_facts.rs"]
mod aux_facts;

impl oasis7_world_state::world_rules::RecipeStartedReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn started_pending(&self, id: u64) -> Result<bool, Self::Error> {
        Ok(self.point("pending_recipe_jobs", &id)?.is_some())
    }
    fn started_settled(&self, id: u64) -> Result<bool, Self::Error> {
        Ok(self.point("settled_recipe_job_ids", &id)?.is_some())
    }
    fn started_visit_jobs(
        &self,
        mut visit: impl FnMut(&RecipeJobState) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.visit_recipe_jobs(|_, job| visit(job))
    }
    fn started_material(&self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        self.quote_with_ledger_data(ledger, |value| {
            if *ledger == MaterialLedgerId::World && value.is_none_or(|value| value.is_empty()) {
                return match self.point("materials", kind)? {
                    None => Ok(0),
                    Some(row) => {
                        fact_decode::with_i64_data_v1(self.raw(row)?, &self.common, |v| Ok(*v))
                    }
                };
            }
            self.work(
                1,
                (kind.len() + 1)
                    .saturating_mul(value.map_or(1, |v| (v.len() + 1).ilog2() as usize * 2 + 4)),
            )?;
            Ok(value.and_then(|v| v.get(kind)).copied().unwrap_or(0))
        })
    }
    fn started_with_path<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::LogisticsPathAuthorityV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.point("completed_logistics_paths", id)? {
            None => visit(None),
            Some(row) => {
                fact_decode::with_recipe_path_data_v1(self.raw(row)?, &self.common, |value| {
                    visit(Some(value))
                })
            }
        }
    }
}
impl CurrentProjectionReadFactsV1<'_, '_> {
    fn retained_root_data(
        &self,
    ) -> Result<
        ReservedValue<Option<oasis7_world_state::FormalRetainedHistoryRootV1>>,
        BodyMutationError,
    > {
        self.check()?;
        let raw = {
            let mut common = self.common.borrow_mut();
            self.columns
                .scalar_original_json_data_v1("retained_history_root", &mut common)?
        };
        let value = fact_decode::owned_retained_root_data_v1(raw, &self.common)?;
        self.check()?;
        Ok(value)
    }
}

impl CurrentProjectionReadFactsV1<'_, '_> {
    /// The electricity sidecar has exactly one real typed-key row per Agent,
    /// including Agents whose actual Electricity is zero. Visit keys only.
    pub(super) fn visit_agent_ids_data_v1(
        &self,
        mut visit: impl FnMut(&str) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.check()?;
        let mut rows = {
            let mut common = self.common.borrow_mut();
            self.agent_electricity.iter_data_v1(&mut common)?
        };
        loop {
            let row = {
                let mut common = self.common.borrow_mut();
                rows.next_data_v1(&mut common)?
            };
            let Some(row) = row else { break };
            let id = row
                .original_key_value()
                .as_str()
                .ok_or_else(|| invalid("Agent electricity original key type"))?;
            self.work(1, id.len())?;
            if visit(id)?.is_break() {
                break;
            }
        }
        self.check()?;
        Ok(())
    }
}

#[path = "incremental_normal_projection_commitment_task_reduce.rs"]
pub(super) mod task_reduce;

impl oasis7_world_state::world_rules::IndustrialStartEnvelopeReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn start_with_request<T>(
        &self,
        key: &str,
        visit: impl FnOnce(
            Option<&oasis7_world_state::world_rules::WorldRuleReceiptV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::Requests,key)?{None=>visit(None),Some(row)=>fact_decode::with_world_rule_receipt_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
}

impl oasis7_world_state::world_rules::IndustrialReceiptReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    type Error = BodyMutationError;
    fn receipt_work(&self, steps: u64, bytes: usize, heap: usize) -> Result<(), Self::Error> {
        self.work(steps, bytes)?;
        if heap == 0 {
            Ok(())
        } else {
            self.before_owned(heap)
        }
    }
    fn receipt_count(&self) -> Result<usize, Self::Error> {
        self.work(1, 0)?;
        Ok(self.rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialReceipts)?.map_or(0,|rows|rows.len()))
    }
    fn receipt_with_root<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::RootOperationV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        oasis7_world_state::world_rules::IndustrialStartReadonlyFactsV1::start_with_root(
            self, id, visit,
        )
    }
    fn receipt_with_existing<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::IndustrialReceiptV1>,
        ) -> Result<T, Self::Error>,
    ) -> Result<T, Self::Error> {
        oasis7_world_state::world_rules::IndustrialScratchReadonlyFactsV1::industrial_with_receipt(
            self, id, visit,
        )
    }
    fn receipt_visit(
        &self,
        mut visit: impl FnMut(
            &oasis7_agent_api::world_action::IndustrialReceiptV1,
        ) -> Result<ControlFlow<()>, Self::Error>,
    ) -> Result<(), Self::Error> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialReceipts,|row|fact_decode::with_industrial_receipt_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
}

#[derive(Debug)]
struct ProjectionAuthorizationCause(
    crate::world::incremental_authorization_root_current::SourceError,
);
impl std::fmt::Display for ProjectionAuthorizationCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for ProjectionAuthorizationCause {}

#[path = "incremental_normal_projection_commitment_aux_cognition.rs"]
mod aux_cognition;
pub(super) use aux_cognition::{
    CognitionProjectionFeesV1, prepare_current_cognition_projection_data_v1,
};

pub(super) use aux_cognition::prepare_current_native_authorizations_data_v1;

pub(super) use aux_cognition::feedback::{
    CurrentFeedbackHistoryFeesV1, prepare_current_feedback_history_data_v1,
};

#[path = "incremental_normal_projection_commitment_owner_chat.rs"]
mod owner_chat;
pub(in crate::world) use owner_chat::{
    PaidCurrentOwnerKeyMatchDataV1, prepare_current_owner_chat_projection_data_v1,
    prepare_current_owner_key_match_data_v1,
};

impl oasis7_world_state::world_rules::SubstitutionReadonlyFactsV1
    for CurrentProjectionReadFactsV1<'_, '_>
{
    fn substitution_rules_present(&self) -> Result<bool, BodyMutationError> {
        self.check()?;
        Ok(self
            .columns
            .world_rule_rows_data_v1(&mut self.common.borrow_mut())?
            .is_some())
    }
    fn substitution_with_authority<T>(
        &self,
        id: &str,
        visit: impl FnOnce(
            Option<&oasis7_agent_api::world_action::SubstitutionExecutionAuthorityV1>,
        ) -> Result<T, BodyMutationError>,
    ) -> Result<T, BodyMutationError> {
        match self.rule_point(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::SubstitutionAuthorities,id)?{None=>visit(None),Some(row)=>fact_decode::with_substitution_authority_data_v1(self.raw(row)?,&self.common,|value|visit(Some(value)))}
    }
    fn substitution_visit_authorities(
        &self,
        mut visit: impl FnMut(
            &oasis7_agent_api::world_action::SubstitutionExecutionAuthorityV1,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::SubstitutionAuthorities,|row|fact_decode::with_substitution_authority_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
    fn substitution_visit_roots(
        &self,
        mut visit: impl FnMut(
            &oasis7_agent_api::world_action::RootOperationV1,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialRoots,|row|fact_decode::with_industrial_root_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
    fn substitution_visit_lots(
        &self,
        mut visit: impl FnMut(
            &oasis7_agent_api::world_action::IndustrialLotV1,
        ) -> Result<ControlFlow<()>, BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        self.visit_rule_rows(oasis7_world_state::incremental_world_rule_projection_delta::WorldRuleLogicalColumnV1::IndustrialLots,|row|fact_decode::with_industrial_lot_data_v1(self.raw(row)?,&self.common,|value|visit(value)))
    }
}

pub(super) use aux_cognition::prepare_current_builtin_recipe_quote_data_v1;

#[path = "incremental_normal_projection_commitment_visible_rules.rs"]
pub(super) mod visible_rules;

#[path = "incremental_normal_projection_commitment_query.rs"]
pub(super) mod query;

pub(in crate::world) use aux_cognition::status::{
    CurrentWorldRuleStatusFeesV1, prepare_current_world_rule_status_data_v1,
};
