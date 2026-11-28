use super::*;

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub(super) fn apply_decision_guardrails(
        &self,
        decision: AgentDecision,
        observation: &Observation,
    ) -> (AgentDecision, Option<String>) {
        match decision {
            AgentDecision::Act(action) => {
                let (guarded_action, note) =
                    self.apply_action_guardrails(action, Some(observation));
                (AgentDecision::Act(guarded_action), note)
            }
            AgentDecision::Query(query) => (AgentDecision::Query(query), None),
            AgentDecision::Wait => {
                if let Some((guarded_action, note)) =
                    self.rewrite_wait_to_sustained_production("wait", observation)
                {
                    return (AgentDecision::Act(guarded_action), Some(note));
                }
                (AgentDecision::Wait, None)
            }
            AgentDecision::WaitTicks(ticks) => {
                let wait_label = format!("wait_ticks({ticks})");
                if let Some((guarded_action, note)) =
                    self.rewrite_wait_to_sustained_production(wait_label.as_str(), observation)
                {
                    return (AgentDecision::Act(guarded_action), Some(note));
                }
                (AgentDecision::WaitTicks(ticks), None)
            }
            AgentDecision::ModuleCommand { response } => {
                (AgentDecision::ModuleCommand { response }, None)
            }
        }
    }

    pub(super) fn rewrite_wait_to_sustained_production(
        &self,
        wait_label: &str,
        observation: &Observation,
    ) -> Option<(Action, String)> {
        if !self.recipe_coverage.is_fully_covered() {
            return None;
        }

        let Some(factory_id) = self.preferred_sustained_factory_id() else {
            return Some((
                Action::HarvestRadiation {
                    agent_id: self.agent_id.clone(),
                    max_amount: self.config.harvest_max_amount_cap,
                },
                format!(
                    "recipe coverage complete; {} rewritten to harvest_radiation because no known factory is available for sustained production",
                    wait_label
                ),
            ));
        };
        let recipe_id = self.next_recovery_recipe_id_for_existing_factory();
        let (guarded_action, guard_note) = self.apply_action_guardrails(
            Action::ScheduleRecipe {
                owner: ResourceOwner::Agent {
                    agent_id: self.agent_id.clone(),
                },
                factory_id: factory_id.clone(),
                recipe_id: recipe_id.clone(),
                batches: 1,
            },
            Some(observation),
        );
        let mut notes = vec![format!(
            "recipe coverage complete; {} rewritten to sustained production via schedule_recipe(factory_id={}, recipe_id={}, batches=1)",
            wait_label, factory_id, recipe_id
        )];
        if let Some(guard_note) = guard_note {
            notes.push(guard_note);
        }
        Some((guarded_action, notes.join("; ")))
    }

    pub(super) fn apply_action_guardrails(
        &self,
        action: Action,
        observation: Option<&Observation>,
    ) -> (Action, Option<String>) {
        match action {
            Action::MoveAgent { agent_id, to } if agent_id == self.agent_id => {
                let Some(observation) = observation else {
                    return (Action::MoveAgent { agent_id, to }, None);
                };
                let (move_action, move_note) =
                    self.guarded_move_to_location(to.as_str(), observation);
                self.guard_move_action_with_electricity(
                    move_action,
                    observation,
                    move_note.into_iter().collect(),
                )
            }
            Action::HarvestRadiation {
                agent_id,
                max_amount,
            } if max_amount > self.config.harvest_max_amount_cap => {
                let capped = self.config.harvest_max_amount_cap;
                (
                    Action::HarvestRadiation {
                        agent_id,
                        max_amount: capped,
                    },
                    Some(format!(
                        "harvest_radiation.max_amount clamped: {} -> {}",
                        max_amount, capped
                    )),
                )
            }
            Action::MineCompound {
                owner,
                location_id,
                compound_mass_g,
            } => {
                let Some(observation) = observation else {
                    return (
                        Action::MineCompound {
                            owner,
                            location_id,
                            compound_mass_g,
                        },
                        None,
                    );
                };
                let owner_is_self = matches!(
                    &owner,
                    ResourceOwner::Agent { agent_id } if agent_id == self.agent_id.as_str()
                );
                if !owner_is_self {
                    return (
                        Action::MineCompound {
                            owner,
                            location_id,
                            compound_mass_g,
                        },
                        None,
                    );
                }

                let mut location_id = location_id;
                let mut mine_notes = Vec::new();
                let mut mine_mass_g =
                    compound_mass_g.clamp(1, DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G);
                if mine_mass_g != compound_mass_g {
                    mine_notes.push(format!(
                        "mine_compound.mass clamped by mine_max_per_action_g: {} -> {} (mine_max_per_action_g={})",
                        compound_mass_g, mine_mass_g, DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G
                    ));
                }

                let inferred_current_location_id =
                    Self::current_location_id_from_observation(observation)
                        .or_else(|| {
                            observation
                                .visible_locations
                                .iter()
                                .min_by_key(|location| location.distance_cm)
                                .map(|location| location.location_id.as_str())
                        })
                        .map(str::to_string);
                let location_is_visible = observation
                    .visible_locations
                    .iter()
                    .any(|location| location.location_id == location_id);
                if !location_is_visible {
                    if let Some(current_location_id) = inferred_current_location_id.as_deref() {
                        if current_location_id != location_id.as_str() {
                            mine_notes.push(format!(
                                "mine_compound.location_id normalized by guardrail: requested_location_id={} -> inferred_current_location_id={}",
                                location_id, current_location_id
                            ));
                            location_id = current_location_id.to_string();
                        }
                    } else {
                        mine_notes.push(format!(
                            "mine_compound guardrail rerouted to harvest_radiation: requested_location_id={} is not visible and inferred_current_location_id is unavailable",
                            location_id
                        ));
                        return (
                            Action::HarvestRadiation {
                                agent_id: self.agent_id.clone(),
                                max_amount: self.config.harvest_max_amount_cap,
                            },
                            Some(mine_notes.join("; ")),
                        );
                    }
                }

                if let Some(cooldown_remaining_ticks) = self
                    .mine_depletion_cooldown_remaining_ticks(location_id.as_str(), observation.time)
                {
                    if let Some(alternative_location_id) = self.find_alternative_mine_location(
                        observation,
                        location_id.as_str(),
                        observation.time,
                    ) {
                        let (move_action, move_note) = self.guarded_move_to_location(
                            alternative_location_id.as_str(),
                            observation,
                        );
                        mine_notes.push(format!(
                            "mine_compound cooldown guardrail rerouted to move_agent: location_id={} cooldown_remaining_ticks={} alternative_location={}",
                            location_id, cooldown_remaining_ticks, alternative_location_id
                        ));
                        if let Some(move_note) = move_note {
                            mine_notes.push(move_note);
                        }
                        return self.guard_move_action_with_electricity(
                            move_action,
                            observation,
                            mine_notes,
                        );
                    }
                    mine_notes.push(format!(
                        "mine_compound cooldown guardrail rerouted to harvest_radiation: location_id={} cooldown_remaining_ticks={} and no alternative visible location",
                        location_id, cooldown_remaining_ticks
                    ));
                    return (
                        Action::HarvestRadiation {
                            agent_id: self.agent_id.clone(),
                            max_amount: self.config.harvest_max_amount_cap,
                        },
                        Some(mine_notes.join("; ")),
                    );
                }

                if let Some(known_available) = self
                    .known_compound_availability_by_location
                    .get(location_id.as_str())
                    .copied()
                {
                    if known_available <= 0 {
                        if let Some(alternative_location_id) = self.find_alternative_mine_location(
                            observation,
                            location_id.as_str(),
                            observation.time,
                        ) {
                            let (move_action, move_note) = self.guarded_move_to_location(
                                alternative_location_id.as_str(),
                                observation,
                            );
                            mine_notes.push(format!(
                                "mine_compound depleted location guardrail rerouted to move_agent: location_id={} known_available={} alternative_location={}",
                                location_id, known_available, alternative_location_id
                            ));
                            if let Some(move_note) = move_note {
                                mine_notes.push(move_note);
                            }
                            return self.guard_move_action_with_electricity(
                                move_action,
                                observation,
                                mine_notes,
                            );
                        }
                        mine_notes.push(format!(
                            "mine_compound depleted location guardrail rerouted to harvest_radiation: location_id={} known_available={} and no alternative visible location",
                            location_id, known_available
                        ));
                        return (
                            Action::HarvestRadiation {
                                agent_id: self.agent_id.clone(),
                                max_amount: self.config.harvest_max_amount_cap,
                            },
                            Some(mine_notes.join("; ")),
                        );
                    }

                    if mine_mass_g > known_available {
                        let clamped = known_available.max(1);
                        mine_notes.push(format!(
                            "mine_compound.mass clamped by known_location_compound_available: {} -> {} (location_id={}, known_available={})",
                            mine_mass_g, clamped, location_id, known_available
                        ));
                        mine_mass_g = clamped;
                    }
                }

                if let Some(current_location_id) = inferred_current_location_id.as_deref()
                    && current_location_id != location_id.as_str()
                {
                    let (move_action, move_note) =
                        self.guarded_move_to_location(location_id.as_str(), observation);
                    let mut notes = mine_notes;
                    notes.push(format!(
                            "mine_compound location precheck rerouted to move_agent: current_location={} mine_location={}",
                            current_location_id, location_id
                        ));
                    if let Some(move_note) = move_note {
                        notes.push(move_note);
                    }
                    return self.guard_move_action_with_electricity(
                        move_action,
                        observation,
                        notes,
                    );
                }

                (
                    Action::MineCompound {
                        owner,
                        location_id,
                        compound_mass_g: mine_mass_g,
                    },
                    (!mine_notes.is_empty()).then_some(mine_notes.join("; ")),
                )
            }
            Action::BuildFactory {
                owner,
                location_id,
                factory_id,
                factory_kind,
            } => {
                let Some(observation) = observation else {
                    return (
                        Action::BuildFactory {
                            owner,
                            location_id,
                            factory_id,
                            factory_kind,
                        },
                        None,
                    );
                };
                let owner_is_self = matches!(
                    &owner,
                    ResourceOwner::Agent { agent_id } if agent_id == self.agent_id.as_str()
                );
                if !owner_is_self {
                    return (
                        Action::BuildFactory {
                            owner,
                            location_id,
                            factory_id,
                            factory_kind,
                        },
                        None,
                    );
                }

                let mut location_id = location_id;
                let mut build_notes = Vec::new();
                let inferred_current_location_id =
                    Self::current_location_id_from_observation(observation)
                        .or_else(|| {
                            observation
                                .visible_locations
                                .iter()
                                .min_by_key(|location| location.distance_cm)
                                .map(|location| location.location_id.as_str())
                        })
                        .map(str::to_string);
                let location_is_visible = observation
                    .visible_locations
                    .iter()
                    .any(|location| location.location_id == location_id);
                if !location_is_visible {
                    if let Some(current_location_id) = inferred_current_location_id.as_deref() {
                        if current_location_id != location_id.as_str() {
                            build_notes.push(format!(
                                "build_factory.location_id normalized by guardrail: requested_location_id={} -> inferred_current_location_id={}",
                                location_id, current_location_id
                            ));
                            location_id = current_location_id.to_string();
                        }
                    } else {
                        build_notes.push(format!(
                            "build_factory guardrail rerouted to harvest_radiation: requested_location_id={} is not visible and inferred_current_location_id is unavailable",
                            location_id
                        ));
                        return (
                            Action::HarvestRadiation {
                                agent_id: self.agent_id.clone(),
                                max_amount: self.config.harvest_max_amount_cap,
                            },
                            Some(build_notes.join("; ")),
                        );
                    }
                }

                if let Some(current_location_id) = inferred_current_location_id.as_deref()
                    && current_location_id != location_id.as_str()
                {
                    let (move_action, move_note) =
                        self.guarded_move_to_location(location_id.as_str(), observation);
                    build_notes.push(format!(
                            "build_factory location precheck rerouted to move_agent: current_location={} target_location={}",
                            current_location_id, location_id
                        ));
                    if let Some(move_note) = move_note {
                        build_notes.push(move_note);
                    }
                    return self.guard_move_action_with_electricity(
                        move_action,
                        observation,
                        build_notes,
                    );
                }

                if let Some(existing_factory_id) = self.resolve_existing_factory_id_for_build(
                    factory_id.as_str(),
                    factory_kind.as_str(),
                ) {
                    let existing_factory_kind = self
                        .known_factory_kind_for_id(existing_factory_id.as_str())
                        .unwrap_or_else(|| factory_kind.clone());
                    if let Some(recipe_id) = self
                        .next_recovery_recipe_id_for_factory_kind(existing_factory_kind.as_str())
                    {
                        let (guarded_schedule_action, schedule_note) = self
                            .apply_action_guardrails(
                                Action::ScheduleRecipe {
                                    owner: owner.clone(),
                                    factory_id: existing_factory_id.clone(),
                                    recipe_id: recipe_id.clone(),
                                    batches: 1,
                                },
                                Some(observation),
                            );
                        let mut notes = build_notes;
                        notes.push(format!(
                            "build_factory dedup guardrail rerouted to schedule_recipe: requested_factory_id={} factory_kind={} existing_factory_id={} existing_factory_kind={} recipe_id={}",
                            factory_id, factory_kind, existing_factory_id, existing_factory_kind, recipe_id
                        ));
                        if let Some(schedule_note) = schedule_note {
                            notes.push(schedule_note);
                        }
                        return (guarded_schedule_action, Some(notes.join("; ")));
                    }

                    if let Some((missing_recipe_id, required_factory_kind)) =
                        self.next_missing_recipe_requirement()
                    {
                        if let Some(required_factory_id) =
                            self.canonical_factory_id_for_kind(required_factory_kind.as_str())
                        {
                            let (guarded_schedule_action, schedule_note) = self
                                .apply_action_guardrails(
                                    Action::ScheduleRecipe {
                                        owner: owner.clone(),
                                        factory_id: required_factory_id.clone(),
                                        recipe_id: missing_recipe_id.clone(),
                                        batches: 1,
                                    },
                                    Some(observation),
                                );
                            let mut notes = build_notes;
                            notes.push(format!(
                                "build_factory dedup guardrail rerouted to schedule_recipe on compatible factory: requested_factory_id={} factory_kind={} existing_factory_id={} missing_recipe_id={} required_factory_kind={} required_factory_id={}",
                                factory_id, factory_kind, existing_factory_id, missing_recipe_id, required_factory_kind, required_factory_id
                            ));
                            if let Some(schedule_note) = schedule_note {
                                notes.push(schedule_note);
                            }
                            return (guarded_schedule_action, Some(notes.join("; ")));
                        }

                        if let Some(current_location_id) =
                            Self::current_location_id_from_observation(observation)
                        {
                            let mut notes = build_notes;
                            notes.push(format!(
                                "build_factory dedup guardrail rerouted to build missing compatible factory: requested_factory_id={} factory_kind={} existing_factory_id={} missing_recipe_id={} required_factory_kind={} build_location={}",
                                factory_id, factory_kind, existing_factory_id, missing_recipe_id, required_factory_kind, current_location_id
                            ));
                            return (
                                Action::BuildFactory {
                                    owner,
                                    location_id: current_location_id.to_string(),
                                    factory_id: required_factory_kind.clone(),
                                    factory_kind: required_factory_kind,
                                },
                                Some(notes.join("; ")),
                            );
                        }
                    }

                    let mut notes = build_notes;
                    notes.push(format!(
                        "build_factory dedup guardrail skipped schedule reroute: requested_factory_id={} factory_kind={} existing_factory_id={} has_no_compatible_recipe",
                        factory_id, factory_kind, existing_factory_id
                    ));
                    return (
                        Action::BuildFactory {
                            owner,
                            location_id,
                            factory_id,
                            factory_kind,
                        },
                        Some(notes.join("; ")),
                    );
                }

                (
                    Action::BuildFactory {
                        owner,
                        location_id,
                        factory_id,
                        factory_kind,
                    },
                    (!build_notes.is_empty()).then_some(build_notes.join("; ")),
                )
            }
            Action::ScheduleRecipe {
                owner,
                factory_id,
                recipe_id,
                batches,
            } => {
                let Some(observation) = observation else {
                    return (
                        Action::ScheduleRecipe {
                            owner,
                            factory_id,
                            recipe_id,
                            batches,
                        },
                        None,
                    );
                };
                let Some(mut cost_per_batch) =
                    Self::default_recipe_hardware_cost_per_batch(recipe_id.as_str())
                else {
                    return (
                        Action::ScheduleRecipe {
                            owner,
                            factory_id,
                            recipe_id,
                            batches,
                        },
                        None,
                    );
                };
                if cost_per_batch <= 0 {
                    return (
                        Action::ScheduleRecipe {
                            owner,
                            factory_id,
                            recipe_id,
                            batches,
                        },
                        None,
                    );
                }
                let mut electricity_cost_per_batch =
                    Self::default_recipe_electricity_cost_per_batch(recipe_id.as_str())
                        .unwrap_or(0);

                let owner_is_self = matches!(
                    &owner,
                    ResourceOwner::Agent { agent_id } if agent_id == self.agent_id.as_str()
                );
                if !owner_is_self {
                    return (
                        Action::ScheduleRecipe {
                            owner,
                            factory_id,
                            recipe_id,
                            batches,
                        },
                        None,
                    );
                }

                let mut factory_id = factory_id;
                let mut recipe_id = recipe_id;
                let mut schedule_notes = Vec::new();
                if let Some(canonical_factory_id) =
                    self.normalize_schedule_factory_id(factory_id.as_str())
                {
                    schedule_notes.push(format!(
                        "schedule_recipe factory_id normalized by guardrail: requested_factory_id={} -> canonical_factory_id={}",
                        factory_id, canonical_factory_id
                    ));
                    factory_id = canonical_factory_id;
                }

                if let Some(required_factory_kind) =
                    Self::required_factory_kind_for_recipe(recipe_id.as_str())
                {
                    let factory_kind_mismatch = self
                        .known_factory_kind_for_id(factory_id.as_str())
                        .is_some_and(|known_factory_kind| {
                            known_factory_kind.as_str() != required_factory_kind
                        });
                    let factory_unknown = !self
                        .known_factory_locations
                        .contains_key(factory_id.as_str());
                    if factory_kind_mismatch || factory_unknown {
                        if let Some(canonical_factory_id) =
                            self.canonical_factory_id_for_kind(required_factory_kind)
                        {
                            if canonical_factory_id != factory_id {
                                schedule_notes.push(format!(
                                    "schedule_recipe factory kind compatibility guardrail rerouted factory_id: requested_factory_id={} required_factory_kind={} -> canonical_factory_id={}",
                                    factory_id, required_factory_kind, canonical_factory_id
                                ));
                                factory_id = canonical_factory_id;
                            }
                        } else if let Some(current_location_id) =
                            Self::current_location_id_from_observation(observation)
                        {
                            let mut notes = schedule_notes;
                            notes.push(format!(
                                "schedule_recipe missing required factory kind rerouted to build_factory: recipe_id={} required_factory_kind={} requested_factory_id={} build_location={}",
                                recipe_id, required_factory_kind, factory_id, current_location_id
                            ));
                            return (
                                Action::BuildFactory {
                                    owner,
                                    location_id: current_location_id.to_string(),
                                    factory_id: required_factory_kind.to_string(),
                                    factory_kind: required_factory_kind.to_string(),
                                },
                                Some(notes.join("; ")),
                            );
                        } else {
                            schedule_notes.push(format!(
                                "schedule_recipe missing required factory kind detected but current_location_id is unknown; keep original action for downstream recovery: recipe_id={} required_factory_kind={} requested_factory_id={}",
                                recipe_id, required_factory_kind, factory_id
                            ));
                        }
                    }
                }

                if self.recipe_coverage.is_completed(recipe_id.as_str())
                    && let Some(factory_kind) = self
                        .known_factory_kind_for_id(factory_id.as_str())
                        .or_else(|| {
                            Self::required_factory_kind_for_recipe(recipe_id.as_str())
                                .map(str::to_string)
                        })
                {
                    if let Some(next_recipe_id) = self
                        .recipe_coverage
                        .next_uncovered_recipe_for_factory_kind_excluding(
                            factory_kind.as_str(),
                            recipe_id.as_str(),
                        )
                    {
                        schedule_notes.push(format!(
                                "schedule_recipe coverage hard-switch applied: completed_recipe={} -> next_uncovered_recipe={} switch_factory_id={} target_factory_kind={}",
                                recipe_id, next_recipe_id, factory_id, factory_kind
                            ));
                        recipe_id = next_recipe_id;
                        if let Some(next_cost_per_batch) =
                            Self::default_recipe_hardware_cost_per_batch(recipe_id.as_str())
                        {
                            cost_per_batch = next_cost_per_batch;
                        }
                        electricity_cost_per_batch =
                            Self::default_recipe_electricity_cost_per_batch(recipe_id.as_str())
                                .unwrap_or(0);
                    } else if let Some((next_recipe_id, required_factory_kind)) =
                        self.next_missing_recipe_requirement()
                        && required_factory_kind != factory_kind
                    {
                        if let Some(next_factory_id) =
                            self.canonical_factory_id_for_kind(required_factory_kind.as_str())
                        {
                            schedule_notes.push(format!(
                                        "schedule_recipe coverage hard-switch applied: completed_recipe={} -> next_uncovered_recipe={} switch_factory_id={} target_factory_kind={}",
                                        recipe_id, next_recipe_id, next_factory_id, required_factory_kind
                                    ));
                            factory_id = next_factory_id;
                            recipe_id = next_recipe_id;
                            if let Some(next_cost_per_batch) =
                                Self::default_recipe_hardware_cost_per_batch(recipe_id.as_str())
                            {
                                cost_per_batch = next_cost_per_batch;
                            }
                            electricity_cost_per_batch =
                                Self::default_recipe_electricity_cost_per_batch(recipe_id.as_str())
                                    .unwrap_or(0);
                        } else if let Some(current_location_id) =
                            Self::current_location_id_from_observation(observation)
                        {
                            let mut notes = schedule_notes.clone();
                            notes.push(format!(
                                        "schedule_recipe coverage hard-switch rerouted to build_factory: completed_recipe={} next_uncovered_recipe={} required_factory_kind={} build_location={}",
                                        recipe_id, next_recipe_id, required_factory_kind, current_location_id
                                    ));
                            return (
                                Action::BuildFactory {
                                    owner,
                                    location_id: current_location_id.to_string(),
                                    factory_id: required_factory_kind.clone(),
                                    factory_kind: required_factory_kind,
                                },
                                Some(notes.join("; ")),
                            );
                        }
                    }
                }
                if let Some(factory_location_id) =
                    self.known_factory_locations.get(factory_id.as_str())
                    && let Some(current_location_id) =
                        Self::current_location_id_from_observation(observation)
                    && current_location_id != factory_location_id
                {
                    let (move_action, move_note) =
                        self.guarded_move_to_location(factory_location_id.as_str(), observation);
                    let mut notes = schedule_notes.clone();
                    notes.push(format!(
                                "schedule_recipe factory location precheck rerouted to move_agent: current_location={} factory_location={}",
                                current_location_id, factory_location_id
                            ));
                    if let Some(move_note) = move_note {
                        notes.push(move_note);
                    }
                    return self.guard_move_action_with_electricity(
                        move_action,
                        observation,
                        notes,
                    );
                }

                let available_hardware = observation.self_resources.get(ResourceKind::Data);
                let available_electricity =
                    observation.self_resources.get(ResourceKind::Electricity);
                let available_compound = observation.self_resources.get(ResourceKind::Data);
                if available_hardware < cost_per_batch {
                    let hardware_shortfall = cost_per_batch.saturating_sub(available_hardware);
                    let target_recovery_mass_g = hardware_shortfall
                        .saturating_mul(DEFAULT_REFINE_RECOVERY_MASS_G_PER_HARDWARE)
                        .max(DEFAULT_REFINE_RECOVERY_MASS_G_PER_HARDWARE);
                    let recovery_mass_g = target_recovery_mass_g.clamp(
                        DEFAULT_REFINE_RECOVERY_MASS_G_PER_HARDWARE,
                        DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G,
                    );
                    let capped_from = (target_recovery_mass_g > recovery_mass_g)
                        .then_some(target_recovery_mass_g);
                    let missing_compound_g = recovery_mass_g.saturating_sub(available_compound);
                    if missing_compound_g > 0 {
                        let mine_mass_g =
                            missing_compound_g.clamp(1, DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G);
                        let mine_required_electricity = ((mine_mass_g + 999) / 1000)
                            .saturating_mul(DEFAULT_MINE_ELECTRICITY_COST_PER_KG);
                        if available_electricity >= mine_required_electricity {
                            if let Some(current_location_id) =
                                Self::current_location_id_from_observation(observation)
                            {
                                return (
                                    Action::MineCompound {
                                        owner,
                                        location_id: current_location_id.to_string(),
                                        compound_mass_g: mine_mass_g,
                                    },
                                    Some({
                                        let mut notes = schedule_notes.clone();
                                        notes.push(format!(
                                            "schedule_recipe guardrail rerouted to mine_compound before refine: available_hardware={} < recipe_hardware_cost_per_batch={}; hardware_shortfall={}; recovery_mass_g={}{}; available_compound={}; mine_mass_g={}",
                                            available_hardware,
                                            cost_per_batch,
                                            hardware_shortfall,
                                            recovery_mass_g,
                                            capped_from
                                                .map(|from| format!(" (capped_from={} by mine_max_per_action_g={})", from, DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G))
                                                .unwrap_or_default(),
                                            available_compound,
                                            mine_mass_g
                                        ));
                                        notes.join("; ")
                                    }),
                                );
                            }
                            return (
                                Action::HarvestRadiation {
                                    agent_id: self.agent_id.clone(),
                                    max_amount: self.config.harvest_max_amount_cap,
                                },
                                Some({
                                    let mut notes = schedule_notes.clone();
                                    notes.push(format!(
                                        "schedule_recipe guardrail rerouted to harvest_radiation: available_hardware={} < recipe_hardware_cost_per_batch={} and available_compound={} < recovery_mass_g={} but current_location_id is unknown for mine_compound",
                                        available_hardware,
                                        cost_per_batch,
                                        available_compound,
                                        recovery_mass_g
                                    ));
                                    notes.join("; ")
                                }),
                            );
                        } else {
                            return (
                                Action::HarvestRadiation {
                                    agent_id: self.agent_id.clone(),
                                    max_amount: self.config.harvest_max_amount_cap,
                                },
                                Some({
                                    let mut notes = schedule_notes.clone();
                                    notes.push(format!(
                                        "schedule_recipe guardrail rerouted to harvest_radiation: available_hardware={} < recipe_hardware_cost_per_batch={} and available_compound={} < recovery_mass_g={} with available_electricity={} < mine_required_electricity={}",
                                        available_hardware,
                                        cost_per_batch,
                                        available_compound,
                                        recovery_mass_g,
                                        available_electricity,
                                        mine_required_electricity
                                    ));
                                    notes.join("; ")
                                }),
                            );
                        }
                    }

                    let required_refine_electricity = ((recovery_mass_g + 999) / 1000)
                        .saturating_mul(DEFAULT_REFINE_ELECTRICITY_COST_PER_KG);
                    if available_electricity >= required_refine_electricity {
                        return (
                            Action::RefineCompound {
                                owner,
                                compound_mass_g: recovery_mass_g,
                            },
                            Some({
                                let mut notes = schedule_notes.clone();
                                notes.push(format!(
                                    "schedule_recipe guardrail rerouted to refine_compound: available_hardware={} < recipe_hardware_cost_per_batch={}; hardware_shortfall={}; recovery_mass_g={}{}",
                                    available_hardware,
                                    cost_per_batch,
                                    hardware_shortfall,
                                    recovery_mass_g,
                                    capped_from
                                        .map(|from| format!(" (capped_from={} by mine_max_per_action_g={})", from, DEFAULT_MINE_COMPOUND_MAX_PER_ACTION_G))
                                        .unwrap_or_default()
                                ));
                                notes.join("; ")
                            }),
                        );
                    }
                    return (
                        Action::HarvestRadiation {
                            agent_id: self.agent_id.clone(),
                            max_amount: self.config.harvest_max_amount_cap,
                        },
                        Some({
                            let mut notes = schedule_notes.clone();
                            notes.push(format!(
                                "schedule_recipe guardrail rerouted to harvest_radiation: available_hardware={} < recipe_hardware_cost_per_batch={} and available_electricity={} < refine_required_electricity={} (recovery_mass_g={})",
                                available_hardware, cost_per_batch, available_electricity, required_refine_electricity, recovery_mass_g
                            ));
                            notes.join("; ")
                        }),
                    );
                }

                if electricity_cost_per_batch > 0
                    && available_electricity < electricity_cost_per_batch
                {
                    return (
                        Action::HarvestRadiation {
                            agent_id: self.agent_id.clone(),
                            max_amount: self.config.harvest_max_amount_cap,
                        },
                        Some({
                            let mut notes = schedule_notes.clone();
                            notes.push(format!(
                                "schedule_recipe electricity precheck rerouted to harvest_radiation: available_electricity={} < recipe_electricity_cost_per_batch={} (recipe_id={})",
                                available_electricity, electricity_cost_per_batch, recipe_id
                            ));
                            notes.join("; ")
                        }),
                    );
                }

                let requested_batches = batches.max(1);
                let max_batches_by_hardware = available_hardware / cost_per_batch;
                let max_batches_by_electricity = if electricity_cost_per_batch > 0 {
                    available_electricity / electricity_cost_per_batch
                } else {
                    i64::MAX
                };
                let max_batches = max_batches_by_hardware.min(max_batches_by_electricity);

                if requested_batches > max_batches {
                    (
                        Action::ScheduleRecipe {
                            owner,
                            factory_id,
                            recipe_id,
                            batches: max_batches,
                        },
                        Some({
                            let mut notes = schedule_notes;
                            notes.push(format!(
                                "schedule_recipe.batches clamped by resource guardrail: {} -> {} (available_hardware={}, recipe_hardware_cost_per_batch={}, available_electricity={}, recipe_electricity_cost_per_batch={})",
                                requested_batches, max_batches, available_hardware, cost_per_batch, available_electricity, electricity_cost_per_batch
                            ));
                            notes.join("; ")
                        }),
                    )
                } else {
                    (
                        Action::ScheduleRecipe {
                            owner,
                            factory_id,
                            recipe_id,
                            batches: requested_batches,
                        },
                        (!schedule_notes.is_empty()).then_some(schedule_notes.join("; ")),
                    )
                }
            }
            other => (other, None),
        }
    }

    pub(super) fn apply_execute_until_guardrails(
        &self,
        mut directive: ExecuteUntilDirective,
        observation: &Observation,
    ) -> (ExecuteUntilDirective, Option<String>) {
        let mut notes = Vec::new();
        let original_action = directive.action.clone();
        let original_action_label = Self::action_label_for_rewrite(&original_action);
        let (guarded_action, action_note) =
            self.apply_action_guardrails(directive.action, Some(observation));
        directive.action = guarded_action;
        if let Some(action_note) = action_note {
            notes.push(action_note);
        }

        let rewritten_action_label = Self::action_label_for_rewrite(&directive.action);
        if original_action_label != rewritten_action_label {
            let previous_until_summary =
                summarize_execute_until_conditions(&directive.until_conditions);
            directive.until_conditions =
                default_execute_until_conditions_for_action(&directive.action);
            let rebuilt_until_summary =
                summarize_execute_until_conditions(&directive.until_conditions);
            notes.push(format!(
                "execute_until.until rebuilt after action guardrail rewrite: action={} -> {}; until={} -> {}",
                original_action_label, rewritten_action_label, previous_until_summary, rebuilt_until_summary
            ));
        }

        if matches!(directive.action, Action::HarvestRadiation { .. })
            && directive.max_ticks > DEFAULT_LLM_HARVEST_EXECUTE_UNTIL_MAX_TICKS
        {
            let original = directive.max_ticks;
            directive.max_ticks = DEFAULT_LLM_HARVEST_EXECUTE_UNTIL_MAX_TICKS;
            notes.push(format!(
                "execute_until.max_ticks clamped for harvest_radiation: {} -> {}; force replan sooner to avoid long harvest tail",
                original, directive.max_ticks
            ));
        }

        let note = if notes.is_empty() {
            None
        } else {
            Some(notes.join("; "))
        };
        (directive, note)
    }
}
