use super::*;

impl World {
    pub(super) fn action_to_event_gameplay_alliances(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::FormAlliance {
                proposer_agent_id,
                alliance_id,
                members,
                charter,
            } => {
                if !self.state.agents.contains_key(proposer_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: proposer_agent_id.clone(),
                        },
                    }));
                }
                let alliance_id = alliance_id.trim();
                if alliance_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["alliance_id cannot be empty".to_string()],
                        },
                    }));
                }
                if self.state.alliances.contains_key(alliance_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("alliance already exists: {alliance_id}")],
                        },
                    }));
                }
                let charter = charter.trim();
                if charter.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["alliance charter cannot be empty".to_string()],
                        },
                    }));
                }

                let mut member_set = BTreeSet::new();
                member_set.insert(proposer_agent_id.trim().to_string());
                for member in members {
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
                    if !self.state.agents.contains_key(member) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: member.clone(),
                            },
                        }));
                    }
                    if let Some(current_alliance_id) = self.agent_alliance_id(member.as_str()) {
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
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(member_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: member_agent_id.clone(),
                        },
                    }));
                }
                let alliance_id = alliance_id.trim();
                if alliance_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["alliance_id cannot be empty".to_string()],
                        },
                    }));
                }
                let Some(alliance) = self.state.alliances.get(alliance_id) else {
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
                if let Some(current_alliance_id) = self.agent_alliance_id(member_agent_id) {
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
                if self.alliance_has_active_war(alliance_id) {
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
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(member_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: member_agent_id.clone(),
                        },
                    }));
                }
                let alliance_id = alliance_id.trim();
                if alliance_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["alliance_id cannot be empty".to_string()],
                        },
                    }));
                }
                let Some(alliance) = self.state.alliances.get(alliance_id) else {
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
                if self.alliance_has_active_war(alliance_id) {
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
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                let alliance_id = alliance_id.trim();
                if alliance_id.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["alliance_id cannot be empty".to_string()],
                        },
                    }));
                }
                let Some(alliance) = self.state.alliances.get(alliance_id) else {
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
                if self.alliance_has_active_war(alliance_id) {
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
                let reason = reason.trim();
                if reason.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["dissolve reason cannot be empty".to_string()],
                        },
                    }));
                }
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
}
