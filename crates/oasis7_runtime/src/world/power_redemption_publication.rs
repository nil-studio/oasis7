//! Event-bound sparse projection for raw power redemption outcomes.

use oasis7_agent_api::world_action::ResourceKind;
use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::agent_cell::AgentCell;
use super::super::{
    DomainEvent, MaterialLedgerId, NodeAssetBalance, ProtocolPowerReserve, WorldError, WorldState,
    WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedPowerRedemptionEvent {
    event: DomainEvent,
    agents: BTreeMap<String, AgentCell>,
    node_balance: Option<(String, NodeAssetBalance)>,
    reserve: Option<ProtocolPowerReserve>,
    nonce: Option<(String, u64)>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedPowerRedemptionEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut prepared = Self {
            event: event.clone(),
            agents: BTreeMap::new(),
            node_balance: None,
            reserve: None,
            nonce: None,
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        };
        match event {
            DomainEvent::PowerRedeemed {
                node_id,
                target_agent_id,
                burned_credits,
                granted_power_units,
                reserve_remaining,
                nonce,
            } => {
                prepared.prepare_redeemed(
                    state,
                    node_id,
                    target_agent_id,
                    *burned_credits,
                    *granted_power_units,
                    *reserve_remaining,
                    *nonce,
                    now,
                )?;
            }
            DomainEvent::PowerRedeemRejected {
                node_id,
                target_agent_id,
                ..
            } => prepared.prepare_rejected(state, node_id, target_agent_id, now),
            _ => {
                return Err(invalid(
                    "power preparation requires a redemption outcome event",
                ));
            }
        }
        Ok(prepared)
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_redeemed(
        &mut self,
        state: &WorldState,
        node_id: &str,
        target_agent_id: &str,
        burned_credits: u64,
        granted: i64,
        reserve_remaining: i64,
        nonce: u64,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if burned_credits == 0 {
            return Err(invalid("burned_credits must be > 0"));
        }
        if granted <= 0 {
            return Err(invalid(format!(
                "granted_power_units must be > 0, got {granted}"
            )));
        }
        let min = state.reward_asset_config.min_redeem_power_unit;
        if min <= 0 {
            return Err(invalid("min_redeem_power_unit must be positive"));
        }
        if granted < min {
            return Err(invalid(format!(
                "granted_power_units below minimum: granted={granted} min={min}"
            )));
        }
        if nonce == 0 {
            return Err(invalid("nonce must be > 0"));
        }
        if let Some(last) = state.node_redeem_nonces.get(node_id)
            && nonce <= *last
        {
            return Err(invalid(format!(
                "nonce replay detected: node_id={node_id} nonce={nonce} last_nonce={last}"
            )));
        }
        let mut balance = state
            .node_asset_balances
            .get(node_id)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "power redeem burn failed: node balance not found: {node_id}"
                ))
            })?;
        if balance.power_credit_balance < burned_credits {
            return Err(invalid(format!(
                "power redeem burn failed: insufficient power credits: balance={} burn={burned_credits}",
                balance.power_credit_balance
            )));
        }
        let next_burned = balance.total_burned_credits.checked_add(burned_credits).ok_or_else(|| invalid(format!("power redeem burn failed: total_burned_credits overflow: current={} burn={burned_credits}", balance.total_burned_credits)))?;
        if state.protocol_power_reserve.available_power_units < granted {
            return Err(invalid(format!(
                "insufficient protocol power reserve: available={} requested={granted}",
                state.protocol_power_reserve.available_power_units
            )));
        }
        let next_available = state.protocol_power_reserve.available_power_units - granted;
        if next_available != reserve_remaining {
            return Err(invalid(format!(
                "reserve remaining mismatch: computed={next_available} event={reserve_remaining}"
            )));
        }
        let cap = state.reward_asset_config.max_redeem_power_per_epoch;
        if cap <= 0 {
            return Err(invalid("max_redeem_power_per_epoch must be positive"));
        }
        let next_redeemed = state
            .protocol_power_reserve
            .redeemed_power_units
            .checked_add(granted)
            .ok_or_else(|| invalid("redeemed_power_units overflow"))?;
        if next_redeemed > cap {
            return Err(invalid(format!(
                "epoch redeem cap exceeded: next={next_redeemed} cap={cap}"
            )));
        }
        let mut target = state.agents.get(target_agent_id).cloned().ok_or_else(|| {
            WorldError::AgentNotFound {
                agent_id: target_agent_id.to_string(),
            }
        })?;
        let current = target.state.resources.get(ResourceKind::Electricity);
        let next_electricity = current.checked_add(granted).ok_or_else(|| {
            invalid(format!(
                "power redeem add electricity failed: overflow current={current} delta={granted}"
            ))
        })?;

        balance.power_credit_balance -= burned_credits;
        balance.total_burned_credits = next_burned;
        let mut reserve = state.protocol_power_reserve.clone();
        reserve.available_power_units = next_available;
        reserve.redeemed_power_units = next_redeemed;
        if next_electricity == 0 {
            target
                .state
                .resources
                .amounts
                .remove(&ResourceKind::Electricity);
        } else {
            target
                .state
                .resources
                .amounts
                .insert(ResourceKind::Electricity, next_electricity);
        }
        target.last_active = now;
        self.agents.insert(target_agent_id.to_string(), target);
        if node_id != target_agent_id
            && let Some(mut node) = state.agents.get(node_id).cloned()
        {
            node.last_active = now;
            self.agents.insert(node_id.to_string(), node);
        }
        self.node_balance = Some((node_id.to_string(), balance));
        self.reserve = Some(reserve);
        self.nonce = Some((node_id.to_string(), nonce));
        Ok(())
    }

    fn prepare_rejected(
        &mut self,
        state: &WorldState,
        node_id: &str,
        target_id: &str,
        now: WorldTime,
    ) {
        if let Some(mut node) = state.agents.get(node_id).cloned() {
            node.last_active = now;
            self.agents.insert(node_id.to_string(), node);
        }
        if let Some(mut target) = self
            .agents
            .get(target_id)
            .cloned()
            .or_else(|| state.agents.get(target_id).cloned())
        {
            target.last_active = now;
            self.agents.insert(target_id.to_string(), target);
        }
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agents = self.agents.clone();
        let target = match &self.event {
            DomainEvent::PowerRedeemed {
                target_agent_id, ..
            }
            | DomainEvent::PowerRedeemRejected {
                target_agent_id, ..
            } => target_agent_id,
            _ => unreachable!(),
        };
        if let Some(cell) = agents.get_mut(target) {
            cell.mailbox.push_back(self.event.clone());
        }
        agents
    }
    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agents.extend(self.agents);
        if let Some((id, value)) = self.node_balance {
            state.node_asset_balances.insert(id, value);
        }
        if let Some(value) = self.reserve {
            state.protocol_power_reserve = value;
        }
        if let Some((id, value)) = self.nonce {
            state.node_redeem_nonces.insert(id, value);
        }
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
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &self.routed_agents(),
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
    pub(crate) fn serialize_node_balances<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self
            .node_balance
            .as_ref()
            .map(|(id, value)| BTreeMap::from([(id.clone(), value.clone())]))
            .unwrap_or_default();
        out.serialize_field(
            "node_asset_balances",
            &SparseMapProjection {
                base: &state.node_asset_balances,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_reserve<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "protocol_power_reserve",
            self.reserve
                .as_ref()
                .unwrap_or(&state.protocol_power_reserve),
        )
    }
    pub(crate) fn serialize_nonces<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self
            .nonce
            .as_ref()
            .map(|(id, value)| BTreeMap::from([(id.clone(), *value)]))
            .unwrap_or_default();
        out.serialize_field(
            "node_redeem_nonces",
            &SparseMapProjection {
                base: &state.node_redeem_nonces,
                updates: &updates,
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
