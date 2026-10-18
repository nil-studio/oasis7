//! Event-bound sparse projection for node-points settlement publication.

use serde::ser::SerializeStruct;
use std::collections::{BTreeMap, BTreeSet};

use super::super::util::hash_json;
use super::super::{
    DomainEvent, MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD, MainTokenAccountBalance,
    MainTokenNodePointsBridgeEpochRecord, MaterialLedgerId, NodeAssetBalance,
    SystemOrderPoolBudget, WorldError, WorldState,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedNodePointsSettlement {
    event: DomainEvent,
    node_balances: BTreeMap<String, NodeAssetBalance>,
    reward_mint_records: Vec<super::super::NodeRewardMintRecord>,
    budget: Option<(u64, SystemOrderPoolBudget)>,
    treasury_balances: BTreeMap<String, u64>,
    account_balances: BTreeMap<String, MainTokenAccountBalance>,
    supply: super::super::MainTokenSupplyState,
    bridge_record: (u64, MainTokenNodePointsBridgeEpochRecord),
    world_materials: BTreeMap<String, i64>,
}

impl PreparedNodePointsSettlement {
    pub(crate) fn prepare(state: &WorldState, event: &DomainEvent) -> Result<Self, WorldError> {
        let DomainEvent::NodePointsSettlementApplied {
            report,
            signer_node_id,
            settlement_hash,
            minted_records,
            main_token_bridge_total_amount: total_amount,
            main_token_bridge_distributions: distributions,
        } = event
        else {
            return Err(invalid(
                "node points settlement preparation requires a settlement event",
            ));
        };
        if signer_node_id.trim().is_empty() {
            return Err(invalid("settlement signer_node_id cannot be empty"));
        }
        let expected_hash = hash_json(report)?;
        if &expected_hash != settlement_hash {
            return Err(invalid(format!(
                "settlement_hash mismatch: expected={expected_hash} actual={settlement_hash}"
            )));
        }
        let points_per_credit = state.reward_asset_config.points_per_credit;
        if points_per_credit == 0 {
            return Err(invalid("points_per_credit must be positive"));
        }
        if !state.node_identity_bindings.contains_key(signer_node_id) {
            return Err(invalid(format!(
                "node identity is not bound: {signer_node_id}"
            )));
        }

        let mut settlement_points = BTreeMap::new();
        for settlement in &report.settlements {
            if settlement.node_id.trim().is_empty() {
                return Err(invalid("report settlement contains empty node_id"));
            }
            if settlement_points
                .insert(settlement.node_id.clone(), settlement.awarded_points)
                .is_some()
            {
                return Err(invalid(format!(
                    "duplicate settlement node in report: {}",
                    settlement.node_id
                )));
            }
            if !state
                .node_identity_bindings
                .contains_key(&settlement.node_id)
            {
                return Err(invalid(format!(
                    "node identity is not bound: {}",
                    settlement.node_id
                )));
            }
        }

        let mut budget = state
            .system_order_pool_budgets
            .get(&report.epoch_index)
            .cloned();
        if let Some(item) = budget.as_mut() {
            super::super::state::support::ensure_system_order_budget_caps_for_epoch(report, item);
        }
        let mut seen_nodes = BTreeSet::new();
        for record in minted_records {
            if record.epoch_index != report.epoch_index {
                return Err(invalid(format!(
                    "mint record epoch mismatch: report={} record={}",
                    report.epoch_index, record.epoch_index
                )));
            }
            if &record.signer_node_id != signer_node_id {
                return Err(invalid(format!(
                    "mint record signer mismatch: event={} record={}",
                    signer_node_id, record.signer_node_id
                )));
            }
            if &record.settlement_hash != settlement_hash {
                return Err(invalid(format!(
                    "mint record settlement_hash mismatch for node {}",
                    record.node_id
                )));
            }
            let Some(awarded_points) = settlement_points.get(&record.node_id) else {
                return Err(invalid(format!(
                    "mint record node is missing in report settlements: {}",
                    record.node_id
                )));
            };
            if record.source_awarded_points != *awarded_points {
                return Err(invalid(format!(
                    "mint record awarded points mismatch for node {}: report={} record={}",
                    record.node_id, awarded_points, record.source_awarded_points
                )));
            }
            if record.minted_power_credits == 0 {
                return Err(invalid(format!(
                    "mint record has zero minted_power_credits for node {}",
                    record.node_id
                )));
            }
            let max_minted = record.source_awarded_points / points_per_credit;
            if record.minted_power_credits > max_minted {
                return Err(invalid(format!(
                    "minted credits exceed settlement cap for node {}: minted={} cap={max_minted}",
                    record.node_id, record.minted_power_credits
                )));
            }
            if !seen_nodes.insert(record.node_id.clone()) {
                return Err(invalid(format!(
                    "duplicate mint record node in one action: {}",
                    record.node_id
                )));
            }
            if state.reward_mint_records.iter().any(|existing| {
                existing.epoch_index == record.epoch_index && existing.node_id == record.node_id
            }) {
                return Err(invalid(format!(
                    "mint record already exists for epoch={} node={}",
                    record.epoch_index, record.node_id
                )));
            }
            super::super::state::support::verify_reward_mint_record_signature_with_state(
                state, record,
            )
            .map_err(|reason| {
                invalid(format!(
                    "mint record signature invalid (epoch={} node={}): {}",
                    record.epoch_index, record.node_id, reason
                ))
            })?;
            if let Some(item) = budget.as_mut() {
                let cap = item
                    .node_credit_caps
                    .get(&record.node_id)
                    .copied()
                    .unwrap_or(0);
                let allocated = item
                    .node_credit_allocated
                    .get(&record.node_id)
                    .copied()
                    .unwrap_or(0);
                let remaining = cap.saturating_sub(allocated);
                if record.minted_power_credits > remaining {
                    return Err(invalid(format!(
                        "minted credits exceed node budget cap for {}: minted={} remaining={remaining}",
                        record.node_id, record.minted_power_credits
                    )));
                }
                if record.minted_power_credits > item.remaining_credit_budget {
                    return Err(invalid(format!(
                        "minted credits exceed remaining system order budget: minted={} remaining={}",
                        record.minted_power_credits, item.remaining_credit_budget
                    )));
                }
                item.remaining_credit_budget = item
                    .remaining_credit_budget
                    .saturating_sub(record.minted_power_credits);
                item.node_credit_allocated
                    .entry(record.node_id.clone())
                    .and_modify(|value| *value = value.saturating_add(record.minted_power_credits))
                    .or_insert(record.minted_power_credits);
            }
        }

        let mut node_balances = BTreeMap::new();
        for record in minted_records {
            let balance = node_balances
                .entry(record.node_id.clone())
                .or_insert_with(|| {
                    state
                        .node_asset_balances
                        .get(&record.node_id)
                        .cloned()
                        .unwrap_or_else(|| NodeAssetBalance {
                            node_id: record.node_id.clone(),
                            ..NodeAssetBalance::default()
                        })
                });
            balance.power_credit_balance = balance
                .power_credit_balance
                .saturating_add(record.minted_power_credits);
            balance.total_minted_credits = balance
                .total_minted_credits
                .saturating_add(record.minted_power_credits);
        }
        let mut reward_mint_records = state.reward_mint_records.clone();
        reward_mint_records.extend_from_slice(minted_records);

        if state
            .main_token_node_points_bridge_records
            .contains_key(&report.epoch_index)
        {
            return Err(invalid(format!(
                "main token node points bridge already processed for epoch={}",
                report.epoch_index
            )));
        }
        if settlement_hash.trim().is_empty() {
            return Err(invalid("main token bridge settlement_hash cannot be empty"));
        }
        let expected_budget = state
            .main_token_epoch_issuance_records
            .get(&report.epoch_index)
            .map(|record| record.node_service_reward_amount)
            .unwrap_or(0);
        if *total_amount > expected_budget {
            return Err(invalid(format!(
                "main token bridge total exceeds epoch node_service budget: epoch={} total={} budget={expected_budget}",
                report.epoch_index, total_amount
            )));
        }
        let mut sum = 0_u64;
        let mut distributed_nodes = BTreeSet::new();
        for item in distributions {
            if item.node_id.trim().is_empty() {
                return Err(invalid(
                    "main token bridge distribution node_id cannot be empty",
                ));
            }
            if item.account_id.trim().is_empty() {
                return Err(invalid(format!(
                    "main token bridge distribution account_id cannot be empty: node={}",
                    item.node_id
                )));
            }
            if item.amount == 0 {
                return Err(invalid(format!(
                    "main token bridge distribution amount must be > 0: node={}",
                    item.node_id
                )));
            }
            if !distributed_nodes.insert(item.node_id.clone()) {
                return Err(invalid(format!(
                    "duplicate main token bridge distribution for node={}",
                    item.node_id
                )));
            }
            sum = sum.checked_add(item.amount).ok_or_else(|| {
                invalid(format!(
                    "main token bridge distribution sum overflow: epoch={}",
                    report.epoch_index
                ))
            })?;
        }
        if sum != *total_amount {
            return Err(invalid(format!(
                "main token bridge sum mismatch: epoch={} total={} distributions_sum={sum}",
                report.epoch_index, total_amount
            )));
        }
        let treasury = state
            .main_token_treasury_balances
            .get(MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD)
            .copied()
            .unwrap_or(0);
        if treasury < *total_amount {
            return Err(invalid(format!(
                "main token bridge treasury insufficient: epoch={} balance={treasury} total={}",
                report.epoch_index, total_amount
            )));
        }
        let mut treasury_balances = BTreeMap::new();
        let mut account_balances = BTreeMap::new();
        let mut supply = state.main_token_supply.clone();
        if *total_amount > 0 {
            treasury_balances.insert(
                MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD.to_string(),
                treasury - total_amount,
            );
            for item in distributions {
                let account = account_balances
                    .entry(item.account_id.clone())
                    .or_insert_with(|| {
                        state
                            .main_token_balances
                            .get(&item.account_id)
                            .cloned()
                            .unwrap_or_else(|| MainTokenAccountBalance {
                                account_id: item.account_id.clone(),
                                ..MainTokenAccountBalance::default()
                            })
                    });
                account.liquid_balance = account
                    .liquid_balance
                    .checked_add(item.amount)
                    .ok_or_else(|| {
                        invalid(format!(
                            "main token bridge account overflow: account={} current={} amount={}",
                            item.account_id, account.liquid_balance, item.amount
                        ))
                    })?;
            }
            supply.circulating_supply = supply
                .circulating_supply
                .checked_add(*total_amount)
                .ok_or_else(|| {
                    invalid(format!(
                        "main token bridge circulating overflow: current={} amount={}",
                        supply.circulating_supply, total_amount
                    ))
                })?;
            if supply.circulating_supply > supply.total_supply {
                return Err(invalid(format!(
                    "main token bridge circulating exceeds total: circulating={} total={}",
                    supply.circulating_supply, supply.total_supply
                )));
            }
        }
        let bridge_record = (
            report.epoch_index,
            MainTokenNodePointsBridgeEpochRecord {
                epoch_index: report.epoch_index,
                settlement_hash: settlement_hash.clone(),
                total_amount: *total_amount,
                distributions: distributions.clone(),
            },
        );
        let world_materials = state
            .material_ledgers
            .get(&MaterialLedgerId::world())
            .filter(|ledger| !ledger.is_empty())
            .unwrap_or(&state.materials)
            .clone();
        Ok(Self {
            event: event.clone(),
            node_balances,
            reward_mint_records,
            budget: budget.map(|item| (report.epoch_index, item)),
            treasury_balances,
            account_balances,
            supply,
            bridge_record,
            world_materials,
        })
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.node_asset_balances.extend(self.node_balances);
        state.reward_mint_records = self.reward_mint_records;
        if let Some((epoch, budget)) = self.budget {
            state.system_order_pool_budgets.insert(epoch, budget);
        }
        state
            .main_token_treasury_balances
            .extend(self.treasury_balances);
        state.main_token_balances.extend(self.account_balances);
        state.main_token_supply = self.supply;
        state
            .main_token_node_points_bridge_records
            .insert(self.bridge_record.0, self.bridge_record.1);
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.world_materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.world_materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_main_token_accounts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("main_token_supply", &self.supply)?;
        out.serialize_field(
            "main_token_balances",
            &SparseMapProjection {
                base: &state.main_token_balances,
                updates: &self.account_balances,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_treasury<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_treasury_balances",
            &SparseMapProjection {
                base: &state.main_token_treasury_balances,
                updates: &self.treasury_balances,
                deletion: None,
            },
        )?;
        Ok(())
    }

    pub(crate) fn serialize_bridge<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let records = BTreeMap::from([(self.bridge_record.0, self.bridge_record.1.clone())]);
        out.serialize_field(
            "main_token_node_points_bridge_records",
            &SparseMapProjection {
                base: &state.main_token_node_points_bridge_records,
                updates: &records,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_node_balances<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "node_asset_balances",
            &SparseMapProjection {
                base: &state.node_asset_balances,
                updates: &self.node_balances,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_reward_mints<S: SerializeStruct>(
        &self,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("reward_mint_records", &self.reward_mint_records)
    }

    pub(crate) fn serialize_budgets<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let budgets = self
            .budget
            .as_ref()
            .map(|(epoch, value)| BTreeMap::from([(*epoch, value.clone())]))
            .unwrap_or_default();
        out.serialize_field(
            "system_order_pool_budgets",
            &SparseMapProjection {
                base: &state.system_order_pool_budgets,
                updates: &budgets,
                deletion: None,
            },
        )
    }
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
