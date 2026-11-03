use super::*;

impl<C: LlmCompletionClient> LlmAgentBehavior<C> {
    pub(super) fn required_factory_kind_for_recipe(recipe_id: &str) -> Option<&'static str> {
        match recipe_id.trim() {
            "recipe.smelter.iron_ingot"
            | "recipe.smelter.copper_wire"
            | "recipe.smelter.polymer_resin"
            | "recipe.smelter.alloy_plate" => Some("factory.smelter.mk1"),
            "recipe.assembler.gear"
            | "recipe.assembler.control_chip"
            | "recipe.assembler.motor_mk1"
            | "recipe.assembler.logistics_drone"
            | "recipe.assembler.sensor_pack"
            | "recipe.assembler.module_rack"
            | "recipe.assembler.factory_core" => Some("factory.assembler.mk1"),
            _ => None,
        }
    }

    pub(super) fn default_recipe_hardware_cost_per_batch(recipe_id: &str) -> Option<i64> {
        match recipe_id.trim() {
            "recipe.smelter.iron_ingot"
            | "recipe.smelter.copper_wire"
            | "recipe.smelter.polymer_resin"
            | "recipe.assembler.gear"
            | "recipe.assembler.control_chip" => Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH),
            "recipe.smelter.alloy_plate"
            | "recipe.assembler.motor_mk1"
            | "recipe.assembler.sensor_pack" => Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH * 2),
            "recipe.assembler.module_rack" => Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH * 3),
            "recipe.assembler.logistics_drone" | "recipe.assembler.factory_core" => {
                Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH * 4)
            }
            _ => None,
        }
    }

    pub(super) fn default_recipe_electricity_cost_per_batch(recipe_id: &str) -> Option<i64> {
        match recipe_id.trim() {
            "recipe.assembler.gear" => Some(4),
            "recipe.assembler.control_chip" => Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH),
            "recipe.smelter.iron_ingot"
            | "recipe.smelter.copper_wire"
            | "recipe.smelter.polymer_resin" => Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH),
            "recipe.assembler.motor_mk1" => Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH * 2),
            "recipe.smelter.alloy_plate" => Some(9),
            "recipe.assembler.sensor_pack" => Some(8),
            "recipe.assembler.module_rack" => Some(10),
            "recipe.assembler.logistics_drone" => {
                Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH * 4)
            }
            "recipe.assembler.factory_core" => Some(14),
            _ => None,
        }
    }

    pub(super) fn current_location_id_from_observation(observation: &Observation) -> Option<&str> {
        observation
            .visible_locations
            .iter()
            .find(|location| location.distance_cm == 0)
            .map(|location| location.location_id.as_str())
    }

    pub(super) fn normalize_schedule_factory_id(&self, factory_id: &str) -> Option<String> {
        let requested_factory_id = factory_id.trim();
        if requested_factory_id.is_empty()
            || self
                .known_factory_locations
                .contains_key(requested_factory_id)
        {
            return None;
        }
        self.known_factory_kind_aliases
            .get(requested_factory_id)
            .filter(|canonical_factory_id| {
                self.known_factory_locations
                    .contains_key(canonical_factory_id.as_str())
            })
            .cloned()
            .or_else(|| {
                self.known_factory_kind_aliases
                    .iter()
                    .find(|(factory_kind, canonical_factory_id)| {
                        self.known_factory_locations
                            .contains_key(canonical_factory_id.as_str())
                            && requested_factory_id.starts_with(format!("{factory_kind}.").as_str())
                    })
                    .map(|(_, canonical_factory_id)| canonical_factory_id.clone())
            })
    }

    pub(super) fn canonical_factory_id_for_kind(&self, factory_kind: &str) -> Option<String> {
        let requested_factory_kind = factory_kind.trim();
        if requested_factory_kind.is_empty() {
            return None;
        }
        self.known_factory_kind_aliases
            .get(requested_factory_kind)
            .filter(|canonical_factory_id| {
                self.known_factory_locations
                    .contains_key(canonical_factory_id.as_str())
            })
            .cloned()
    }

    pub(super) fn known_factory_kind_for_id(&self, factory_id: &str) -> Option<String> {
        let factory_id = factory_id.trim();
        if factory_id.is_empty() {
            return None;
        }
        if let Some(factory_kind) = self.known_factory_kinds_by_id.get(factory_id) {
            return Some(factory_kind.clone());
        }
        self.known_factory_kind_aliases
            .iter()
            .find(|(_, canonical_factory_id)| canonical_factory_id.as_str() == factory_id)
            .map(|(factory_kind, _)| factory_kind.clone())
    }

    pub(super) fn resolve_existing_factory_id_for_build(
        &self,
        factory_id: &str,
        factory_kind: &str,
    ) -> Option<String> {
        let requested_factory_id = factory_id.trim();
        if !requested_factory_id.is_empty()
            && self
                .known_factory_locations
                .contains_key(requested_factory_id)
        {
            return Some(requested_factory_id.to_string());
        }
        let requested_factory_kind = factory_kind.trim();
        if requested_factory_kind.is_empty() {
            return None;
        }
        self.canonical_factory_id_for_kind(requested_factory_kind)
            .or_else(|| {
                self.normalize_schedule_factory_id(requested_factory_id)
                    .filter(|factory_id| {
                        self.known_factory_kind_for_id(factory_id.as_str())
                            .as_deref()
                            == Some(requested_factory_kind)
                    })
            })
    }

    pub(super) fn next_recovery_recipe_id_for_existing_factory(&self) -> String {
        self.next_recovery_recipe_id_for_factory_kind("factory.assembler.mk1")
            .unwrap_or_else(|| TRACKED_RECIPE_IDS[0].to_string())
    }

    pub(super) fn next_recovery_recipe_id_for_factory_kind(
        &self,
        factory_kind: &str,
    ) -> Option<String> {
        self.recipe_coverage
            .missing_recipe_ids()
            .into_iter()
            .find(|recipe_id| {
                Self::required_factory_kind_for_recipe(recipe_id.as_str()) == Some(factory_kind)
            })
            .or_else(|| {
                TRACKED_RECIPE_IDS
                    .iter()
                    .find(|recipe_id| {
                        Self::required_factory_kind_for_recipe(recipe_id) == Some(factory_kind)
                    })
                    .map(|recipe_id| (*recipe_id).to_string())
            })
    }

    pub(super) fn next_missing_recipe_requirement(&self) -> Option<(String, String)> {
        self.recipe_coverage
            .missing_recipe_ids()
            .into_iter()
            .find_map(|recipe_id| {
                Self::required_factory_kind_for_recipe(recipe_id.as_str())
                    .map(|required_factory_kind| (recipe_id, required_factory_kind.to_string()))
            })
    }

    pub(super) fn preferred_sustained_factory_id(&self) -> Option<String> {
        self.canonical_factory_id_for_kind("factory.assembler.mk1")
            .or_else(|| self.known_factory_locations.keys().next().cloned())
    }

    pub(super) fn find_reachable_move_relay(
        &self,
        to: &str,
        observation: &Observation,
    ) -> Option<(String, i64, i64, i64)> {
        if DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK <= 0 {
            return None;
        }
        let target_location = observation
            .visible_locations
            .iter()
            .find(|location| location.location_id == to)?;
        if target_location.distance_cm <= DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK {
            return None;
        }

        let mut best: Option<(String, i64, i64)> = None;
        for candidate in &observation.visible_locations {
            if candidate.location_id == target_location.location_id {
                continue;
            }
            if candidate.distance_cm <= 0
                || candidate.distance_cm > DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK
            {
                continue;
            }

            let candidate_to_target =
                crate::geometry::space_distance_cm(candidate.pos, target_location.pos);
            if candidate_to_target >= target_location.distance_cm {
                continue;
            }

            let should_replace = match &best {
                None => true,
                Some((_, best_candidate_to_target, best_distance_from_self)) => {
                    candidate_to_target < *best_candidate_to_target
                        || (candidate_to_target == *best_candidate_to_target
                            && candidate.distance_cm < *best_distance_from_self)
                }
            };

            if should_replace {
                best = Some((
                    candidate.location_id.clone(),
                    candidate_to_target,
                    candidate.distance_cm,
                ));
            }
        }

        best.map(
            |(relay_location_id, relay_to_target_distance, relay_distance_from_self)| {
                (
                    relay_location_id,
                    target_location.distance_cm,
                    relay_distance_from_self,
                    relay_to_target_distance,
                )
            },
        )
    }

    pub(super) fn find_exploration_move_relay(
        &self,
        to: &str,
        observation: &Observation,
    ) -> Option<(String, i64)> {
        if DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK <= 0 {
            return None;
        }
        let mut best: Option<(String, i64)> = None;
        for candidate in &observation.visible_locations {
            if candidate.location_id == to
                || candidate.distance_cm <= 0
                || candidate.distance_cm > DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK
            {
                continue;
            }
            let should_replace = match &best {
                None => true,
                Some((best_location_id, best_distance)) => {
                    candidate.distance_cm > *best_distance
                        || (candidate.distance_cm == *best_distance
                            && candidate.location_id < *best_location_id)
                }
            };
            if should_replace {
                best = Some((candidate.location_id.clone(), candidate.distance_cm));
            }
        }
        best
    }

    pub(super) fn find_alternative_mine_location(
        &self,
        observation: &Observation,
        depleted_location_id: &str,
        current_time: u64,
    ) -> Option<String> {
        let mut best_known_positive: Option<(String, u32, i64, i64)> = None;
        let mut best_unknown: Option<(String, u32, i64)> = None;

        for candidate in &observation.visible_locations {
            if candidate.location_id == depleted_location_id
                || candidate.distance_cm <= 0
                || self
                    .mine_depletion_cooldown_remaining_ticks(
                        candidate.location_id.as_str(),
                        current_time,
                    )
                    .is_some()
            {
                continue;
            }
            let failure_penalty =
                self.mine_failure_penalty(candidate.location_id.as_str(), current_time);
            match self
                .known_compound_availability_by_location
                .get(candidate.location_id.as_str())
                .copied()
            {
                Some(known_available) if known_available <= 0 => {}
                Some(known_available) => {
                    let should_replace = match &best_known_positive {
                        None => true,
                        Some((
                            best_location_id,
                            best_failure_penalty,
                            best_available,
                            best_distance_cm,
                        )) => {
                            failure_penalty < *best_failure_penalty
                                || (failure_penalty == *best_failure_penalty
                                    && known_available > *best_available)
                                || (failure_penalty == *best_failure_penalty
                                    && known_available == *best_available
                                    && candidate.distance_cm < *best_distance_cm)
                                || (failure_penalty == *best_failure_penalty
                                    && known_available == *best_available
                                    && candidate.distance_cm == *best_distance_cm
                                    && candidate.location_id < *best_location_id)
                        }
                    };
                    if should_replace {
                        best_known_positive = Some((
                            candidate.location_id.clone(),
                            failure_penalty,
                            known_available,
                            candidate.distance_cm,
                        ));
                    }
                }
                None => {
                    let should_replace = match &best_unknown {
                        None => true,
                        Some((best_location_id, best_failure_penalty, best_distance_cm)) => {
                            failure_penalty < *best_failure_penalty
                                || (failure_penalty == *best_failure_penalty
                                    && candidate.distance_cm < *best_distance_cm)
                                || (failure_penalty == *best_failure_penalty
                                    && candidate.distance_cm == *best_distance_cm
                                    && candidate.location_id < *best_location_id)
                        }
                    };
                    if should_replace {
                        best_unknown = Some((
                            candidate.location_id.clone(),
                            failure_penalty,
                            candidate.distance_cm,
                        ));
                    }
                }
            }
        }

        best_known_positive
            .map(|(location_id, _, _, _)| location_id)
            .or_else(|| best_unknown.map(|(location_id, _, _)| location_id))
    }

    pub(super) fn mine_depletion_cooldown_remaining_ticks(
        &self,
        location_id: &str,
        current_time: u64,
    ) -> Option<u64> {
        let cooldown_until_time = self
            .depleted_mine_location_cooldowns
            .get(location_id)
            .copied()?;
        if current_time > cooldown_until_time {
            return None;
        }
        Some(cooldown_until_time - current_time + 1)
    }

    pub(super) fn mine_failure_penalty(&self, location_id: &str, current_time: u64) -> u32 {
        let Some(streak) = self.mine_failure_streaks_by_location.get(location_id) else {
            return 0;
        };
        if current_time.saturating_sub(streak.last_time) > DEFAULT_MINE_FAILURE_STREAK_WINDOW_TICKS
        {
            return 0;
        }
        streak.count
    }

    pub(super) fn record_mine_failure_streak(
        &mut self,
        location_id: &str,
        current_time: u64,
    ) -> u32 {
        let streak = self
            .mine_failure_streaks_by_location
            .entry(location_id.to_string())
            .and_modify(|streak| {
                if current_time.saturating_sub(streak.last_time)
                    > DEFAULT_MINE_FAILURE_STREAK_WINDOW_TICKS
                {
                    streak.count = 1;
                } else {
                    streak.count = streak.count.saturating_add(1);
                }
                streak.last_time = current_time;
            })
            .or_insert(MineFailureStreak {
                count: 1,
                last_time: current_time,
            });
        streak.count
    }

    pub(super) fn clear_mine_failure_streak(&mut self, location_id: &str) {
        self.mine_failure_streaks_by_location.remove(location_id);
    }

    pub(super) fn trim_mine_failure_streaks(&mut self, current_time: u64) {
        self.mine_failure_streaks_by_location.retain(|_, streak| {
            current_time.saturating_sub(streak.last_time)
                <= DEFAULT_MINE_FAILURE_STREAK_WINDOW_TICKS
        });
    }

    pub(super) fn guarded_move_to_location(
        &self,
        to: &str,
        observation: &Observation,
    ) -> (Action, Option<String>) {
        if let Some((
            relay_location_id,
            target_distance,
            relay_distance_from_self,
            relay_to_target_distance,
        )) = self.find_reachable_move_relay(to, observation)
        {
            return (
                Action::MoveAgent {
                    agent_id: self.agent_id.clone(),
                    to: relay_location_id.clone(),
                },
                Some(format!(
                    "move_agent segmented by distance guardrail: target={} distance_cm={} exceeds max_distance_cm={}; rerouted_via={} relay_distance_cm={} relay_to_target_cm={}",
                    to,
                    target_distance,
                    DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK,
                    relay_location_id,
                    relay_distance_from_self,
                    relay_to_target_distance
                )),
            );
        }

        let visible_target_distance = observation
            .visible_locations
            .iter()
            .find(|location| location.location_id == to)
            .map(|location| location.distance_cm);
        let target_known_too_far = visible_target_distance
            .is_some_and(|distance| distance > DEFAULT_MAX_MOVE_DISTANCE_CM_PER_TICK);
        let target_blocked_by_history = self.move_distance_exceeded_targets.contains(to);
        if (target_known_too_far || target_blocked_by_history)
            && let Some((relay_location_id, relay_distance_from_self)) =
                self.find_exploration_move_relay(to, observation)
        {
            return (
                Action::MoveAgent {
                    agent_id: self.agent_id.clone(),
                    to: relay_location_id.clone(),
                },
                Some(format!(
                    "move_agent fallback relay after move_distance_exceeded: target={} target_distance_cm={} blocked_by_history={} rerouted_via={} relay_distance_cm={}",
                    to,
                    visible_target_distance
                        .map(|distance| distance.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    target_blocked_by_history,
                    relay_location_id,
                    relay_distance_from_self
                )),
            );
        }

        (
            Action::MoveAgent {
                agent_id: self.agent_id.clone(),
                to: to.to_string(),
            },
            None,
        )
    }

    pub(super) fn visible_location_distance_cm(
        observation: &Observation,
        location_id: &str,
    ) -> Option<i64> {
        observation
            .visible_locations
            .iter()
            .find(|location| location.location_id == location_id)
            .map(|location| location.distance_cm.max(0))
    }

    pub(super) fn default_move_electricity_cost(distance_cm: i64) -> i64 {
        if distance_cm <= 0 {
            return 0;
        }
        let distance_km = (distance_cm + CM_PER_KM - 1) / CM_PER_KM;
        distance_km.saturating_mul(DEFAULT_MOVE_COST_PER_KM_ELECTRICITY)
    }

    pub(super) fn guard_move_action_with_electricity(
        &self,
        action: Action,
        observation: &Observation,
        mut notes: Vec<String>,
    ) -> (Action, Option<String>) {
        let Action::MoveAgent { agent_id, to } = action else {
            return (action, (!notes.is_empty()).then_some(notes.join("; ")));
        };
        if agent_id != self.agent_id {
            return (
                Action::MoveAgent { agent_id, to },
                (!notes.is_empty()).then_some(notes.join("; ")),
            );
        }

        let Some(distance_cm) = Self::visible_location_distance_cm(observation, to.as_str()) else {
            return (
                Action::MoveAgent { agent_id, to },
                (!notes.is_empty()).then_some(notes.join("; ")),
            );
        };

        let available_electricity = observation.self_resources.get(ResourceKind::Electricity);
        let required_electricity = Self::default_move_electricity_cost(distance_cm);
        if required_electricity > 0 && available_electricity < required_electricity {
            notes.push(format!(
                "move_agent electricity precheck rerouted to harvest_radiation: to={} distance_cm={} available_electricity={} < required_electricity={}",
                to, distance_cm, available_electricity, required_electricity
            ));
            return (
                Action::HarvestRadiation {
                    agent_id: self.agent_id.clone(),
                    max_amount: self.config.harvest_max_amount_cap,
                },
                Some(notes.join("; ")),
            );
        }

        (
            Action::MoveAgent { agent_id, to },
            (!notes.is_empty()).then_some(notes.join("; ")),
        )
    }

    pub(super) fn remember_factory_location_hint(
        &mut self,
        factory_id: &str,
        location_id: &str,
        factory_kind: Option<&str>,
    ) {
        let factory_id = factory_id.trim();
        let location_id = location_id.trim();
        if factory_id.is_empty() || location_id.is_empty() {
            return;
        }
        self.known_factory_locations
            .insert(factory_id.to_string(), location_id.to_string());
        if let Some(factory_kind) = factory_kind
            .map(str::trim)
            .filter(|factory_kind| !factory_kind.is_empty())
        {
            self.known_factory_kinds_by_id
                .insert(factory_id.to_string(), factory_kind.to_string());
            self.known_factory_kind_aliases
                .insert(factory_kind.to_string(), factory_id.to_string());
        }
    }

    pub(super) fn observe_memory_summary(observation: &Observation) -> String {
        format!(
            "obs@T{} agents={} locations={} visibility_cm={}",
            observation.time,
            observation.visible_agents.len(),
            observation.visible_locations.len(),
            observation.visibility_range_cm,
        )
    }

    pub(super) fn run_prompt_module(
        &self,
        request: &LlmModuleCallRequest,
        observation: &Observation,
    ) -> serde_json::Value {
        let result = match request.module.as_str() {
            "agent.modules.list" => Ok(serde_json::json!({
                "modules": [
                    {
                        "name": "agent.modules.list",
                        "description": "列出 Agent 可调用的模块能力与参数。",
                        "args": {}
                    },
                    {
                        "name": "environment.current_observation",
                        "description": "读取当前 tick 的环境观测。",
                        "args": {}
                    },
                    {
                        "name": "memory.short_term.recent",
                        "description": "读取最近短期记忆。",
                        "args": { "limit": "u64, optional, default=3, max=8" }
                    },
                    {
                        "name": "memory.long_term.search",
                        "description": "按关键词检索长期记忆（query 为空时按重要度返回）。",
                        "args": {
                            "query": "string, optional",
                            "limit": "u64, optional, default=3, max=8"
                        }
                    },
                    {
                        "name": "world.rules.guide",
                        "description": "读取世界玩法规则、阶段目标与失败恢复建议。",
                        "args": {
                            "topic": "string, optional, enum=quickstart|resources|industry|governance|economic|social|recovery|all"
                        }
                    },
                    {
                        "name": "module.lifecycle.status",
                        "description": "读取模块生命周期快照（artifact 与 installed）。",
                        "args": {
                            "module_id": "string, optional",
                            "limit_artifacts": "u64, optional, default=32, max=256",
                            "limit_installed": "u64, optional, default=32, max=256"
                        }
                    },
                    {
                        "name": "power.order_book.status",
                        "description": "读取电力订单簿快照。",
                        "args": { "limit_orders": "u64, optional, default=32, max=256" }
                    },
                    {
                        "name": "module.market.status",
                        "description": "读取模块市场挂牌/竞价状态。",
                        "args": {
                            "wasm_hash": "string, optional",
                            "limit_listings": "u64, optional, default=32, max=256",
                            "limit_bids": "u64, optional, default=32, max=256"
                        }
                    },
                    {
                        "name": "social.state.status",
                        "description": "读取社会事实与关系边状态。",
                        "args": {
                            "include_inactive": "bool, optional, default=true",
                            "limit_facts": "u64, optional, default=32, max=256",
                            "limit_edges": "u64, optional, default=32, max=256"
                        }
                    }
                ]
            })),
            "environment.current_observation" => {
                self.environment_current_observation_module(observation)
            }
            "memory.short_term.recent" | "memory.long_term.search" => {
                self.prompt_memory_module(request)
            }
            "world.rules.guide" => Self::prompt_world_rules_guide_module(request),
            "module.lifecycle.status"
            | "power.order_book.status"
            | "module.market.status"
            | "social.state.status" => Self::prompt_status_module(request, observation),
            other => Err(format!("unsupported module: {other}")),
        };

        match result {
            Ok(data) => serde_json::json!({
                "ok": true,
                "module": request.module,
                "result": data,
            }),
            Err(err) => serde_json::json!({
                "ok": false,
                "module": request.module,
                "error": err,
            }),
        }
    }

    fn prompt_memory_module(
        &self,
        request: &LlmModuleCallRequest,
    ) -> Result<serde_json::Value, String> {
        match request.module.as_str() {
            "memory.short_term.recent" => {
                let limit = parse_limit_arg(request.args.get("limit"), 3, 8);
                let mut entries: Vec<MemoryEntry> =
                    self.memory.short_term.recent(limit).cloned().collect();
                entries.reverse();
                serde_json::to_value(entries)
                    .map_err(|err| format!("serialize short-term memory failed: {err}"))
            }
            "memory.long_term.search" => {
                let limit = parse_limit_arg(request.args.get("limit"), 3, 8);
                let query = request
                    .args
                    .get("query")
                    .and_then(|value| value.as_str())
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty());

                let mut entries: Vec<LongTermMemoryEntry> = match query {
                    Some(query) => self
                        .memory
                        .long_term
                        .search_by_content(query)
                        .into_iter()
                        .cloned()
                        .collect(),
                    None => self
                        .memory
                        .long_term
                        .top_by_importance(limit)
                        .into_iter()
                        .cloned()
                        .collect(),
                };

                entries.sort_by(|left, right| {
                    right
                        .importance
                        .partial_cmp(&left.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                entries.truncate(limit);

                serde_json::to_value(entries)
                    .map_err(|err| format!("serialize long-term memory failed: {err}"))
            }
            _ => unreachable!("unsupported module reached its specialized dispatcher"),
        }
    }

    fn prompt_world_rules_guide_module(
        request: &LlmModuleCallRequest,
    ) -> Result<serde_json::Value, String> {
        match request.module.as_str() {
            "world.rules.guide" => {
                let topic = request
                    .args
                    .get("topic")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("quickstart")
                    .to_ascii_lowercase();
                let topic_ref = topic.as_str();
                let topic = match topic_ref {
                    "quickstart" | "resources" | "industry" | "governance" | "economic"
                    | "social" | "recovery" | "all" => topic_ref,
                    _ => "quickstart",
                };

                let guide = match topic {
                    "resources" => serde_json::json!({
                        "goal": "稳定基础资源，避免停摆",
                        "steps": [
                            "先读 observation.last_action 与 resources，识别瓶颈是 electricity 还是 data。",
                            "electricity 紧缺优先 harvest_radiation；data 紧缺优先 mine_compound。",
                            "当 compound_mass 与 data 足够时再 refine_compound，避免空转。"
                        ],
                        "key_checks": [
                            "last_action.reject_reason",
                            "self_resources.electricity",
                            "self_resources.data"
                        ]
                    }),
                    "industry" => serde_json::json!({
                        "goal": "形成工业闭环（采矿 -> 精炼 -> 建厂 -> 排产）",
                        "steps": [
                            "mine_compound 获取可精炼原料。",
                            "refine_compound 产出 hardware/data。",
                            "post_onboarding 优先 build_factory(factory.smelter.mk1) 作为第一条可持续产线。",
                            "先 schedule_recipe 覆盖 iron_ingot/copper_wire/polymer_resin/alloy_plate；assembler 侧先补 gear/control_chip/motor/logistics_drone，再在更高阶段推进 sensor_pack/module_rack/factory_core。"
                        ],
                        "success_signals": [
                            "action_kind_build_factory >= 1",
                            "action_kind_schedule_recipe >= 1",
                            "known_factory_kind_aliases contains factory.smelter.mk1"
                        ]
                    }),
                    "governance" => serde_json::json!({
                        "goal": "把文明推进到可治理状态",
                        "steps": [
                            "先 open_governance_proposal 建立公共议题。",
                            "再 cast_governance_vote 推进共识。",
                            "根据局势切换 resolve_crisis 或 grant_meta_progress 收敛事件。"
                        ],
                        "anti_pattern": "避免 open/cast 无限循环。"
                    }),
                    "economic" => serde_json::json!({
                        "goal": "验证经济协作可闭环",
                        "steps": [
                            "open_economic_contract 发起契约。",
                            "accept_economic_contract 建立对手方关系。",
                            "settle_economic_contract 完成结算并记录结果。"
                        ]
                    }),
                    "social" => serde_json::json!({
                        "goal": "构建可追踪社会关系",
                        "steps": [
                            "publish_social_fact 提交可验证事实。",
                            "declare_social_edge 建立关系边并维护权重。",
                            "必要时 challenge/adjudicate/revoke 管理争议事实。"
                        ]
                    }),
                    "recovery" => serde_json::json!({
                        "goal": "遇到拒绝时快速回到可执行路径",
                        "by_reject_reason": {
                            "insufficient_resource.data": "优先 mine_compound 补 data 前置，再 refine_compound。",
                            "insufficient_resource.electricity": "先 harvest_radiation，必要时 transfer_resource(kind=electricity)。",
                            "factory_not_found": "smelter 配方先 build_factory(factory.smelter.mk1)，assembler 配方先 build_factory(factory.assembler.mk1)。",
                            "location_not_found": "仅使用 visible_locations 中的 location_id。",
                            "agent_already_at_location": "不要重复 move_agent，改为生产/采集动作。"
                        }
                    }),
                    "all" => serde_json::json!({
                        "phases": [
                            "resources",
                            "industry",
                            "governance",
                            "economic",
                            "social",
                            "recovery"
                        ],
                        "note": "建议按阶段读取 topic，避免一次加载过多规则后丢失执行重点。"
                    }),
                    _ => serde_json::json!({
                        "goal": "开局用最少步数建立可持续推进",
                        "steps": [
                            "第 1 步：调用 environment.current_observation 识别当前瓶颈。",
                            "第 2 步：调用 world.rules.guide(topic=resources|industry) 确认下一阶段动作链。",
                            "第 3 步：只提交一个最关键动作，观察结果后再推进下一步。"
                        ],
                        "decision_loop": [
                            "观察",
                            "识别瓶颈",
                            "补前置",
                            "推进主线",
                            "复盘 reject_reason"
                        ]
                    }),
                };

                Ok(serde_json::json!({
                    "topic": topic,
                    "available_topics": [
                        "quickstart",
                        "resources",
                        "industry",
                        "governance",
                        "economic",
                        "social",
                        "recovery",
                        "all"
                    ],
                    "guide": guide,
                }))
            }
            _ => unreachable!("unsupported module reached its specialized dispatcher"),
        }
    }

    fn prompt_status_module(
        request: &LlmModuleCallRequest,
        observation: &Observation,
    ) -> Result<serde_json::Value, String> {
        match request.module.as_str() {
            "module.lifecycle.status" => {
                let module_id_filter = request
                    .args
                    .get("module_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let limit_artifacts = parse_limit_arg(request.args.get("limit_artifacts"), 32, 256);
                let limit_installed = parse_limit_arg(request.args.get("limit_installed"), 32, 256);
                let (artifacts_total, artifacts) = limited_filtered_page(
                    observation.module_lifecycle.artifacts.iter(),
                    limit_artifacts,
                    |artifact| {
                        module_id_filter.is_none_or(|module_id| {
                            artifact
                                .module_id_hint
                                .as_deref()
                                .is_some_and(|hint| hint == module_id)
                        })
                    },
                );
                let (installed_total, installed_modules) = limited_filtered_page(
                    observation.module_lifecycle.installed_modules.iter(),
                    limit_installed,
                    |installed| {
                        module_id_filter.is_none_or(|module_id| installed.module_id == module_id)
                    },
                );
                Ok(serde_json::json!({
                    "artifacts_total": artifacts_total,
                    "artifacts": artifacts,
                    "installed_modules_total": installed_total,
                    "installed_modules": installed_modules,
                }))
            }
            "power.order_book.status" => {
                let limit_orders = parse_limit_arg(request.args.get("limit_orders"), 32, 256);
                let (open_orders_total, open_orders) = limited_filtered_page(
                    observation.power_market.open_orders.iter(),
                    limit_orders,
                    |_| true,
                );
                Ok(serde_json::json!({
                    "next_order_id": observation.power_market.next_order_id,
                    "open_orders_total": open_orders_total,
                    "open_orders": open_orders,
                }))
            }
            "module.market.status" => {
                let wasm_hash_filter = request
                    .args
                    .get("wasm_hash")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let limit_listings = parse_limit_arg(request.args.get("limit_listings"), 32, 256);
                let limit_bids = parse_limit_arg(request.args.get("limit_bids"), 32, 256);
                let (listings_total, listings) = limited_filtered_page(
                    observation.module_market.listings.iter(),
                    limit_listings,
                    |listing| {
                        wasm_hash_filter.is_none_or(|wasm_hash| listing.wasm_hash == wasm_hash)
                    },
                );
                let (bids_total, bids) = limited_filtered_page(
                    observation.module_market.bids.iter(),
                    limit_bids,
                    |bid| wasm_hash_filter.is_none_or(|wasm_hash| bid.wasm_hash == wasm_hash),
                );
                Ok(serde_json::json!({
                    "listings_total": listings_total,
                    "listings": listings,
                    "bids_total": bids_total,
                    "bids": bids,
                }))
            }
            "social.state.status" => {
                let include_inactive = request
                    .args
                    .get("include_inactive")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true);
                let limit_facts = parse_limit_arg(request.args.get("limit_facts"), 32, 256);
                let limit_edges = parse_limit_arg(request.args.get("limit_edges"), 32, 256);
                let (facts_total, facts) = limited_filtered_page(
                    observation.social_state.facts.iter(),
                    limit_facts,
                    |fact| include_inactive || fact.supports_backing(),
                );
                let (edges_total, edges) = limited_filtered_page(
                    observation.social_state.edges.iter(),
                    limit_edges,
                    |edge| include_inactive || edge.is_active(),
                );
                Ok(serde_json::json!({
                    "facts_total": facts_total,
                    "facts": facts,
                    "edges_total": edges_total,
                    "edges": edges,
                }))
            }
            _ => unreachable!("unsupported module reached its specialized dispatcher"),
        }
    }

    pub(super) fn next_prompt_intent_id(&mut self) -> String {
        let intent_id = format!("llm-intent-{}", self.next_effect_intent_id);
        self.next_effect_intent_id = self.next_effect_intent_id.saturating_add(1);
        intent_id
    }

    pub(super) fn append_conversation_message(
        &mut self,
        time: u64,
        role: LlmChatRole,
        content: &str,
    ) -> Option<LlmChatMessageTrace> {
        let normalized = content.trim();
        if normalized.is_empty() {
            return None;
        }
        let trace = LlmChatMessageTrace {
            time,
            agent_id: self.agent_id.clone(),
            role,
            content: summarize_trace_text(normalized, PROMPT_CONVERSATION_ITEM_MAX_CHARS * 2),
        };
        self.conversation_history.push(trace.clone());
        if self.conversation_history.len() > CONVERSATION_HISTORY_MAX_ITEMS {
            let overflow = self.conversation_history.len() - CONVERSATION_HISTORY_MAX_ITEMS;
            self.conversation_history.drain(0..overflow);
            self.conversation_trace_cursor =
                self.conversation_trace_cursor.saturating_sub(overflow);
        }
        Some(trace)
    }
}

fn limited_filtered_page<'a, T: Clone + 'a>(
    items: impl Iterator<Item = &'a T>,
    limit: usize,
    mut include: impl FnMut(&T) -> bool,
) -> (usize, Vec<T>) {
    let mut total = 0usize;
    let mut page = Vec::new();
    for item in items {
        if !include(item) {
            continue;
        }
        total += 1;
        if page.len() < limit {
            page.push(item.clone());
        }
    }
    (total, page)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyClient;

    impl LlmCompletionClient for DummyClient {
        fn complete(
            &self,
            _request: &LlmCompletionRequest,
        ) -> Result<LlmCompletionResult, LlmClientError> {
            unreachable!("helper mapping tests never call the completion client");
        }
    }

    #[test]
    fn smelter_recipes_map_to_smelter_factory_kind() {
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::required_factory_kind_for_recipe(
                "recipe.smelter.iron_ingot"
            ),
            Some("factory.smelter.mk1")
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::required_factory_kind_for_recipe(
                "recipe.smelter.copper_wire"
            ),
            Some("factory.smelter.mk1")
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::required_factory_kind_for_recipe(
                "recipe.smelter.polymer_resin"
            ),
            Some("factory.smelter.mk1")
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::required_factory_kind_for_recipe(
                "recipe.smelter.alloy_plate"
            ),
            Some("factory.smelter.mk1")
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::required_factory_kind_for_recipe(
                "recipe.assembler.factory_core"
            ),
            Some("factory.assembler.mk1")
        );
    }

    #[test]
    fn smelter_recipes_expose_default_cost_fallbacks() {
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_hardware_cost_per_batch(
                "recipe.smelter.iron_ingot"
            ),
            Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH)
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_electricity_cost_per_batch(
                "recipe.smelter.iron_ingot"
            ),
            Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH)
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_hardware_cost_per_batch(
                "recipe.assembler.gear"
            ),
            Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH)
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_hardware_cost_per_batch(
                "recipe.assembler.control_chip"
            ),
            Some(DEFAULT_RECIPE_HARDWARE_COST_PER_BATCH)
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_electricity_cost_per_batch(
                "recipe.assembler.motor_mk1"
            ),
            Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH * 2)
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_electricity_cost_per_batch(
                "recipe.assembler.logistics_drone"
            ),
            Some(DEFAULT_RECIPE_ELECTRICITY_COST_PER_BATCH * 4)
        );
        assert_eq!(
            LlmAgentBehavior::<DummyClient>::default_recipe_electricity_cost_per_batch(
                "recipe.assembler.factory_core"
            ),
            Some(14)
        );
    }
}
