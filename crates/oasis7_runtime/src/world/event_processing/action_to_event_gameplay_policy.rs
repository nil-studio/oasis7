//! Deterministic gameplay policy publication, separate from controller admission.
use super::*;

impl World {
    pub(super) fn gameplay_policy_event(&self, action_id: ActionId, action: &Action) -> Result<WorldEventBody, WorldError> {
        let Action::UpdateGameplayPolicy {
                operator_agent_id,
                electricity_tax_bps,
                data_tax_bps,
                power_trade_fee_bps,
                max_open_contracts_per_agent,
                blocked_agents,
                forbidden_location_ids,
            } = action else {
            return Err(WorldError::GovernancePolicyInvalid { reason: "expected gameplay policy action".into() });
        };

                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                if !self.has_policy_update_governance_authorization(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "update gameplay policy requires passed governance proposal total_weight >= {}",
                                GAMEPLAY_POLICY_UPDATE_MIN_GOVERNANCE_TOTAL_WEIGHT
                            )],
                        },
                    }));
                }
                if *electricity_tax_bps > GAMEPLAY_POLICY_MAX_TAX_BPS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "electricity_tax_bps must be <= {}",
                                GAMEPLAY_POLICY_MAX_TAX_BPS
                            )],
                        },
                    }));
                }
                if *data_tax_bps > GAMEPLAY_POLICY_MAX_TAX_BPS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "data_tax_bps must be <= {}",
                                GAMEPLAY_POLICY_MAX_TAX_BPS
                            )],
                        },
                    }));
                }
                if *power_trade_fee_bps > GAMEPLAY_POLICY_MAX_TAX_BPS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "power_trade_fee_bps must be <= {}",
                                GAMEPLAY_POLICY_MAX_TAX_BPS
                            )],
                        },
                    }));
                }
                if *max_open_contracts_per_agent < GAMEPLAY_POLICY_MIN_CONTRACT_QUOTA
                    || *max_open_contracts_per_agent > GAMEPLAY_POLICY_MAX_CONTRACT_QUOTA
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "max_open_contracts_per_agent must be within {}..={}",
                                GAMEPLAY_POLICY_MIN_CONTRACT_QUOTA,
                                GAMEPLAY_POLICY_MAX_CONTRACT_QUOTA
                            )],
                        },
                    }));
                }
                let mut normalized_blocked_agents = BTreeSet::new();
                for value in blocked_agents {
                    let candidate = value.trim();
                    if candidate.is_empty() {
                        continue;
                    }
                    if !self.state.agents.contains_key(candidate) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: candidate.to_string(),
                            },
                        }));
                    }
                    normalized_blocked_agents.insert(candidate.to_string());
                }
                let mut normalized_forbidden_location_ids = BTreeSet::new();
                for value in forbidden_location_ids {
                    let candidate = value.trim();
                    if candidate.is_empty() {
                        continue;
                    }
                    normalized_forbidden_location_ids.insert(candidate.to_string());
                }
                Ok(WorldEventBody::Domain(DomainEvent::GameplayPolicyUpdated {
                    operator_agent_id: operator_agent_id.clone(),
                    electricity_tax_bps: *electricity_tax_bps,
                    data_tax_bps: *data_tax_bps,
                    power_trade_fee_bps: *power_trade_fee_bps,
                    max_open_contracts_per_agent: *max_open_contracts_per_agent,
                    blocked_agents: normalized_blocked_agents.into_iter().collect(),
                    forbidden_location_ids: normalized_forbidden_location_ids.into_iter().collect(),
                }))
    }
}
