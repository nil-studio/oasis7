import {
  ATTRACTION_THRESHOLDS,
  BASE_MODEL,
  BASE_PLAYER_GAMEPLAY,
  TASK_GAME_076_BEATS,
  TASK_GAME_076_SCENARIO_VERSION,
} from "./gameplay_attraction_fixture_data.js";
import { fixtureRollbackQuote, fixtureRollbackWindowConsistent } from "./gameplay_attraction_fixture_rollback.js";

export { TASK_GAME_076_BEATS, TASK_GAME_076_SCENARIO_VERSION } from "./gameplay_attraction_fixture_data.js";

const CONTENT_VOLUME_SUPPLEMENT_SEGMENTS = Object.freeze([
  "diagnosis_focus",
  "route_tradeoff",
  "micro_commission",
  "incident_recovery",
  "opportunity_scan",
  "return_package",
]);

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

export function buildTaskGame076ScenarioSnapshot({ variant = "attraction_evidence", overrides = {} } = {}) {
  const snapshot = {
    time: 12,
    config: {
      space: {
        width_cm: 10_000_000,
        depth_cm: 5_000_000,
        height_cm: 1_000_000,
      },
    },
    model: clone(BASE_MODEL),
    player_gameplay: clone(BASE_PLAYER_GAMEPLAY),
    task_game_076_scenario: {
      version: TASK_GAME_076_SCENARIO_VERSION,
      variant,
      provenance: "viewer_fixture_only",
      provider_mode: "deterministic_mock",
    },
  };

  if (variant === "weak_blocked") {
    Object.assign(snapshot.player_gameplay, {
      can_interrupt: false,
      can_reprioritize: false,
      replacement_intent_summary: null,
      handoff_result: null,
      first_win_goal_id: null,
      player_action: null,
      world_change_due_to_player: null,
      player_leverage_verdict: null,
      leverage_class: null,
      repair_available: false,
      rebuild_available: false,
      pivot_available: false,
      recovery_path_kind: null,
      recovery_path_detail: null,
      available_actions: [],
    });
  }

  if (variant === "weak_high_progress") {
    Object.assign(snapshot.player_gameplay, {
      stage_status: "completed",
      execution_state: "completed",
      goal_title: "High progress but low attraction",
      objective: "Advance numeric production.",
      progress_percent: 92,
      progress_detail: "Progression pass is nearly complete.",
      last_world_change: null,
      resume_anchor: null,
      resume_next_step: null,
      blocker_kind: null,
      blocker_detail: null,
      causality_kind: null,
      causality_detail: null,
      next_step_hint: null,
      branch_hint: null,
      can_interrupt: false,
      can_reprioritize: false,
      replacement_intent_summary: null,
      handoff_result: null,
      first_win_goal_id: null,
      player_action: null,
      world_change_due_to_player: null,
      player_leverage_verdict: null,
      leverage_class: null,
      same_loop_repeat_count: 4,
      grind_only_flag: true,
      repair_available: null,
      rebuild_available: null,
      pivot_available: null,
      recovery_path_kind: null,
      recovery_path_detail: null,
      available_actions: [],
      recent_feedback: null,
    });
  }

  return {
    ...snapshot,
    ...clone(overrides),
    player_gameplay: {
      ...snapshot.player_gameplay,
      ...(overrides.player_gameplay ? clone(overrides.player_gameplay) : {}),
    },
    model: overrides.model ? clone(overrides.model) : snapshot.model,
  };
}

