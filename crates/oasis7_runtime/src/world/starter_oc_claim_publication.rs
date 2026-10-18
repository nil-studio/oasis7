//! Full-event-bound sparse projection for raw starter OC claims.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::{
    AgentCell, DomainEvent, MainTokenAccountBalance, MainTokenSupplyState, MaterialLedgerId,
    WorldError, WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;
use crate::gameplay_state::StarterOcClaimState;

#[derive(Debug)]
pub(crate) struct PreparedStarterOcClaimed {
    event: DomainEvent,
    agent: AgentCell,
    account: MainTokenAccountBalance,
    supply: MainTokenSupplyState,
    treasury: BTreeMap<String, u64>,
    claim: StarterOcClaimState,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedStarterOcClaimed {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::StarterOcClaimed {
            agent_id,
            player_id,
            public_key,
            amount,
            claimed_at,
            source_treasury_bucket_id,
        } = event
        else {
            return Err(invalid("starter OC preparation requires StarterOcClaimed"));
        };
        let mut agent =
            state
                .agents
                .get(agent_id)
                .cloned()
                .ok_or_else(|| WorldError::AgentNotFound {
                    agent_id: agent_id.clone(),
                })?;
        if state.starter_oc_claims.contains_key(agent_id) {
            return Err(invalid(format!(
                "starter OC already claimed for agent: {agent_id}"
            )));
        }
        if player_id.trim().is_empty() {
            return Err(invalid("starter OC player_id cannot be empty"));
        }
        if *amount == 0 {
            return Err(invalid("starter OC amount must be > 0"));
        }
        let mut supply = state.main_token_supply.clone();
        let mut treasury = BTreeMap::new();
        if let Some(bucket_id) = source_treasury_bucket_id {
            let current = state
                .main_token_treasury_balances
                .get(bucket_id)
                .copied()
                .unwrap_or(0);
            if current < *amount {
                return Err(invalid(format!(
                    "starter OC treasury insufficient: bucket={bucket_id} balance={current} amount={amount}"
                )));
            }
            treasury.insert(bucket_id.clone(), current - *amount);
            supply.circulating_supply =
                supply
                    .circulating_supply
                    .checked_add(*amount)
                    .ok_or_else(|| {
                        invalid(format!(
                            "starter OC circulating overflow: current={} amount={amount}",
                            supply.circulating_supply
                        ))
                    })?;
        } else {
            supply.total_supply = supply.total_supply.checked_add(*amount).ok_or_else(|| {
                invalid(format!(
                    "starter OC total supply overflow: current={} amount={amount}",
                    supply.total_supply
                ))
            })?;
            supply.total_issued = supply.total_issued.checked_add(*amount).ok_or_else(|| {
                invalid(format!(
                    "starter OC total issued overflow: current={} amount={amount}",
                    supply.total_issued
                ))
            })?;
            supply.circulating_supply =
                supply
                    .circulating_supply
                    .checked_add(*amount)
                    .ok_or_else(|| {
                        invalid(format!(
                            "starter OC circulating overflow: current={} amount={amount}",
                            supply.circulating_supply
                        ))
                    })?;
        }
        if supply.circulating_supply > supply.total_supply {
            return Err(invalid(format!(
                "starter OC circulating exceeds total: circulating={} total={}",
                supply.circulating_supply, supply.total_supply
            )));
        }
        let mut account = state
            .main_token_balances
            .get(agent_id)
            .cloned()
            .unwrap_or_else(|| MainTokenAccountBalance {
                account_id: agent_id.clone(),
                ..MainTokenAccountBalance::default()
            });
        if account.account_id != *agent_id {
            return Err(invalid(format!(
                "starter OC account key mismatch: key={agent_id} value={}",
                account.account_id
            )));
        }
        account.liquid_balance = account.liquid_balance.checked_add(*amount).ok_or_else(|| {
            invalid(format!(
                "starter OC liquid balance overflow: account={agent_id} current={} amount={amount}",
                account.liquid_balance
            ))
        })?;
        agent.last_active = now;
        Ok(Self {
            event: event.clone(),
            agent,
            account,
            supply,
            treasury,
            claim: StarterOcClaimState {
                agent_id: agent_id.clone(),
                player_id: player_id.clone(),
                public_key: public_key.clone(),
                amount: *amount,
                claimed_at: *claimed_at,
                source_treasury_bucket_id: source_treasury_bucket_id.clone(),
            },
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        })
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        let agent_id = self.claim.agent_id.clone();
        state.agents.insert(agent_id.clone(), self.agent);
        state
            .main_token_balances
            .insert(agent_id.clone(), self.account);
        state.main_token_supply = self.supply;
        state.main_token_treasury_balances.extend(self.treasury);
        state.starter_oc_claims.insert(agent_id, self.claim);
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut agent = self.agent.clone();
        agent.mailbox.push_back(self.event.clone());
        let updates = BTreeMap::from([(self.claim.agent_id.clone(), agent)]);
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &updates,
                deletion: None,
            },
        )
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
    pub(crate) fn serialize_supply_balances<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("main_token_supply", &self.supply)?;
        let updates = BTreeMap::from([(self.claim.agent_id.clone(), self.account.clone())]);
        out.serialize_field(
            "main_token_balances",
            &SparseMapProjection {
                base: &state.main_token_balances,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_claims<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = BTreeMap::from([(self.claim.agent_id.clone(), self.claim.clone())]);
        out.serialize_field(
            "starter_oc_claims",
            &SparseMapProjection {
                base: &state.starter_oc_claims,
                updates: &updates,
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
                updates: &self.treasury,
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
