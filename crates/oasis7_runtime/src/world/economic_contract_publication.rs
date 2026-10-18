//! Sparse atomic projection for economic-contract lifecycle events.

use oasis7_agent_api::world_action::ResourceKind;
use serde::ser::SerializeStruct;
use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    AgentCell, DomainEvent, EconomicContractFulfillmentKind, EconomicContractState,
    EconomicContractStatus, MaterialLedgerId, WorldError, WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

const REPUTATION_WINDOW_TICKS: u64 = 20;

#[derive(Debug)]
pub(crate) struct PreparedEconomicContractEvent {
    event: DomainEvent,
    contract_id: String,
    contract: EconomicContractState,
    agents: BTreeMap<String, AgentCell>,
    resources: BTreeMap<ResourceKind, i64>,
    reputation: BTreeMap<String, i64>,
    pair_settled: BTreeMap<String, WorldTime>,
    window_started: BTreeMap<String, WorldTime>,
    window_accumulated: BTreeMap<String, i64>,
    materials: BTreeMap<String, i64>,
}

impl PreparedEconomicContractEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut agents = BTreeMap::new();
        let mut resources = BTreeMap::new();
        let mut reputation = BTreeMap::new();
        let mut pair_settled = BTreeMap::new();
        let mut window_started = BTreeMap::new();
        let mut window_accumulated = BTreeMap::new();
        let (contract_id, contract) = match event {
            DomainEvent::EconomicContractOpened {
                creator_agent_id,
                contract_id,
                counterparty_agent_id,
                fulfillment_kind,
                settlement_kind,
                settlement_amount,
                reputation_stake,
                expires_at,
                description,
            } => {
                require_agent(state, creator_agent_id)?;
                require_agent(state, counterparty_agent_id)?;
                touch(state, &mut agents, creator_agent_id, now, true)?;
                (
                    contract_id.clone(),
                    EconomicContractState {
                        contract_id: contract_id.clone(),
                        creator_agent_id: creator_agent_id.clone(),
                        counterparty_agent_id: counterparty_agent_id.clone(),
                        fulfillment_kind: *fulfillment_kind,
                        settlement_kind: *settlement_kind,
                        settlement_amount: *settlement_amount,
                        reputation_stake: *reputation_stake,
                        expires_at: *expires_at,
                        description: description.clone(),
                        status: EconomicContractStatus::Open,
                        accepted_at: None,
                        settled_at: None,
                        settlement_success: None,
                        transfer_amount: 0,
                        tax_amount: 0,
                        settlement_notes: None,
                    },
                )
            }
            DomainEvent::EconomicContractAccepted {
                accepter_agent_id,
                contract_id,
            } => {
                let mut contract = contract(state, contract_id)?;
                if contract.status != EconomicContractStatus::Open {
                    return Err(invalid(format!(
                        "economic contract status invalid for acceptance: {:?}",
                        contract.status
                    )));
                }
                if contract.counterparty_agent_id != *accepter_agent_id {
                    return Err(invalid(format!(
                        "economic contract accepter mismatch expected={} actual={accepter_agent_id}",
                        contract.counterparty_agent_id
                    )));
                }
                contract.status = EconomicContractStatus::Accepted;
                contract.accepted_at = Some(now);
                touch(state, &mut agents, accepter_agent_id, now, true)?;
                (contract_id.clone(), contract)
            }
            DomainEvent::EconomicContractSettled {
                operator_agent_id,
                contract_id,
                success,
                transfer_amount,
                tax_amount,
                notes,
                creator_reputation_delta,
                counterparty_reputation_delta,
            } => {
                let mut contract = contract(state, contract_id)?;
                if contract.status != EconomicContractStatus::Accepted {
                    return Err(invalid(format!(
                        "economic contract status invalid for settlement: {:?}",
                        contract.status
                    )));
                }
                require_agent(state, operator_agent_id)?;
                reject_service(&contract)?;
                if !success {
                    return Err(invalid("atomic exchange settlement requires success=true"));
                }
                if *transfer_amount <= 0 {
                    return Err(invalid(format!(
                        "economic contract settlement transfer must be > 0, got {transfer_amount}"
                    )));
                }
                if *tax_amount < 0 {
                    return Err(invalid(format!(
                        "economic contract settlement tax must be >= 0, got {tax_amount}"
                    )));
                }
                let debit = transfer_amount.checked_add(*tax_amount).ok_or_else(|| {
                    invalid(format!(
                        "economic contract settlement debit overflow transfer={transfer_amount} tax={tax_amount}"
                    ))
                })?;
                let creator_id = contract.creator_agent_id.clone();
                let counterparty_id = contract.counterparty_agent_id.clone();
                let kind = contract.settlement_kind;
                let creator_current = require_agent(state, &creator_id)?.state.resources.get(kind);
                if creator_current < debit {
                    return Err(invalid(format!(
                        "economic contract settlement debit failed agent={creator_id} kind={kind:?} amount={debit} available={creator_current}"
                    )));
                }
                let creator_after = creator_current - debit;
                let counterparty_next = if creator_id == counterparty_id {
                    creator_after.checked_add(*transfer_amount)
                } else {
                    Some(
                        require_agent(state, &counterparty_id)?
                            .state
                            .resources
                            .get(kind),
                    )
                    .and_then(|value| value.checked_add(*transfer_amount))
                }
                .ok_or_else(|| invalid(format!(
                    "economic contract settlement credit failed agent={counterparty_id} kind={kind:?} amount={transfer_amount} overflow"
                )))?;
                let treasury_current = state.resources.get(&kind).copied().unwrap_or(0);
                let treasury_next = treasury_current.checked_add(*tax_amount).ok_or_else(|| invalid(format!(
                    "economic contract settlement treasury overflow kind={kind:?} current={treasury_current} delta={tax_amount}"
                )))?;
                let creator_score =
                    checked_reputation(state, &creator_id, *creator_reputation_delta, "creator")?;
                if let Some(next) = creator_score {
                    reputation.insert(creator_id.clone(), next);
                }
                let counterparty_score = checked_reputation(
                    state,
                    &counterparty_id,
                    *counterparty_reputation_delta,
                    "counterparty",
                )?;
                if let Some(next) = counterparty_score {
                    reputation.insert(counterparty_id.clone(), next);
                }

                set_resource(
                    state,
                    &mut agents,
                    &creator_id,
                    kind,
                    if creator_id == counterparty_id {
                        counterparty_next
                    } else {
                        creator_after
                    },
                )?;
                if creator_id != counterparty_id {
                    set_resource(
                        state,
                        &mut agents,
                        &counterparty_id,
                        kind,
                        counterparty_next,
                    )?;
                }
                resources.insert(kind, treasury_next);
                pair_settled.insert(
                    WorldState::economic_contract_pair_key(&creator_id, &counterparty_id),
                    now,
                );
                record_window(
                    state,
                    &mut window_started,
                    &mut window_accumulated,
                    &creator_id,
                    *creator_reputation_delta,
                    now,
                );
                record_window(
                    state,
                    &mut window_started,
                    &mut window_accumulated,
                    &counterparty_id,
                    *counterparty_reputation_delta,
                    now,
                );
                contract.status = EconomicContractStatus::Settled;
                contract.settled_at = Some(now);
                contract.settlement_success = Some(*success);
                contract.transfer_amount = *transfer_amount;
                contract.tax_amount = *tax_amount;
                contract.settlement_notes = Some(notes.clone());
                touch(state, &mut agents, operator_agent_id, now, true)?;
                (contract_id.clone(), contract)
            }
            DomainEvent::EconomicContractExpired {
                contract_id,
                creator_agent_id,
                counterparty_agent_id,
                creator_reputation_delta,
                counterparty_reputation_delta,
            } => {
                let mut contract = contract(state, contract_id)?;
                reject_service(&contract)?;
                match contract.status {
                    EconomicContractStatus::Open | EconomicContractStatus::Accepted => {}
                    EconomicContractStatus::Settled | EconomicContractStatus::Expired => {
                        return Err(invalid(format!(
                            "economic contract already finalized before expiry: {contract_id}"
                        )));
                    }
                }
                contract.status = EconomicContractStatus::Expired;
                contract.settled_at = Some(now);
                contract.settlement_success = Some(false);
                contract.transfer_amount = 0;
                contract.tax_amount = 0;
                contract.settlement_notes = Some("auto expired by gameplay lifecycle".into());
                saturating_reputation(
                    state,
                    &mut reputation,
                    creator_agent_id,
                    *creator_reputation_delta,
                );
                saturating_reputation(
                    state,
                    &mut reputation,
                    counterparty_agent_id,
                    *counterparty_reputation_delta,
                );
                touch(state, &mut agents, creator_agent_id, now, false)?;
                touch(state, &mut agents, counterparty_agent_id, now, false)?;
                (contract_id.clone(), contract)
            }
            _ => {
                return Err(invalid(
                    "economic contract preparation requires a supported event",
                ));
            }
        };
        Ok(Self {
            event: event.clone(),
            contract_id,
            contract,
            agents,
            resources,
            reputation,
            pair_settled,
            window_started,
            window_accumulated,
            materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|v| !v.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        })
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state
            .economic_contracts
            .insert(self.contract_id, self.contract);
        state.agents.extend(self.agents);
        for (kind, amount) in self.resources {
            if amount == 0 {
                state.resources.remove(&kind);
            } else {
                state.resources.insert(kind, amount);
            }
        }
        state.reputation_scores.extend(self.reputation);
        state
            .contract_pair_last_success_settled_at
            .extend(self.pair_settled);
        state
            .reputation_reward_window_started_at
            .extend(self.window_started);
        state
            .reputation_reward_window_accumulated
            .extend(self.window_accumulated);
        state.materials = self.materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.materials);
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut updates = self.agents.clone();
        if let Some(actor) = self.event.agent_id()
            && let Some(agent) = updates.get_mut(actor)
        {
            agent.mailbox.push_back(self.event.clone());
        }
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_resources<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "resources",
            &ResourceProjection {
                base: &state.resources,
                updates: &self.resources,
            },
        )
    }
    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_contracts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = BTreeMap::from([(self.contract_id.clone(), self.contract.clone())]);
        out.serialize_field(
            "economic_contracts",
            &SparseMapProjection {
                base: &state.economic_contracts,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_reputation<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "contract_pair_last_success_settled_at",
            &SparseMapProjection {
                base: &state.contract_pair_last_success_settled_at,
                updates: &self.pair_settled,
                deletion: None,
            },
        )?;
        out.serialize_field(
            "reputation_reward_window_started_at",
            &SparseMapProjection {
                base: &state.reputation_reward_window_started_at,
                updates: &self.window_started,
                deletion: None,
            },
        )?;
        out.serialize_field(
            "reputation_reward_window_accumulated",
            &SparseMapProjection {
                base: &state.reputation_reward_window_accumulated,
                updates: &self.window_accumulated,
                deletion: None,
            },
        )?;
        out.serialize_field(
            "reputation_scores",
            &SparseMapProjection {
                base: &state.reputation_scores,
                updates: &self.reputation,
                deletion: None,
            },
        )
    }
}