function truthyText(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function playerReadableText(value) {
  return (
    truthyText(value) &&
    !/\b[a-z][a-z0-9]*(?:_[a-z0-9]+)+\b/.test(value) &&
    !/\b[a-z][a-z0-9]*(?:\.[a-z][a-z0-9]*)+\b/.test(value)
  );
}

function actionsWithChoices(playerGameplay) {
  return (playerGameplay.available_actions || []).filter((action) => !action.disabled_reason);
}

function attractionVerdict(playerGameplay) {
  const hasCause = truthyText(playerGameplay.player_action) && truthyText(playerGameplay.world_change_due_to_player);
  const hasOption =
    actionsWithChoices(playerGameplay).length > 0 ||
    truthyText(playerGameplay.branch_hint) ||
    playerGameplay.can_interrupt === true ||
    playerGameplay.can_reprioritize === true;
  const hasContinue =
    truthyText(playerGameplay.next_step_hint) ||
    truthyText(playerGameplay.resume_next_step) ||
    truthyText(playerGameplay.branch_hint);
  const hasReward =
    truthyText(playerGameplay.first_win_goal_id) ||
    truthyText(playerGameplay.leverage_class) ||
    truthyText(playerGameplay.player_leverage_verdict);
  const hasRecovery =
    playerGameplay.repair_available === true ||
    playerGameplay.rebuild_available === true ||
    playerGameplay.pivot_available === true;
  const isGrindOnly =
    playerGameplay.grind_only_flag === true ||
    Number(playerGameplay.same_loop_repeat_count || 0) >= 4;

  if (!hasCause || !hasOption || !hasContinue || !hasReward || isGrindOnly) {
    return "progression_pass_but_attraction_weak";
  }
  if (!hasRecovery && playerGameplay.stage_status === "blocked") {
    return "agency_weakened";
  }
  return "attraction_watch";
}

function scoreAttractionCard(playerGameplay) {
  const verdict = attractionVerdict(playerGameplay);
  if (verdict === "progression_pass_but_attraction_weak") {
    return { hookScore: 2, replayIntent: 1 };
  }
  if (verdict === "agency_weakened") {
    return { hookScore: 3, replayIntent: 2 };
  }
  const choiceBonus = actionsWithChoices(playerGameplay).length >= 2 || truthyText(playerGameplay.branch_hint) ? 1 : 0;
  const rewardBonus = truthyText(playerGameplay.leverage_class) || truthyText(playerGameplay.first_win_goal_id) ? 1 : 0;
  return {
    hookScore: Math.min(5, 3 + choiceBonus + rewardBonus),
    replayIntent: Math.min(5, 3 + choiceBonus + rewardBonus),
  };
}

function buildAttractionCardFromSnapshot(snapshot, index) {
  const playerGameplay = snapshot.player_gameplay || {};
  const verdict = attractionVerdict(playerGameplay);
  const score = scoreAttractionCard(playerGameplay);
  const actions = actionsWithChoices(playerGameplay);
  return {
    sample_id: `task-game-076.${index + 1}`,
    scenario_version: TASK_GAME_076_SCENARIO_VERSION,
    variant: snapshot.task_game_076_scenario?.variant || "custom",
    provenance: "viewer_fixture_only",
    declared_fixture_provenance: snapshot.task_game_076_scenario?.provenance || null,
    hook_score: score.hookScore,
    replay_intent: score.replayIntent,
    action_effect_feedback:
      playerGameplay.recent_feedback?.effect ||
      playerGameplay.last_world_change ||
      "no player-visible action effect recorded",
    what_did_i_cause:
      playerGameplay.world_change_due_to_player ||
      playerGameplay.last_world_change ||
      "no player-caused world change recorded",
    biggest_boredom_point:
      verdict === "progression_pass_but_attraction_weak"
        ? "progress advanced without new choices, reward, or return hook"
        : "watch recovery pacing and repeated waiting",
    no_op_or_follow_up_route:
      playerGameplay.next_step_hint ||
      playerGameplay.resume_next_step ||
      playerGameplay.branch_hint ||
      "route to TASK-GAME-076 follow-up",
    meaningful_decision_count:
      (actions.length > 0 ? 1 : 0) +
      (truthyText(playerGameplay.branch_hint) ? 1 : 0) +
      (playerGameplay.can_interrupt === true || playerGameplay.can_reprioritize === true ? 1 : 0),
    reward_or_unlock_count:
      (truthyText(playerGameplay.first_win_goal_id) ? 1 : 0) +
      (truthyText(playerGameplay.leverage_class) ? 1 : 0),
    stall_or_wait_periods:
      playerGameplay.stage_status === "blocked" && playerGameplay.repair_available !== true ? 2 : 0,
    branch_offer_clarity: truthyText(playerGameplay.branch_hint) || actions.length >= 2 ? "clear" : "unclear",
    continue_reason:
      playerGameplay.branch_hint ||
      playerGameplay.player_leverage_verdict ||
      playerGameplay.resume_next_step ||
      playerGameplay.next_step_hint ||
      null,
    return_hook:
      playerGameplay.first_win_goal_id ||
      playerGameplay.player_leverage_verdict ||
      playerGameplay.recovery_path_detail ||
      null,
    leverage_class: playerGameplay.leverage_class || null,
    content_profile: snapshot.task_game_076_scenario?.content_profile || null,
    blocked_without_recovery: playerGameplay.stage_status === "blocked" &&
      !(playerGameplay.repair_available === true || playerGameplay.rebuild_available === true ||
        playerGameplay.pivot_available === true),
    decision_count_scope: "snapshot_choice_opportunity_proxy",
    verdict,
  };
}

function defaultAttractionSamples() {
  return [
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "first_control_choice",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 6,
            player_operation_count: 4,
            passive_wait_minutes: 1,
            action_families: ["select_target", "refresh_snapshot", "advance_step", "choose_recovery"],
            content_units: ["identity", "goal", "blocker", "control_proof"],
            implemented_segments: ["diagnosis_focus"],
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            recovery_action_id: "recovery.replenish_iron_input",
          },
        },
        player_gameplay: {
          branch_hint: "Repair the smelter line now or pivot to a lower-input branch.",
          same_loop_repeat_count: 1,
          diagnosis_choices: [
            {
              id: "diagnosis.resource",
              label: "Diagnose resource shortfall",
              target: "factory-0 iron input",
              expected_benefit: "Reveal the fastest recovery path",
              risk: "low",
            },
            {
              id: "diagnosis.risk",
              label: "Diagnose risk exposure",
              target: "smelter stall risk",
              expected_benefit: "Reduce repeated waiting",
              risk: "medium",
            },
          ],
          selected_diagnosis: "diagnosis.resource",
        },
      },
    }),
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "first_visible_output",
          route_phase: "before_incident_recovery",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 7,
            player_operation_count: 5,
            passive_wait_minutes: 1,
            action_families: ["inspect_output", "claim_reward", "choose_branch"],
            content_units: ["input", "output", "new_use", "repair_choice"],
            implemented_segments: ["route_tradeoff", "micro_commission"],
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            recovery_action_id: "recovery.replenish_iron_input",
          },
        },
        player_gameplay: {
          stage_status: "completed",
          execution_state: "completed",
          progress_percent: 78,
          progress_detail: "Smelter recovery produced the first reusable alloy output.",
          last_world_change: "Recovered smelter produced alloy and unlocked a repair-or-expand choice.",
          world_change_due_to_player: "player recovery action converted the stalled smelter into reusable alloy output",
          player_leverage_verdict: "continue: alloy output opens repair, expand, or tradeoff paths",
          branch_hint: "Expand alloy output, repair material input, or specialize the line.",
          same_loop_repeat_count: 1,
          route_tradeoff: {
            mode: "stabilize",
            route_commitment_id: "route.stabilize_input_line",
            output_delta: 1,
            risk_delta: -2,
            stability_delta: 3,
            affected_future_beats: ["micro_commission", "incident_recovery", "return_package"],
            next_commission_modifier: "starter_batch requires 1 less recovery step",
            incident_risk_modifier: "input jam risk lowered; output pace slower",
            midrun_feedback: {
              timing: "before_incident_recovery",
              visible_metric_delta: "risk -2, stability +3, output +1",
              player_readable_state: "The line is slower, but the risk meter drops before the first local order.",
            },
            visible_consequence_text: "Stabilizing now lowers the next jam risk before you choose the repair path.",
            forecast_delta_text: "Forecast: +1 output, -2 risk, +3 stability.",
          },
          micro_commission: {
            status: "completed",
            requirement: "Convert recovered ore into starter alloy batch",
            steps: ["collect ore", "feed smelter", "confirm alloy output"],
            reward_id: "starter_batch.alloy_plate",
            output_item_id: "starter_batch.alloy_plate",
            output_display_name: "Starter Batch: Alloy Plate x3",
            batch_size: 3,
            assigned_local_demand_id: "local_order.edge_outpost_plate_shortfall",
            screenshot_caption: "Starter Batch: Alloy Plate x3 for Edge Outpost plate shortfall",
            delivery_status: "assigned_to_local_demand",
            result_card_action_id: "deliver_starter_batch_to_edge_outpost",
            local_demand_progress_preview: {
              before: 0,
              after: 3,
              target: 6,
              visible_result_text: "Edge Outpost plate shortfall moves from 0/6 to 3/6 after delivery.",
            },
          },
        },
      },
    }),
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "incident_recovery",
          route_phase: "repair_completed",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 5,
            player_operation_count: 3,
            passive_wait_minutes: 0,
            action_families: ["choose_repair", "inspect_risk", "confirm_recovery"],
            content_units: ["incident_cause", "repair_option", "residual_risk"],
            implemented_segments: ["incident_recovery"],
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            recovery_action_id: "recovery.root_cause_patch",
          },
        },
        player_gameplay: {
          stage_status: "blocked",
          execution_state: "blocked",
          progress_percent: 80,
          last_world_change: "Root-cause repair isolated the input jam and preserved the starter line.",
          world_change_due_to_player: "player picked root-cause repair instead of waiting through another stall",
          player_leverage_verdict: "continue: lower residual risk makes the next output reliable",
          branch_hint: "Finish the root-cause patch, then scan nearby demand.",
          leverage_class: "risk_reduction",
          same_loop_repeat_count: 1,
          incident_recovery: {
            incident_id: "incident.input_jam.factory_0",
            cause: "iron feed jam after the first alloy batch",
            repair_options: ["quick_patch", "root_cause_fix"],
            triggered_by_route: "route.stabilize_input_line",
            route_consequence_text: "Because you stabilized the input line, the jam stayed low-risk and root-cause repair preserves output reliability.",
            selected_repair: "root_cause_fix",
            residual_risk: "low",
            repaired_capability_delta: "stable_input_line +1",
            residual_risk_after_repair: "low",
            repair_tradeoff_costs: {
              quick_patch: {
                time_cost: 1,
                output_delta: 0,
                progress_preserved: true,
                risk_after: "medium",
                stability_delta: 0,
                visible_tradeoff_text: "Quick patch keeps output moving now but leaves medium jam risk.",
              },
              root_cause_fix: {
                time_cost: 2,
                output_delta: -1,
                risk_after: "low",
                stability_delta: 2,
                visible_tradeoff_text: "Root-cause fix costs one output tick now but drops future jam risk to low.",
              },
            },
          },
        },
      },
    }),
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "opportunity_scan",
          route_phase: "repair_completed",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 5,
            player_operation_count: 3,
            passive_wait_minutes: 0,
            action_families: ["scan_market", "compare_hook", "choose_immediate_action"],
            content_units: ["local_demand", "future_hook", "immediate_action"],
            implemented_segments: ["opportunity_scan"],
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            recovery_action_id: "recovery.root_cause_patch",
          },
        },
        player_gameplay: {
          progress_percent: 84,
          last_world_change: "Opportunity scan found a nearby outpost order for alloy plate.",
          world_change_due_to_player: "player converted factory recovery into a local order opportunity",
          player_leverage_verdict: "continue: local demand gives the starter batch a destination",
          branch_hint: "Deliver alloy plates locally or harden the repair path first.",
          leverage_class: "local_market_leverage",
          same_loop_repeat_count: 1,
          opportunity_scan: {
            direction: "local_market",
            hooks: ["edge_outpost_plate_shortfall", "repair_path_hardening"],
            immediate_action_id: "deliver_starter_batch_to_edge_outpost",
            generated_from: [
              "route.stabilize_input_line",
              "starter_batch.alloy_plate",
              "root_cause_fix",
            ],
            recommended_next_action_reason: "The stable repaired line makes the starter batch reliable enough for a local order.",
            discarded_hook_reason: "facility expansion deferred until the first local delivery proves demand.",
          },
        },
      },
    }),
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "return_package",
          route_phase: "completed",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 6,
            player_operation_count: 4,
            passive_wait_minutes: 1,
            action_families: ["review_leverage", "choose_return_goal", "share_replay"],
            content_units: ["return_hook", "independent_route", "share_replay", "next_session_goal"],
            implemented_segments: ["return_package"],
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            recovery_action_id: "recovery.root_cause_patch",
            next_session_goal: "deliver_starter_batch_to_edge_outpost",
          },
        },
        player_gameplay: {
          progress_percent: 86,
          last_world_change: "Small player route preserved recovery leverage without joining a major power.",
          world_change_due_to_player: "player chose a repair-first route that kept independent capability alive",
          player_leverage_verdict: "return: independent repair leverage can compound next session",
          branch_hint: "Return to expand independently, trade output, or harden the repair path.",
          leverage_class: "independent_repair_leverage",
          same_loop_repeat_count: 1,
          return_package: {
            earned_summary: "Recovered the first smelter, produced a starter alloy batch, and found a local order.",
            next_session_goal: "deliver_starter_batch_to_edge_outpost",
            first_action_on_return: "open local order and choose delivery or hardening",
            choice_memory: [
              "selected route.stabilize_input_line",
              "completed Starter Batch: Alloy Plate x3",
              "repaired with root_cause_fix",
            ],
            second_session_first_screen_memory:
              "Welcome back: your stabilized input line and Starter Batch are ready for Edge Outpost delivery.",
            unlocked_variant: "stable_line_local_delivery",
            why_this_goal: "Because you stabilized the input line and completed a starter batch, the next session can safely deliver to the edge outpost.",
            recovery_plan: null,
          },
        },
      },
    }),
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "local_order_contribution",
          route_phase: "completed",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 5,
            player_operation_count: 3,
            passive_wait_minutes: 0,
            action_families: ["accept_local_order", "assign_output", "confirm_contribution"],
            content_units: ["local_order", "contribution_target", "delivery_choice"],
            implemented_segments: ["micro_commission", "opportunity_scan"],
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            recovery_action_id: "recovery.root_cause_patch",
            next_session_goal: "complete_edge_outpost_plate_delivery",
          },
        },
        player_gameplay: {
          progress_percent: 88,
          last_world_change: "Starter batch was assigned to the edge outpost plate shortfall.",
          world_change_due_to_player: "player turned a small factory output into progress on a shared local project",
          player_leverage_verdict: "continue: local order contribution proves small-player value",
          branch_hint: "Complete the local delivery or reinvest output into a second batch.",
          leverage_class: "shared_project_contribution",
          same_loop_repeat_count: 1,
          local_demand_id: "local_order.edge_outpost_plate_shortfall",
          contribution_target_id: "corp_project.restore_factory_anchor",
          next_session_goal: "complete_edge_outpost_plate_delivery",
          local_demand_progress_after_delivery: {
            local_demand_id: "local_order.edge_outpost_plate_shortfall",
            contribution_target_id: "corp_project.restore_factory_anchor",
            before: 0,
            after: 3,
            target: 6,
            progress_delta: 3,
            visible_result_text: "Edge Outpost plate shortfall progressed 0/6 -> 3/6 after your Starter Batch delivery.",
          },
        },
      },
    }),
  ].map((snapshot) => {
    const phase = snapshot.task_game_076_scenario.route_phase;
    if (phase) {
      snapshot.player_gameplay.route_tradeoff = {
        mode: "stabilize",
        route_commitment_id: "route.stabilize_input_line",
        ...snapshot.player_gameplay.route_tradeoff,
        ...fixtureRollbackQuote("stabilize", phase),
      };
    }
    return snapshot;
  });
}

