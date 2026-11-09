//! Pure resource observation on the actual World. Never installs a grant,
//! reserves assets, steps the World, signs a response, or dispatches I/O.
use super::World;
use crate::{
    Action, ActionEnvelope, AgentDecisionCausalContextV1, DomainEvent, MaterialLedgerId,
    WorldEventBody,
};
use std::collections::BTreeSet;

#[path = "native_resource_basis_types.rs"]
mod types;
pub use types::*;

type Result<T> = std::result::Result<T, ResourceBasisBlockerV1>;

impl World {
    /// The caller must authenticate its Agent and pin independently. The result
    /// is unsigned historical data, not current authority or a dispatch token.
    pub fn native_resource_basis(
        &self,
        agent: &str,
        selection: &ResourceBasisSelectionV1,
    ) -> Result<NativeResourceBasisFactV1> {
        if !selection.validate()
            || agent.trim().is_empty()
            || agent.len() > 256
            || agent.chars().any(char::is_control)
        {
            return Err(ResourceBasisBlockerV1::InvalidSelection);
        }
        let actor_owner = self.resource_basis_actor_owner(agent)?;
        let actor_electricity_before = self
            .state
            .agents
            .get(agent)
            .ok_or(ResourceBasisBlockerV1::AgentOwnerUnavailable)?
            .state
            .resources
            .get(oasis7_agent_api::world_action::ResourceKind::Electricity);
        let mut ledgers = Vec::new();
        let mut objects = Vec::new();
        let quote = match selection {
            ResourceBasisSelectionV1::Transfer {
                from,
                to,
                material,
                amount,
                distance_km,
                priority,
                routing,
            } => {
                let route_ids = match routing {
                    ResourceBasisRoutingV1::Direct => vec![],
                    ResourceBasisRoutingV1::Routes { route_ids } => route_ids.clone(),
                };
                let action = Action::TransferMaterial {
                    requester_agent_id: agent.into(),
                    from_ledger: from.clone(),
                    to_ledger: to.clone(),
                    kind: material.clone(),
                    amount: *amount,
                    distance_km: *distance_km,
                    priority: Some(*priority),
                    route_id: None,
                    route_ids: route_ids.clone(),
                    auto_reroute: false,
                };
                let mut preview = self.clone();
                let request = "native-resource-basis-transfer-preview-v1";
                preview
                    .bind_agent_causal_decision(
                        request,
                        agent,
                        &action,
                        AgentDecisionCausalContextV1::default(),
                    )
                    .map_err(|_| ResourceBasisBlockerV1::AdmissionUnavailable)?;
                preview
                    .prepare_agent_delegation_commit(request, &action)
                    .map_err(|_| ResourceBasisBlockerV1::AdmissionUnavailable)?;
                let authorization = preview
                    .bound_agent_delegation_authorization(request, &action)
                    .map_err(|_| ResourceBasisBlockerV1::AdmissionUnavailable)?
                    .ok_or(ResourceBasisBlockerV1::AdmissionUnavailable)?;
                let event = preview
                    .action_to_event(&ActionEnvelope {
                        id: 0,
                        action: action.clone(),
                        governance_authorization: None,
                        committed_recipe_origin: None,
                    })
                    .map_err(|_| ResourceBasisBlockerV1::DomainUnavailable)?;
                if !matches!(
                    event,
                    WorldEventBody::Domain(
                        DomainEvent::MaterialTransferred { .. }
                            | DomainEvent::MaterialTransitStarted { .. }
                    )
                ) {
                    return Err(ResourceBasisBlockerV1::DomainUnavailable);
                }
                let quote = self
                    .logistics_transfer_quote_with_route_id(
                        agent,
                        from,
                        to,
                        material,
                        *amount,
                        *distance_km,
                        Some(*priority),
                        None,
                        &route_ids,
                        false,
                    )
                    .map_err(|_| ResourceBasisBlockerV1::DomainUnavailable)?;
                if !quote.submission_feasible || quote.kind != *material {
                    return Err(ResourceBasisBlockerV1::DomainUnavailable);
                }
                ledgers.push(self.resource_basis_ledger(from, [material.as_str()])?);
                ledgers.push(self.resource_basis_ledger(to, [material.as_str()])?);
                ResourceBasisQuoteV1::Transfer(Box::new(ResourceTransferQuoteV1 {
                    action,
                    authorization,
                    quote,
                }))
            }
            ResourceBasisSelectionV1::Build {
                site,
                factory,
                factory_kind,
            } => {
                let quote = self
                    .builtin_factory_quote(agent, site, factory_kind, factory)
                    .map_err(|_| ResourceBasisBlockerV1::AdmissionUnavailable)?;
                let preview = quote
                    .preview
                    .as_ref()
                    .filter(|_| quote.rejection.is_none())
                    .ok_or(ResourceBasisBlockerV1::DomainUnavailable)?;
                ledgers.push(self.resource_basis_ledger(
                    &preview.consume_ledger,
                    preview.consume.iter().map(|value| value.kind.as_str()),
                )?);
                objects.push(self.resource_basis_site(site)?);
                ResourceBasisQuoteV1::Build(Box::new(quote))
            }
            ResourceBasisSelectionV1::Schedule {
                factory,
                recipe,
                batches,
            } => {
                let quote = self
                    .builtin_recipe_quote(agent, factory, recipe, *batches)
                    .map_err(|_| ResourceBasisBlockerV1::AdmissionUnavailable)?;
                let preview = quote
                    .preview
                    .as_ref()
                    .filter(|_| quote.rejection.is_none())
                    .ok_or(ResourceBasisBlockerV1::DomainUnavailable)?;
                if preview.consume_ledger == preview.output_ledger {
                    ledgers.push(
                        self.resource_basis_ledger(
                            &preview.consume_ledger,
                            preview
                                .consume
                                .iter()
                                .chain(&preview.produce)
                                .chain(&preview.byproducts)
                                .map(|value| value.kind.as_str()),
                        )?,
                    );
                } else {
                    ledgers.push(self.resource_basis_ledger(
                        &preview.consume_ledger,
                        preview.consume.iter().map(|value| value.kind.as_str()),
                    )?);
                    ledgers.push(
                        self.resource_basis_ledger(
                            &preview.output_ledger,
                            preview
                                .produce
                                .iter()
                                .chain(&preview.byproducts)
                                .map(|value| value.kind.as_str()),
                        )?,
                    );
                }
                objects.push(self.resource_basis_factory(factory)?);
                ResourceBasisQuoteV1::Schedule(Box::new(quote))
            }
        };
        let mut refs = BTreeSet::new();
        for ledger in &ledgers {
            refs.insert(ledger.ledger.to_string());
            refs.extend(ledger.materials.iter().map(|value| value.material.clone()));
        }
        match selection {
            ResourceBasisSelectionV1::Transfer { routing, .. } => {
                if let ResourceBasisRoutingV1::Routes { route_ids } = routing {
                    refs.extend(route_ids.iter().cloned());
                }
            }
            ResourceBasisSelectionV1::Build { site, factory, .. } => {
                refs.extend([site.clone(), factory.clone()]);
            }
            ResourceBasisSelectionV1::Schedule {
                factory, recipe, ..
            } => {
                refs.extend([factory.clone(), recipe.clone()]);
            }
        }
        Ok(NativeResourceBasisFactV1 {
            agent_id: agent.into(),
            actor_owner,
            quoted_at_tick: self.state.time,
            actor_electricity_before,
            selection: selection.clone(),
            quote,
            ledgers,
            objects,
            derived_resource_refs: refs.into_iter().collect(),
        })
    }