struct ResourceProjection<'a> {
    base: &'a BTreeMap<ResourceKind, i64>,
    updates: &'a BTreeMap<ResourceKind, i64>,
}
impl serde::Serialize for ResourceProjection<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let keys = self
            .base
            .keys()
            .chain(self.updates.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        let count = keys
            .iter()
            .filter(|key| {
                self.updates
                    .get(key)
                    .or_else(|| self.base.get(key))
                    .is_some_and(|value| *value != 0)
            })
            .count();
        let mut map = serializer.serialize_map(Some(count))?;
        for key in keys {
            if let Some(value) = self.updates.get(&key).or_else(|| self.base.get(&key))
                && *value != 0
            {
                serde::ser::SerializeMap::serialize_entry(&mut map, &key, value)?;
            }
        }
        serde::ser::SerializeMap::end(map)
    }
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
fn contract(state: &WorldState, id: &str) -> Result<EconomicContractState, WorldError> {
    state
        .economic_contracts
        .get(id)
        .cloned()
        .ok_or_else(|| invalid(format!("economic contract not found: {id}")))
}
fn require_agent<'a>(state: &'a WorldState, id: &str) -> Result<&'a AgentCell, WorldError> {
    state
        .agents
        .get(id)
        .ok_or_else(|| WorldError::AgentNotFound {
            agent_id: id.into(),
        })
}
fn touch(
    state: &WorldState,
    updates: &mut BTreeMap<String, AgentCell>,
    id: &str,
    now: WorldTime,
    required: bool,
) -> Result<(), WorldError> {
    if let Some(agent) = updates.get_mut(id) {
        agent.last_active = now;
        return Ok(());
    }
    match state.agents.get(id).cloned() {
        Some(mut agent) => {
            agent.last_active = now;
            updates.insert(id.into(), agent);
            Ok(())
        }
        None if !required => Ok(()),
        None => Err(WorldError::AgentNotFound {
            agent_id: id.into(),
        }),
    }
}
fn reject_service(contract: &EconomicContractState) -> Result<(), WorldError> {
    if contract.fulfillment_kind == EconomicContractFulfillmentKind::Service {
        Err(invalid(
            "service contracts unavailable: collateral/evidence/remedy not implemented",
        ))
    } else {
        Ok(())
    }
}
fn set_resource(
    state: &WorldState,
    updates: &mut BTreeMap<String, AgentCell>,
    id: &str,
    kind: ResourceKind,
    amount: i64,
) -> Result<(), WorldError> {
    let mut agent = updates
        .get(id)
        .cloned()
        .or_else(|| state.agents.get(id).cloned())
        .ok_or_else(|| WorldError::AgentNotFound {
            agent_id: id.into(),
        })?;
    if amount == 0 {
        agent.state.resources.amounts.remove(&kind);
    } else {
        agent.state.resources.amounts.insert(kind, amount);
    }
    updates.insert(id.into(), agent);
    Ok(())
}
fn checked_reputation(
    state: &WorldState,
    id: &str,
    delta: i64,
    role: &str,
) -> Result<Option<i64>, WorldError> {
    if delta == 0 {
        return Ok(None);
    }
    let current = state.reputation_scores.get(id).copied().unwrap_or(0);
    current.checked_add(delta).map(Some).ok_or_else(|| {
        invalid(format!(
            "{role} reputation overflow agent={id} current={current} delta={delta}"
        ))
    })
}
fn saturating_reputation(
    state: &WorldState,
    updates: &mut BTreeMap<String, i64>,
    id: &str,
    delta: i64,
) {
    if delta != 0 {
        let current = updates
            .get(id)
            .copied()
            .or_else(|| state.reputation_scores.get(id).copied())
            .unwrap_or(0);
        updates.insert(id.into(), current.saturating_add(delta));
    }
}
fn record_window(
    state: &WorldState,
    starts: &mut BTreeMap<String, WorldTime>,
    totals: &mut BTreeMap<String, i64>,
    id: &str,
    delta: i64,
    now: WorldTime,
) {
    if delta <= 0 {
        return;
    }
    let started = starts
        .get(id)
        .copied()
        .or_else(|| state.reputation_reward_window_started_at.get(id).copied());
    let in_window = started.is_some_and(|v| now.saturating_sub(v) < REPUTATION_WINDOW_TICKS);
    if in_window {
        let current = totals
            .get(id)
            .copied()
            .or_else(|| state.reputation_reward_window_accumulated.get(id).copied())
            .unwrap_or(0);
        totals.insert(id.into(), current.saturating_add(delta));
    } else {
        starts.insert(id.into(), now);
        totals.insert(id.into(), delta);
    }
}
