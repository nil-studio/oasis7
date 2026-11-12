//! Fixed original economic-contract Action predicates; not hooked yet.
//! Actual Current implementations must read closed typed data, never substitute
//! a caller verification/permission callback for these predicates.
use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::ops::Deref;
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) enum ContractRecordV1<'a> {
    Legacy(&'a oasis7_world_state::EconomicContractState),
    Current(ReservedValue<oasis7_world_state::EconomicContractState>),
}
impl Deref for ContractRecordV1<'_> {
    type Target = oasis7_world_state::EconomicContractState;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(value) => value,
            Self::Current(value) => value,
        }
    }
}
pub(in crate::world) trait ContractFactsV1<'source>: sealed::Sealed {
    type Error: From<WorldError>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_outputs(&mut self, action: &Action) -> Result<(), Self::Error>;
    fn reserve_pair_key(&mut self, a: &str, b: &str) -> Result<(), Self::Error>;
    fn now(&self) -> u64;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn contract_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn contract(&mut self, id: &str) -> Result<Option<ContractRecordV1<'source>>, Self::Error>;
    fn blocked_parties(&mut self, a: &str, b: &str) -> Result<bool, Self::Error>;
    fn active_contract_count(&mut self, creator: &str) -> Result<usize, Self::Error>;
    fn max_open_contracts_per_agent(&mut self) -> Result<u16, Self::Error>;
    fn pair_last_settled_at(&mut self, pair: &str) -> Result<Option<u64>, Self::Error>;
    fn data_access_grant(&mut self, owner: &str, accessor: &str) -> Result<bool, Self::Error>;
    fn electricity_tax_bps(&mut self) -> Result<u16, Self::Error>;
    fn power_trade_fee_bps(&mut self) -> Result<u16, Self::Error>;
    fn data_tax_bps(&mut self) -> Result<u16, Self::Error>;
    fn resource(&mut self, id: &str, kind: ResourceKind) -> Result<i64, Self::Error>;
    fn reward_window_started_at(&mut self, id: &str) -> Result<Option<u64>, Self::Error>;
    fn reward_window_accumulated(&mut self, id: &str) -> Result<Option<i64>, Self::Error>;
    fn world_binding(&mut self) -> Result<Option<String>, Self::Error>;
}
fn reputation_reward_budget<'source, S: ContractFactsV1<'source>>(
    source: &mut S,
    agent: &str,
) -> Result<i64, S::Error> {
    let cap = ECONOMIC_CONTRACT_REPUTATION_WINDOW_CAP;
    if cap <= 0 {
        return Ok(0);
    }
    let in_window = source
        .reward_window_started_at(agent)?
        .is_some_and(|start| {
            source.now().saturating_sub(start) < ECONOMIC_CONTRACT_REPUTATION_WINDOW_TICKS
        });
    if !in_window {
        return Ok(cap);
    }
    let accumulated = source.reward_window_accumulated(agent)?.unwrap_or(0).max(0);
    Ok(cap.saturating_sub(accumulated).max(0))
}
pub(in crate::world) struct LegacyContractFactsV1<'a> {
    world: &'a World,
}
impl<'a> LegacyContractFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a World) -> Self {
        Self { world }
    }
}
impl sealed::Sealed for LegacyContractFactsV1<'_> {}
impl<'a> ContractFactsV1<'a> for LegacyContractFactsV1<'a> {
    type Error = WorldError;
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_outputs(&mut self, _: &Action) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_pair_key(&mut self, _: &str, _: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn now(&self) -> u64 {
        self.world.state.time
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.agents.contains_key(id))
    }
    fn contract_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.economic_contracts.contains_key(id))
    }
    fn contract(&mut self, id: &str) -> Result<Option<ContractRecordV1<'a>>, Self::Error> {
        Ok(self
            .world
            .state
            .economic_contracts
            .get(id)
            .map(ContractRecordV1::Legacy))
    }
    fn blocked_parties(&mut self, a: &str, b: &str) -> Result<bool, Self::Error> {
        Ok(self
            .world
            .state
            .gameplay_policy
            .blocked_agents
            .iter()
            .any(|value| value == a || value == b))
    }
    fn active_contract_count(&mut self, id: &str) -> Result<usize, Self::Error> {
        Ok(self
            .world
            .state
            .economic_contracts
            .values()
            .filter(|value| {
                value.creator_agent_id == id
                    && matches!(
                        value.status,
                        EconomicContractStatus::Open | EconomicContractStatus::Accepted
                    )
            })
            .count())
    }
    fn max_open_contracts_per_agent(&mut self) -> Result<u16, Self::Error> {
        Ok(self
            .world
            .state
            .gameplay_policy
            .max_open_contracts_per_agent)
    }
    fn pair_last_settled_at(&mut self, id: &str) -> Result<Option<u64>, Self::Error> {
        Ok(self
            .world
            .state
            .contract_pair_last_success_settled_at
            .get(id)
            .copied())
    }
    fn data_access_grant(&mut self, a: &str, b: &str) -> Result<bool, Self::Error> {
        Ok(self
            .world
            .state
            .data_access_permissions
            .get(a)
            .is_some_and(|values| values.contains(b)))
    }
    fn electricity_tax_bps(&mut self) -> Result<u16, Self::Error> {
        Ok(self.world.state.gameplay_policy.electricity_tax_bps)
    }
    fn power_trade_fee_bps(&mut self) -> Result<u16, Self::Error> {
        Ok(self.world.state.gameplay_policy.power_trade_fee_bps)
    }
    fn data_tax_bps(&mut self) -> Result<u16, Self::Error> {
        Ok(self.world.state.gameplay_policy.data_tax_bps)
    }
    fn resource(&mut self, id: &str, kind: ResourceKind) -> Result<i64, Self::Error> {
        Ok(self
            .world
            .state
            .agents
            .get(id)
            .map(|cell| cell.state.resources.get(kind))
            .unwrap_or(0))
    }
    fn reward_window_started_at(&mut self, id: &str) -> Result<Option<u64>, Self::Error> {
        Ok(self
            .world
            .state
            .reputation_reward_window_started_at
            .get(id)
            .copied())
    }
    fn reward_window_accumulated(&mut self, id: &str) -> Result<Option<i64>, Self::Error> {
        Ok(self
            .world
            .state
            .reputation_reward_window_accumulated
            .get(id)
            .copied())
    }
    fn world_binding(&mut self) -> Result<Option<String>, Self::Error> {
        Ok(self.world.world_rule_binding())
    }
}
pub(in crate::world) fn evaluate_contract_action_v1<'source, S: ContractFactsV1<'source>>(
    source: &mut S,
    action_id: ActionId,
    action: &Action,
) -> Result<WorldEventBody, S::Error> {
    source.reserve_outputs(action)?;
    match action {
        Action::OpenEconomicContract {
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
            if *fulfillment_kind == EconomicContractFulfillmentKind::Service {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "service contracts unavailable: collateral/evidence/remedy not implemented",
                ));
            }
            if !source.agent_exists(creator_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: creator_agent_id.clone(),
                    },
                }));
            }
            if !source.agent_exists(counterparty_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: counterparty_agent_id.clone(),
                    },
                }));
            }
            if creator_agent_id == counterparty_agent_id {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["economic contract requires distinct parties".to_string()],
                    },
                }));
            }
            source.charge(contract_id.len())?;
            let contract_id = contract_id.trim();
            if contract_id.is_empty() {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "contract_id cannot be empty",
                ));
            }
            if source.contract_exists(contract_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("economic contract already exists: {contract_id}")],
                    },
                }));
            }
            if *settlement_amount <= 0 {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::InvalidAmount {
                        amount: *settlement_amount,
                    },
                }));
            }
            if *reputation_stake <= 0 || *reputation_stake > ECONOMIC_CONTRACT_MAX_REPUTATION_STAKE
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "reputation_stake must be within 1..={}",
                            ECONOMIC_CONTRACT_MAX_REPUTATION_STAKE
                        )],
                    },
                }));
            }
            if *expires_at <= source.now() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "expires_at must be greater than current world time".to_string(),
                        ],
                    },
                }));
            }
            source.charge(description.len())?;
            let description = description.trim();
            if description.is_empty() {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "economic contract description cannot be empty",
                ));
            }
            if source.blocked_parties(creator_agent_id, counterparty_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["economic contract blocked by gameplay policy".to_string()],
                    },
                }));
            }
            let active_contract_count = source.active_contract_count(creator_agent_id)?;
            if active_contract_count >= usize::from(source.max_open_contracts_per_agent()?) {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "economic contract quota exceeded for creator {}",
                            creator_agent_id
                        )],
                    },
                }));
            }
            Ok(WorldEventBody::Domain(
                DomainEvent::EconomicContractOpened {
                    creator_agent_id: creator_agent_id.clone(),
                    contract_id: contract_id.to_string(),
                    counterparty_agent_id: counterparty_agent_id.clone(),
                    fulfillment_kind: *fulfillment_kind,
                    settlement_kind: *settlement_kind,
                    settlement_amount: *settlement_amount,
                    reputation_stake: *reputation_stake,
                    expires_at: *expires_at,
                    description: description.to_string(),
                },
            ))
        }
        Action::AcceptEconomicContract {
            accepter_agent_id,
            contract_id,
        } => {
            if !source.agent_exists(accepter_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: accepter_agent_id.clone(),
                    },
                }));
            }
            source.charge(contract_id.len())?;
            let contract_id = contract_id.trim();
            if contract_id.is_empty() {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "contract_id cannot be empty",
                ));
            }
            let Some(contract) = source.contract(contract_id)? else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("economic contract not found: {contract_id}")],
                    },
                }));
            };
            if contract.status != EconomicContractStatus::Open {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("economic contract is not open: {}", contract_id)],
                    },
                }));
            }
            if contract.counterparty_agent_id != *accepter_agent_id {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "contract accepter mismatch expected {}",
                            contract.counterparty_agent_id
                        )],
                    },
                }));
            }
            if source.now() > contract.expires_at {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "economic contract has expired at {}",
                            contract.expires_at
                        )],
                    },
                }));
            }
            Ok(WorldEventBody::Domain(
                DomainEvent::EconomicContractAccepted {
                    accepter_agent_id: accepter_agent_id.clone(),
                    contract_id: contract_id.to_string(),
                },
            ))
        }
        Action::SettleEconomicContract {
            operator_agent_id,
            contract_id,
            success,
            notes,
        } => {
            if !source.agent_exists(operator_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.clone(),
                    },
                }));
            }
            source.charge(contract_id.len())?;
            let contract_id = contract_id.trim();
            if contract_id.is_empty() {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "contract_id cannot be empty",
                ));
            }
            let Some(contract) = source.contract(contract_id)? else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("economic contract not found: {contract_id}")],
                    },
                }));
            };
            if contract.status != EconomicContractStatus::Accepted {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "economic contract status is not accepted: {}",
                            contract_id
                        )],
                    },
                }));
            }
            if contract.fulfillment_kind == EconomicContractFulfillmentKind::Service {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "service contracts unavailable: collateral/evidence/remedy not implemented",
                ));
            }
            if !success {
                return Ok(economic_contract_rule_denied(
                    action_id,
                    "atomic exchange settlement requires success=true",
                ));
            }
            if contract.creator_agent_id != *operator_agent_id
                && contract.counterparty_agent_id != *operator_agent_id
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "settlement operator must belong to contract parties".to_string(),
                        ],
                    },
                }));
            }
            source.charge(notes.len())?;
            let notes = notes.trim();
            if notes.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "economic contract settlement notes cannot be empty".to_string(),
                        ],
                    },
                }));
            }

            let (
                transfer_amount,
                tax_amount,
                creator_reputation_delta,
                counterparty_reputation_delta,
            ) = if *success {
                source.reserve_pair_key(
                    &contract.creator_agent_id,
                    &contract.counterparty_agent_id,
                )?;
                let pair = oasis7_world_state::WorldState::economic_contract_pair_key(
                    &contract.creator_agent_id,
                    &contract.counterparty_agent_id,
                );
                if let Some(ready_at) = source
                    .pair_last_settled_at(&pair)?
                    .map(|last| last.saturating_add(ECONOMIC_CONTRACT_PAIR_COOLDOWN_TICKS))
                    && source.now() < ready_at
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "economic contract settlement denied: pair cooldown active until tick {}",
                                ready_at
                            )],
                        },
                    }));
                }
                source.charge(contract.creator_agent_id.len())?;
                source.charge(contract.counterparty_agent_id.len())?;
                if contract.settlement_kind == ResourceKind::Data
                    && contract.creator_agent_id != contract.counterparty_agent_id
                    && !source.data_access_grant(
                        &contract.creator_agent_id,
                        &contract.counterparty_agent_id,
                    )?
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "economic contract data settlement denied: missing access grant owner={} grantee={}",
                                contract.creator_agent_id, contract.counterparty_agent_id
                            )],
                        },
                    }));
                }
                let tax_bps = match contract.settlement_kind {
                    ResourceKind::Electricity => source
                        .electricity_tax_bps()?
                        .saturating_add(source.power_trade_fee_bps()?)
                        .min(GAMEPLAY_POLICY_MAX_TAX_BPS),
                    ResourceKind::Data => source.data_tax_bps()?,
                };
                let tax_amount = contract
                    .settlement_amount
                    .saturating_mul(i64::from(tax_bps))
                    .saturating_div(10_000);
                let total_required = contract.settlement_amount.saturating_add(tax_amount);
                let available =
                    source.resource(&contract.creator_agent_id, contract.settlement_kind)?;
                if available < total_required {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientResource {
                            agent_id: contract.creator_agent_id.clone(),
                            kind: contract.settlement_kind,
                            requested: total_required,
                            available,
                        },
                    }));
                }
                let success_reputation_reward = World::economic_contract_success_reputation_reward(
                    contract.settlement_amount,
                    contract.reputation_stake,
                );
                let creator_reward_budget =
                    reputation_reward_budget(source, &contract.creator_agent_id)?;
                let counterparty_reward_budget =
                    reputation_reward_budget(source, &contract.counterparty_agent_id)?;
                let creator_reward = success_reputation_reward.min(creator_reward_budget);
                let counterparty_reward = success_reputation_reward.min(counterparty_reward_budget);
                (
                    contract.settlement_amount,
                    tax_amount,
                    creator_reward,
                    counterparty_reward,
                )
            } else {
                unreachable!("atomic exchanges require success=true before settlement")
            };

            Ok(WorldEventBody::Domain(
                DomainEvent::EconomicContractSettled {
                    operator_agent_id: operator_agent_id.clone(),
                    contract_id: contract_id.to_string(),
                    world_binding: source.world_binding()?,
                    success: *success,
                    transfer_amount,
                    tax_amount,
                    notes: notes.to_string(),
                    creator_reputation_delta,
                    counterparty_reputation_delta,
                },
            ))
        }
        _ => unreachable!("unsupported variant reached its specialized dispatcher"),
    }
}
