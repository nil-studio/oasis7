use super::super::super::{
    GovernanceEvent, GovernanceFinalityEpochSnapshot, GovernanceIdentityPenaltyRecord,
    GovernanceIdentityProfileState, Proposal, ProposalDecision, ProposalId, ProposalStatus,
    TickConsensusRecord, WorldError, WorldEvent, WorldEventBody, WorldEventId,
};
use super::super::World;
use super::PreparedEventStateDelta;

pub(crate) struct PreparedGovernanceApprovalPublication {
    proposal_id: ProposalId,
    proposal: Proposal,
    events: Vec<WorldEvent>,
    next_event_id: WorldEventId,
    next_event_id_era: u64,
    journal_events: Vec<WorldEvent>,
    journal_events_evicted: u64,
    consensus_record: TickConsensusRecord,
}

impl PreparedGovernanceApprovalPublication {
    fn install(self, world: &mut World) -> Result<(), WorldError> {
        let PreparedGovernanceApprovalPublication {
            proposal_id,
            proposal,
            events,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted,
            consensus_record,
        } = self;
        world.install_prepared_tick_consensus_record(consensus_record)?;
        world.proposals.insert(proposal_id, proposal);
        if let Some(event) = events.last() {
            world.state.time = event.time;
        }
        world.next_event_id = next_event_id;
        world.next_event_id_era = next_event_id_era;
        world.journal.events = journal_events;
        world.runtime_backpressure_stats.journal_events_evicted = world
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(journal_events_evicted);
        Ok(())
    }
}