    fn resource_basis_actor_owner(&self, agent: &str) -> Result<ResourceActorOwnerV1> {
        let owner = self
            .current_agent_owner(agent)
            .ok_or(ResourceBasisBlockerV1::AgentOwnerUnavailable)?;
        Ok(ResourceActorOwnerV1 {
            player_id: owner.player_id,
            source_id: owner.source_id,
            public_key: owner.public_key,
            generation: owner.generation,
        })
    }

    fn resource_basis_ledger<'a>(
        &self,
        ledger: &MaterialLedgerId,
        materials: impl IntoIterator<Item = &'a str>,
    ) -> Result<ResourceLedgerFactV1> {
        let unavailable = ResourceBasisBlockerV1::LedgerOrObjectUnavailable;
        let owner = match ledger {
            MaterialLedgerId::World => ResourceLedgerOwnerV1::World,
            MaterialLedgerId::Agent(agent) => ResourceLedgerOwnerV1::Agent {
                agent_id: agent.clone(),
                owner: self.resource_basis_actor_owner(agent)?,
            },
            MaterialLedgerId::Site(site) => {
                let value = self
                    .state
                    .factory_site_authorities
                    .get(site)
                    .ok_or(unavailable)?;
                ResourceLedgerOwnerV1::Site {
                    site_id: site.clone(),
                    owner_agent_id: value.owner_agent_id.clone(),
                    authority_revision: value.authority_revision,
                }
            }
            MaterialLedgerId::Factory(factory) => {
                let value = self.state.factories.get(factory).ok_or(unavailable)?;
                ResourceLedgerOwnerV1::Factory {
                    factory_id: factory.clone(),
                    builder_agent_id: value.builder_agent_id.clone(),
                }
            }
        };
        let balances = self.state.material_ledgers.get(ledger);
        let selected: BTreeSet<_> = materials.into_iter().collect();
        Ok(ResourceLedgerFactV1 {
            ledger: ledger.clone(),
            owner,
            exists: balances.is_some(),
            materials: selected
                .into_iter()
                .map(|material| ResourceMaterialBalanceV1 {
                    material: material.into(),
                    amount: balances.and_then(|values| values.get(material)).copied(),
                })
                .collect(),
        })
    }

    fn resource_basis_site(&self, site: &str) -> Result<ResourceObjectFactV1> {
        let value = self
            .state
            .factory_site_authorities
            .get(site)
            .ok_or(ResourceBasisBlockerV1::LedgerOrObjectUnavailable)?;
        Ok(ResourceObjectFactV1::Site {
            site_id: site.into(),
            owner_agent_id: value.owner_agent_id.clone(),
            authorized_agent_ids: value.authorized_agent_ids.clone(),
            authority_revision: value.authority_revision,
            location_id: value.location_id.clone(),
        })
    }

    fn resource_basis_factory(&self, factory: &str) -> Result<ResourceObjectFactV1> {
        let value = self
            .state
            .factories
            .get(factory)
            .ok_or(ResourceBasisBlockerV1::LedgerOrObjectUnavailable)?;
        Ok(ResourceObjectFactV1::Factory {
            factory_id: factory.into(),
            builder_agent_id: value.builder_agent_id.clone(),
            input_ledger: value.input_ledger.clone(),
            output_ledger: value.output_ledger.clone(),
        })
    }
}
