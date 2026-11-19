//! Sole original Alliance Action predicates over two sealed read-only sources.
//! Data queries cannot replace any predicate or mint an execution capability.
use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::ops::Deref;
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) enum AllianceRecordV1<'a> {
    Legacy(&'a oasis7_world_state::AllianceState),
    Current(ReservedValue<oasis7_world_state::AllianceState>),
}
impl Deref for AllianceRecordV1<'_> {
    type Target = oasis7_world_state::AllianceState;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Legacy(value) => value,
            Self::Current(value) => value,
        }
    }
}
pub(in crate::world) enum AllianceIdV1<'a> {
    Legacy(&'a str),
    Current(ReservedValue<String>),
}
impl std::fmt::Display for AllianceIdV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy(value) => f.write_str(value),
            Self::Current(value) => f.write_str(value),
        }
    }
}
pub(in crate::world) trait AllianceFactsV1<'source>: sealed::Sealed {
    type Error: From<WorldError>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_outputs(&mut self, action: &Action) -> Result<(), Self::Error>;
    fn reserve_normalization(
        &mut self,
        proposer: &str,
        members: &[String],
    ) -> Result<(), Self::Error>;
    fn reserve_former_members(&mut self, members: &[String]) -> Result<(), Self::Error>;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn alliance_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn alliance(&mut self, id: &str) -> Result<Option<AllianceRecordV1<'source>>, Self::Error>;
    fn membership(&mut self, agent: &str) -> Result<Option<AllianceIdV1<'source>>, Self::Error>;
    fn active_war(&mut self, alliance: &str) -> Result<bool, Self::Error>;
}
pub(in crate::world) struct LegacyAllianceFactsV1<'a> {
    world: &'a World,
}
impl<'a> LegacyAllianceFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a World) -> Self {
        Self { world }
    }
}
impl sealed::Sealed for LegacyAllianceFactsV1<'_> {}
impl<'a> AllianceFactsV1<'a> for LegacyAllianceFactsV1<'a> {
    type Error = WorldError;
    fn charge(&mut self, _: usize) -> Result<(), WorldError> {
        Ok(())
    }
    fn reserve_outputs(&mut self, _: &Action) -> Result<(), WorldError> {
        Ok(())
    }
    fn reserve_normalization(&mut self, _: &str, _: &[String]) -> Result<(), WorldError> {
        Ok(())
    }
    fn reserve_former_members(&mut self, _: &[String]) -> Result<(), WorldError> {
        Ok(())
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self.world.state.agents.contains_key(id))
    }
    fn alliance_exists(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self.world.state.alliances.contains_key(id))
    }
    fn alliance(&mut self, id: &str) -> Result<Option<AllianceRecordV1<'a>>, WorldError> {
        Ok(self
            .world
            .state
            .alliances
            .get(id)
            .map(AllianceRecordV1::Legacy))
    }
    fn membership(&mut self, agent: &str) -> Result<Option<AllianceIdV1<'a>>, WorldError> {
        Ok(self
            .world
            .state
            .alliances
            .iter()
            .find(|(_, alliance)| alliance.members.iter().any(|member| member == agent))
            .map(|(id, _)| AllianceIdV1::Legacy(id)))
    }
    fn active_war(&mut self, id: &str) -> Result<bool, WorldError> {
        Ok(self.world.state.wars.values().any(|war| {
            war.active && (war.aggressor_alliance_id == id || war.defender_alliance_id == id)
        }))
    }
}
pub(in crate::world) fn evaluate_alliance_action_v1<'source, S: AllianceFactsV1<'source>>(
    source: &mut S,
    action_id: ActionId,
    action: &Action,
) -> Result<WorldEventBody, S::Error> {
    source.reserve_outputs(action)?;
    match action {
        Action::FormAlliance {
            proposer_agent_id,
            alliance_id,
            members,
            charter,
        } => {
            if !source.agent_exists(proposer_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: proposer_agent_id.clone(),
                    },
                }));
            }
            source.charge(alliance_id.len())?;
            let alliance_id = alliance_id.trim();
            if alliance_id.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["alliance_id cannot be empty".to_string()],
                    },
                }));
            }
            if source.alliance_exists(alliance_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("alliance already exists: {alliance_id}")],
                    },
                }));
            }
            source.charge(charter.len())?;
            let charter = charter.trim();
            if charter.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["alliance charter cannot be empty".to_string()],
                    },
                }));
            }

            source.reserve_normalization(proposer_agent_id, members)?;
            let mut member_set = BTreeSet::new();
            member_set.insert(proposer_agent_id.trim().to_string());
            for member in members {
                source.charge(member.len())?;
                let member = member.trim();
                if member.is_empty() {
                    continue;
                }
                member_set.insert(member.to_string());
            }
            if member_set.len() < 2 {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["alliance requires at least 2 unique members".to_string()],
                    },
                }));
            }
            let normalized_members: Vec<String> = member_set.into_iter().collect();
            if normalized_members.len() > WAR_MAX_ALLIANCE_MEMBERS {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "alliance members exceed max {WAR_MAX_ALLIANCE_MEMBERS}"
                        )],
                    },
                }));
            }
            for member in &normalized_members {
                if !source.agent_exists(member)? {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: member.clone(),
                        },
                    }));
                }
                if let Some(current_alliance_id) = source.membership(member.as_str())? {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "member {} already belongs to alliance {}",
                                member, current_alliance_id
                            )],
                        },
                    }));
                }
            }
            Ok(WorldEventBody::Domain(DomainEvent::AllianceFormed {
                proposer_agent_id: proposer_agent_id.clone(),
                alliance_id: alliance_id.to_string(),
                members: normalized_members,
                charter: charter.to_string(),
            }))
        }
        Action::JoinAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => {
            if !source.agent_exists(operator_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.clone(),
                    },
                }));
            }
            if !source.agent_exists(member_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: member_agent_id.clone(),
                    },
                }));
            }
            source.charge(alliance_id.len())?;
            let alliance_id = alliance_id.trim();
            if alliance_id.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["alliance_id cannot be empty".to_string()],
                    },
                }));
            }
            let Some(alliance) = source.alliance(alliance_id)? else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("alliance not found: {alliance_id}")],
                    },
                }));
            };
            if !alliance
                .members
                .iter()
                .any(|member| member == operator_agent_id)
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "operator {} is not a member of alliance {}",
                            operator_agent_id, alliance_id
                        )],
                    },
                }));
            }
            if alliance
                .members
                .iter()
                .any(|member| member == member_agent_id)
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "member {} already in alliance {}",
                            member_agent_id, alliance_id
                        )],
                    },
                }));
            }
            if let Some(current_alliance_id) = source.membership(member_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "member {} already belongs to alliance {}",
                            member_agent_id, current_alliance_id
                        )],
                    },
                }));
            }
            if source.active_war(alliance_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "alliance {} has active war and cannot change members",
                            alliance_id
                        )],
                    },
                }));
            }
            if alliance.members.len().saturating_add(1) > WAR_MAX_ALLIANCE_MEMBERS {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "alliance members exceed max {WAR_MAX_ALLIANCE_MEMBERS}"
                        )],
                    },
                }));
            }
            Ok(WorldEventBody::Domain(DomainEvent::AllianceJoined {
                operator_agent_id: operator_agent_id.clone(),
                alliance_id: alliance_id.to_string(),
                member_agent_id: member_agent_id.clone(),
            }))
        }
        Action::LeaveAlliance {
            operator_agent_id,
            alliance_id,
            member_agent_id,
        } => {
            if !source.agent_exists(operator_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.clone(),
                    },
                }));
            }
            if !source.agent_exists(member_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: member_agent_id.clone(),
                    },
                }));
            }
            source.charge(alliance_id.len())?;
            let alliance_id = alliance_id.trim();
            if alliance_id.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["alliance_id cannot be empty".to_string()],
                    },
                }));
            }
            let Some(alliance) = source.alliance(alliance_id)? else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("alliance not found: {alliance_id}")],
                    },
                }));
            };
            if !alliance
                .members
                .iter()
                .any(|member| member == operator_agent_id)
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "operator {} is not a member of alliance {}",
                            operator_agent_id, alliance_id
                        )],
                    },
                }));
            }
            if !alliance
                .members
                .iter()
                .any(|member| member == member_agent_id)
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "member {} is not in alliance {}",
                            member_agent_id, alliance_id
                        )],
                    },
                }));
            }
            if source.active_war(alliance_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "alliance {} has active war and cannot change members",
                            alliance_id
                        )],
                    },
                }));
            }
            if alliance.members.len() <= WAR_MIN_ALLIANCE_MEMBERS {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "alliance must keep at least {WAR_MIN_ALLIANCE_MEMBERS} members; use dissolve_alliance instead"
                        )],
                    },
                }));
            }
            Ok(WorldEventBody::Domain(DomainEvent::AllianceLeft {
                operator_agent_id: operator_agent_id.clone(),
                alliance_id: alliance_id.to_string(),
                member_agent_id: member_agent_id.clone(),
            }))
        }
        Action::DissolveAlliance {
            operator_agent_id,
            alliance_id,
            reason,
        } => {
            if !source.agent_exists(operator_agent_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: operator_agent_id.clone(),
                    },
                }));
            }
            source.charge(alliance_id.len())?;
            let alliance_id = alliance_id.trim();
            if alliance_id.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["alliance_id cannot be empty".to_string()],
                    },
                }));
            }
            let Some(alliance) = source.alliance(alliance_id)? else {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!("alliance not found: {alliance_id}")],
                    },
                }));
            };
            if !alliance
                .members
                .iter()
                .any(|member| member == operator_agent_id)
            {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "operator {} is not a member of alliance {}",
                            operator_agent_id, alliance_id
                        )],
                    },
                }));
            }
            if source.active_war(alliance_id)? {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "alliance {} has active war and cannot dissolve",
                            alliance_id
                        )],
                    },
                }));
            }
            source.charge(reason.len())?;
            let reason = reason.trim();
            if reason.is_empty() {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["dissolve reason cannot be empty".to_string()],
                    },
                }));
            }
            source.reserve_former_members(&alliance.members)?;
            Ok(WorldEventBody::Domain(DomainEvent::AllianceDissolved {
                operator_agent_id: operator_agent_id.clone(),
                alliance_id: alliance_id.to_string(),
                reason: reason.to_string(),
                former_members: alliance.members.clone(),
            }))
        }
        _ => unreachable!("unsupported variant reached its specialized dispatcher"),
    }
}
