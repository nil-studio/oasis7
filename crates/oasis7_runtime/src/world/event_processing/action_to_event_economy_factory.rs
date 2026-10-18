use super::*;
use crate::FACTORY_BUILD_STARTED_MODERN_VERSION;
use crate::FactoryBuildPowerObligationV1;
use oasis7_wasm_abi::FactoryModuleSpec;

impl World {
    fn starter_assembler_build_blocker(&self, factory_id: &str) -> Option<String> {
        if factory_id != crate::STARTER_ASSEMBLER_FACTORY_ID {
            return None;
        }
        self.state
            .starter_industrial_feasibility()
            .disabled_reason()
    }

    pub(in crate::world) fn validate_module_backed_factory_admission(
        &self,
        action_id: ActionId,
        builder_agent_id: &String,
        site_id: &String,
        module_id: &str,
        spec: &FactoryModuleSpec,
    ) -> Result<Option<RejectReason>, WorldError> {
        if module_id.trim().is_empty() {
            return Ok(Some(RejectReason::RuleDenied {
                notes: vec!["factory module_id cannot be empty".to_string()],
            }));
        }
        if let WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) =
            self.build_factory_to_event(action_id, builder_agent_id, site_id, spec, false)?
        {
            return Ok(Some(reason));
        }
        if self
            .state
            .factory_construction_power_profiles
            .get(spec.factory_id.as_str())
            .and_then(|profile| profile.source_module_id.as_deref())
            != Some(module_id)
        {
            return Ok(Some(RejectReason::RuleDenied {
                notes: vec![format!(
                    "construction power profile source mismatch: factory_id={} module_id={}",
                    spec.factory_id, module_id
                )],
            }));
        }
        Ok(None)
    }

    pub(in crate::world) fn build_factory_to_event(
        &self,
        action_id: ActionId,
        builder_agent_id: &String,
        site_id: &String,
        spec: &FactoryModuleSpec,
        validate_materials: bool,
    ) -> Result<WorldEventBody, WorldError> {
        if !self.state.agents.contains_key(builder_agent_id) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::AgentNotFound {
                    agent_id: builder_agent_id.clone(),
                },
            }));
        }
        if spec.factory_id.trim().is_empty() {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["factory_id cannot be empty".to_string()],
                },
            }));
        }
        if self.state.retired_factory_ids.contains(&spec.factory_id) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("factory identity is retired: {}", spec.factory_id)],
                },
            }));
        }
        if self.state.factories.contains_key(&spec.factory_id)
            || self
                .state
                .pending_factory_builds
                .values()
                .any(|job| job.spec.factory_id == spec.factory_id)
        {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("factory already exists: {}", spec.factory_id)],
                },
            }));
        }
        if let Some(reason) = self.starter_assembler_build_blocker(spec.factory_id.as_str()) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            }));
        }
        if spec.recipe_slots == 0 {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec!["recipe_slots must be > 0".to_string()],
                },
            }));
        }
        let Some(factory_profile) = self.state.factory_profiles.get(&spec.factory_id) else {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "factory profile unknown: factory_id={}",
                        spec.factory_id
                    )],
                },
            }));
        };
        if let Err(reason) = super::action_to_event_economy_support::factory_profile_matches_spec(
            factory_profile,
            spec,
        ) {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![reason],
                },
            }));
        }
        let Some(site_authority) = self.state.factory_site_authorities.get(site_id) else {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("site_unknown: site is not registered: {site_id}")],
                },
            }));
        };
        let Some(location_authority) = self.state.agent_location_authorities.get(builder_agent_id)
        else {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "builder location authority is unknown: builder_agent_id={builder_agent_id}"
                    )],
                },
            }));
        };
        let location_anchor_revision = match self
            .state
            .active_location_anchor_revision(site_authority.location_id.as_str(), self.state.time)
        {
            Ok(revision) => revision,
            Err(WorldError::ResourceBalanceInvalid { reason }) => {
                return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![reason],
                    },
                }));
            }
            Err(error) => return Err(error),
        };
        let authorized = site_authority.owner_agent_id == *builder_agent_id
            || site_authority
                .authorized_agent_ids
                .iter()
                .any(|agent_id| agent_id == builder_agent_id);
        if site_authority.site_id != *site_id
            || location_authority.agent_id != *builder_agent_id
            || !site_authority.active
            || !site_authority.chunk_ready
            || site_authority.location_id.trim().is_empty()
            || site_authority.authority_revision == 0
            || !location_authority.active
            || location_authority.authority_revision == 0
            || location_authority.location_id != site_authority.location_id
            || location_authority.effective_at > self.state.time
            || !authorized
        {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "site_access_or_location_blocked: site={} builder={} site_active={} chunk_ready={} site_location={} builder_location={} builder_location_active={} authorized={}",
                        site_id,
                        builder_agent_id,
                        site_authority.active,
                        site_authority.chunk_ready,
                        site_authority.location_id,
                        location_authority.location_id,
                        location_authority.active,
                        authorized
                    )],
                },
            }));
        }
        let Some(power_profile) = self
            .state
            .factory_construction_power_profiles
            .get(spec.factory_id.as_str())
        else {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "construction power profile unknown: factory_id={}",
                        spec.factory_id
                    )],
                },
            }));
        };
        if power_profile.factory_id != spec.factory_id
            || !power_profile.active
            || power_profile.authority_revision == 0
            || power_profile.electricity_amount < 0
        {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "construction power profile inactive_or_stale: factory_id={} revision={} active={}",
                        spec.factory_id, power_profile.authority_revision, power_profile.active
                    )],
                },
            }));
        }
        let available_power = self
            .state
            .agents
            .get(builder_agent_id)
            .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
            .unwrap_or(0);
        if available_power < power_profile.electricity_amount {
            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                action_id,
                reason: RejectReason::InsufficientResource {
                    agent_id: builder_agent_id.clone(),
                    kind: ResourceKind::Electricity,
                    requested: power_profile.electricity_amount,
                    available: available_power,
                },
            }));
        }
        // Direct builds use the builder ledger; module fallback is resolved upstream.
        let consume_ledger = MaterialLedgerId::agent(builder_agent_id.clone());
        let (material_balances_before, material_balances_after) = if validate_materials {
            let required_materials =
                match super::action_to_event_economy_support::aggregate_material_stacks_for_admission(
                    "factory build_cost",
                    &spec.build_cost,
                ) {
                    Ok(required_materials) => required_materials,
                    Err(reason) => {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![reason],
                            },
                        }));
                    }
                };
            let mut material_balances_before = BTreeMap::new();
            let mut material_balances_after = BTreeMap::new();
            for (material_kind, requested) in &required_materials {
                let available =
                    self.ledger_material_balance(&consume_ledger, material_kind.as_str());
                if available < *requested {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientMaterial {
                            material_kind: material_kind.clone(),
                            requested: *requested,
                            available,
                        },
                    }));
                }
                material_balances_before.insert(material_kind.clone(), available);
                material_balances_after
                    .insert(material_kind.clone(), available.saturating_sub(*requested));
            }
            (material_balances_before, material_balances_after)
        } else {
            (BTreeMap::new(), BTreeMap::new())
        };

        let construction_power_obligation = FactoryBuildPowerObligationV1 {
            payer_agent_id: builder_agent_id.clone(),
            profile_key: spec.factory_id.clone(),
            profile_revision: power_profile.authority_revision,
            electricity_amount: power_profile.electricity_amount,
            mode: power_profile.mode,
            material_ledger: Some(consume_ledger.clone()),
            electricity_before: Some(available_power),
            electricity_after: Some(
                available_power.saturating_sub(power_profile.electricity_amount),
            ),
            material_balances_before: Some(material_balances_before),
            material_balances_after: Some(material_balances_after),
        };

        let build_ticks = spec.build_time_ticks.max(1);
        let ready_at = self.state.time.saturating_add(build_ticks as u64);
        Ok(WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
            job_id: action_id,
            builder_agent_id: builder_agent_id.clone(),
            site_id: site_id.clone(),
            spec: spec.clone(),
            consume_ledger,
            ready_at,
            contract_version: Some(FACTORY_BUILD_STARTED_MODERN_VERSION),
            site_authority_revision: Some(site_authority.authority_revision),
            site_location_id: Some(site_authority.location_id.clone()),
            location_anchor_revision: Some(location_anchor_revision),
            construction_power_obligation: Some(construction_power_obligation),
        }))
    }
}
