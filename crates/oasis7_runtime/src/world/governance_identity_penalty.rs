use super::super::util::sha256_hex;
use super::super::{GovernanceEvent, GovernanceIdentityPenaltyStatus, WorldError, WorldEventBody};
use super::World;

impl World {
    #[expect(
        clippy::too_many_arguments,
        reason = "Identity penalty inputs are independently audited governance fields and retain their public API shape."
    )]
    pub fn apply_identity_penalty(
        &mut self,
        target_agent_id: impl Into<String>,
        evidence_hash: impl Into<String>,
        reason: impl Into<String>,
        slash_stake: u64,
        appeal_window_ticks: u64,
        initiator: impl Into<String>,
        signer_node_ids: Vec<String>,
    ) -> Result<u64, WorldError> {
        let target_agent_id = target_agent_id.into();
        if !self.state.agents.contains_key(target_agent_id.as_str()) {
            return Err(WorldError::AgentNotFound {
                agent_id: target_agent_id.clone(),
            });
        }
        let evidence_hash = evidence_hash.into();
        Self::validate_governance_identity_evidence_hash(evidence_hash.as_str())?;
        let reason = reason.into();
        Self::validate_governance_identity_field("identity penalty reason", reason.as_str())?;
        let initiator = initiator.into();
        Self::validate_governance_identity_field("identity penalty initiator", initiator.as_str())?;
        if appeal_window_ticks == 0 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "identity penalty appeal_window_ticks must be > 0".to_string(),
            });
        }
        let threshold = self
            .governance_execution_policy
            .emergency_veto_guardian_threshold;
        let signer_node_ids = self.validate_guardian_signers(&signer_node_ids, threshold)?;
        let penalty_id = self.next_governance_identity_penalty_id.max(1);
        let event = GovernanceEvent::IdentityPenaltyApplied {
            penalty_id,
            target_agent_id,
            evidence_hash,
            initiator,
            reason,
            slash_stake,
            appeal_deadline_tick: self.state.time.saturating_add(appeal_window_ticks),
            threshold,
            signer_node_ids,
        };
        self.append_event(WorldEventBody::Governance(event), None)?;
        Ok(penalty_id)
    }

    pub fn appeal_identity_penalty(
        &mut self,
        penalty_id: u64,
        appellant: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<(), WorldError> {
        let appellant = appellant.into();
        Self::validate_governance_identity_field(
            "identity penalty appeal appellant",
            appellant.as_str(),
        )?;
        let reason = reason.into();
        Self::validate_governance_identity_field(
            "identity penalty appeal reason",
            reason.as_str(),
        )?;
        let Some(penalty) = self.governance_identity_penalties.get(&penalty_id) else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("identity penalty not found: penalty_id={penalty_id}"),
            });
        };
        if penalty.status != GovernanceIdentityPenaltyStatus::Applied {
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
        let event = GovernanceEvent::IdentityPenaltyAppealed {
            penalty_id,
            appellant,
            reason,
        };
        self.append_event(WorldEventBody::Governance(event), None)?;
        Ok(())
    }

    pub fn resolve_identity_penalty_appeal(
        &mut self,
        penalty_id: u64,
        resolver: impl Into<String>,
        accepted: bool,
        reason: impl Into<String>,
    ) -> Result<(), WorldError> {
        let resolver = resolver.into();
        Self::validate_governance_identity_field(
            "identity penalty appeal resolver",
            resolver.as_str(),
        )?;
        let reason = reason.into();
        Self::validate_governance_identity_field(
            "identity penalty appeal resolution",
            reason.as_str(),
        )?;
        let Some(penalty) = self.governance_identity_penalties.get(&penalty_id) else {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("identity penalty not found: penalty_id={penalty_id}"),
            });
        };
        if penalty.status != GovernanceIdentityPenaltyStatus::Appealed {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!(
                    "identity penalty appeal is not pending: penalty_id={} status={:?}",
                    penalty_id, penalty.status
                ),
            });
        }
        let event = GovernanceEvent::IdentityPenaltyResolved {
            penalty_id,
            resolver,
            accepted,
            reason,
        };
        self.append_event(WorldEventBody::Governance(event), None)?;
        Ok(())
    }

    pub(super) fn validate_governance_identity_field(
        label: &str,
        value: &str,
    ) -> Result<(), WorldError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("{label} cannot be empty"),
            });
        }
        if trimmed.len() > 512 {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: format!("{label} exceeds max length"),
            });
        }
        Ok(())
    }

    pub(super) fn validate_governance_identity_evidence_hash(
        evidence_hash: &str,
    ) -> Result<(), WorldError> {
        let trimmed = evidence_hash.trim();
        if trimmed.is_empty() {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "identity penalty evidence_hash cannot be empty".to_string(),
            });
        }
        if trimmed.chars().any(|ch| ch.is_whitespace()) {
            return Err(WorldError::GovernancePolicyInvalid {
                reason: "identity penalty evidence_hash cannot contain whitespace".to_string(),
            });
        }
        Ok(())
    }

    pub(super) fn build_identity_penalty_incident_id(
        target_agent_id: &str,
        evidence_hash: &str,
    ) -> String {
        sha256_hex(format!("identity-incident-v1|{target_agent_id}|{evidence_hash}").as_bytes())
    }

    pub(super) fn build_identity_penalty_chain_hash(
        penalty_id: u64,
        target_agent_id: &str,
        evidence_hash: &str,
        reason: &str,
        incident_id: &str,
    ) -> String {
        sha256_hex(
            format!(
                "identity-chain-v1|{penalty_id}|{target_agent_id}|{evidence_hash}|{reason}|{incident_id}"
            )
            .as_bytes(),
        )
    }

    pub(super) fn build_identity_penalty_stage_evidence_hash(
        stage: &str,
        actor: &str,
        reason: &str,
    ) -> String {
        sha256_hex(format!("identity-stage-v1|{stage}|{actor}|{reason}").as_bytes())
    }

    pub(super) fn extend_identity_penalty_chain_hash(
        base_chain_hash: &str,
        stage: &str,
        stage_evidence_hash: &str,
    ) -> String {
        sha256_hex(
            format!("identity-chain-v1|{base_chain_hash}|{stage}|{stage_evidence_hash}").as_bytes(),
        )
    }
}