pub(super) fn prepare(
    world: &World,
    body: &WorldEventBody,
) -> Result<Option<PreparedEventStateDelta>, WorldError> {
    match body {
        WorldEventBody::Governance(
            event @ GovernanceEvent::Approved {
                proposal_id,
                approver,
                decision,
            },
        ) => {
            let proposal =
                world
                    .proposals
                    .get(proposal_id)
                    .ok_or(WorldError::ProposalNotFound {
                        proposal_id: *proposal_id,
                    })?;
            let next = prepare_approved_proposal(proposal, *proposal_id, approver, decision)?;
            Ok(Some(PreparedEventStateDelta::GovernanceProposalStatus {
                event: event.clone(),
                proposal_id: *proposal_id,
                next,
            }))
        }
        WorldEventBody::Governance(
            event @ GovernanceEvent::Queued {
                proposal_id,
                manifest_hash,
                queued_at_tick,
                not_before_tick,
                activate_epoch,
                timelock_ticks,
            },
        ) => {
            let proposal =
                world
                    .proposals
                    .get(proposal_id)
                    .ok_or(WorldError::ProposalNotFound {
                        proposal_id: *proposal_id,
                    })?;
            let next = prepare_queued_proposal(
                proposal,
                *proposal_id,
                manifest_hash,
                *queued_at_tick,
                *not_before_tick,
                *activate_epoch,
                *timelock_ticks,
            )?;
            Ok(Some(PreparedEventStateDelta::GovernanceProposalStatus {
                event: event.clone(),
                proposal_id: *proposal_id,
                next,
            }))
        }
        WorldEventBody::Governance(
            event @ GovernanceEvent::Applied {
                proposal_id,
                manifest_hash,
                ..
            },
        ) => {
            let proposal =
                world
                    .proposals
                    .get(proposal_id)
                    .ok_or(WorldError::ProposalNotFound {
                        proposal_id: *proposal_id,
                    })?;
            let next = prepare_applied_proposal(proposal, *proposal_id, manifest_hash)?;
            Ok(Some(PreparedEventStateDelta::GovernanceProposalStatus {
                event: event.clone(),
                proposal_id: *proposal_id,
                next,
            }))
        }
        WorldEventBody::Governance(GovernanceEvent::Proposed {
            proposal_id,
            author,
            base_manifest_hash,
            manifest,
            patch,
        }) => {
            let (next, next_proposal_id, next_proposal_id_era) = world.prepare_governance_proposal(
                *proposal_id,
                author,
                base_manifest_hash,
                manifest,
                patch,
            );
            Ok(Some(PreparedEventStateDelta::GovernanceProposal {
                proposal_id: *proposal_id,
                next,
                next_proposal_id,
                next_proposal_id_era,
            }))
        }
        WorldEventBody::Governance(GovernanceEvent::ShadowReport {
            proposal_id,
            manifest_hash,
        }) => {
            let next = world.prepare_governance_proposal_shadow(*proposal_id, manifest_hash)?;
            Ok(Some(PreparedEventStateDelta::GovernanceProposalShadow {
                proposal_id: *proposal_id,
                next,
            }))
        }
        WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyApplied {
            penalty_id,
            target_agent_id,
            evidence_hash,
            initiator,
            reason,
            slash_stake,
            appeal_deadline_tick,
            threshold,
            signer_node_ids,
        }) => {
            let (target_agent_id, next, next_profile, profile_was_present, next_penalty_id) = world
                .prepare_governance_identity_penalty_application(
                    *penalty_id,
                    target_agent_id,
                    evidence_hash,
                    initiator,
                    reason,
                    *slash_stake,
                    *appeal_deadline_tick,
                    *threshold,
                    signer_node_ids,
                )?;
            Ok(Some(
                PreparedEventStateDelta::GovernanceIdentityPenaltyApplication {
                    penalty_id: *penalty_id,
                    target_agent_id,
                    next,
                    next_profile,
                    profile_was_present,
                    next_penalty_id,
                },
            ))
        }
        WorldEventBody::Governance(GovernanceEvent::FinalityEpochSnapshotSet {
            snapshot,
            previous,
        }) => {
            world.validate_governance_finality_epoch_snapshot_set(snapshot, previous)?;
            Ok(Some(
                PreparedEventStateDelta::GovernanceFinalityEpochSnapshot {
                    epoch_id: snapshot.epoch_id,
                    next: Some(snapshot.clone()),
                },
            ))
        }
        WorldEventBody::Governance(GovernanceEvent::FinalityEpochSnapshotRemoved {
            epoch_id,
            snapshot,
        }) => {
            world.validate_governance_finality_epoch_snapshot_removal(*epoch_id, snapshot)?;
            Ok(Some(
                PreparedEventStateDelta::GovernanceFinalityEpochSnapshot {
                    epoch_id: *epoch_id,
                    next: None,
                },
            ))
        }
        WorldEventBody::Governance(GovernanceEvent::EmergencyVetoed {
            proposal_id,
            reason,
            threshold,
            signer_node_ids,
            ..
        }) => {
            let next = world.prepare_governance_emergency_veto(
                *proposal_id,
                reason,
                *threshold,
                signer_node_ids,
            )?;
            Ok(Some(PreparedEventStateDelta::GovernanceEmergencyVeto {
                proposal_id: *proposal_id,
                next,
            }))
        }
        WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyAppealed {
            penalty_id,
            appellant,
            reason,
        }) => {
            let next =
                world.prepare_governance_identity_penalty_appeal(*penalty_id, appellant, reason)?;
            Ok(Some(
                PreparedEventStateDelta::GovernanceIdentityPenaltyAppeal {
                    penalty_id: *penalty_id,
                    next,
                },
            ))
        }
        WorldEventBody::Governance(GovernanceEvent::IdentityPenaltyResolved {
            penalty_id,
            resolver,
            accepted,
            reason,
        }) => {
            let (target_agent_id, next, next_profile) = world
                .prepare_governance_identity_penalty_resolution(
                    *penalty_id,
                    resolver,
                    *accepted,
                    reason,
                )?;
            Ok(Some(
                PreparedEventStateDelta::GovernanceIdentityPenaltyResolution {
                    penalty_id: *penalty_id,
                    target_agent_id,
                    next,
                    next_profile,
                },
            ))
        }
        _ => Ok(None),
    }
}

