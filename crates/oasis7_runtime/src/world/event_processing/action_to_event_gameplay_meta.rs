use super::*;

impl World {
    pub(super) fn resolve_crisis_action_to_event(
        &self,
        action_id: ActionId,
        resolver_agent_id: &str,
        crisis_id: &str,
        strategy: &str,
        success: bool,
    ) -> Result<WorldEventBody, WorldError> {
        if !self.state.agents.contains_key(resolver_agent_id) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::AgentNotFound {
                    agent_id: resolver_agent_id.to_string(),
                },
            }));
        }
        let crisis_id = crisis_id.trim();
        if crisis_id.is_empty() {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["crisis_id cannot be empty".to_string()],
                },
            }));
        }
        let Some(crisis) = self.state.crises.get(crisis_id) else {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("crisis not found: {crisis_id}")],
                },
            }));
        };
        if crisis.status != CrisisStatus::Active {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "crisis is not active and cannot be resolved: {}",
                        crisis_id
                    )],
                },
            }));
        }
        if self.state.time > crisis.expires_at {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "crisis expired at {} and cannot be resolved: {}",
                        crisis.expires_at, crisis_id
                    )],
                },
            }));
        }
        let strategy = strategy.trim();
        if strategy.is_empty() {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["crisis strategy cannot be empty".to_string()],
                },
            }));
        }
        let severity = crisis.severity.max(1);
        let impact = if success {
            i64::from(severity).saturating_mul(CRISIS_BASE_IMPACT_PER_SEVERITY)
        } else {
            -i64::from(severity).saturating_mul(CRISIS_BASE_IMPACT_PER_SEVERITY)
        };
        Ok(WorldEventBody::Domain(DomainEvent::CrisisResolved {
            resolver_agent_id: resolver_agent_id.to_string(),
            crisis_id: crisis_id.to_string(),
            strategy: strategy.to_string(),
            success,
            impact,
        }))
    }

    pub(super) fn grant_meta_progress_action_to_event(
        &self,
        action_id: ActionId,
        operator_agent_id: &str,
        target_agent_id: &str,
        track: &str,
        points: i64,
        achievement_id: Option<&str>,
    ) -> Result<WorldEventBody, WorldError> {
        if !self.state.agents.contains_key(operator_agent_id) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::AgentNotFound {
                    agent_id: operator_agent_id.to_string(),
                },
            }));
        }
        if !self.state.agents.contains_key(target_agent_id) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::AgentNotFound {
                    agent_id: target_agent_id.to_string(),
                },
            }));
        }
        let track = track.trim();
        if track.is_empty() {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["meta progression track cannot be empty".to_string()],
                },
            }));
        }
        if points == 0 {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::InvalidAmount { amount: points },
            }));
        }
        let normalized_achievement = achievement_id.map(str::trim);
        if normalized_achievement.is_some_and(|value| value.is_empty()) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["achievement_id cannot be empty".to_string()],
                },
            }));
        }
        Ok(WorldEventBody::Domain(DomainEvent::MetaProgressGranted {
            operator_agent_id: operator_agent_id.to_string(),
            target_agent_id: target_agent_id.to_string(),
            track: track.to_string(),
            points,
            achievement_id: normalized_achievement.map(str::to_string),
        }))
    }
}
