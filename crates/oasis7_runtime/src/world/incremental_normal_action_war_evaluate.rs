//! Sole fixed DeclareWar predicates shared by legacy World and actual Current.
//! Sources supply paid observations; every business predicate stays in this kernel.
use super::*;
use crate::world::event_processing::alliance_fixed::AllianceRecordV1;
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait WarFactsV1<'source>: sealed::Sealed {
    type Error: From<WorldError>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_outputs(&mut self, action: &Action) -> Result<(), Self::Error>;
    fn reserve_overlap(
        &mut self,
        aggressor: &[String],
        defender: &[String],
    ) -> Result<(), Self::Error>;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn war_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn alliance(&mut self, id: &str) -> Result<Option<AllianceRecordV1<'source>>, Self::Error>;
    fn active_conflict(&mut self, aggressor: &str, defender: &str) -> Result<bool, Self::Error>;
    fn resource(&mut self, agent: &str, kind: ResourceKind) -> Result<i64, Self::Error>;
}
pub(in crate::world) struct LegacyWarFactsV1<'a> {
    world: &'a World,
}
impl<'a> LegacyWarFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a World) -> Self {
        Self { world }
    }
}
impl sealed::Sealed for LegacyWarFactsV1<'_> {}
impl<'a> WarFactsV1<'a> for LegacyWarFactsV1<'a> {
    type Error = WorldError;
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_outputs(&mut self, _: &Action) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_overlap(&mut self, _: &[String], _: &[String]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.agents.contains_key(id))
    }
    fn war_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.wars.contains_key(id))
    }
    fn alliance(&mut self, id: &str) -> Result<Option<AllianceRecordV1<'a>>, Self::Error> {
        Ok(self
            .world
            .state
            .alliances
            .get(id)
            .map(AllianceRecordV1::Legacy))
    }
    fn active_conflict(&mut self, a: &str, b: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.wars.values().any(|war| {
            war.active
                && (war.aggressor_alliance_id == a
                    || war.defender_alliance_id == a
                    || war.aggressor_alliance_id == b
                    || war.defender_alliance_id == b)
        }))
    }
    fn resource(&mut self, agent: &str, kind: ResourceKind) -> Result<i64, Self::Error> {
        Ok(self
            .world
            .state
            .agents
            .get(agent)
            .map(|cell| cell.state.resources.get(kind))
            .unwrap_or(0))
    }
}
pub(in crate::world) fn evaluate_war_action_v1<'source, S: WarFactsV1<'source>>(
    source: &mut S,
    action_id: ActionId,
    action: &Action,
) -> Result<WorldEventBody, S::Error> {
    source.reserve_outputs(action)?;
    match action {
        Action::DeclareWar {
            initiator_agent_id,
            war_id,
            aggressor_alliance_id,
            defender_alliance_id,
            objective,
            intensity,
        } => {
            if !source.agent_exists(initiator_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: initiator_agent_id.clone(),
                    },
                }));
            }
            source.charge(war_id.len())?;
            let war_id = war_id.trim();
            if war_id.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["war_id cannot be empty".to_string()],
                    },
                }));
            }
            if source.war_exists(war_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("war already exists: {war_id}")],
                    },
                }));
            }
            source.charge(aggressor_alliance_id.len())?;
            let aggressor_alliance_id = aggressor_alliance_id.trim();
            source.charge(defender_alliance_id.len())?;
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
            source.charge(aggressor_alliance_id.len())?;
            source.charge(defender_alliance_id.len())?;
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
            let Some(aggressor) = source.alliance(aggressor_alliance_id)? else {
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
            let Some(defender) = source.alliance(defender_alliance_id)? else {
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
            source.reserve_overlap(&aggressor.members, &defender.members)?;
            let mut aggressor_member_set: BTreeSet<&str> = BTreeSet::new();
            for member in &aggressor.members {
                source.charge(member.len())?;
                aggressor_member_set.insert(member.as_str());
            }
            let mut has_member_overlap = false;
            for member in &defender.members {
                source.charge(member.len())?;
                if aggressor_member_set.contains(member.as_str()) {
                    has_member_overlap = true;
                    break;
                }
            }
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
            source.charge(objective.len())?;
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
            let has_active_conflict =
                source.active_conflict(aggressor_alliance_id, defender_alliance_id)?;
            if has_active_conflict {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "aggressor or defender alliance already has an active war".to_string(),
                        ],
                    },
                }));
            }
            let (mobilization_electricity_cost, mobilization_data_cost) =
                World::war_mobilization_costs(*intensity);
            let initiator_electricity =
                source.resource(initiator_agent_id, ResourceKind::Electricity)?;
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
            let initiator_data = source.resource(initiator_agent_id, ResourceKind::Data)?;
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