function buildContentVolumeCard(cards) {
  const profiles = cards.map((card) => card.content_profile || {});
  const effectivePlayMinutes = profiles.reduce((sum, profile) => sum + Number(profile.effective_play_minutes || 0), 0);
  const playerOperationCount = profiles.reduce((sum, profile) => sum + Number(profile.player_operation_count || 0), 0);
  const passiveWaitMinutes = profiles.reduce((sum, profile) => sum + Number(profile.passive_wait_minutes || 0), 0);
  const actionFamilies = new Set();
  const contentUnits = new Set();
  for (const profile of profiles) {
    for (const family of Array.isArray(profile.action_families) ? profile.action_families : []) {
      if (truthyText(family)) actionFamilies.add(family);
    }
    for (const unit of Array.isArray(profile.content_units) ? profile.content_units : []) {
      if (truthyText(unit)) contentUnits.add(unit);
    }
  }
  const totalWindowMinutes = Math.max(1, effectivePlayMinutes + passiveWaitMinutes);
  const passiveWaitShare = Number((passiveWaitMinutes / totalWindowMinutes).toFixed(2));
  const missing = [];
  if (effectivePlayMinutes < ATTRACTION_THRESHOLDS.minEffectivePlayMinutes) missing.push("effective_play_minutes");
  if (playerOperationCount < ATTRACTION_THRESHOLDS.minPlayerOperationCount) missing.push("player_operation_count");
  if (contentUnits.size < ATTRACTION_THRESHOLDS.minContentUnitCount) missing.push("content_unit_count");
  if (actionFamilies.size < ATTRACTION_THRESHOLDS.minDistinctActionFamilyCount) missing.push("distinct_action_family_count");
  if (passiveWaitShare > ATTRACTION_THRESHOLDS.maxPassiveWaitShare) missing.push("passive_wait_share");
  return {
    role: "diagnostic_targets",
    blocks_sufficiency: false,
    status: missing.length === 0 ? "content_volume_pass" : "content_volume_weak",
    effective_play_minutes: effectivePlayMinutes,
    target_effective_play_minutes: ATTRACTION_THRESHOLDS.minEffectivePlayMinutes,
    player_operation_count: playerOperationCount,
    content_unit_count: contentUnits.size,
    distinct_action_family_count: actionFamilies.size,
    passive_wait_minutes: passiveWaitMinutes,
    authorized_away_execution_minutes: profiles.reduce((sum, profile) => sum + Number(profile.authorized_away_execution_minutes || 0), 0),
    forced_attention_wait_minutes: profiles.reduce((sum, profile) => sum + Number(profile.forced_attention_wait_minutes || 0), 0),
    wait_classification: "unclassified_passive_wait_is_not_forced_attention",
    passive_wait_share: passiveWaitShare,
    action_families: Array.from(actionFamilies).sort(),
    content_units: Array.from(contentUnits).sort(),
    missing,
  };
}

