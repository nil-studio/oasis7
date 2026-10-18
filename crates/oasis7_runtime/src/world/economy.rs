use oasis7_wasm_abi::{
    FactoryBuildDecision, FactoryBuildRequest, MaterialStack, ModuleCallCaller,
    ModuleCallErrorCode, ModuleCallFailure, ModuleCallInput, ModuleCallOrigin, ModuleContext,
    ModuleKind, ModuleOutput, ModuleSandbox, ProductValidationDecision, ProductValidationRequest,
    RecipeExecutionPlan, RecipeExecutionRequest,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;

use super::super::util::to_canonical_cbor;
use super::super::{
    Action, ActionEnvelope, ActionId, DomainEvent, M4_PRODUCT_ALLOY_PLATE_MODULE_ID,
    M4_PRODUCT_CONTROL_CHIP_MODULE_ID, M4_PRODUCT_FACTORY_CORE_MODULE_ID,
    M4_PRODUCT_IRON_INGOT_MODULE_ID, M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID,
    M4_PRODUCT_MODULE_RACK_MODULE_ID, M4_PRODUCT_MOTOR_MODULE_ID, M4_PRODUCT_SENSOR_PACK_MODULE_ID,
    MaterialLedgerId, ProductValidationReceiptV1, RejectReason, WorldError, WorldEvent,
    WorldEventBody, WorldTime,
};
use super::World;
use oasis7_agent_api::world_action::ResourceKind;

const FACTORY_BUILD_DECISION_EMIT_KIND: &str = "economy.factory_build_decision";
const RECIPE_EXECUTION_PLAN_EMIT_KIND: &str = "economy.recipe_execution_plan";
const PRODUCT_VALIDATION_EMIT_KIND: &str = "economy.product_validation";
const FACTORY_DURABILITY_PPM_BASE: i64 = 1_000_000;
const FACTORY_DEPRECIATION_PPM_PER_MAINTENANCE_UNIT: i64 = 1_000;
const FACTORY_DEPRECIATION_REASON: &str = "depreciation_tick";
const FACTORY_LOAD_FACTOR_BASE_BPS: i64 = 10_000;
const FACTORY_LOAD_FACTOR_MAX_BPS: i64 = 20_000;
const SURVIVAL_KEYWORDS: &[&str] = &[
    "survival",
    "lifeline",
    "life",
    "repair",
    "maintenance",
    "critical",
    "bootstrap",
    "food",
    "oxygen",
    "water",
];
const ENERGY_KEYWORDS: &[&str] = &["power", "energy", "electricity", "battery", "grid"];
const SCALE_KEYWORDS: &[&str] = &[
    "throughput",
    "automation",
    "industry",
    "production",
    "factory",
];
const EXPANSION_KEYWORDS: &[&str] = &["expand", "outpost", "colony", "colonize", "frontier"];
const CRITICAL_ELECTRICITY_FLOOR: i64 = 5;
const CRITICAL_HARDWARE_PART_FLOOR: i64 = 3;
const BOTTLENECK_LOW_STOCK_THRESHOLDS: &[(&str, i64)] = &[
    ("iron_ingot", 8),
    ("copper_wire", 8),
    ("control_chip", 4),
    ("motor_mk1", 4),
];

pub(super) fn invalid_recipe_plan_stack_note(
    label: &str,
    stacks: &[MaterialStack],
) -> Option<String> {
    stacks
        .iter()
        .find(|stack| stack.kind.trim().is_empty() || stack.amount <= 0)
        .map(|_| format!("recipe plan {label} stack must have non-empty kind and amount > 0"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ProductionQueuePriority {
    Survival = 0,
    Energy = 1,
    Scale = 2,
    Expansion = 3,
}

#[derive(Debug, Clone, Serialize)]
struct ProductValidationModuleCallRequest {
    product_id: String,
    stack: MaterialStack,
    deterministic_seed: u64,
    kind: String,
    amount: i64,
}

#[expect(
    clippy::large_enum_variant,
    reason = "Action resolution variants preserve the existing runtime error/API shape; boxing would change callers."
)]
pub(super) enum EconomyActionResolution {
    Resolved(Action),
    Rejected(RejectReason),
}

impl World {
    // ---------------------------------------------------------------------
    // Economy runtime helpers
    // ---------------------------------------------------------------------

    pub fn pending_factory_builds_len(&self) -> usize {
        self.state.pending_factory_builds.len()
    }

    pub fn pending_recipe_jobs_len(&self) -> usize {
        self.state.pending_recipe_jobs.len()
    }

    pub fn has_factory(&self, factory_id: &str) -> bool {
        self.state.factories.contains_key(factory_id)
    }

    pub(super) fn factory_owner_rejection(
        &self,
        requester_agent_id: &str,
        factory_id: &str,
    ) -> Option<RejectReason> {
        self.state.factories.get(factory_id).and_then(|factory| {
            (factory.builder_agent_id != requester_agent_id).then(|| RejectReason::RuleDenied {
                notes: vec![format!(
                    "schedule recipe requester is not factory builder: factory_id={factory_id} requester={requester_agent_id} builder={}",
                    factory.builder_agent_id
                )],
            })
        })
    }

    pub(super) fn resolve_module_backed_economy_action(
        &mut self,
        envelope: &ActionEnvelope,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<EconomyActionResolution, WorldError> {
        match &envelope.action {
            Action::BuildFactoryWithModule {
                builder_agent_id,
                site_id,
                module_id,
                spec,
            } => {
                if let Some(reason) = self.validate_module_backed_factory_admission(
                    envelope.id,
                    builder_agent_id,
                    site_id,
                    module_id,
                    spec,
                )? {
                    return Ok(EconomyActionResolution::Rejected(reason));
                }
                let preferred_ledger = MaterialLedgerId::agent(builder_agent_id.clone());
                let request_ledger = preferred_ledger;
                let request = FactoryBuildRequest {
                    factory_id: spec.factory_id.clone(),
                    site_id: site_id.clone(),
                    builder: builder_agent_id.clone(),
                    available_inputs: spec
                        .build_cost
                        .iter()
                        .map(|stack| {
                            MaterialStack::new(
                                stack.kind.clone(),
                                self.ledger_material_balance(&request_ledger, stack.kind.as_str()),
                            )
                        })
                        .collect(),
                    available_inputs_by_ledger: Some(self.material_stacks_by_ledger()),
                    available_power: self
                        .agent_resource_balance(builder_agent_id, ResourceKind::Electricity)
                        .unwrap_or(0),
                };
                let mut module_staged = self.clone();
                let decision = module_staged.evaluate_factory_build_with_module(
                    module_id.as_str(),
                    envelope.id,
                    &request,
                    sandbox,
                )?;
                if !decision.accepted {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec![format!(
                                "factory module denied: {}",
                                decision
                                    .reject_reason
                                    .as_deref()
                                    .unwrap_or("build rejected")
                            )],
                        },
                    ));
                }

                let mut resolved_spec = spec.clone();
                if !decision.consume.is_empty() {
                    resolved_spec.build_cost = decision.consume;
                }
                if decision.duration_ticks > 0 {
                    resolved_spec.build_time_ticks = decision.duration_ticks;
                }

                if let WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) =
                    module_staged.build_factory_to_event(
                        envelope.id,
                        builder_agent_id,
                        site_id,
                        &resolved_spec,
                        true,
                    )?
                {
                    return Ok(EconomyActionResolution::Rejected(reason));
                }
                *self = module_staged;

                Ok(EconomyActionResolution::Resolved(Action::BuildFactory {
                    builder_agent_id: builder_agent_id.clone(),
                    site_id: site_id.clone(),
                    spec: resolved_spec,
                }))
            }
            Action::ScheduleRecipeWithModule {
                requester_agent_id,
                factory_id,
                recipe_id,
                module_id,
                desired_batches,
                deterministic_seed,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    ));
                }
                let Some(factory) = self.state.factories.get(factory_id) else {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::FactoryNotFound {
                            factory_id: factory_id.clone(),
                        },
                    ));
                };
                if let Some(reason) = self.factory_owner_rejection(requester_agent_id, factory_id) {
                    return Ok(EconomyActionResolution::Rejected(reason));
                }
                if module_id.trim().is_empty() {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec!["recipe module_id cannot be empty".to_string()],
                        },
                    ));
                }
                if *desired_batches == 0 {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec!["desired_batches must be > 0".to_string()],
                        },
                    ));
                }
                let preferred_ledger = factory.input_ledger.clone();
                let available_inputs = self.ledger_material_stacks(&preferred_ledger);

                let request = RecipeExecutionRequest {
                    recipe_id: recipe_id.clone(),
                    factory_id: factory_id.clone(),
                    desired_batches: *desired_batches,
                    available_inputs,
                    available_inputs_by_ledger: Some(self.material_stacks_by_ledger()),
                    available_power: self
                        .state
                        .agents
                        .get(&factory.builder_agent_id)
                        .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
                        .unwrap_or(0),
                    deterministic_seed: *deterministic_seed,
                };
                let plan = self.evaluate_recipe_with_module(
                    module_id.as_str(),
                    envelope.id,
                    &request,
                    sandbox,
                )?;

                if let Some(reason) = &plan.reject_reason {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec![format!("recipe module denied: {reason}")],
                        },
                    ));
                }
                if plan.accepted_batches > *desired_batches {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec![format!(
                                "recipe module accepted_batches exceeds desired_batches: accepted_batches={} desired_batches={}",
                                plan.accepted_batches, desired_batches
                            )],
                        },
                    ));
                }

                Ok(EconomyActionResolution::Resolved(Action::ScheduleRecipe {
                    requester_agent_id: requester_agent_id.clone(),
                    factory_id: factory_id.clone(),
                    recipe_id: recipe_id.clone(),
                    plan,
                    logistics_route_ids: Vec::new(),
                    logistics_path_ids: Vec::new(),
                }))
            }
            Action::ValidateProductWithModule {
                requester_agent_id,
                module_id,
                stack,
                deterministic_seed,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    ));
                }
                if module_id.trim().is_empty() {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec!["product module_id cannot be empty".to_string()],
                        },
                    ));
                }
                if stack.amount <= 0 {
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied {
                            notes: vec!["product stack amount must be > 0".to_string()],
                        },
                    ));
                }
                let request = ProductValidationRequest {
                    product_id: stack.kind.clone(),
                    stack: stack.clone(),
                    deterministic_seed: *deterministic_seed,
                };
                let decision = self.evaluate_product_with_module(
                    module_id.as_str(),
                    envelope.id,
                    None,
                    &request,
                    sandbox,
                )?;
                if !decision.accepted {
                    let notes = if decision.notes.is_empty() {
                        vec![format!("product module denied: {}", decision.product_id)]
                    } else {
                        decision
                            .notes
                            .iter()
                            .map(|note| format!("product module denied: {note}"))
                            .collect()
                    };
                    return Ok(EconomyActionResolution::Rejected(
                        RejectReason::RuleDenied { notes },
                    ));
                }
                Ok(EconomyActionResolution::Resolved(Action::ValidateProduct {
                    requester_agent_id: requester_agent_id.clone(),
                    module_id: module_id.clone(),
                    stack: stack.clone(),
                    decision,
                }))
            }
            _ => Ok(EconomyActionResolution::Resolved(envelope.action.clone())),
        }
    }

    fn prepare_due_economy_event_bodies_at(&self, now: WorldTime) -> Vec<WorldEventBody> {
        let mut due_builds: Vec<_> = self
            .state
            .pending_factory_builds
            .values()
            .filter(|job| job.ready_at <= now)
            .cloned()
            .collect();
        due_builds.sort_by_key(|job| {
            (
                self.production_priority_for_factory_job(job) as u8,
                job.ready_at,
                job.job_id,
            )
        });

        let mut bodies = Vec::with_capacity(due_builds.len());
        for job in due_builds {
            bodies.push(WorldEventBody::Domain(DomainEvent::FactoryBuilt {
                job_id: job.job_id,
                builder_agent_id: job.builder_agent_id,
                site_id: job.site_id,
                spec: job.spec,
            }));
        }

        let mut due_recipes: Vec<_> = self
            .state
            .pending_recipe_jobs
            .values()
            .filter(|job| job.ready_at <= now)
            .cloned()
            .collect();
        due_recipes.sort_by_key(|job| {
            (
                self.production_priority_for_recipe_job(job) as u8,
                job.ready_at,
                job.job_id,
            )
        });

        bodies.reserve(due_recipes.len());
        for job in due_recipes {
            bodies.push(WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                committed_recipe_origin: job.committed_recipe_origin,
                job_id: job.job_id,
                requester_agent_id: job.requester_agent_id,
                factory_id: job.factory_id,
                recipe_id: job.recipe_id,
                accepted_batches: job.accepted_batches,
                produce: job.produce,
                byproducts: job.byproducts,
                output_ledger: job.output_ledger,
                bottleneck_tags: job.bottleneck_tags,
                logistics_route_ids: job.logistics_route_ids,
                logistics_path_ids: job.logistics_path_ids,
            }));
        }
        bodies
    }

    #[cfg(test)]
    pub(crate) fn prepared_due_economy_event_bodies_for_test(
        &self,
        now: WorldTime,
    ) -> Vec<WorldEventBody> {
        self.prepare_due_economy_event_bodies_at(now)
    }

    pub(super) fn process_due_economy_jobs(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let bodies = self.prepare_due_economy_event_bodies_at(self.state.time);
        let mut emitted = Vec::with_capacity(bodies.len());
        for body in bodies {
            self.append_event(body, None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        Ok(emitted)
    }

    pub(super) fn process_due_economy_jobs_with_modules(
        &mut self,
        sandbox: &mut dyn ModuleSandbox,
        product_validation_checkpoint: &mut dyn FnMut(&World) -> Result<(), WorldError>,
    ) -> Result<Vec<WorldEvent>, WorldError> {
        let now = self.state.time;
        let mut emitted = Vec::new();
        let mut due_builds: Vec<_> = self
            .state
            .pending_factory_builds
            .values()
            .filter(|job| job.ready_at <= now)
            .cloned()
            .collect();
        due_builds.sort_by_key(|job| {
            (
                self.production_priority_for_factory_job(job) as u8,
                job.ready_at,
                job.job_id,
            )
        });

        for job in due_builds {
            self.append_event(
                WorldEventBody::Domain(DomainEvent::FactoryBuilt {
                    job_id: job.job_id,
                    builder_agent_id: job.builder_agent_id,
                    site_id: job.site_id,
                    spec: job.spec,
                }),
                None,
            )?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }

        let mut due_recipes: Vec<_> = self
            .state
            .pending_recipe_jobs
            .values()
            .filter(|job| job.ready_at <= now)
            .cloned()
            .collect();
        due_recipes.sort_by_key(|job| {
            (
                self.production_priority_for_recipe_job(job) as u8,
                job.ready_at,
                job.job_id,
            )
        });

        for job in due_recipes {
            let mut validation_rejected = false;
            let outputs = job.produce.iter().chain(job.byproducts.iter());
            for (output_index, stack) in outputs.enumerate() {
                let validation_index = Some(output_index as u32);
                if let Some(rejected) = self.resume_product_validation_receipt(
                    job.job_id,
                    validation_index,
                    job.requester_agent_id.as_str(),
                    job.factory_id.as_str(),
                    job.recipe_id.as_str(),
                    stack,
                    sandbox,
                    &mut emitted,
                )? {
                    validation_rejected = rejected;
                    if rejected {
                        break;
                    }
                    continue;
                }
                let prior_attempt = self.product_validation_attempt_for_output(
                    job.job_id,
                    validation_index,
                    job.requester_agent_id.as_str(),
                    stack,
                    None,
                )?;
                let Some(module_id) = prior_attempt
                    .as_ref()
                    .map(|attempt| attempt.module_id.clone())
                    .or_else(|| self.resolve_product_module_for_stack(stack.kind.as_str()))
                else {
                    continue;
                };
                let request = ProductValidationRequest {
                    product_id: stack.kind.clone(),
                    stack: stack.clone(),
                    deterministic_seed: job
                        .job_id
                        .wrapping_mul(1_000_003)
                        .wrapping_add(output_index as u64)
                        .wrapping_add(self.state.time),
                };
                let mut failure_detail = None;
                let decision = if let Some(cached) = self.cached_product_validation_decision(
                    job.job_id,
                    validation_index,
                    job.requester_agent_id.as_str(),
                    module_id.as_str(),
                    stack,
                )? {
                    cached
                } else if let Some(attempt) = prior_attempt {
                    if attempt.requester_agent_id != job.requester_agent_id
                        || attempt.module_id != module_id
                        || attempt.stack != *stack
                    {
                        return Err(WorldError::ResourceBalanceInvalid {
                            reason: format!(
                                "product validation retry conflicts with persisted attempt: job_id={} index={validation_index:?}",
                                job.job_id
                            ),
                        });
                    }
                    let (decision, detail) = Self::fail_closed_product_validation(
                        module_id.as_str(),
                        stack,
                        "attempt had no committed decision; module call was not retried",
                    );
                    failure_detail = Some(detail);
                    decision
                } else {
                    self.record_product_validation_attempt(
                        job.job_id,
                        validation_index,
                        job.requester_agent_id.as_str(),
                        module_id.as_str(),
                        stack,
                        sandbox,
                        &mut emitted,
                    )?;
                    product_validation_checkpoint(self)?;
                    match self.evaluate_product_with_module(
                        module_id.as_str(),
                        job.job_id,
                        Some(output_index),
                        &request,
                        sandbox,
                    ) {
                        Ok(decision) => decision,
                        Err(error) => {
                            let (decision, detail) = Self::fail_closed_product_validation(
                                module_id.as_str(),
                                stack,
                                format!("{error:?}"),
                            );
                            failure_detail = Some(detail);
                            decision
                        }
                    }
                };
                let validation_receipt = ProductValidationReceiptV1 {
                    job_id: job.job_id,
                    validation_index,
                    requester_agent_id: job.requester_agent_id.clone(),
                    module_id: module_id.clone(),
                    stack: stack.clone(),
                    decision: decision.clone(),
                    failure_detail: failure_detail.clone(),
                };
                let validation_event = self.action_to_event(&ActionEnvelope {
                    committed_recipe_origin: None,
                    id: job.job_id,
                    action: Action::ValidateProduct {
                        requester_agent_id: job.requester_agent_id.clone(),
                        module_id: module_id.clone(),
                        stack: stack.clone(),
                        decision,
                    },
                })?;
                let rejected = matches!(
                    validation_event,
                    WorldEventBody::Domain(DomainEvent::ActionRejected { .. })
                );
                let recorded_event_id = self.append_event(
                    WorldEventBody::Domain(DomainEvent::ProductValidationRecorded {
                        receipt: validation_receipt,
                    }),
                    None,
                )?;
                let recorded_event_id_era =
                    self.product_validation_event_id_era_after_append(recorded_event_id);
                if let Some(event) = self.journal.events.last().cloned() {
                    self.route_product_validation_event_at(&event, recorded_event_id_era, sandbox)?;
                }
                let validation_event_id = self.append_event(validation_event, None)?;
                let validation_event_id_era =
                    self.product_validation_event_id_era_after_append(validation_event_id);
                if let Some(event) = self.journal.events.last().cloned() {
                    self.route_product_validation_event_at(
                        &event,
                        validation_event_id_era,
                        sandbox,
                    )?;
                }
                if rejected {
                    let blocker_detail = failure_detail.unwrap_or_else(|| {
                        format!(
                            "product validation rejected for {} before production settlement",
                            stack.kind
                        )
                    });
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::FactoryProductionBlocked {
                            action_id: job.job_id,
                            requester_agent_id: job.requester_agent_id.clone(),
                            factory_id: job.factory_id.clone(),
                            recipe_id: job.recipe_id.clone(),
                            blocker_kind: "product_validation".to_string(),
                            blocker_detail,
                        }),
                        None,
                    )?;
                    if let Some(event) = self.journal.events.last() {
                        emitted.push(event.clone());
                    }
                    // The input/power sink happened at RecipeStarted, but a
                    // rejected validation never reaches production settlement.
                    validation_rejected = true;
                    break;
                }
            }

            if validation_rejected {
                continue;
            }
            self.append_event(
                WorldEventBody::Domain(DomainEvent::RecipeCompleted {
                    committed_recipe_origin: job.committed_recipe_origin,
                    job_id: job.job_id,
                    requester_agent_id: job.requester_agent_id,
                    factory_id: job.factory_id,
                    recipe_id: job.recipe_id,
                    accepted_batches: job.accepted_batches,
                    produce: job.produce,
                    byproducts: job.byproducts,
                    output_ledger: job.output_ledger,
                    bottleneck_tags: job.bottleneck_tags,
                    logistics_route_ids: job.logistics_route_ids,
                    logistics_path_ids: job.logistics_path_ids,
                }),
                None,
            )?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        Ok(emitted)
    }

    fn prepare_factory_depreciation_event_bodies(&self) -> Vec<WorldEventBody> {
        let now = self.state.time;
        let mut prepared = Vec::new();
        let mut factories: Vec<_> = self.state.factories.values().cloned().collect();
        factories.sort_by(|lhs, rhs| lhs.factory_id.cmp(&rhs.factory_id));
        let mut active_jobs_by_factory = BTreeMap::<String, i64>::new();
        for job in self.state.pending_recipe_jobs.values() {
            active_jobs_by_factory
                .entry(job.factory_id.clone())
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
        }

        for factory in factories {
            if factory.built_at >= now {
                continue;
            }
            let current = factory.durability_ppm.clamp(0, FACTORY_DURABILITY_PPM_BASE);
            if current <= 0 {
                continue;
            }
            let maintenance_rate = factory.spec.maintenance_per_tick.max(0);
            if maintenance_rate <= 0 {
                continue;
            }
            let base_decay = maintenance_rate
                .saturating_mul(FACTORY_DEPRECIATION_PPM_PER_MAINTENANCE_UNIT)
                .clamp(0, FACTORY_DURABILITY_PPM_BASE);
            if base_decay <= 0 {
                continue;
            }
            let active_jobs = active_jobs_by_factory
                .get(factory.factory_id.as_str())
                .copied()
                .unwrap_or(0);
            let recipe_slots = i64::from(factory.spec.recipe_slots.max(1));
            let utilization_bps = ((active_jobs as i128)
                .saturating_mul(FACTORY_LOAD_FACTOR_BASE_BPS as i128)
                .saturating_div(recipe_slots as i128))
            .clamp(0, FACTORY_LOAD_FACTOR_BASE_BPS as i128)
                as i64;
            let load_factor_bps = FACTORY_LOAD_FACTOR_BASE_BPS
                .saturating_add(utilization_bps)
                .clamp(FACTORY_LOAD_FACTOR_BASE_BPS, FACTORY_LOAD_FACTOR_MAX_BPS);
            let decay = ((base_decay as i128)
                .saturating_mul(load_factor_bps as i128)
                .saturating_div(FACTORY_LOAD_FACTOR_BASE_BPS as i128))
            .clamp(0, FACTORY_DURABILITY_PPM_BASE as i128) as i64;
            if decay <= 0 {
                continue;
            }
            let durability_ppm = current
                .saturating_sub(decay)
                .clamp(0, FACTORY_DURABILITY_PPM_BASE);
            if durability_ppm == current {
                continue;
            }

            prepared.push(WorldEventBody::Domain(
                DomainEvent::FactoryDurabilityChanged {
                    factory_id: factory.factory_id.clone(),
                    previous_durability_ppm: current,
                    durability_ppm,
                    reason: FACTORY_DEPRECIATION_REASON.to_string(),
                },
            ));
        }
        prepared
    }

    #[cfg(test)]
    pub(crate) fn prepare_factory_depreciation_event_bodies_for_test(&self) -> Vec<WorldEventBody> {
        self.prepare_factory_depreciation_event_bodies()
    }

    pub(super) fn process_factory_depreciation(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let prepared = self.prepare_factory_depreciation_event_bodies();
        let mut emitted = Vec::with_capacity(prepared.len());
        for body in prepared {
            self.append_event(body, None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        Ok(emitted)
    }

    fn evaluate_factory_build_with_module(
        &mut self,
        module_id: &str,
        action_id: ActionId,
        request: &FactoryBuildRequest,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<FactoryBuildDecision, WorldError> {
        let trace_id = format!("action-{action_id}-{module_id}-factory");
        let output = self.execute_economy_module_call(module_id, &trace_id, request, sandbox)?;
        if !output.effects.is_empty() {
            return self.economy_module_output_invalid(
                module_id,
                &trace_id,
                "factory module output must not contain effects",
            );
        }
        self.extract_economy_emit(
            module_id,
            &trace_id,
            &output,
            FACTORY_BUILD_DECISION_EMIT_KIND,
        )
    }

    fn evaluate_recipe_with_module(
        &mut self,
        module_id: &str,
        action_id: ActionId,
        request: &RecipeExecutionRequest,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<RecipeExecutionPlan, WorldError> {
        let trace_id = format!("action-{action_id}-{module_id}-recipe");
        let output = self.execute_economy_module_call(module_id, &trace_id, request, sandbox)?;
        if !output.effects.is_empty() {
            return self.economy_module_output_invalid(
                module_id,
                &trace_id,
                "recipe module output must not contain effects",
            );
        }
        self.extract_economy_emit(
            module_id,
            &trace_id,
            &output,
            RECIPE_EXECUTION_PLAN_EMIT_KIND,
        )
    }

    fn production_priority_for_factory_job(
        &self,
        job: &super::super::FactoryBuildJobState,
    ) -> ProductionQueuePriority {
        let mut tokens = vec![
            job.spec.factory_id.to_ascii_lowercase(),
            job.spec.display_name.to_ascii_lowercase(),
        ];
        tokens.extend(job.spec.tags.iter().map(|tag| tag.to_ascii_lowercase()));
        production_priority_from_tokens(
            &tokens,
            self.is_electricity_critical(),
            self.is_hardware_parts_critical(),
            ProductionQueuePriority::Expansion,
        )
    }

    fn production_priority_for_recipe_job(
        &self,
        job: &super::super::RecipeJobState,
    ) -> ProductionQueuePriority {
        let mut tokens = vec![job.recipe_id.to_ascii_lowercase()];
        tokens.extend(
            job.produce
                .iter()
                .map(|stack| stack.kind.to_ascii_lowercase()),
        );
        tokens.extend(
            job.byproducts
                .iter()
                .map(|stack| stack.kind.to_ascii_lowercase()),
        );
        let baseline = self
            .production_priority_from_recipe_role_tags(job)
            .unwrap_or_else(|| {
                production_priority_from_tokens(
                    &tokens,
                    self.is_electricity_critical(),
                    self.is_hardware_parts_critical(),
                    ProductionQueuePriority::Scale,
                )
            });
        if self.is_bottleneck_pressure_active(&job.bottleneck_tags) {
            return bump_recipe_priority_under_bottleneck(baseline);
        }
        baseline
    }

    fn production_priority_from_recipe_role_tags(
        &self,
        job: &super::super::RecipeJobState,
    ) -> Option<ProductionQueuePriority> {
        job.produce
            .iter()
            .chain(job.byproducts.iter())
            .filter_map(|stack| self.product_profile(stack.kind.as_str()))
            .filter_map(|profile| production_priority_from_role_tag(profile.role_tag.as_str()))
            .min()
    }

    fn is_electricity_critical(&self) -> bool {
        self.resource_balance(ResourceKind::Electricity) <= CRITICAL_ELECTRICITY_FLOOR
    }

    fn is_hardware_parts_critical(&self) -> bool {
        self.material_balance("hardware_part") <= CRITICAL_HARDWARE_PART_FLOOR
    }

    fn is_bottleneck_pressure_active(&self, bottleneck_tags: &[String]) -> bool {
        bottleneck_tags.iter().any(|tag| {
            self.total_material_balance(tag.as_str()) <= bottleneck_low_stock_threshold(tag)
        })
    }

    fn total_material_balance(&self, material_kind: &str) -> i64 {
        self.state
            .material_ledgers
            .values()
            .filter_map(|ledger| ledger.get(material_kind))
            .copied()
            .sum()
    }

    fn evaluate_product_with_module(
        &mut self,
        module_id: &str,
        action_id: ActionId,
        validation_index: Option<usize>,
        request: &ProductValidationRequest,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ProductValidationDecision, WorldError> {
        let trace_id = validation_index.map_or_else(
            || format!("action-{action_id}-{module_id}-product"),
            |index| format!("action-{action_id}-{module_id}-product-{index}"),
        );
        // Keep backward compatibility for product modules that decode legacy stack-only payloads.
        let wire_request = ProductValidationModuleCallRequest {
            product_id: request.product_id.clone(),
            stack: request.stack.clone(),
            deterministic_seed: request.deterministic_seed,
            kind: request.stack.kind.clone(),
            amount: request.stack.amount,
        };
        let output =
            self.execute_economy_module_call(module_id, &trace_id, &wire_request, sandbox)?;
        if !output.effects.is_empty() {
            return self.economy_module_output_invalid(
                module_id,
                &trace_id,
                "product module output must not contain effects",
            );
        }
        self.extract_economy_emit(module_id, &trace_id, &output, PRODUCT_VALIDATION_EMIT_KIND)
    }

    fn resolve_product_module_for_stack(&self, product_kind: &str) -> Option<String> {
        if let Some(module_id) = Self::builtin_product_module_for_kind(product_kind)
            && self.module_registry.active.contains_key(module_id)
        {
            return Some(module_id.to_string());
        }
        let suffix = format!(".{product_kind}");
        self.module_registry
            .active
            .keys()
            .find(|module_id| {
                module_id.contains(".product.") && module_id.ends_with(suffix.as_str())
            })
            .cloned()
    }

    fn builtin_product_module_for_kind(product_kind: &str) -> Option<&'static str> {
        match product_kind {
            "iron_ingot" => Some(M4_PRODUCT_IRON_INGOT_MODULE_ID),
            "alloy_plate" => Some(M4_PRODUCT_ALLOY_PLATE_MODULE_ID),
            "control_chip" => Some(M4_PRODUCT_CONTROL_CHIP_MODULE_ID),
            "motor_mk1" => Some(M4_PRODUCT_MOTOR_MODULE_ID),
            "logistics_drone" => Some(M4_PRODUCT_LOGISTICS_DRONE_MODULE_ID),
            "sensor_pack" => Some(M4_PRODUCT_SENSOR_PACK_MODULE_ID),
            "module_rack" => Some(M4_PRODUCT_MODULE_RACK_MODULE_ID),
            "factory_core" => Some(M4_PRODUCT_FACTORY_CORE_MODULE_ID),
            _ => None,
        }
    }

    fn execute_economy_module_call<T: Serialize>(
        &mut self,
        module_id: &str,
        trace_id: &str,
        request: &T,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<ModuleOutput, WorldError> {
        let manifest = self.active_module_manifest(module_id)?.clone();
        let world_config_hash = self.current_manifest_hash()?;
        let module_manifest_hash = super::super::util::hash_json(&manifest)?;
        let action_bytes = to_canonical_cbor(request)?;
        let ctx = ModuleContext {
            v: "wasm-1".to_string(),
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            time: self.state.time,
            origin: ModuleCallOrigin {
                kind: "action".to_string(),
                id: trace_id.to_string(),
            },
            caller: ModuleCallCaller::LegacyUnspecified,
            limits: manifest.limits.clone(),
            stage: Some("economy_action".to_string()),
            world_config_hash: Some(world_config_hash.clone()),
            manifest_hash: Some(module_manifest_hash),
            journal_height: Some(self.journal.events.len() as u64),
            module_version: Some(manifest.version.clone()),
            module_kind: Some(match manifest.kind {
                ModuleKind::Reducer => "reducer".to_string(),
                ModuleKind::Pure => "pure".to_string(),
            }),
            module_role: Some(match manifest.role {
                super::super::ModuleRole::Rule => "rule".to_string(),
                super::super::ModuleRole::Domain => "domain".to_string(),
                super::super::ModuleRole::Gameplay => "gameplay".to_string(),
                super::super::ModuleRole::Body => "body".to_string(),
                super::super::ModuleRole::AgentInternal => "agent_internal".to_string(),
            }),
        };
        let state = match manifest.kind {
            ModuleKind::Reducer => Some(
                self.state
                    .module_states
                    .get(module_id)
                    .cloned()
                    .unwrap_or_default(),
            ),
            ModuleKind::Pure => None,
        };
        let input = ModuleCallInput {
            ctx,
            event: None,
            action: Some(action_bytes),
            state,
        };
        let input_bytes = to_canonical_cbor(&input)?;
        // Economy evaluation runs inside the outer staged action world; avoid
        // re-entering the public clone-and-publish boundary here.
        self.execute_module_call_with_manifest_and_state_key(
            module_id,
            module_id,
            &manifest,
            trace_id.to_string(),
            input_bytes,
            sandbox,
        )
    }

    fn extract_economy_emit<T: DeserializeOwned>(
        &mut self,
        module_id: &str,
        trace_id: &str,
        output: &ModuleOutput,
        expected_emit_kind: &str,
    ) -> Result<T, WorldError> {
        let mut payload = None;
        for emit in &output.emits {
            if emit.kind != expected_emit_kind {
                continue;
            }
            if payload.is_some() {
                return self.economy_module_output_invalid(
                    module_id,
                    trace_id,
                    format!("multiple {expected_emit_kind} emits in module output"),
                );
            }
            payload = Some(emit.payload.clone());
        }
        let Some(payload) = payload else {
            return self.economy_module_output_invalid(
                module_id,
                trace_id,
                format!("missing {expected_emit_kind} emit in module output"),
            );
        };
        match serde_json::from_value(payload) {
            Ok(parsed) => Ok(parsed),
            Err(err) => self.economy_module_output_invalid(
                module_id,
                trace_id,
                format!("decode {expected_emit_kind} failed: {err}"),
            ),
        }
    }

    fn economy_module_output_invalid<T>(
        &mut self,
        module_id: &str,
        trace_id: &str,
        detail: impl Into<String>,
    ) -> Result<T, WorldError> {
        let failure = ModuleCallFailure {
            module_id: module_id.to_string(),
            trace_id: trace_id.to_string(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail: detail.into(),
        };
        self.append_event(WorldEventBody::ModuleCallFailed(failure.clone()), None)?;
        Err(WorldError::ModuleCallFailed {
            module_id: failure.module_id,
            trace_id: failure.trace_id,
            code: failure.code,
            detail: failure.detail,
        })
    }

    fn material_stacks_by_ledger(&self) -> BTreeMap<String, Vec<MaterialStack>> {
        self.state
            .material_ledgers
            .keys()
            .map(|ledger_id| {
                (
                    ledger_id.to_string(),
                    self.ledger_material_stacks(ledger_id),
                )
            })
            .collect()
    }
}

fn production_priority_from_tokens(
    tokens: &[String],
    electricity_critical: bool,
    hardware_critical: bool,
    default_priority: ProductionQueuePriority,
) -> ProductionQueuePriority {
    if has_any_keyword(tokens, SURVIVAL_KEYWORDS) {
        return ProductionQueuePriority::Survival;
    }
    if hardware_critical && has_keyword(tokens, "repair") {
        return ProductionQueuePriority::Survival;
    }
    if has_any_keyword(tokens, ENERGY_KEYWORDS) {
        if electricity_critical {
            return ProductionQueuePriority::Survival;
        }
        return ProductionQueuePriority::Energy;
    }
    if has_any_keyword(tokens, SCALE_KEYWORDS) {
        return ProductionQueuePriority::Scale;
    }
    if has_any_keyword(tokens, EXPANSION_KEYWORDS) {
        return ProductionQueuePriority::Expansion;
    }
    default_priority
}

fn bump_recipe_priority_under_bottleneck(
    priority: ProductionQueuePriority,
) -> ProductionQueuePriority {
    match priority {
        ProductionQueuePriority::Survival => ProductionQueuePriority::Survival,
        ProductionQueuePriority::Energy => ProductionQueuePriority::Survival,
        ProductionQueuePriority::Scale => ProductionQueuePriority::Energy,
        ProductionQueuePriority::Expansion => ProductionQueuePriority::Scale,
    }
}

fn bottleneck_low_stock_threshold(tag: &str) -> i64 {
    BOTTLENECK_LOW_STOCK_THRESHOLDS
        .iter()
        .find_map(|(kind, threshold)| (*kind == tag).then_some(*threshold))
        .unwrap_or(4)
}

fn has_any_keyword(tokens: &[String], keywords: &[&str]) -> bool {
    keywords.iter().any(|keyword| has_keyword(tokens, keyword))
}

fn has_keyword(tokens: &[String], keyword: &str) -> bool {
    tokens.iter().any(|token| token.contains(keyword))
}

fn production_priority_from_role_tag(role_tag: &str) -> Option<ProductionQueuePriority> {
    match role_tag.trim().to_ascii_lowercase().as_str() {
        "survival" => Some(ProductionQueuePriority::Survival),
        "energy" => Some(ProductionQueuePriority::Energy),
        "scale" => Some(ProductionQueuePriority::Scale),
        "explore" | "governance" => Some(ProductionQueuePriority::Expansion),
        _ => None,
    }
}
