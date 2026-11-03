use super::*;

impl World {
    pub(super) fn action_to_event_gameplay_governance(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::OpenGovernanceProposal {
                proposer_agent_id,
                proposal_key,
                title,
                description,
                options,
                voting_window_ticks,
                quorum_weight,
                pass_threshold_bps,
            } => {
                if !self.state.agents.contains_key(proposer_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: proposer_agent_id.clone(),
                        },
                    }));
                }
                let proposal_key = proposal_key.trim();
                if proposal_key.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["proposal_key cannot be empty".to_string()],
                        },
                    }));
                }
                if self.state.governance_proposals.contains_key(proposal_key) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("proposal already exists: {proposal_key}")],
                        },
                    }));
                }
                let title = title.trim();
                if title.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["proposal title cannot be empty".to_string()],
                        },
                    }));
                }
                let description = description.trim();
                if description.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["proposal description cannot be empty".to_string()],
                        },
                    }));
                }
                if *voting_window_ticks < GOVERNANCE_MIN_VOTING_WINDOW_TICKS
                    || *voting_window_ticks > GOVERNANCE_MAX_VOTING_WINDOW_TICKS
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "voting_window_ticks must be within {}..={}",
                                GOVERNANCE_MIN_VOTING_WINDOW_TICKS,
                                GOVERNANCE_MAX_VOTING_WINDOW_TICKS
                            )],
                        },
                    }));
                }
                if *quorum_weight == 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["quorum_weight must be > 0".to_string()],
                        },
                    }));
                }
                if *pass_threshold_bps < GOVERNANCE_MIN_PASS_THRESHOLD_BPS
                    || *pass_threshold_bps > GOVERNANCE_MAX_PASS_THRESHOLD_BPS
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "pass_threshold_bps must be within {}..={}",
                                GOVERNANCE_MIN_PASS_THRESHOLD_BPS,
                                GOVERNANCE_MAX_PASS_THRESHOLD_BPS
                            )],
                        },
                    }));
                }
                let mut unique_options = BTreeSet::new();
                for value in options {
                    let normalized = value.trim();
                    if normalized.is_empty() {
                        continue;
                    }
                    unique_options.insert(normalized.to_string());
                }
                if unique_options.len() < 2 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "governance proposal requires at least 2 unique options"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                let closes_at = self.state.time.saturating_add(*voting_window_ticks);
                Ok(WorldEventBody::Domain(
                    DomainEvent::GovernanceProposalOpened {
                        proposer_agent_id: proposer_agent_id.clone(),
                        proposal_key: proposal_key.to_string(),
                        title: title.to_string(),
                        description: description.to_string(),
                        options: unique_options.into_iter().collect(),
                        voting_window_ticks: *voting_window_ticks,
                        closes_at,
                        quorum_weight: *quorum_weight,
                        pass_threshold_bps: *pass_threshold_bps,
                    },
                ))
            }
            Action::CastGovernanceVote {
                voter_agent_id,
                proposal_key,
                option,
                weight,
            } => {
                if !self.state.agents.contains_key(voter_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: voter_agent_id.clone(),
                        },
                    }));
                }
                let proposal_key = proposal_key.trim();
                if proposal_key.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["proposal_key cannot be empty".to_string()],
                        },
                    }));
                }
                let Some(proposal) = self.state.governance_proposals.get(proposal_key) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("governance proposal not found: {}", proposal_key)],
                        },
                    }));
                };
                if proposal.status != GovernanceProposalStatus::Open {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "governance proposal is not open: {}",
                                proposal_key
                            )],
                        },
                    }));
                }
                if self.state.time > proposal.closes_at {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "governance proposal has closed at {}",
                                proposal.closes_at
                            )],
                        },
                    }));
                }
                let option = option.trim();
                if option.is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["vote option cannot be empty".to_string()],
                        },
                    }));
                }
                if !proposal.options.iter().any(|candidate| candidate == option) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "vote option '{}' is not allowed for proposal {}",
                                option, proposal_key
                            )],
                        },
                    }));
                }
                if *weight == 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["vote weight must be > 0".to_string()],
                        },
                    }));
                }
                if *weight > GOVERNANCE_MAX_VOTE_WEIGHT {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "vote weight must be <= {GOVERNANCE_MAX_VOTE_WEIGHT}"
                            )],
                        },
                    }));
                }
                if let Err(WorldError::ResourceBalanceInvalid { reason }) = self
                    .state
                    .governance_effective_vote_weight_for_agent(proposal, voter_agent_id, *weight)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![reason],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::GovernanceVoteCast {
                    voter_agent_id: voter_agent_id.clone(),
                    proposal_key: proposal_key.to_string(),
                    option: option.to_string(),
                    weight: *weight,
                }))
            }
            Action::ResolveCrisis {
                resolver_agent_id,
                crisis_id,
                strategy,
                success,
            } => self.resolve_crisis_action_to_event(
                action_id,
                resolver_agent_id,
                crisis_id,
                strategy,
                *success,
            ),
            Action::GrantMetaProgress {
                operator_agent_id,
                target_agent_id,
                track,
                points,
                achievement_id,
            } => self.grant_meta_progress_action_to_event(
                action_id,
                operator_agent_id,
                target_agent_id,
                track,
                *points,
                achievement_id.as_deref(),
            ),
            _ => unreachable!("unsupported variant reached its specialized dispatcher"),
        }
    }
}