pub(crate) fn prepare_applied_proposal(
    proposal: &Proposal,
    proposal_id: ProposalId,
    manifest_hash: &Option<String>,
) -> Result<Proposal, WorldError> {
    let ProposalStatus::Approved {
        manifest_hash: approved_hash,
        ..
    } = &proposal.status
    else {
        return Err(WorldError::ProposalInvalidState {
            proposal_id,
            expected: "approved".to_string(),
            found: proposal.status.label(),
        });
    };
    let mut next = proposal.clone();
    next.status = ProposalStatus::Applied {
        manifest_hash: manifest_hash
            .clone()
            .unwrap_or_else(|| approved_hash.clone()),
    };
    Ok(next)
}

pub(crate) fn prepare_approved_proposal(
    proposal: &Proposal,
    proposal_id: ProposalId,
    approver: &str,
    decision: &ProposalDecision,
) -> Result<Proposal, WorldError> {
    let mut next = proposal.clone();
    match decision {
        ProposalDecision::Approve => {
            let ProposalStatus::Shadowed { manifest_hash } = &proposal.status else {
                return Err(WorldError::ProposalInvalidState {
                    proposal_id,
                    expected: "shadowed".to_string(),
                    found: proposal.status.label(),
                });
            };
            next.status = ProposalStatus::Approved {
                manifest_hash: manifest_hash.clone(),
                approver: approver.to_string(),
            };
        }
        ProposalDecision::Reject { reason } => {
            next.queued_at_tick = None;
            next.not_before_tick = None;
            next.activate_epoch = None;
            next.timelock_ticks = 0;
            next.status = ProposalStatus::Rejected {
                reason: reason.clone(),
            };
        }
    }
    Ok(next)
}

pub(crate) fn prepare_queued_proposal(
    proposal: &Proposal,
    proposal_id: ProposalId,
    manifest_hash: &str,
    queued_at_tick: u64,
    not_before_tick: u64,
    activate_epoch: u64,
    timelock_ticks: u64,
) -> Result<Proposal, WorldError> {
    let ProposalStatus::Approved {
        manifest_hash: approved_hash,
        ..
    } = &proposal.status
    else {
        return Err(WorldError::ProposalInvalidState {
            proposal_id,
            expected: "approved".to_string(),
            found: proposal.status.label(),
        });
    };
    if approved_hash != manifest_hash {
        return Err(WorldError::GovernancePolicyInvalid {
            reason: format!(
                "queued manifest hash drift: proposal_id={} approved={} queued={}",
                proposal_id, approved_hash, manifest_hash
            ),
        });
    }
    if not_before_tick < queued_at_tick {
        return Err(WorldError::GovernancePolicyInvalid {
            reason: format!(
                "invalid queued timeline: proposal_id={} queued_at={} not_before={}",
                proposal_id, queued_at_tick, not_before_tick
            ),
        });
    }
    let mut next = proposal.clone();
    next.queued_at_tick = Some(queued_at_tick);
    next.not_before_tick = Some(not_before_tick);
    next.activate_epoch = Some(activate_epoch);
    next.timelock_ticks = timelock_ticks;
    Ok(next)
}

impl World {
    pub(super) fn prepare_governance_proposal(
        &self,
        proposal_id: ProposalId,
        author: &str,
        base_manifest_hash: &str,
        manifest: &super::super::super::Manifest,
        patch: &Option<super::super::super::ManifestPatch>,
    ) -> (Proposal, ProposalId, u64) {
        let proposal = Proposal {
            id: proposal_id,
            author: author.to_string(),
            base_manifest_hash: base_manifest_hash.to_string(),
            manifest: manifest.clone(),
            patch: patch.clone(),
            queued_at_tick: None,
            not_before_tick: None,
            activate_epoch: None,
            timelock_ticks: 0,
            status: ProposalStatus::Proposed,
        };
        let (allocated, next_proposal_id, next_proposal_id_era) =
            Self::preview_next_proposal_id(self.next_proposal_id, self.next_proposal_id_era);
        let (next_proposal_id, next_proposal_id_era) = if allocated == proposal_id {
            (next_proposal_id, next_proposal_id_era)
        } else {
            (
                self.next_proposal_id.max(proposal_id.saturating_add(1)),
                self.next_proposal_id_era,
            )
        };
        (proposal, next_proposal_id, next_proposal_id_era)
    }

