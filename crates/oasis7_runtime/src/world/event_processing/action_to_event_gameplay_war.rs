use super::*;

impl World {
    pub(super) fn action_to_event_gameplay_war(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::DeclareWar {
                initiator_agent_id,
                war_id,
                aggressor_alliance_id,
                defender_alliance_id,
                objective,
                intensity,
            } => {
                if !self.state.agents.contains_key(initiator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: initiator_agent_id.clone(),
                        },
                    }));
                }
                let war_id = war_id.trim();
                if war_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["war_id cannot be empty".to_string()],
                        },
                    }));
                }
                if self.state.wars.contains_key(war_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("war already exists: {war_id}")],
                        },
                    }));
                }
                let aggressor_alliance_id = aggressor_alliance_id.trim();
                let defender_alliance_id = defender_alliance_id.trim();
                if aggressor_alliance_id.is_empty() || defender_alliance_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "aggressor_alliance_id and defender_alliance_id cannot be empty"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                if aggressor_alliance_id == defender_alliance_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "aggressor_alliance_id and defender_alliance_id must differ"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                let Some(aggressor) = self.state.alliances.get(aggressor_alliance_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "aggressor alliance not found: {}",
                                aggressor_alliance_id
                            )],
                        },
                    }));
                };
                if aggressor.members.len() < WAR_MIN_ALLIANCE_MEMBERS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "aggressor alliance requires at least {WAR_MIN_ALLIANCE_MEMBERS} members"
                            )],
                        },
                    }));
                }
                if !aggressor
                    .members
                    .iter()
                    .any(|member| member == initiator_agent_id)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "initiator {} is not a member of aggressor alliance {}",
                                initiator_agent_id, aggressor_alliance_id
                            )],
                        },
                    }));
                }
                let Some(defender) = self.state.alliances.get(defender_alliance_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "defender alliance not found: {}",
                                defender_alliance_id
                            )],
                        },
                    }));
                };
                if defender.members.len() < WAR_MIN_ALLIANCE_MEMBERS {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "defender alliance requires at least {WAR_MIN_ALLIANCE_MEMBERS} members"
                            )],
                        },
                    }));
                }
                let aggressor_member_set: BTreeSet<&str> =
                    aggressor.members.iter().map(String::as_str).collect();
                let has_member_overlap = defender
                    .members
                    .iter()
                    .any(|member| aggressor_member_set.contains(member.as_str()));
                if has_member_overlap {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "aggressor and defender alliances cannot share members".to_string(),
                            ],
                        },
                    }));
                }
                let objective = objective.trim();
                if objective.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["war objective cannot be empty".to_string()],
                        },
                    }));
                }
                if *intensity == 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["war intensity must be > 0".to_string()],
                        },
                    }));
                }
                if *intensity > WAR_MAX_INTENSITY {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("war intensity exceeds max {}", WAR_MAX_INTENSITY)],
                        },
                    }));
                }
                let has_active_conflict = self.state.wars.values().any(|war| {
                    if !war.active {
                        return false;
                    }
                    war.aggressor_alliance_id == aggressor_alliance_id
                        || war.defender_alliance_id == aggressor_alliance_id
                        || war.aggressor_alliance_id == defender_alliance_id
                        || war.defender_alliance_id == defender_alliance_id
                });
                if has_active_conflict {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "aggressor or defender alliance already has an active war"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                let (mobilization_electricity_cost, mobilization_data_cost) =
                    Self::war_mobilization_costs(*intensity);
                let initiator_electricity = self
                    .state
                    .agents
                    .get(initiator_agent_id)
                    .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
                    .unwrap_or(0);
                if initiator_electricity < mobilization_electricity_cost {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientResource {
                            agent_id: initiator_agent_id.clone(),
                            kind: ResourceKind::Electricity,
                            requested: mobilization_electricity_cost,
                            available: initiator_electricity,
                        },
                    }));
                }
                let initiator_data = self
                    .state
                    .agents
                    .get(initiator_agent_id)
                    .map(|cell| cell.state.resources.get(ResourceKind::Data))
                    .unwrap_or(0);
                if initiator_data < mobilization_data_cost {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientResource {
                            agent_id: initiator_agent_id.clone(),
                            kind: ResourceKind::Data,
                            requested: mobilization_data_cost,
                            available: initiator_data,
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::WarDeclared {
                    initiator_agent_id: initiator_agent_id.clone(),
                    war_id: war_id.to_string(),
                    aggressor_alliance_id: aggressor_alliance_id.to_string(),
                    defender_alliance_id: defender_alliance_id.to_string(),
                    objective: objective.to_string(),
                    intensity: *intensity,
                    mobilization_electricity_cost,
                    mobilization_data_cost,
                }))
            }
            _ => unreachable!("unsupported variant reached its specialized dispatcher"),
        }
    }
}
