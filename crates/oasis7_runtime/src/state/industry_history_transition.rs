use super::*;
use serde::ser::SerializeStruct;

#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "Prepared events carry stable persistence payloads; boxing a variant would change the internal replay handoff shape."
)]
pub(crate) enum PreparedIndustryHistoryEvent {
    AgentLocation(DomainEvent, String, AgentLocationAuthorityV1),
    LocationAnchor(DomainEvent, String, LocationAnchorV1),
    FactorySite(DomainEvent, String, FactorySiteAuthorityV1),
    ConstructionPower(DomainEvent, String, FactoryConstructionPowerProfileV1),
    ValidationReceipt {
        event: DomainEvent,
        job_id: ActionId,
        receipts: Vec<ProductValidationReceiptV1>,
        latest: Option<LastProductValidationState>,
        agent: Option<(String, AgentCell)>,
    },
    ValidationAttempt(DomainEvent, ActionId, Vec<ProductValidationAttemptV1>),
}

impl PreparedIndustryHistoryEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        match event {
            DomainEvent::AgentLocationAuthorityUpdated { authority } => {
                if authority.agent_id.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "agent_id cannot be empty".into(),
                    });
                }
                if authority.location_id.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "location_id cannot be empty".into(),
                    });
                }
                if !state.agents.contains_key(&authority.agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: authority.agent_id.clone(),
                    });
                }
                if authority.effective_at > now {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "agent location authority not yet effective: agent_id={} effective_at={} now={now}",
                            authority.agent_id, authority.effective_at
                        ),
                    });
                }
                if authority.active {
                    super::factory_authority::require_active_location_anchor(
                        &state.location_anchors,
                        &authority.location_id,
                        now,
                    )?;
                }
                let existing = state.agent_location_authorities.get(&authority.agent_id);
                if existing != Some(authority) {
                    super::factory_authority::next_revision(
                        existing.map(|v| v.authority_revision),
                        authority.authority_revision,
                        "agent location",
                    )?;
                }
                Ok(Self::AgentLocation(
                    event.clone(),
                    authority.agent_id.clone(),
                    authority.clone(),
                ))
            }
            DomainEvent::LocationAnchorUpdated { anchor } => {
                if anchor.location_id.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: "location_id cannot be empty".into(),
                    });
                }
                let existing = state.location_anchors.get(&anchor.location_id);
                if existing != Some(anchor) {
                    super::factory_authority::next_revision(
                        existing.map(|v| v.authority_revision),
                        anchor.authority_revision,
                        "location anchor",
                    )?;
                }
                Ok(Self::LocationAnchor(
                    event.clone(),
                    anchor.location_id.clone(),
                    anchor.clone(),
                ))
            }
            DomainEvent::FactorySiteAuthorityUpdated { authority } => {
                let mut next = authority.clone();
                super::factory_authority::normalize_allowlist(&mut next)?;
                if next.active {
                    super::factory_authority::require_active_location_anchor(
                        &state.location_anchors,
                        &next.location_id,
                        now,
                    )?;
                }
                let existing = state.factory_site_authorities.get(&next.site_id);
                if existing != Some(&next) {
                    super::factory_authority::next_revision(
                        existing.map(|v| v.authority_revision),
                        next.authority_revision,
                        "factory site",
                    )?;
                }
                Ok(Self::FactorySite(event.clone(), next.site_id.clone(), next))
            }
            DomainEvent::FactoryConstructionPowerProfileUpdated { profile } => {
                let mut authority_state = WorldState::default();
                if let Some(existing) = state
                    .factory_construction_power_profiles
                    .get(&profile.factory_id)
                {
                    authority_state
                        .factory_construction_power_profiles
                        .insert(profile.factory_id.clone(), existing.clone());
                }
                authority_state.apply_factory_construction_power_profile_updated(profile)?;
                Ok(Self::ConstructionPower(
                    event.clone(),
                    profile.factory_id.clone(),
                    authority_state
                        .factory_construction_power_profiles
                        .remove(&profile.factory_id)
                        .unwrap(),
                ))
            }
            DomainEvent::ProductValidationRecorded { receipt } => {
                if state.settled_recipe_job_ids.contains(&receipt.job_id) {
                    return Ok(Self::ValidationReceipt {
                        event: event.clone(),
                        job_id: receipt.job_id,
                        receipts: state
                            .product_validation_receipts
                            .get(&receipt.job_id)
                            .cloned()
                            .unwrap_or_default(),
                        latest: state.latest_product_validation.clone(),
                        agent: state
                            .agents
                            .get(&receipt.requester_agent_id)
                            .map(|cell| (receipt.requester_agent_id.clone(), cell.clone())),
                    });
                }
                let mut receipts = state
                    .product_validation_receipts
                    .get(&receipt.job_id)
                    .cloned()
                    .unwrap_or_default();
                if let Some(existing) = receipts
                    .iter()
                    .find(|existing| existing.validation_index == receipt.validation_index)
                {
                    if existing != receipt {
                        return Err(WorldError::ResourceBalanceInvalid {
                            reason: format!(
                                "product validation conflicts with persisted receipt: job_id={} index={:?}",
                                receipt.job_id, receipt.validation_index
                            ),
                        });
                    }
                } else {
                    receipts.push(receipt.clone());
                }
                let latest = if receipt.decision.accepted
                    && receipt.decision.product_id == receipt.stack.kind
                    && receipt.stack.amount > 0
                    && receipt.stack.amount <= receipt.decision.stack_limit as i64
                {
                    Some(LastProductValidationState {
                        product_id: receipt.stack.kind.clone(),
                        tradable: receipt.decision.tradable,
                    })
                } else {
                    state.latest_product_validation.clone()
                };
                let agent = state.agents.get(&receipt.requester_agent_id).map(|cell| {
                    let mut cell = cell.clone();
                    cell.last_active = now;
                    (receipt.requester_agent_id.clone(), cell)
                });
                Ok(Self::ValidationReceipt {
                    event: event.clone(),
                    job_id: receipt.job_id,
                    receipts,
                    latest,
                    agent,
                })
            }
            DomainEvent::ProductValidationAttemptStarted { attempt } => {
                if attempt.job_id == 0 || attempt.module_id.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "product validation attempt identity is invalid: job_id={} module_id={}",
                            attempt.job_id, attempt.module_id
                        ),
                    });
                }
                let mut attempts = state
                    .product_validation_attempts
                    .get(&attempt.job_id)
                    .cloned()
                    .unwrap_or_default();
                if !state.settled_recipe_job_ids.contains(&attempt.job_id) {
                    if let Some(existing) = attempts
                        .iter()
                        .find(|existing| existing.validation_index == attempt.validation_index)
                    {
                        if existing != attempt {
                            return Err(WorldError::ResourceBalanceInvalid {
                                reason: format!(
                                    "product validation attempt conflicts with persisted intent: job_id={} index={:?}",
                                    attempt.job_id, attempt.validation_index
                                ),
                            });
                        }
                    } else {
                        attempts.push(attempt.clone());
                    }
                }
                Ok(Self::ValidationAttempt(
                    event.clone(),
                    attempt.job_id,
                    attempts,
                ))
            }
            _ => unreachable!("industry history preparation requires a supported event"),
        }
    }

    pub(crate) fn matches(&self, event: &DomainEvent) -> bool {
        match self {
            Self::AgentLocation(body, ..)
            | Self::LocationAnchor(body, ..)
            | Self::FactorySite(body, ..)
            | Self::ConstructionPower(body, ..)
            | Self::ValidationAttempt(body, ..) => body == event,
            Self::ValidationReceipt { event: body, .. } => body == event,
        }
    }
    pub(crate) fn install(self, state: &mut WorldState) {
        match self {
            Self::AgentLocation(_, key, value) => {
                state.agent_location_authorities.insert(key, value);
            }
            Self::LocationAnchor(_, key, value) => {
                state.location_anchors.insert(key, value);
            }
            Self::FactorySite(_, key, value) => {
                state.factory_site_authorities.insert(key, value);
            }
            Self::ConstructionPower(_, key, value) => {
                state.factory_construction_power_profiles.insert(key, value);
            }
            Self::ValidationReceipt {
                job_id,
                receipts,
                latest,
                agent,
                ..
            } => {
                state.product_validation_receipts.insert(job_id, receipts);
                state.latest_product_validation = latest;
                if let Some((id, cell)) = agent {
                    state.agents.insert(id, cell);
                }
            }
            Self::ValidationAttempt(_, job_id, attempts) => {
                state.product_validation_attempts.insert(job_id, attempts);
            }
        }
    }

    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updated = if let Self::ValidationReceipt {
            agent: Some((id, cell)),
            ..
        } = self
        {
            Some((id.clone(), cell.clone()))
        } else {
            None
        };
        let event = match self {
            Self::AgentLocation(event, ..)
            | Self::LocationAnchor(event, ..)
            | Self::FactorySite(event, ..)
            | Self::ConstructionPower(event, ..)
            | Self::ValidationAttempt(event, ..) => event,
            Self::ValidationReceipt { event, .. } => event,
        };
        let routed_id = event.agent_id();
        let mut updates = BTreeMap::new();
        if let Some(id) = routed_id
            && let Some(base) = updated
                .as_ref()
                .filter(|(key, _)| key == id)
                .map(|(_, cell)| cell)
                .or_else(|| state.agents.get(id))
        {
            let mut cell = base.clone();
            cell.mailbox.push_back(event.clone());
            updates.insert(id.to_string(), cell);
        } else if let Some((id, cell)) = updated {
            updates.insert(id, cell);
        }
        out.serialize_field(
            "agents",
            &super::command_projection::CommandAgentMapProjection {
                agents: &state.agents,
                updates: &updates,
            },
        )
    }

    pub(crate) fn serialize_receipts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        use super::module_release_transition::ReleaseMapProjection;
        match self {
            Self::ValidationReceipt {
                job_id, receipts, ..
            } => out.serialize_field(
                "product_validation_receipts",
                &ReleaseMapProjection {
                    base: &state.product_validation_receipts,
                    updates: &BTreeMap::from([(*job_id, receipts.clone())]),
                },
            )?,
            _ if !state.product_validation_receipts.is_empty() => out.serialize_field(
                "product_validation_receipts",
                &state.product_validation_receipts,
            )?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn serialize_latest<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let latest = match self {
            Self::ValidationReceipt { latest, .. } => latest,
            _ => &state.latest_product_validation,
        };
        if latest.is_some() {
            out.serialize_field("latest_product_validation", latest)
        } else {
            Ok(())
        }
    }

    pub(crate) fn serialize_authorities<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        use super::module_release_transition::ReleaseMapProjection;
        macro_rules! field {
            ($variant:ident, $name:literal, $base:ident) => {
                if let Self::$variant(_, key, value) = self {
                    out.serialize_field(
                        $name,
                        &ReleaseMapProjection {
                            base: &state.$base,
                            updates: &BTreeMap::from([(key.clone(), value.clone())]),
                        },
                    )?;
                } else {
                    out.serialize_field($name, &state.$base)?;
                }
            };
        }
        field!(
            AgentLocation,
            "agent_location_authorities",
            agent_location_authorities
        );
        field!(LocationAnchor, "location_anchors", location_anchors);
        field!(
            FactorySite,
            "factory_site_authorities",
            factory_site_authorities
        );
        field!(
            ConstructionPower,
            "factory_construction_power_profiles",
            factory_construction_power_profiles
        );
        Ok(())
    }

    pub(crate) fn serialize_attempts<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        use super::module_release_transition::ReleaseMapProjection;
        if let Self::ValidationAttempt(_, job_id, attempts) = self {
            out.serialize_field(
                "product_validation_attempts",
                &ReleaseMapProjection {
                    base: &state.product_validation_attempts,
                    updates: &BTreeMap::from([(*job_id, attempts.clone())]),
                },
            )
        } else if !state.product_validation_attempts.is_empty() {
            out.serialize_field(
                "product_validation_attempts",
                &state.product_validation_attempts,
            )
        } else {
            Ok(())
        }
    }
}
