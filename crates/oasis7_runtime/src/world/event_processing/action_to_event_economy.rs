use super::super::super::{FactoryProductionStatus, MaterialMarketQuote};
use super::super::logistics::{logistics_path_id, normalize_logistics_route_kind};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

const FACTORY_DURABILITY_PPM_BASE: i64 = 1_000_000;
const FACTORY_MAINTENANCE_PART_KIND: &str = "hardware_part";
const FACTORY_MAINTENANCE_REPAIR_PPM_PER_PART: i64 = 25_000;
const FACTORY_RECYCLE_BASE_PPM: i64 = 700_000;
const BOTTLENECK_TAG_KINDS: &[&str] = &["iron_ingot", "copper_wire", "control_chip", "motor_mk1"];
impl World {
    pub(super) fn action_to_event_economy(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        if let Some(event) = self.profile_governance_event(action_id, action) {
            return Ok(WorldEventBody::Domain(event));
        }
        match action {
            Action::EmitResourceTransfer {
                from_agent_id,
                to_agent_id,
                kind,
                amount,
            } => {
                if *amount <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InvalidAmount { amount: *amount },
                    }));
                }
                let Some(from_cell) = self.state.agents.get(from_agent_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: from_agent_id.clone(),
                        },
                    }));
                };
                let Some(to_cell) = self.state.agents.get(to_agent_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: to_agent_id.clone(),
                        },
                    }));
                };
                let distance_cm = space_distance_cm(from_cell.state.pos, to_cell.state.pos);
                if distance_cm > 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentsNotCoLocated {
                            agent_id: from_agent_id.clone(),
                            other_agent_id: to_agent_id.clone(),
                        },
                    }));
                }
                if *kind == ResourceKind::Data
                    && from_agent_id != to_agent_id
                    && !self
                        .state
                        .has_data_access_permission(from_agent_id, to_agent_id)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "data transfer denied: missing access grant owner={} grantee={}",
                                from_agent_id, to_agent_id
                            )],
                        },
                    }));
                }
                let available = from_cell.state.resources.get(*kind);
                if available < *amount {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientResource {
                            agent_id: from_agent_id.clone(),
                            kind: *kind,
                            requested: *amount,
                            available,
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::ResourceTransferred {
                    from_agent_id: from_agent_id.clone(),
                    to_agent_id: to_agent_id.clone(),
                    kind: *kind,
                    amount: *amount,
                }))
            }
            Action::CollectData {
                collector_agent_id,
                electricity_cost,
                data_amount,
            } => {
                if !self.state.agents.contains_key(collector_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: collector_agent_id.clone(),
                        },
                    }));
                }
                if *electricity_cost <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InvalidAmount {
                            amount: *electricity_cost,
                        },
                    }));
                }
                if *data_amount <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InvalidAmount {
                            amount: *data_amount,
                        },
                    }));
                }
                let available = self
                    .state
                    .agents
                    .get(collector_agent_id)
                    .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
                    .unwrap_or(0);
                if available < *electricity_cost {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientResource {
                            agent_id: collector_agent_id.clone(),
                            kind: ResourceKind::Electricity,
                            requested: *electricity_cost,
                            available,
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::DataCollected {
                    collector_agent_id: collector_agent_id.clone(),
                    electricity_cost: *electricity_cost,
                    data_amount: *data_amount,
                }))
            }
            Action::CollectDataAuthenticated {
                collector_agent_id,
                electricity_cost,
                data_amount,
                player_id,
                public_key,
                nonce,
                signature,
            } => action_to_event_economy_support::authenticated_collect_data_to_event(
                self,
                action_id,
                collector_agent_id,
                *electricity_cost,
                *data_amount,
                player_id,
                public_key,
                *nonce,
                signature,
            ),
            Action::GrantDataAccess {
                owner_agent_id,
                grantee_agent_id,
            } => {
                if !self.state.agents.contains_key(owner_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: owner_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(grantee_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: grantee_agent_id.clone(),
                        },
                    }));
                }
                if owner_agent_id == grantee_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "data access grant requires distinct owner and grantee".to_string(),
                            ],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::DataAccessGranted {
                    owner_agent_id: owner_agent_id.clone(),
                    grantee_agent_id: grantee_agent_id.clone(),
                }))
            }
            Action::RevokeDataAccess {
                owner_agent_id,
                grantee_agent_id,
            } => {
                if !self.state.agents.contains_key(owner_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: owner_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(grantee_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: grantee_agent_id.clone(),
                        },
                    }));
                }
                if owner_agent_id == grantee_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "data access revoke requires distinct owner and grantee"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::DataAccessRevoked {
                    owner_agent_id: owner_agent_id.clone(),
                    grantee_agent_id: grantee_agent_id.clone(),
                }))
            }
            Action::BuildFactory {
                builder_agent_id,
                site_id,
                spec,
            } => self.build_factory_to_event(action_id, builder_agent_id, site_id, spec, true),
            Action::BuildFactoryWithModule { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "build_factory_with_module requires module runtime".to_string(),
                        ],
                    },
                }))
            }
            Action::MaintainFactory {
                operator_agent_id,
                factory_id,
                parts,
            } => {
                if *parts <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InvalidAmount { amount: *parts },
                    }));
                }
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                let Some(factory) = self.state.factories.get(factory_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryNotFound {
                            factory_id: factory_id.clone(),
                        },
                    }));
                };
                if factory.builder_agent_id != *operator_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "maintain factory denied: operator {} is not builder {}",
                                operator_agent_id, factory.builder_agent_id
                            )],
                        },
                    }));
                }

                let current = factory.durability_ppm.clamp(0, FACTORY_DURABILITY_PPM_BASE);
                if current >= FACTORY_DURABILITY_PPM_BASE {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "factory {} already at full durability",
                                factory_id
                            )],
                        },
                    }));
                }

                let requested_parts = *parts;
                let consume = vec![MaterialStack::new(
                    FACTORY_MAINTENANCE_PART_KIND.to_string(),
                    requested_parts,
                )];
                let consume_ledger = self.select_material_consume_ledger_with_world_fallback(
                    factory.input_ledger.clone(),
                    &consume,
                );
                let available =
                    self.ledger_material_balance(&consume_ledger, FACTORY_MAINTENANCE_PART_KIND);
                if available < requested_parts {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientMaterial {
                            material_kind: FACTORY_MAINTENANCE_PART_KIND.to_string(),
                            requested: requested_parts,
                            available,
                        },
                    }));
                }

                let repair_ppm = requested_parts
                    .saturating_mul(FACTORY_MAINTENANCE_REPAIR_PPM_PER_PART)
                    .max(0);
                let durability_ppm = current
                    .saturating_add(repair_ppm)
                    .clamp(0, FACTORY_DURABILITY_PPM_BASE);
                let recovered_ppm = durability_ppm.saturating_sub(current);
                if recovered_ppm <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "factory {} maintenance has no effect",
                                factory_id
                            )],
                        },
                    }));
                }
                let consumed_parts = recovered_ppm
                    .saturating_add(FACTORY_MAINTENANCE_REPAIR_PPM_PER_PART - 1)
                    .saturating_div(FACTORY_MAINTENANCE_REPAIR_PPM_PER_PART)
                    .clamp(1, requested_parts);

                Ok(WorldEventBody::Domain(DomainEvent::FactoryMaintained {
                    operator_agent_id: operator_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    consume_ledger,
                    consumed_parts,
                    durability_ppm,
                }))
            }
            Action::RecycleFactory {
                operator_agent_id,
                factory_id,
            } => {
                if !self.state.agents.contains_key(operator_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: operator_agent_id.clone(),
                        },
                    }));
                }
                let Some(factory) = self.state.factories.get(factory_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryNotFound {
                            factory_id: factory_id.clone(),
                        },
                    }));
                };
                if factory.builder_agent_id != *operator_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "recycle factory denied: operator {} is not builder {}",
                                operator_agent_id, factory.builder_agent_id
                            )],
                        },
                    }));
                }
                let active_jobs = self
                    .state
                    .pending_recipe_jobs
                    .values()
                    .filter(|job| job.factory_id == *factory_id)
                    .count();
                if active_jobs > 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryBusy {
                            factory_id: factory_id.clone(),
                            active_jobs,
                            recipe_slots: factory.spec.recipe_slots,
                        },
                    }));
                }

                let durability_ppm = factory.durability_ppm.clamp(0, FACTORY_DURABILITY_PPM_BASE);
                let recovered = factory
                    .spec
                    .build_cost
                    .iter()
                    .filter_map(|stack| {
                        if stack.amount <= 0 {
                            return None;
                        }
                        let recovered_amount = ((stack.amount as i128)
                            .saturating_mul(FACTORY_RECYCLE_BASE_PPM as i128)
                            .saturating_mul(durability_ppm as i128)
                            / (FACTORY_DURABILITY_PPM_BASE as i128)
                            / (FACTORY_DURABILITY_PPM_BASE as i128))
                            as i64;
                        if recovered_amount <= 0 {
                            None
                        } else {
                            Some(MaterialStack::new(stack.kind.clone(), recovered_amount))
                        }
                    })
                    .collect();

                Ok(WorldEventBody::Domain(DomainEvent::FactoryRecycled {
                    operator_agent_id: operator_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recycle_ledger: factory.output_ledger.clone(),
                    recovered,
                    durability_ppm,
                }))
            }
            Action::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                plan,
                logistics_route_ids,
                logistics_path_ids,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    }));
                }
                let Some(factory) = self.state.factories.get(factory_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryNotFound {
                            factory_id: factory_id.clone(),
                        },
                    }));
                };
                if let Some(reason) = self.factory_owner_rejection(requester_agent_id, factory_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason,
                    }));
                }
                let active_jobs = self
                    .state
                    .pending_recipe_jobs
                    .values()
                    .filter(|job| job.factory_id == *factory_id)
                    .count();
                if active_jobs >= factory.spec.recipe_slots as usize {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryBusy {
                            factory_id: factory_id.clone(),
                            active_jobs,
                            recipe_slots: factory.spec.recipe_slots,
                        },
                    }));
                }
                if let Some(reason) = &plan.reject_reason {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("recipe plan rejected: {reason}")],
                        },
                    }));
                }
                for (label, stacks) in [
                    ("consume", plan.consume.as_slice()),
                    ("produce", plan.produce.as_slice()),
                    ("byproduct", plan.byproducts.as_slice()),
                ] {
                    if let Some(note) = invalid_recipe_plan_stack_note(label, stacks) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied { notes: vec![note] },
                        }));
                    }
                }
                if plan.accepted_batches == 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["recipe accepted_batches must be > 0".to_string()],
                        },
                    }));
                }
                if !plan
                    .produce
                    .iter()
                    .any(|stack| !stack.kind.trim().is_empty() && stack.amount > 0)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "recipe plan must produce at least one positive material output"
                                    .to_string(),
                            ],
                        },
                    }));
                }
                let recipe_profile = self.recipe_profile(recipe_id);
                if let Some(profile) = recipe_profile {
                    if !recipe_stage_gate_allowed(
                        self.state.industry_progress.stage,
                        profile.stage_gate.as_str(),
                    ) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "recipe stage gate denied: recipe={} required_stage={} current_stage={}",
                                    recipe_id,
                                    profile.stage_gate,
                                    industry_stage_label(self.state.industry_progress.stage),
                                )],
                            },
                        }));
                    }
                    if !recipe_preferred_tags_compatible(
                        &profile.preferred_factory_tags,
                        &factory.spec.tags,
                    ) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "recipe preferred_factory_tags mismatch: recipe={} preferred={:?} factory_tags={:?}",
                                    recipe_id, profile.preferred_factory_tags, factory.spec.tags
                                )],
                            },
                        }));
                    }
                }
                for stack in &plan.produce {
                    let Some(product_profile) = self.product_profile(stack.kind.as_str()) else {
                        continue;
                    };
                    if !product_unlock_stage_allowed(
                        self.state.industry_progress.stage,
                        product_profile.unlock_stage.as_str(),
                    ) {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "product unlock_stage denied: product={} required_stage={} current_stage={}",
                                    product_profile.product_id,
                                    product_profile.unlock_stage,
                                    industry_stage_label(self.state.industry_progress.stage),
                                )],
                            },
                        }));
                    }
                }
                let effective_consume =
                    merge_recipe_consume_with_maintenance_sink(self, &plan.consume, &plan.produce);
                let preferred_consume_ledger = factory.input_ledger.clone();
                // Both direct and module-backed new submissions are
                // site-bound. Historical RecipeStarted events remain
                // reducer-compatible through their persisted ledgers.
                let consume_ledger = preferred_consume_ledger.clone();
                let output_ledger = factory.output_ledger.clone();
                let mut resolved_logistics_route_ids = Vec::new();
                let mut resolved_logistics_path_ids = logistics_path_ids.clone();
                resolved_logistics_path_ids.sort();
                if !logistics_route_ids.is_empty() {
                    let mut seen_route_ids = BTreeSet::new();
                    for route_id in logistics_route_ids {
                        if !seen_route_ids.insert(route_id) {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "duplicate logistics route binding: route_id={route_id}"
                                    )],
                                },
                            }));
                        }
                    }
                    let consumed_kinds: BTreeSet<String> = effective_consume
                        .iter()
                        .filter_map(|stack| normalize_logistics_route_kind(stack.kind.as_str()))
                        .collect();
                    if consumed_kinds.len() != logistics_route_ids.len() {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "logistics route binding count must equal distinct consumed material kinds: routes={} kinds={}",
                                    logistics_route_ids.len(),
                                    consumed_kinds.len()
                                )],
                            },
                        }));
                    }
                    let mut bound_kinds = BTreeSet::new();
                    for route_id in logistics_route_ids {
                        let Some(route) = self.state.logistics_routes.get(route_id) else {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "logistics route not found: route_id={route_id}"
                                    )],
                                },
                            }));
                        };
                        if !self.state.completed_logistics_route_ids.contains(route_id) {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "logistics route is not completed: route_id={route_id}"
                                    )],
                                },
                            }));
                        }
                        if route.to_ledger != consume_ledger {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "logistics route destination mismatch: route_id={route_id}"
                                    )],
                                },
                            }));
                        }
                        let Some(route_kind) = normalize_logistics_route_kind(route.kind.as_str())
                        else {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "logistics route material kind is invalid: route_id={route_id}"
                                    )],
                                },
                            }));
                        };
                        if !consumed_kinds.contains(&route_kind)
                            || !bound_kinds.insert(route_kind.clone())
                        {
                            return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "logistics route material mismatch: route_id={route_id}"
                                    )],
                                },
                            }));
                        }
                    }
                    resolved_logistics_route_ids = logistics_route_ids.clone();
                    resolved_logistics_route_ids.sort();
                }
                if resolved_logistics_path_ids.is_empty() {
                    resolved_logistics_path_ids = resolved_logistics_route_ids
                        .iter()
                        .filter_map(|route_id| logistics_path_id(std::slice::from_ref(route_id)))
                        .collect();
                }
                if !resolved_logistics_path_ids.is_empty()
                    && let Err(reason) = self.state.allocate_recipe_path_amounts(
                        &consume_ledger,
                        &resolved_logistics_route_ids,
                        &resolved_logistics_path_ids,
                        &effective_consume,
                    )
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![reason],
                        },
                    }));
                }
                let required_materials =
                    match action_to_event_economy_support::aggregate_material_stacks_for_admission(
                        "recipe consume",
                        &effective_consume,
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
                for (material_kind, requested) in required_materials {
                    let available =
                        self.ledger_material_balance(&consume_ledger, material_kind.as_str());
                    if available < requested {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InsufficientMaterial {
                                material_kind,
                                requested,
                                available,
                            },
                        }));
                    }
                }
                if plan.power_required < 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["recipe power_required must be >= 0".to_string()],
                        },
                    }));
                }
                let power_owner_agent_id = factory.builder_agent_id.clone();
                let available_power = self
                    .state
                    .agents
                    .get(&power_owner_agent_id)
                    .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
                    .unwrap_or(0);
                if available_power < plan.power_required {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientResource {
                            agent_id: power_owner_agent_id,
                            kind: ResourceKind::Electricity,
                            requested: plan.power_required,
                            available: available_power,
                        },
                    }));
                }
                let market_quotes = build_material_market_quotes(
                    self,
                    &preferred_consume_ledger,
                    &effective_consume,
                );
                let bottleneck_tags =
                    resolve_recipe_bottleneck_tags(recipe_profile, &effective_consume);
                let scarcity_delay_ticks = compute_local_scarcity_delay_ticks(
                    self,
                    &preferred_consume_ledger,
                    &consume_ledger,
                    &effective_consume,
                    &bottleneck_tags,
                );
                let duration_ticks = plan
                    .duration_ticks
                    .max(1)
                    .saturating_add(scarcity_delay_ticks);
                let ready_at = self.state.time.saturating_add(duration_ticks as u64);
                Ok(WorldEventBody::Domain(DomainEvent::RecipeStarted {
                    committed_recipe_origin: None,
                    job_id: action_id,
                    requester_agent_id: requester_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recipe_id: recipe_id.clone(),
                    accepted_batches: plan.accepted_batches,
                    consume: effective_consume,
                    produce: plan.produce.clone(),
                    byproducts: plan.byproducts.clone(),
                    power_required: plan.power_required,
                    power_owner_agent_id: Some(factory.builder_agent_id.clone()),
                    duration_ticks,
                    consume_ledger,
                    output_ledger,
                    bottleneck_tags,
                    market_quotes,
                    logistics_route_ids: resolved_logistics_route_ids,
                    logistics_path_ids: resolved_logistics_path_ids,
                    ready_at,
                }))
            }
            Action::PauseFactoryProduction {
                requester_agent_id,
                factory_id,
                reason,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    }));
                }
                let Some(factory) = self.state.factories.get(factory_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryNotFound {
                            factory_id: factory_id.clone(),
                        },
                    }));
                };
                if factory.builder_agent_id != *requester_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "pause factory denied: requester {} is not builder {}",
                                requester_agent_id, factory.builder_agent_id
                            )],
                        },
                    }));
                }
                let active_jobs = self
                    .state
                    .pending_recipe_jobs
                    .values()
                    .filter(|job| job.factory_id == *factory_id)
                    .count();
                if active_jobs > 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::FactoryBusy {
                            factory_id: factory_id.clone(),
                            active_jobs,
                            recipe_slots: factory.spec.recipe_slots,
                        },
                    }));
                }
                if factory.production.status != FactoryProductionStatus::Idle {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "factory production pause requires idle factory status".to_string(),
                            ],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(
                    DomainEvent::FactoryProductionPaused {
                        action_id,
                        requester_agent_id: requester_agent_id.clone(),
                        factory_id: factory_id.clone(),
                        reason: reason.clone(),
                    },
                ))
            }
            Action::ScheduleRecipeWithModule { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "schedule_recipe_with_module requires module runtime".to_string(),
                        ],
                    },
                }))
            }
            Action::ValidateProduct {
                requester_agent_id,
                module_id,
                stack,
                decision,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    }));
                }
                if module_id.trim().is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["product module_id cannot be empty".to_string()],
                        },
                    }));
                }
                if !decision.accepted {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: if decision.notes.is_empty() {
                                vec!["product validation rejected".to_string()]
                            } else {
                                decision.notes.clone()
                            },
                        },
                    }));
                }
                if decision.product_id != stack.kind {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "validated product mismatch expected={} got={}",
                                stack.kind, decision.product_id
                            )],
                        },
                    }));
                }
                if stack.amount <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["product stack amount must be > 0".to_string()],
                        },
                    }));
                }
                if stack.amount > decision.stack_limit as i64 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "product stack exceeds limit amount={} limit={}",
                                stack.amount, decision.stack_limit
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::ProductValidated {
                    requester_agent_id: requester_agent_id.clone(),
                    module_id: module_id.clone(),
                    stack: stack.clone(),
                    stack_limit: decision.stack_limit,
                    tradable: decision.tradable,
                    quality_levels: decision.quality_levels.clone(),
                    notes: decision.notes.clone(),
                }))
            }
            Action::ValidateProductWithModule { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "validate_product_with_module requires module runtime".to_string(),
                        ],
                    },
                }))
            }
            _ => unreachable!("action_to_event_economy received unsupported action variant"),
        }
    }
}

#[path = "action_to_event_economy_factory.rs"]
mod action_to_event_economy_factory;
#[path = "action_to_event_economy_profiles.rs"]
mod action_to_event_economy_profiles;
#[path = "action_to_event_economy_support.rs"]
mod action_to_event_economy_support;

use super::super::economy::invalid_recipe_plan_stack_note;
pub(crate) use action_to_event_economy_support::build_material_market_quotes;
use action_to_event_economy_support::{
    compute_local_scarcity_delay_ticks, industry_stage_label,
    merge_recipe_consume_with_maintenance_sink, product_unlock_stage_allowed,
    recipe_preferred_tags_compatible, recipe_stage_gate_allowed, resolve_recipe_bottleneck_tags,
};

pub(crate) fn ensure_profile_field_whitelist<T: serde::Serialize>(
    profile: &T,
    allowed_fields: &[&str],
    profile_label: &str,
) -> Result<(), String> {
    action_to_event_economy_support::ensure_profile_field_whitelist(
        profile,
        allowed_fields,
        profile_label,
    )
}