function implementedContentSegments(cards) {
  const segments = new Set();
  for (const card of cards) {
    const profile = card.content_profile || {};
    for (const segment of Array.isArray(profile.implemented_segments) ? profile.implemented_segments : []) {
      if (truthyText(segment)) segments.add(segment);
    }
  }
  return Array.from(segments).sort();
}

function buildGameplayTruthCoverage(samples, cards) {
  const profileValues = cards.map((card) => card.content_profile || {});
  const gameplayValues = samples.map((sample) => sample.player_gameplay || {});
  const hasProfile = (key) => profileValues.some((profile) => truthyText(profile[key]));
  const hasGameplay = (key) => gameplayValues.some((gameplay) => truthyText(gameplay[key]));
  return {
    local_demand_id: hasProfile("local_demand_id") || hasGameplay("local_demand_id"),
    contribution_target_id: hasProfile("contribution_target_id") || hasGameplay("contribution_target_id"),
    world_change_due_to_player: gameplayValues.some((gameplay) => truthyText(gameplay.world_change_due_to_player)),
    next_session_goal: hasProfile("next_session_goal") || hasGameplay("next_session_goal") ||
      gameplayValues.some((gameplay) => truthyText(gameplay.return_package?.next_session_goal)),
    recovery_action_id: hasProfile("recovery_action_id") ||
      gameplayValues.some((gameplay) => truthyText(gameplay.incident_recovery?.selected_repair)),
  };
}

