#[path = "product_validation_result.rs"]
mod product_validation_result;

use super::super::super::{FactoryProductionStatus, MaterialMarketQuote};
use super::super::logistics::{logistics_path_id, normalize_logistics_route_kind};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

const FACTORY_DURABILITY_PPM_BASE: i64 = 1_000_000;
const FACTORY_MAINTENANCE_PART_KIND: &str = "hardware_part";
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

                let Some(repair) =
                    crate::world::incremental_action_factory_lifecycle::math::maintenance_repair_v1(
                        current,
                        requested_parts,
                    )
                else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "factory {} maintenance has no effect",
                                factory_id
                            )],
                        },
                    }));
                };
                let consumed_parts = repair.consumed_parts;
                let durability_ppm = repair.durability_ppm;

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
                        crate::world::incremental_action_factory_lifecycle::math::recycle_stack_amount_v1(stack.amount, durability_ppm)
                            .map(|amount| MaterialStack::new(stack.kind.clone(), amount))
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
            } => factory_schedule_recipe::evaluate_schedule_recipe_v1(
                &mut factory_schedule_sources::LegacyFactoryRecipeFactsV1::new(self),
                action_id,
                requester_agent_id,
                factory_id,
                recipe_id,
                plan,
                logistics_route_ids,
                logistics_path_ids,
            ),
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
            } => Ok(self.product_validation_result_to_event(
                action_id,
                requester_agent_id,
                module_id,
                stack,
                decision,
            )),
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
    resolve_recipe_bottleneck_tags,
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

pub(crate) use action_to_event_economy_support::{
    recipe_preferred_tags_compatible, recipe_stage_gate_allowed,
};

#[path = "../incremental_normal_action_factory_schedule_build.rs"]
mod factory_schedule_build;
#[path = "../incremental_normal_action_factory_schedule_recipe.rs"]
mod factory_schedule_recipe;
#[path = "../incremental_normal_action_factory_schedule_sources.rs"]
mod factory_schedule_sources;
pub(in crate::world) mod factory_schedule_fixed {
    pub(in crate::world) use super::action_to_event_economy_support::resolve_recipe_bottleneck_tags;
    pub(in crate::world) use super::factory_schedule_build::*;
    pub(in crate::world) use super::factory_schedule_recipe::*;
    pub(in crate::world) use super::factory_schedule_sources::*;
}
pub(in crate::world) use action_to_event_economy_support::factory_schedule_quotes as factory_schedule_market_fixed;
