use super::*;
use crate::agent_claims::{
    auto_restricted_starter_claim_amount, split_agent_claim_upfront_funding,
};
use crate::{
    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
    RestrictedStarterClaimGrantStatus,
};

impl World {
    pub(super) fn action_to_event_gameplay_claims(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::ClaimStarterOc {
                agent_id,
                player_id,
                public_key,
            } => {
                let agent_id = agent_id.trim();
                if agent_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "claim starter OC rejected: agent_id cannot be empty".to_string(),
                            ],
                        },
                    }));
                }
                if !self.state.agents.contains_key(agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: agent_id.to_string(),
                        },
                    }));
                }
                if self.state.is_manufactured_agent(agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["manufactured_agent_cannot_claim_starter_support".into()],
                        },
                    }));
                }
                let player_id = player_id.trim();
                if player_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "claim starter OC rejected: player_id cannot be empty".to_string(),
                            ],
                        },
                    }));
                }
                let public_key = public_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned);
                if self.main_token_liquid_balance(agent_id) > 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "claim starter OC rejected: agent already has liquid OC: agent_id={agent_id}"
                            )],
                        },
                    }));
                }
                if self.state.starter_oc_claims.contains_key(agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "claim starter OC rejected: agent already claimed: agent_id={agent_id}"
                            )],
                        },
                    }));
                }
                if self
                    .state
                    .starter_oc_claims
                    .values()
                    .any(|claim| claim.player_id == player_id)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "claim starter OC rejected: player already claimed: player_id={player_id}"
                            )],
                        },
                    }));
                }
                if let Some(public_key) = public_key.as_deref()
                    && self
                        .state
                        .starter_oc_claims
                        .values()
                        .any(|claim| claim.public_key.as_deref() == Some(public_key))
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "claim starter OC rejected: public key already claimed".to_string(),
                            ],
                        },
                    }));
                }

                let source_treasury_bucket_id = if self
                    .main_token_treasury_balance(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL)
                    >= STARTER_OC_CLAIM_AMOUNT
                {
                    Some(MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL.to_string())
                } else if self.state.main_token_supply.total_supply == 0
                    && self.state.main_token_supply.total_issued == 0
                    && self.state.main_token_supply.circulating_supply == 0
                {
                    None
                } else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "claim starter OC rejected: ecosystem pool has insufficient OC: bucket={} balance={} amount={}",
                                MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
                                self.main_token_treasury_balance(
                                    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL
                                ),
                                STARTER_OC_CLAIM_AMOUNT
                            )],
                        },
                    }));
                };

                Ok(WorldEventBody::Domain(DomainEvent::StarterOcClaimed {
                    agent_id: agent_id.to_string(),
                    player_id: player_id.to_string(),
                    public_key,
                    amount: STARTER_OC_CLAIM_AMOUNT,
                    claimed_at: self.state.time,
                    source_treasury_bucket_id,
                }))
            }
            Action::ClaimAgent {
                claimer_agent_id,
                target_agent_id,
            } => {
                if !self.state.agents.contains_key(claimer_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: claimer_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(target_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: target_agent_id.clone(),
                        },
                    }));
                }
                if self.state.agent_claims.contains_key(target_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "agent already claimed: target_agent_id={target_agent_id}"
                            )],
                        },
                    }));
                }

                if let Some(reason) =
                    self.industrial_acquisition_rejection(claimer_agent_id, target_agent_id)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason,
                    }));
                }
                let quote = match self.agent_claim_quote_for_owner(claimer_agent_id) {
                    Ok(quote) => quote,
                    Err(err) => {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!("claim agent rejected: {err:?}")],
                            },
                        }));
                    }
                };
                let total_upfront_amount = quote
                    .activation_fee_amount
                    .saturating_add(quote.claim_bond_amount)
                    .saturating_add(quote.upkeep_per_epoch);
                let auto_issued_restricted_amount = auto_restricted_starter_claim_amount(
                    quote.slot_index,
                    self.main_token_liquid_balance(claimer_agent_id),
                    self.main_token_restricted_starter_claim_balance(claimer_agent_id),
                    self.main_token_treasury_balance(
                        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
                    ),
                    total_upfront_amount,
                    self.restricted_starter_claim_grant(claimer_agent_id)
                        .is_some_and(|grant| {
                            grant.status == RestrictedStarterClaimGrantStatus::Issued
                        }),
                );
                let funding = match split_agent_claim_upfront_funding(
                    quote.slot_index,
                    self.main_token_liquid_balance(claimer_agent_id),
                    self.main_token_restricted_starter_claim_balance(claimer_agent_id)
                        .saturating_add(auto_issued_restricted_amount),
                    quote.activation_fee_amount,
                    quote.claim_bond_amount,
                    quote.upkeep_per_epoch,
                ) {
                    Ok(funding) => funding,
                    Err(reason) => {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "claim agent rejected: restricted/liquid funding unavailable: {reason}"
                                )],
                            },
                        }));
                    }
                };
                let current_epoch = self.current_governance_epoch();
                let event = DomainEvent::AgentClaimed {
                    acquisition_contract_version: 1,
                    claimer_agent_id: claimer_agent_id.clone(),
                    target_agent_id: target_agent_id.clone(),
                    reputation_tier: quote.reputation_tier,
                    slot_index: quote.slot_index,
                    activation_fee_amount: quote.activation_fee_amount,
                    activation_fee_burn_amount: quote.activation_fee_burn_amount,
                    activation_fee_treasury_amount: quote.activation_fee_treasury_amount,
                    claim_bond_amount: quote.claim_bond_amount,
                    upfront_restricted_spent_amount: funding.upfront.restricted_amount,
                    upfront_liquid_spent_amount: funding.upfront.liquid_amount,
                    auto_issued_restricted_amount,
                    auto_issued_restricted_source_treasury_bucket_id:
                        (auto_issued_restricted_amount > 0
                            && funding.claim_bond.restricted_amount > 0)
                            .then(|| {
                                MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
                                    .to_string()
                            }),
                    claim_bond_locked_restricted_amount: funding.claim_bond.restricted_amount,
                    claim_bond_locked_liquid_amount: funding.claim_bond.liquid_amount,
                    upkeep_per_epoch: quote.upkeep_per_epoch,
                    claimed_at_epoch: current_epoch,
                    upkeep_paid_through_epoch: current_epoch,
                    release_cooldown_epochs: quote.release_cooldown_epochs,
                    grace_epochs: quote.grace_epochs,
                    idle_warning_epochs: quote.idle_warning_epochs,
                    forced_idle_reclaim_epochs: quote.forced_idle_reclaim_epochs,
                    forced_reclaim_penalty_bps: quote.forced_reclaim_penalty_bps,
                };
                if let Err(err) = super::super::super::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare(&self.state,&event,self.state.time) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("claim agent rejected: {err:?}")],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(event))
            }
            Action::ReleaseAgentClaim {
                claimer_agent_id,
                target_agent_id,
            } => {
                if !self.state.agents.contains_key(claimer_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: claimer_agent_id.clone(),
                        },
                    }));
                }
                let Some(claim) = self.state.agent_claims.get(target_agent_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "agent claim not found: target_agent_id={target_agent_id}"
                            )],
                        },
                    }));
                };
                if claim.claim_owner_id != *claimer_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "agent claim owner mismatch: target_agent_id={} owner={} claimer={}",
                                target_agent_id, claim.claim_owner_id, claimer_agent_id
                            )],
                        },
                    }));
                }
                let requested_at_epoch = self.current_governance_epoch();
                let ready_at_epoch =
                    requested_at_epoch.saturating_add(claim.release_cooldown_epochs);
                let event = DomainEvent::AgentClaimReleaseRequested {
                    claimer_agent_id: claimer_agent_id.clone(),
                    target_agent_id: target_agent_id.clone(),
                    requested_at_epoch,
                    ready_at_epoch,
                };
                if let Err(err) = super::super::super::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle::prepare(
                    &self.state,
                    &event,
                    self.state.time,
                ) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("release agent claim rejected: {err:?}")],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(event))
            }
            _ => unreachable!("unsupported variant reached its specialized dispatcher"),
        }
    }
}