export function buildTaskGame076RouteSnapshot({ mode, phase = "completed" }) {
  if (!["accelerate", "stabilize"].includes(mode)) throw new RangeError(`Unknown fixture route: ${mode}`);
  const isAccelerate = mode === "accelerate";
  const snapshot = buildTaskGame076ScenarioSnapshot({
    variant: "attraction_evidence",
    overrides: {
      task_game_076_scenario: {
        version: TASK_GAME_076_SCENARIO_VERSION,
        variant: `route_branch_${mode}`,
        route_phase: phase,
        provenance: "deterministic_provider_backed",
        provider_mode: "deterministic_provider_backed",
        content_profile: {
          effective_play_minutes: 5,
          player_operation_count: 3,
          passive_wait_minutes: 0,
          action_families: ["choose_route", "resolve_route_consequence"],
          content_units: ["route_commitment", "route_consequence", "second_session_variant"],
          implemented_segments: ["route_tradeoff", "incident_recovery", "return_package"],
          local_demand_id: isAccelerate
            ? "local_order.fast_plate_surge"
            : "local_order.edge_outpost_plate_shortfall",
          contribution_target_id: "corp_project.restore_factory_anchor",
          recovery_action_id: isAccelerate ? "recovery.quick_patch" : "recovery.root_cause_patch",
          next_session_goal: isAccelerate
            ? "handle_fast_plate_surge_risk_order"
            : "deliver_starter_batch_to_edge_outpost",
        },
      },
      player_gameplay: {
        stage_status: "completed",
        execution_state: "completed",
        progress_percent: isAccelerate ? 82 : 86,
        last_world_change: isAccelerate
          ? "Accelerated route produced faster output but raised a follow-up jam risk."
          : "Stabilized route reduced jam risk and opened safer local delivery.",
        world_change_due_to_player: isAccelerate
          ? "player chose acceleration, trading reliability for a higher-yield local surge order"
          : "player chose stabilization, trading speed for a safer edge outpost delivery",
        player_leverage_verdict: isAccelerate
          ? "return: high-yield surge order is available, but next session starts with risk handling"
          : "return: stable starter batch can be delivered safely next session",
        branch_hint: isAccelerate
          ? "Return to handle a risky surge order or patch the jam first."
          : "Return to deliver safely or reinforce the stable line.",
        leverage_class: isAccelerate ? "risk_reward_leverage" : "stable_delivery_leverage",
        same_loop_repeat_count: 1,
        route_tradeoff: {
          mode,
          route_commitment_id: isAccelerate
            ? "route.accelerate_output"
            : "route.stabilize_input_line",
          output_delta: isAccelerate ? 3 : 1,
          risk_delta: isAccelerate ? 2 : -2,
          stability_delta: isAccelerate ? -1 : 3,
          ...fixtureRollbackQuote(mode, phase),
          affected_future_beats: ["micro_commission", "incident_recovery", "return_package"],
          next_commission_modifier: isAccelerate
            ? "starter_batch gains +1 output but needs quick patch readiness"
            : "starter_batch requires 1 less recovery step",
          incident_risk_modifier: isAccelerate
            ? "input jam risk raised; output arrives faster"
            : "input jam risk lowered; output pace slower",
          midrun_feedback: {
            timing: "before_incident_recovery",
            visible_metric_delta: isAccelerate
              ? "risk +2, stability -1, output +3"
              : "risk -2, stability +3, output +1",
            player_readable_state: isAccelerate
              ? "The output meter jumps, but the risk meter flashes before the next repair choice."
              : "The risk meter drops before the repair choice, but output grows more slowly.",
          },
          visible_consequence_text: isAccelerate
            ? "Acceleration makes the next order richer, but the jam risk is visible before you repair."
            : "Stabilization lowers the visible jam risk before you choose how to repair.",
          forecast_delta_text: isAccelerate
            ? "Forecast: +3 output, +2 risk, -1 stability."
            : "Forecast: +1 output, -2 risk, +3 stability.",
        },
        incident_recovery: {
          incident_id: "incident.input_jam.factory_0",
          cause: isAccelerate
            ? "accelerated smelter feed overheated the input lane"
            : "minor jam found after stabilizing the input lane",
          repair_options: isAccelerate ? ["quick_patch", "risk_accept"] : ["quick_patch", "root_cause_fix"],
          triggered_by_route: isAccelerate ? "route.accelerate_output" : "route.stabilize_input_line",
          route_consequence_text: isAccelerate
            ? "Because you accelerated output, the next incident is higher-yield but starts with elevated jam risk."
            : "Because you stabilized the input line, the incident is lower-risk and easier to preserve.",
          selected_repair: isAccelerate ? "quick_patch" : "root_cause_fix",
          residual_risk: isAccelerate ? "medium" : "low",
          repaired_capability_delta: isAccelerate ? "surge_output +1, residual_risk +1" : "stable_input_line +1",
          residual_risk_after_repair: isAccelerate ? "medium" : "low",
          repair_tradeoff_costs: {
            quick_patch: {
              time_cost: 1,
              output_delta: isAccelerate ? 1 : 0,
              progress_preserved: true,
              risk_after: "medium",
              stability_delta: isAccelerate ? -1 : 0,
              visible_tradeoff_text: "Quick patch preserves momentum but leaves medium jam risk.",
            },
            root_cause_fix: {
              time_cost: 2,
              output_delta: isAccelerate ? -1 : -1,
              risk_after: "low",
              stability_delta: isAccelerate ? 1 : 2,
              visible_tradeoff_text: "Root-cause fix spends an extra action to reduce future jam risk to low.",
            },
          },
        },
        return_package: {
          earned_summary: isAccelerate
            ? "Produced a faster starter batch and exposed a risky surge order."
            : "Recovered the first smelter, produced a stable starter batch, and found a local order.",
          next_session_goal: isAccelerate
            ? "handle_fast_plate_surge_risk_order"
            : "deliver_starter_batch_to_edge_outpost",
          first_action_on_return: isAccelerate
            ? "open surge order and choose quick patch or risk acceptance"
            : "open local order and choose delivery or hardening",
          choice_memory: [
            isAccelerate ? "selected route.accelerate_output" : "selected route.stabilize_input_line",
            isAccelerate ? "accepted higher jam risk" : "completed root-cause repair",
          ],
          second_session_first_screen_memory: isAccelerate
            ? "Welcome back: your accelerated output route opened a risky surge order."
            : "Welcome back: your stabilized input line is ready for safer Edge Outpost delivery.",
          unlocked_variant: isAccelerate ? "risky_surge_order" : "stable_line_local_delivery",
          why_this_goal: isAccelerate
            ? "Because you chose acceleration, the next session starts with a high-yield but risky local order."
            : "Because you stabilized the input line, the next session can safely deliver to the edge outpost.",
          recovery_plan: null,
        },
      },
    },
  });
  const gameplay = snapshot.player_gameplay;
  if (phase === "before_incident_recovery" || phase === "incident_recovery") {
    gameplay.stage_status = phase === "incident_recovery" ? "blocked" : "executing";
    gameplay.execution_state = gameplay.stage_status;
    gameplay.return_package = null;
    if (phase === "before_incident_recovery") gameplay.incident_recovery = null;
    else Object.assign(gameplay.incident_recovery, {
      selected_repair: null, residual_risk_after_repair: null, repaired_capability_delta: null,
    });
  }
  return snapshot;
}