    pub(super) fn prepare_governance_proposal_shadow(
        &self,
        proposal_id: ProposalId,
        manifest_hash: &str,
    ) -> Result<Proposal, WorldError> {
        let mut proposal = self
            .proposals
            .get(&proposal_id)
            .cloned()
            .ok_or(WorldError::ProposalNotFound { proposal_id })?;
        proposal.status = ProposalStatus::Shadowed {
            manifest_hash: manifest_hash.to_string(),
        };
        Ok(proposal)
    }

    pub(crate) fn append_prepared_governance_approval(
        &mut self,
        approved_event: GovernanceEvent,
        queued_event: Option<GovernanceEvent>,
    ) -> Result<(), WorldError> {
        let prepared = self.prepare_governance_approval(approved_event, queued_event)?;
        if self.take_fail_next_append_after_publication_prepare_for_test() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "injected append_event failure after publication preparation".to_string(),
            });
        }
        prepared.install(self)?;
        Ok(())
    }

    fn prepare_governance_approval(
        &self,
        approved_event: GovernanceEvent,
        queued_event: Option<GovernanceEvent>,
    ) -> Result<PreparedGovernanceApprovalPublication, WorldError> {
        let (proposal_id, approver, decision) = match &approved_event {
            GovernanceEvent::Approved {
                proposal_id,
                approver,
                decision,
            } => (*proposal_id, approver.as_str(), decision),
            _ => {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "prepared governance approval requires Approved event".to_string(),
                });
            }
        };
        let proposal = self
            .proposals
            .get(&proposal_id)
            .ok_or(WorldError::ProposalNotFound { proposal_id })?;
        let approved = prepare_approved_proposal(proposal, proposal_id, approver, decision)?;
        let final_proposal = match queued_event.as_ref() {
            Some(GovernanceEvent::Queued {
                proposal_id: queued_proposal_id,
                manifest_hash,
                queued_at_tick,
                not_before_tick,
                activate_epoch,
                timelock_ticks,
            }) if *queued_proposal_id == proposal_id => prepare_queued_proposal(
                &approved,
                proposal_id,
                manifest_hash,
                *queued_at_tick,
                *not_before_tick,
                *activate_epoch,
                *timelock_ticks,
            )?,
            Some(_) => {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: "prepared governance queue does not match Approved event".to_string(),
                });
            }
            None => approved,
        };

        let (approved_event_id, next_event_id, next_event_id_era) =
            Self::preview_next_event_id(self.next_event_id, self.next_event_id_era);
        let mut events = vec![WorldEvent {
            id: approved_event_id,
            time: self.state.time,
            caused_by: None,
            body: WorldEventBody::Governance(approved_event),
        }];
        let (next_event_id, next_event_id_era) = if let Some(queued_event) = queued_event {
            let (queued_event_id, next_event_id, next_event_id_era) =
                Self::preview_next_event_id(next_event_id, next_event_id_era);
            events.push(WorldEvent {
                id: queued_event_id,
                time: self.state.time,
                caused_by: None,
                body: WorldEventBody::Governance(queued_event),
            });
            (next_event_id, next_event_id_era)
        } else {
            (next_event_id, next_event_id_era)
        };

        let mut journal_events = self.journal.events.clone();
        journal_events.extend(events.iter().cloned());
        let max_len = self.runtime_memory_limits.max_journal_events.max(1);
        let overflow = journal_events.len().saturating_sub(max_len);
        if overflow > 0 {
            journal_events.drain(0..overflow);
        }
        let tick_events: Vec<WorldEvent> = journal_events
            .iter()
            .filter(|journal_event| journal_event.time == self.state.time)
            .cloned()
            .collect();
        let state_root = self.current_state_root_hash()?;
        let consensus_record = self.build_tick_consensus_record_for_prepared_events(
            self.state.time,
            tick_events.as_slice(),
            state_root.clone(),
        )?;
        self.validate_tick_consensus_candidate_for_prepared_publication(
            &consensus_record,
            tick_events.as_slice(),
            state_root.as_str(),
        )?;

        Ok(PreparedGovernanceApprovalPublication {
            proposal_id,
            proposal: final_proposal,
            events,
            next_event_id,
            next_event_id_era,
            journal_events,
            journal_events_evicted: overflow as u64,
            consensus_record,
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Governance penalty fields are independently signed/state-machine inputs and remain explicit for compatibility."
    )]
    pub(crate) fn prepare_governance_identity_penalty_application(
        &self,
        penalty_id: u64,
        target_agent_id: &str,
        evidence_hash: &str,
        initiator: &str,
        reason: &str,
        slash_stake: u64,
        appeal_deadline_tick: u64,
        threshold: u16,
        signer_node_ids: &[String],
    ) -> Result<
        (
            String,
            GovernanceIdentityPenaltyRecord,
            GovernanceIdentityProfileState,
            bool,
            u64,
        ),
        WorldError,
    > {
        self.validate_guardian_signers(signer_node_ids, threshold)?;
        if !self.state.agents.contains_key(target_agent_id) {
            return Err(WorldError::AgentNotFound {
                agent_id: target_agent_id.to_string(),
            });
        }
        Self::validate_governance_identity_evidence_hash(evidence_hash)?;
        Self::validate_governance_identity_field("identity penalty reason", reason)?;
        Self::validate_governance_identity_field("identity penalty initiator", initiator)?;
        if self.governance_identity_penalties.contains_key(&penalty_id) {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("duplicate identity penalty id: penalty_id={penalty_id}"),
            });
        }
        let detection_incident_id =
            Self::build_identity_penalty_incident_id(target_agent_id, evidence_hash);
        if self
            .governance_identity_penalties
            .values()
            .any(|record| record.detection_incident_id == detection_incident_id)
        {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "duplicate identity penalty incident: incident_id={detection_incident_id}"
                ),
            });
        }
        let detection_risk_score = self
            .threat_heatmap
            .get(target_agent_id)
            .copied()
            .unwrap_or_default();
        let evidence_chain_hash = Self::build_identity_penalty_chain_hash(
            penalty_id,
            target_agent_id,
            evidence_hash,
            reason,
            detection_incident_id.as_str(),
        );
        let profile_was_present = self
            .state
            .governance_identity_profiles
            .contains_key(target_agent_id);
        let mut profile = self
            .state
            .governance_identity_profiles
            .get(target_agent_id)
            .cloned()
            .unwrap_or_else(|| GovernanceIdentityProfileState {
                agent_id: target_agent_id.to_string(),
                ..GovernanceIdentityProfileState::default()
            });
        if slash_stake > profile.stake_locked {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "identity penalty slash exceeds locked stake: penalty_id={} slash={} stake_locked={}",
                    penalty_id, slash_stake, profile.stake_locked
                ),
            });
        }
        let identity_status_before = profile.status;
        profile.stake_locked = profile.stake_locked.saturating_sub(slash_stake);
        profile.status = super::super::super::GovernanceIdentityStatus::Frozen;
        profile.slash_count = profile.slash_count.saturating_add(1);
        profile.updated_at = self.state.time;
        let next = GovernanceIdentityPenaltyRecord {
            penalty_id,
            target_agent_id: target_agent_id.to_string(),
            evidence_hash: evidence_hash.to_string(),
            reason: reason.to_string(),
            slash_stake,
            appeal_deadline_tick,
            status: super::super::super::GovernanceIdentityPenaltyStatus::Applied,
            identity_status_before,
            detection_source: "world.threat_heatmap.v1".to_string(),
            detection_risk_score,
            detection_incident_id,
            evidence_chain_hash,
            appeal_evidence_hash: None,
            resolution_evidence_hash: None,
            appellant: None,
            appeal_reason: None,
            resolved_by: None,
            resolution_reason: None,
            resolved_at_tick: None,
        };
        let next_penalty_id = self
            .next_governance_identity_penalty_id
            .max(penalty_id.saturating_add(1));
        Ok((
            target_agent_id.to_string(),
            next,
            profile,
            profile_was_present,
            next_penalty_id,
        ))
    }

    pub(crate) fn prepare_governance_identity_penalty_appeal(
        &self,
        penalty_id: u64,
        appellant: &str,
        reason: &str,
    ) -> Result<GovernanceIdentityPenaltyRecord, WorldError> {
        Self::validate_governance_identity_field("identity penalty appeal appellant", appellant)?;
        Self::validate_governance_identity_field("identity penalty appeal reason", reason)?;
        let appeal_evidence_hash =
            Self::build_identity_penalty_stage_evidence_hash("appeal", appellant, reason);
        let mut penalty = self
            .governance_identity_penalties
            .get(&penalty_id)
            .cloned()
            .ok_or(WorldError::GovernancePolicyInvalid {
                reason: format!("identity penalty not found: penalty_id={penalty_id}"),
            })?;
        if penalty.status != super::super::super::GovernanceIdentityPenaltyStatus::Applied {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "identity penalty is not appealable: penalty_id={} status={:?}",
                    penalty_id, penalty.status
                ),
            });
        }
        if self.state.time > penalty.appeal_deadline_tick {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "identity penalty appeal window closed: penalty_id={} deadline_tick={}",
                    penalty_id, penalty.appeal_deadline_tick
                ),
            });
        }
        if penalty.detection_source.trim().is_empty() {
            penalty.detection_source = "world.threat_heatmap.v1".to_string();
        }
        if penalty.detection_incident_id.trim().is_empty() {
            penalty.detection_incident_id = Self::build_identity_penalty_incident_id(
                penalty.target_agent_id.as_str(),
                penalty.evidence_hash.as_str(),
            );
        }
        if penalty.evidence_chain_hash.trim().is_empty() {
            penalty.evidence_chain_hash = Self::build_identity_penalty_chain_hash(
                penalty.penalty_id,
                penalty.target_agent_id.as_str(),
                penalty.evidence_hash.as_str(),
                penalty.reason.as_str(),
                penalty.detection_incident_id.as_str(),
            );
        }
        penalty.status = super::super::super::GovernanceIdentityPenaltyStatus::Appealed;
        penalty.appellant = Some(appellant.to_string());
        penalty.appeal_reason = Some(reason.to_string());
        penalty.appeal_evidence_hash = Some(appeal_evidence_hash.clone());
        penalty.evidence_chain_hash = Self::extend_identity_penalty_chain_hash(
            penalty.evidence_chain_hash.as_str(),
            "appeal",
            appeal_evidence_hash.as_str(),
        );
        Ok(penalty)
    }

    pub(crate) fn prepare_governance_identity_penalty_resolution(
        &self,
        penalty_id: u64,
        resolver: &str,
        accepted: bool,
        reason: &str,
    ) -> Result<
        (
            String,
            GovernanceIdentityPenaltyRecord,
            GovernanceIdentityProfileState,
        ),
        WorldError,
    > {
        Self::validate_governance_identity_field("identity penalty appeal resolver", resolver)?;
        Self::validate_governance_identity_field("identity penalty appeal resolution", reason)?;
        let resolution_evidence_hash = Self::build_identity_penalty_stage_evidence_hash(
            if accepted {
                "resolve_accept"
            } else {
                "resolve_reject"
            },
            resolver,
            reason,
        );
        let mut penalty = self
            .governance_identity_penalties
            .get(&penalty_id)
            .cloned()
            .ok_or(WorldError::GovernancePolicyInvalid {
                reason: format!("identity penalty not found: penalty_id={penalty_id}"),
            })?;
        if penalty.status != super::super::super::GovernanceIdentityPenaltyStatus::Appealed {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "identity penalty appeal is not pending: penalty_id={} status={:?}",
                    penalty_id, penalty.status
                ),
            });
        }
        let target_agent_id = penalty.target_agent_id.clone();
        let mut profile = self
            .state
            .governance_identity_profiles
            .get(target_agent_id.as_str())
            .cloned()
            .ok_or(WorldError::AgentNotFound {
                agent_id: target_agent_id.clone(),
            })?;
        if penalty.detection_source.trim().is_empty() {
            penalty.detection_source = "world.threat_heatmap.v1".to_string();
        }
        if penalty.detection_incident_id.trim().is_empty() {
            penalty.detection_incident_id = Self::build_identity_penalty_incident_id(
                penalty.target_agent_id.as_str(),
                penalty.evidence_hash.as_str(),
            );
        }
        if penalty.evidence_chain_hash.trim().is_empty() {
            penalty.evidence_chain_hash = Self::build_identity_penalty_chain_hash(
                penalty.penalty_id,
                penalty.target_agent_id.as_str(),
                penalty.evidence_hash.as_str(),
                penalty.reason.as_str(),
                penalty.detection_incident_id.as_str(),
            );
        }
        penalty.status = if accepted {
            super::super::super::GovernanceIdentityPenaltyStatus::AppealAccepted
        } else {
            super::super::super::GovernanceIdentityPenaltyStatus::AppealRejected
        };
        penalty.resolved_by = Some(resolver.to_string());
        penalty.resolution_reason = Some(reason.to_string());
        penalty.resolved_at_tick = Some(self.state.time);
        penalty.resolution_evidence_hash = Some(resolution_evidence_hash.clone());
        penalty.evidence_chain_hash = Self::extend_identity_penalty_chain_hash(
            penalty.evidence_chain_hash.as_str(),
            "resolve",
            resolution_evidence_hash.as_str(),
        );
        if accepted {
            profile.stake_locked = profile.stake_locked.saturating_add(penalty.slash_stake);
            profile.status = penalty.identity_status_before;
        }
        profile.updated_at = self.state.time;
        Ok((target_agent_id, penalty, profile))
    }

    pub(crate) fn prepare_governance_emergency_veto(
        &self,
        proposal_id: ProposalId,
        reason: &str,
        threshold: u16,
        signer_node_ids: &[String],
    ) -> Result<Proposal, WorldError> {
        self.validate_guardian_signers(signer_node_ids, threshold)?;
        let mut proposal = self
            .proposals
            .get(&proposal_id)
            .cloned()
            .ok_or(WorldError::ProposalNotFound { proposal_id })?;
        if !matches!(proposal.status, ProposalStatus::Approved { .. }) {
            return Err(WorldError::ProposalInvalidState {
                proposal_id,
                expected: "approved".to_string(),
                found: proposal.status.label(),
            });
        }
        if proposal.not_before_tick.is_none() || proposal.activate_epoch.is_none() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("proposal_id={} is not queued for activation", proposal_id),
            });
        }
        proposal.queued_at_tick = None;
        proposal.not_before_tick = None;
        proposal.activate_epoch = None;
        proposal.timelock_ticks = 0;
        proposal.status = ProposalStatus::Rejected {
            reason: format!("emergency_veto: {reason}"),
        };
        Ok(proposal)
    }

    pub(crate) fn validate_governance_finality_epoch_snapshot_set(
        &self,
        snapshot: &GovernanceFinalityEpochSnapshot,
        previous: &Option<GovernanceFinalityEpochSnapshot>,
    ) -> Result<(), WorldError> {
        let current = self
            .governance_finality_epoch_snapshots
            .get(&snapshot.epoch_id)
            .cloned();
        if current != *previous {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "governance finality snapshot predecessor drift: epoch_id={}",
                    snapshot.epoch_id
                ),
            });
        }
        let mut normalized = snapshot.clone();
        self.normalize_governance_finality_epoch_snapshot(&mut normalized)?;
        if normalized != *snapshot {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "governance finality snapshot normalization drift: epoch_id={}",
                    snapshot.epoch_id
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn validate_governance_finality_epoch_snapshot_removal(
        &self,
        epoch_id: u64,
        snapshot: &GovernanceFinalityEpochSnapshot,
    ) -> Result<(), WorldError> {
        if snapshot.epoch_id != epoch_id
            || self.governance_finality_epoch_snapshots.get(&epoch_id) != Some(snapshot)
        {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("governance finality snapshot removal drift: epoch_id={epoch_id}"),
            });
        }
        Ok(())
    }
}