function buildRouteBranchRegression() {
  const accelerate = buildTaskGame076RouteSnapshot({ mode: "accelerate" });
  const stabilize = buildTaskGame076RouteSnapshot({ mode: "stabilize" });
  const accelerateGameplay = accelerate.player_gameplay;
  const stabilizeGameplay = stabilize.player_gameplay;
  const [accelerateCard, stabilizeCard] = [accelerate, stabilize].map((snapshot) => {
    const gameplay = snapshot.player_gameplay;
    return {
      route_commitment_id: gameplay.route_tradeoff?.route_commitment_id,
      local_demand_id: snapshot.task_game_076_scenario?.content_profile?.local_demand_id,
      next_session_goal: gameplay.return_package?.next_session_goal,
      first_action_on_return: gameplay.return_package?.first_action_on_return,
      incident_route_consequence_text: gameplay.incident_recovery?.route_consequence_text,
      midrun_visible_metric_delta: gameplay.route_tradeoff?.midrun_feedback?.visible_metric_delta,
      rollback_available: gameplay.route_tradeoff?.rollback_available,
      rollback_deadline_beat: gameplay.route_tradeoff?.rollback_deadline_beat,
      rollback_unavailable_reason: gameplay.route_tradeoff?.rollback_unavailable_reason,
      rollback_cost_summary: gameplay.route_tradeoff?.rollback_cost_summary,
      rollback_kept_benefit: gameplay.route_tradeoff?.rollback_kept_benefit,
      rollback_lost_benefit: gameplay.route_tradeoff?.rollback_lost_benefit,
    };
  });
  const pass =
    [accelerate, stabilize].every(fixtureRollbackWindowConsistent) &&
    accelerateGameplay.return_package?.next_session_goal !== stabilizeGameplay.return_package?.next_session_goal &&
    accelerateGameplay.return_package?.first_action_on_return !== stabilizeGameplay.return_package?.first_action_on_return &&
    accelerateGameplay.incident_recovery?.route_consequence_text !== stabilizeGameplay.incident_recovery?.route_consequence_text &&
    accelerate.task_game_076_scenario?.content_profile?.local_demand_id !== stabilize.task_game_076_scenario?.content_profile?.local_demand_id &&
    accelerateGameplay.route_tradeoff?.midrun_feedback?.visible_metric_delta !==
      stabilizeGameplay.route_tradeoff?.midrun_feedback?.visible_metric_delta;
  return {
    status: pass ? "pass" : "fail",
    assertion: "route choice must change later incident consequence and next-session goal",
    accelerate: accelerateCard,
    stabilize: stabilizeCard,
  };
}

function buildSecondRunDesignCard(samples) {
  const gameplays = samples.map((sample) => sample.player_gameplay || {});
  const rollbackQuoteFields = [
    "rollback_deadline_beat",
    "rollback_cost_summary",
    "rollback_kept_benefit",
    "rollback_lost_benefit",
  ];
  const rollbackQuotesComplete = gameplays
    .filter((gameplay) => gameplay.route_tradeoff != null)
    .every((gameplay) =>
      gameplay.route_tradeoff.rollback_available === false ||
      (gameplay.route_tradeoff.rollback_available === true &&
        rollbackQuoteFields.every((field) => playerReadableText(gameplay.route_tradeoff[field]))),
    );
  const routeTradeoffPersists = gameplays.some((gameplay) =>
    Array.isArray(gameplay.route_tradeoff?.affected_future_beats) &&
    gameplay.route_tradeoff.affected_future_beats.length >= 2 &&
    truthyText(gameplay.route_tradeoff.next_commission_modifier) &&
    truthyText(gameplay.route_tradeoff.incident_risk_modifier),
  );
  const namedCommissionOutput = gameplays.some((gameplay) =>
    truthyText(gameplay.micro_commission?.output_item_id) &&
    truthyText(gameplay.micro_commission?.output_display_name) &&
    truthyText(gameplay.micro_commission?.assigned_local_demand_id) &&
    truthyText(gameplay.micro_commission?.screenshot_caption) &&
    gameplay.micro_commission?.delivery_status !== "unassigned",
  );
  const opportunityGeneratedFromChoice = gameplays.some((gameplay) =>
    Array.isArray(gameplay.opportunity_scan?.generated_from) &&
    gameplay.opportunity_scan.generated_from.length > 0 &&
    truthyText(gameplay.opportunity_scan?.recommended_next_action_reason),
  );
  const choiceReflectiveReturnGoal = gameplays.some((gameplay) =>
    Array.isArray(gameplay.return_package?.choice_memory) &&
    gameplay.return_package.choice_memory.length >= 2 &&
    truthyText(gameplay.return_package?.unlocked_variant) &&
    truthyText(gameplay.return_package?.why_this_goal),
  );
  const missing = [];
  if (!routeTradeoffPersists) missing.push("route_tradeoff_persistence");
  if (!namedCommissionOutput) missing.push("named_commission_output");
  if (!opportunityGeneratedFromChoice) missing.push("opportunity_generated_from_choice");
  if (!choiceReflectiveReturnGoal) missing.push("choice_reflective_return_goal");
  if (!rollbackQuotesComplete) missing.push("route_rollback_quote_missing");
  if (!samples.every(fixtureRollbackWindowConsistent)) missing.push("route_rollback_window_inconsistent");
  return {
    status: missing.length === 0 ? "second_run_hook_pass" : "second_run_hook_weak",
    route_tradeoff_persists_across_beats: routeTradeoffPersists,
    named_commission_output: namedCommissionOutput,
    opportunity_generated_from_choice: opportunityGeneratedFromChoice,
    choice_reflective_return_goal: choiceReflectiveReturnGoal,
    missing,
  };
}

function buildAntiScriptDesignCard(samples, completionProfile) {
  const gameplays = samples.map((sample) => sample.player_gameplay || {});
  const visibleMidrunRouteConsequence = gameplays.some((gameplay) =>
    truthyText(gameplay.route_tradeoff?.midrun_feedback?.visible_metric_delta) &&
    truthyText(gameplay.route_tradeoff?.visible_consequence_text) &&
    truthyText(gameplay.route_tradeoff?.forecast_delta_text),
  );
  const localDemandProgressAfterDelivery = gameplays.some((gameplay) =>
    Number(gameplay.local_demand_progress_after_delivery?.progress_delta || 0) > 0 &&
    truthyText(gameplay.local_demand_progress_after_delivery?.visible_result_text),
  ) || gameplays.some((gameplay) =>
    Number(gameplay.micro_commission?.local_demand_progress_preview?.after || 0) >
      Number(gameplay.micro_commission?.local_demand_progress_preview?.before || 0) &&
    truthyText(gameplay.micro_commission?.local_demand_progress_preview?.visible_result_text),
  );
  const secondSessionChoiceMemory = gameplays.some((gameplay) =>
    Array.isArray(gameplay.return_package?.choice_memory) &&
    gameplay.return_package.choice_memory.length >= 2 &&
    truthyText(gameplay.return_package?.second_session_first_screen_memory),
  );
  const repairTradeoffCostVisible = gameplays.some((gameplay) =>
    truthyText(gameplay.incident_recovery?.repair_tradeoff_costs?.quick_patch?.visible_tradeoff_text) &&
    truthyText(gameplay.incident_recovery?.repair_tradeoff_costs?.root_cause_fix?.visible_tradeoff_text) &&
    JSON.stringify(gameplay.incident_recovery.repair_tradeoff_costs.quick_patch) !==
      JSON.stringify(gameplay.incident_recovery.repair_tradeoff_costs.root_cause_fix),
  );
  const boredomNegativeGuard = buildBoredomNegativeRegression().status === "pass";
  const missing = [];
  if (!visibleMidrunRouteConsequence) missing.push("visible_midrun_route_consequence");
  if (completionProfile !== "production_only" && !localDemandProgressAfterDelivery) missing.push("local_demand_progress_after_delivery");
  if (!secondSessionChoiceMemory) missing.push("second_session_choice_memory");
  if (!boredomNegativeGuard) missing.push("boredom_negative_guard");
  if (!repairTradeoffCostVisible) missing.push("repair_tradeoff_cost_visible");
  return {
    status: missing.length === 0 ? "anti_script_pass" : "anti_script_weak",
    visible_midrun_route_consequence: visibleMidrunRouteConsequence,
    local_demand_progress_after_delivery: localDemandProgressAfterDelivery,
    delivery_requirement: completionProfile === "production_only" ? "not_applicable" : "required_for_delivery_profile",
    second_session_choice_memory: secondSessionChoiceMemory,
    boredom_negative_guard: boredomNegativeGuard,
    repair_tradeoff_cost_visible: repairTradeoffCostVisible,
    missing,
  };
}

function buildMotivationDensityCard(cards) {
  const meaningfulDecisionCount = cards.reduce((sum, card) => sum + card.meaningful_decision_count, 0);
  const rewardOrUnlockCount = cards.reduce((sum, card) => sum + card.reward_or_unlock_count, 0);
  const stallOrWaitPeriods = cards.reduce((sum, card) => sum + card.stall_or_wait_periods, 0);
  const branchOfferClarity = cards.some((card) => card.branch_offer_clarity === "clear") ? "clear" : "unclear";
  const continueReason = cards.find((card) => truthyText(card.continue_reason))?.continue_reason || null;
  const returnHook = cards.find((card) => truthyText(card.return_hook))?.return_hook || null;
  const leverageClass = cards.find((card) => truthyText(card.leverage_class))?.leverage_class || null;
  const missing = [];
  if (meaningfulDecisionCount < ATTRACTION_THRESHOLDS.minMeaningfulDecisionCount) missing.push("meaningful_decision_count");
  if (rewardOrUnlockCount < ATTRACTION_THRESHOLDS.minRewardOrUnlockCount) missing.push("reward_or_unlock_count");
  if (branchOfferClarity !== "clear") missing.push("branch_offer_clarity");
  if (!continueReason) missing.push("continue_reason");
  if (!returnHook) missing.push("return_hook");
  if (!leverageClass) missing.push("leverage_class");
  return {
    status: missing.length === 0 ? "motivation_density_pass" : "motivation_density_weak",
    meaningful_decision_count: meaningfulDecisionCount,
    decision_count_scope: "summed_snapshot_choice_opportunity_proxy",
    reward_or_unlock_count: rewardOrUnlockCount,
    stall_or_wait_periods: stallOrWaitPeriods,
    branch_offer_clarity: branchOfferClarity,
    continue_reason: continueReason,
    return_hook: returnHook,
    leverage_class: leverageClass,
    missing,
  };
}

function buildSufficiency(cards, motivationDensityCard) {
  const averageHookScore = cards.reduce((sum, card) => sum + card.hook_score, 0) / Math.max(1, cards.length);
  const averageReplayIntent = cards.reduce((sum, card) => sum + card.replay_intent, 0) / Math.max(1, cards.length);
  const missing = [];
  if (cards.length < ATTRACTION_THRESHOLDS.requiredCards) missing.push("attraction_cards");
  if (averageHookScore < ATTRACTION_THRESHOLDS.minAverageHookScore) missing.push("hook_score");
  if (averageReplayIntent < ATTRACTION_THRESHOLDS.minAverageReplayIntent) missing.push("replay_intent");
  for (const key of motivationDensityCard.missing) {
    if (!missing.includes(key)) missing.push(key);
  }
  if (cards.some((card) => card.blocked_without_recovery)) missing.push("blocked_without_recovery");
  if (cards.some((card) => card.verdict === "progression_pass_but_attraction_weak")) {
    missing.push("progression_pass_but_attraction_weak");
  }
  return {
    status: missing.length === 0 ? "attraction_pass" : "attraction_weak",
    average_hook_score: Number(averageHookScore.toFixed(2)),
    average_replay_intent: Number(averageReplayIntent.toFixed(2)),
    missing,
  };
}

function buildWeakSampleRegression() {
  const weakSnapshot = buildTaskGame076ScenarioSnapshot({ variant: "weak_high_progress" });
  const weakCard = buildAttractionCardFromSnapshot(weakSnapshot, 99);
  return {
    status: weakCard.verdict === "progression_pass_but_attraction_weak" ? "pass" : "fail",
    detected_verdict: weakCard.verdict,
    sample_id: weakCard.sample_id,
    assertion: "high numeric progress without cause, choice, reward, or return hook must not be attraction_pass",
  };
}

function buildBoredomNegativeRegression() {
  const repeatedRecommendationSamples = [
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "boredom_repeated_recommendation",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 12,
            player_operation_count: 6,
            passive_wait_minutes: 5,
            action_families: ["advance_step", "refresh_snapshot"],
            content_units: ["goal", "progress"],
            implemented_segments: ["diagnosis_focus"],
          },
        },
        player_gameplay: {
          stage_status: "executing",
          execution_state: "executing",
          progress_percent: 76,
          last_world_change: "Factory loop continued without a new player-caused consequence.",
          world_change_due_to_player: null,
          player_leverage_verdict: null,
          branch_hint: null,
          same_loop_repeat_count: 3,
          recommended_action_sequence: ["step", "wait", "refresh"],
        },
      },
    }),
    buildTaskGame076ScenarioSnapshot({
      variant: "attraction_evidence",
      overrides: {
        task_game_076_scenario: {
          version: TASK_GAME_076_SCENARIO_VERSION,
          variant: "boredom_repeated_wait",
          provenance: "deterministic_provider_backed",
          provider_mode: "deterministic_provider_backed",
          content_profile: {
            effective_play_minutes: 10,
            player_operation_count: 5,
            passive_wait_minutes: 5,
            action_families: ["advance_step", "refresh_snapshot"],
            content_units: ["goal", "progress"],
            implemented_segments: ["route_tradeoff"],
          },
        },
        player_gameplay: {
          stage_status: "executing",
          execution_state: "executing",
          progress_percent: 79,
          last_world_change: "Recommended action repeated without a visible new choice.",
          world_change_due_to_player: null,
          player_leverage_verdict: null,
          branch_hint: null,
          same_loop_repeat_count: 3,
          recommended_action_sequence: ["wait", "refresh", "step"],
        },
      },
    }),
  ];
  const cards = repeatedRecommendationSamples.map((sample, index) => buildAttractionCardFromSnapshot(sample, index + 200));
  const contentVolumeCard = buildContentVolumeCard(cards);
  const motivationDensityCard = buildMotivationDensityCard(cards);
  const sufficiency = buildSufficiency(cards, motivationDensityCard);
  const repeatedPassiveOnly = repeatedRecommendationSamples.every((sample) => {
    const sequence = sample.player_gameplay?.recommended_action_sequence || [];
    return sequence.length >= 2 && sequence.every((action) => ["step", "wait", "refresh"].includes(action));
  });
  const detectedStatus = repeatedPassiveOnly ? "attraction_weak" : sufficiency.status;
  return {
    status: detectedStatus === "attraction_weak" ? "pass" : "fail",
    detected_status: detectedStatus,
    assertion: "consecutive step/wait/refresh recommendations without new consequence must remain attraction_weak",
  };
}

export function buildTaskGame076AttractionEvidence({ samples = defaultAttractionSamples(), completionProfile = "production_and_delivery" } = {}) {
  const attractionCards = samples.map((sample, index) => buildAttractionCardFromSnapshot(sample, index));
  const motivationDensityCard = buildMotivationDensityCard(attractionCards);
  const contentVolumeCard = buildContentVolumeCard(attractionCards);
  const implementedSegments = implementedContentSegments(attractionCards);
  const gameplayTruthCoverage = buildGameplayTruthCoverage(samples, attractionCards);
  const secondRunDesignCard = buildSecondRunDesignCard(samples);
  const antiScriptDesignCard = buildAntiScriptDesignCard(samples, completionProfile);
  const routeBranchRegression = buildRouteBranchRegression();
  const boredomNegativeRegression = buildBoredomNegativeRegression();
  return {
    task: "TASK-GAME-076",
    scenario_version: TASK_GAME_076_SCENARIO_VERSION,
    evidence_kind: "fixture_structure_model",
    source_scope: "fixture_structure_only",
    completion_profile: completionProfile,
    human_observation_status: "unverified",
    thresholds: clone(ATTRACTION_THRESHOLDS),
    attraction_cards: attractionCards,
    motivation_density_card: motivationDensityCard,
    content_volume_card: contentVolumeCard,
    implemented_content_segments: implementedSegments,
    content_volume_supplement_complete: CONTENT_VOLUME_SUPPLEMENT_SEGMENTS.every((segment) => implementedSegments.includes(segment)),
    gameplay_truth_coverage: gameplayTruthCoverage,
    second_run_design_card: secondRunDesignCard,
    anti_script_design_card: antiScriptDesignCard,
    route_branch_regression: routeBranchRegression,
    raw_snapshots: samples.map((sample) => clone(sample)),
    weak_sample_regression: buildWeakSampleRegression(),
    boredom_negative_regression: boredomNegativeRegression,
    sufficiency: buildSufficiency(attractionCards, motivationDensityCard),
    claim_boundary:
      "Fixture scores and choice proxies only verify structure; live automation verifies its recorded candidate operations; motivation and voluntary return require separate human observations.",
  };
}

function statusForCommands(commands, requiredCommands, { tier, liveOptional = false, visualOptional = false, skipBevy = false, skipRuntimeUnit = false } = {}) {
  const missing = [];
  const failing = [];
  for (const key of requiredCommands) {
    const status = commands[key]?.status || "missing";
    if (status === "fail") {
      failing.push(key);
    } else if (status !== "pass") {
      missing.push(key);
    }
  }
  if (failing.length > 0) return { status: "failed", missingOrFailedCommands: failing };
  if (missing.length > 0) {
    if (liveOptional && tier !== "live") {
      return { status: "covered_by_live_when_run", missingOrFailedCommands: missing };
    }
    if (visualOptional && skipBevy) {
      return { status: "visual_unverified", missingOrFailedCommands: missing };
    }
    if (skipRuntimeUnit && missing.some((key) => key.startsWith("runtime_"))) {
      return { status: "runtime_unverified", missingOrFailedCommands: missing };
    }
    return { status: "unverified", missingOrFailedCommands: missing };
  }
  return { status: "covered", missingOrFailedCommands: [] };
}

function provenanceForBeat(beat, status) {
  if (status !== "covered") {
    if (status === "visual_unverified") return "visual_only";
    if (status === "runtime_unverified") return "unverified";
    return "unverified";
  }
  if (beat.requiredCommands.some((key) => key.startsWith("live_"))) return "live_verified";
  if (beat.requiredCommands.some((key) => key.startsWith("runtime_"))) return "runtime_backed";
  if (beat.requiredCommands.includes("bevy_visual_probe")) return "visual_only";
  return "viewer_fixture_only";
}

export function buildTaskGame076AutomationSummary({
  commands,
  tier,
  outDir,
  skipBevy = false,
  skipRuntimeUnit = false,
  overallStatus,
} = {}) {
  const beats = TASK_GAME_076_BEATS.map((beat) => {
    const result = statusForCommands(commands, beat.requiredCommands, {
      tier,
      liveOptional: beat.liveOptional,
      visualOptional: beat.visualOptional,
      skipBevy,
      skipRuntimeUnit,
    });
    return {
      time: beat.time,
      beat: beat.beat,
      required: [...beat.requiredCommands],
      assertion: beat.assertion,
      gap_status: beat.gapStatus,
      status: result.status,
      provenance: provenanceForBeat(beat, result.status),
      missing_or_failed_commands: result.missingOrFailedCommands,
      ...(result.status !== "covered" ? { status_note: beat.gapStatus } : {}),
    };
  });

  return {
    task: "TASK-GAME-076",
    scenario_version: TASK_GAME_076_SCENARIO_VERSION,
    tier,
    overall_status: overallStatus === "fail" || Object.values(commands).some((command) => command.status === "fail") ? "fail" :
      Object.keys(commands).length === 0 || Object.values(commands).some((command) => !["pass", "skipped"].includes(command.status)) ||
      (overallStatus && overallStatus !== "pass") ? "unverified" : "pass",
    out_dir: outDir,
    commands,
    beats,
    policy: {
      manual_cards_do_not_replace_automation: true,
      bevy_does_not_replace_gameplay_causality: true,
      live_tier_required_for_real_player_path_and_pure_api_claims: true,
      scenario_driver_is_single_source_for_viewer_visual_fixtures: true,
    },
  };
}
