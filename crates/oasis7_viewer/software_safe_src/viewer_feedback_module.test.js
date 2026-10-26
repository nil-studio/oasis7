import { describe, expect, it } from "vitest";
import { createViewerFeedbackModule } from "./viewer_feedback_module.js";

function createFeedbackModule(state) {
  return createViewerFeedbackModule({
    clone: (value) => (value == null ? value : JSON.parse(JSON.stringify(value))),
    feedbackBadgeClass: () => "feedback-badge",
    hostedActionPolicy: () => null,
    isAgentVisibleToCurrentSession: () => true,
    isLocaleZh: (locale) => locale === "zh",
    localeText: (locale, zh, en) => (locale === "zh" ? zh : en),
    state,
  });
}

describe("viewer feedback module", () => {
  it.each(["agent-A", null])("keeps local receipt subject %s independent from a newer runtime intent", (agentId) => {
    const state = {uiLocale:"en",snapshot:{model:{agents:{"agent-A":{id:"agent-A"},"agent-B":{id:"agent-B"}},locations:{base:{id:"base"}}},player_gameplay:{intent_target:"agent-B",recent_feedback:{action:"build",stage:"completed"}}},lastGameplayActionFeedback:{kind:"gameplay_action",action:"build",stage:"completed",agentId}};
    const summary = createFeedbackModule(state).buildGameplaySummary();
    expect(summary.receiptTargetAgentId).toBe(agentId);
    expect(summary.recentFeedback.agentId).toBe(agentId);
    expect(summary.acceptedIntentTarget).toBe("agent-B");
  });
  it("projects agency only from a visible canonical primary intent", () => {
    const agency = { causal_receipt: { receipt_id: "canonical-receipt" }, memory_corrections: [] };
    const state = { uiLocale: "en", snapshot: {
      model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
      player_gameplay: { primary_intent: { intent_id: "intent-1", agent_id: "agent-0", agency_read_model: agency } },
    } };
    const projected = createFeedbackModule(state).buildGameplaySummary().controlProof;
    expect(projected.intentId).toBe("intent-1");
    expect(projected.agency).toEqual(agency);
    expect(projected.agency).not.toBe(agency);
    delete state.snapshot.player_gameplay.primary_intent.intent_id;
    expect(createFeedbackModule(state).buildGameplaySummary().controlProof.agency).toBeNull();
  });
  it.each([
    ["accepted", "accepted"],
    ["applied", "applied"],
    ["stale", "stale"],
    ["rejected", "rejected"],
    ["blocked", "blocked"],
  ])("presents enhanced prompt result status %s", (status, expectedText) => {
    const module = createFeedbackModule({ uiLocale: "en" });
    const feedback = {
      id: "prompt-result",
      kind: "prompt",
      action: "prompt_control_apply",
      agentId: "agent-0",
      accepted: status === "accepted" || status === "applied",
      ok: status === "accepted" || status === "applied",
      stage: status,
      response: {
        status,
        operation: "apply",
        preview: status === "accepted",
        version: 5,
        applied_fields: ["system_prompt"],
        applied_scope: "runtime_instance",
        persistence_scope: "none",
        sync_scope: "none",
        reason_code: status,
        next_step: status === "stale" ? "refresh_version_and_retry" : "continue_runtime",
      },
    };
    const description = module.describeSemanticFeedback(feedback, "en");
    expect(description.label.toLowerCase()).toContain(expectedText);
    expect(description.summary.toLowerCase()).toContain(expectedText);
    if (status === "applied") {
      expect(description.detail).toContain("persistence: none");
      expect(description.detail).toContain("sync: none");
    }
  });

  it("redacts protected metadata from hidden prompt-control receipts", () => {
    const module = createFeedbackModule({ uiLocale: "en" });
    const snapshot = module.snapshotSemanticFeedback({
      id: "hidden-result",
      kind: "prompt",
      action: "prompt_control_apply",
      agentId: "agent-0",
      accepted: false,
      ok: false,
      stage: "blocked",
      response: {
        request_id: "pc-hidden",
        status: "blocked",
        value_visibility: "hidden",
        reason_code: "control_lost",
        next_step: "reauthenticate_and_refresh_binding",
        player_id: "player-secret",
        binding_epoch: 12,
        version: 9,
        digest: "digest-secret",
        applied_fields: ["system_prompt"],
        system_prompt: "latest secret prompt",
      },
    });

    expect(snapshot.response).toEqual({
      request_id: "pc-hidden",
      status: "blocked",
      value_visibility: "hidden",
      reason_code: "control_lost",
      next_step: "reauthenticate_and_refresh_binding",
    });
    expect(JSON.stringify(snapshot)).not.toContain("player-secret");
    expect(JSON.stringify(snapshot)).not.toContain("digest-secret");
    expect(JSON.stringify(snapshot)).not.toContain("latest secret prompt");
    expect(module.describeSemanticFeedback(snapshot, "en").detail).toContain("Re-authenticate and refresh the current binding");
  });

  it("reports a replayed applied receipt without implying a second mutation", () => {
    const module = createFeedbackModule({ uiLocale: "en" });
    const description = module.describeSemanticFeedback({
      kind: "prompt",
      action: "prompt_control_apply",
      stage: "applied",
      accepted: true,
      ok: true,
      response: {
        status: "applied",
        operation: "apply",
        version: 6,
        idempotent_replay: true,
        mutation_count: 1,
        applied_scope: "runtime_instance",
        persistence_scope: "none",
        sync_scope: "none",
      },
    }, "en");

    expect(description.detail).toContain("no second mutation");
  });

  it("projects a hidden chat authority refusal as safe control-loss recovery", () => {
    const module = createFeedbackModule({ uiLocale: "en" });
    const snapshot = module.snapshotSemanticFeedback({
      id: "chat-control-lost",
      kind: "chat",
      action: "agent_chat",
      agentId: "agent-0",
      accepted: false,
      ok: false,
      stage: "error",
      response: {
        status: "blocked",
        value_visibility: "hidden",
        reason_code: "control_lost",
        next_step: "reauthenticate_and_refresh_binding",
        message: "Agent control was lost; re-authenticate and refresh the current binding before retrying.",
        player_id: "player-secret",
        provider_trace: "trace-secret",
      },
    });

    expect(snapshot.response).toEqual(expect.objectContaining({
      status: "blocked",
      value_visibility: "hidden",
      reason_code: "control_lost",
      next_step: "reauthenticate_and_refresh_binding",
    }));
    expect(JSON.stringify(snapshot)).not.toContain("player-secret");
    expect(JSON.stringify(snapshot)).not.toContain("trace-secret");
    const description = module.describeSemanticFeedback(snapshot, "en");
    expect(description.label).toBe("Control lost");
    expect(description.detail).toContain("Re-authenticate and refresh the current binding");
  });

  it.each([
    ["en", "unknown_internal_code", "Current blocker"],
    ["zh", "unknown_internal_code", "当前阻塞"],
    ["en", "power_shortage", "Missing Power"],
    ["zh", "power_shortage", "缺电"],
    ["en", null, null],
    ["zh", null, null],
  ])("keeps the expanded Capability Economics blocker label safe in %s for %s", (locale, blockerKind, expectedLabel) => {
    const summary = createFeedbackModule({
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          goal_kind: "recover_capability",
          stage_status: "blocked",
          blocker_kind: blockerKind,
          blocker_detail: "diagnostic only",
          objective: "Restore production",
        },
      },
      uiLocale: locale,
    }).buildGameplaySummary();

    // main.jsx renders this exact value in the expanded Capability Economics panel.
    expect(summary.economicSurface.blockerLabel).toBe(expectedLabel);
    expect(summary.blockerLabel).toBe(expectedLabel);
    expect(summary.blockerKind).toBe(blockerKind);
  });

  it("preserves camelCase runtime available actions in the gameplay summary", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: {
          agents: {
            "agent-0": { id: "agent-0" },
          },
          locations: {
            base: { id: "base" },
          },
        },
        player_gameplay: {
          goal_kind: "create_first_world_feedback",
          progress_percent: 5,
          availableActions: [
            {
              actionId: "build_factory_smelter_mk1",
              label: "Build smelter",
              protocolAction: "gameplay_action.submit",
              targetAgentId: "agent-0",
              disabledReason: null,
            },
          ],
        },
      },
      uiLocale: "en",
    };

    const summary = createFeedbackModule(state).buildGameplaySummary();

    expect(summary.availableActions).toEqual([
      expect.objectContaining({
        actionId: "build_factory_smelter_mk1",
        label: "Build smelter",
        protocolAction: "gameplay_action.submit",
        targetAgentId: "agent-0",
        disabledReason: null,
        executeKind: "gameplay_action",
      }),
    ]);
    expect(summary.recommendedAction).toEqual(
      expect.objectContaining({
        actionId: "build_factory_smelter_mk1",
        executeKind: "gameplay_action",
      }),
    );
  });

  it("normalizes both persisted future beats from snake_case and camelCase branch recommendations", () => {
    const summaryFor = (playerGameplay) => createFeedbackModule({
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: playerGameplay,
      },
      uiLocale: "en",
    }).buildGameplaySummary();

    const snakeCase = summaryFor({
      branch_recommendations: [{
        action_id: "build_alloy_factory",
        future_beats: ["Open the extra production lane", "Resolve its new supply pressure"],
      }],
    });
    const camelCase = summaryFor({
      branchRecommendations: [{
        actionId: "build_alloy_factory",
        futureBeats: ["Open the extra production lane", "Resolve its new supply pressure"],
      }],
    });

    expect(snakeCase.branchRecommendations[0].futureBeats).toEqual([
      "Open the extra production lane",
      "Resolve its new supply pressure",
    ]);
    expect(camelCase.branchRecommendations[0].futureBeats).toEqual([
      "Open the extra production lane",
      "Resolve its new supply pressure",
    ]);
  });

  it("retains every published fallback tradeoff instead of collapsing the summary to one CTA", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          stage_status: "blocked",
          fallback_tradeoff_preview: [
            { value_class: "safe_wait", available: true, cost: "one bounded recheck", progress_kept: "keeps the current line", opportunity_cost: "delays alternate output", reason: "a confirmed event can clear the blocker", recommended: true },
            { value_class: "repair_now", available: true, cost: "spend repair materials", progress_kept: "keeps the current capability", opportunity_cost: "uses the repair reserve", reason: "the local blocker is repairable", recommended: false },
            { value_class: "reroute_now", available: false, cost: "move to an alternate route", progress_kept: "keeps the goal but changes the route", opportunity_cost: "abandons the current route's near-term output", reason: "no alternate route is currently available", recommended: false },
          ],
        },
      },
      uiLocale: "en",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().fallbackTradeoffPreview).toEqual([
      { valueClass: "safe_wait", available: true, cost: "one bounded recheck", progressKept: "keeps the current line", opportunityCost: "delays alternate output", reason: "a confirmed event can clear the blocker", recommended: true },
      { valueClass: "repair_now", available: true, cost: "spend repair materials", progressKept: "keeps the current capability", opportunityCost: "uses the repair reserve", reason: "the local blocker is repairable", recommended: false },
      { valueClass: "reroute_now", available: false, cost: "move to an alternate route", progressKept: "keeps the goal but changes the route", opportunityCost: "abandons the current route's near-term output", reason: "no alternate route is currently available", recommended: false },
    ]);
    expect(
      createFeedbackModule(state).buildGameplaySummary().fallbackTradeoffPreview
        .filter((option) => option.recommended),
    ).toHaveLength(1);
  });

  it("maps the no-safe-fallback handoff without inventing an executable action", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          no_safe_fallback_reason: "Repair and reroute are unavailable.",
          required_next_decision_action_id: "select_new_goal",
          required_next_decision_class: "goal_selection",
        },
      },
      uiLocale: "en",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().noSafeFallbackHandoff).toEqual({
      reason: "Repair and reroute are unavailable.",
      requiredNextDecisionActionId: "select_new_goal",
      requiredNextDecisionClass: "goal_selection",
    });
  });

  it("presents every wait-resolution quote field through the existing non-actionable recovery panel model", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          wait_resolution_quote: {
            safe_to_wait: false,
            resolution_trigger: "a committed runtime event applies the queued smelter",
            recheck_tick_or_event: "event 8",
            expected_change: "smelter construction becomes visible",
            unresolved_risk: "the action can still be blocked",
            alternative_unlock_condition: "refresh the snapshot and choose an enabled action",
          },
        },
      },
      uiLocale: "en",
    };

    const summary = createFeedbackModule(state).buildGameplaySummary();

    expect(summary.waitResolutionQuote).toEqual(expect.objectContaining({
      safeToWait: false,
      resolutionTrigger: "a committed runtime event applies the queued smelter",
      recheckTickOrEvent: "event 8",
      expectedChange: "smelter construction becomes visible",
      unresolvedRisk: "the action can still be blocked",
      alternativeUnlockCondition: "refresh the snapshot and choose an enabled action",
    }));
    expect(summary.fallbackTradeoffPreview[0]).toEqual(expect.objectContaining({
      valueClass: "safe_wait",
      available: false,
      recommended: false,
      reason: expect.stringContaining("Trigger: a committed runtime event applies the queued smelter"),
      progressKept: "Expected change: smelter construction becomes visible",
      cost: "Recheck: event 8",
      opportunityCost: "Alternative unlock: refresh the snapshot and choose an enabled action",
    }));
    expect(summary.fallbackTradeoffPreview[0].reason).toContain("Unresolved risk: the action can still be blocked");
  });

  it("appends an unavailable wait quote without dropping a published repair choice", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          fallback_tradeoff_preview: [
            { value_class: "repair_now", available: true, cost: "materials", progress_kept: "the line", opportunity_cost: "reserve", reason: "repairable", recommended: true },
          ],
          wait_resolution_quote: {
            safe_to_wait: false,
            resolution_trigger: "a committed runtime event applies the queued smelter",
            recheck_tick_or_event: "event 8",
            expected_change: "smelter construction becomes visible",
            unresolved_risk: "the action can still be blocked",
            alternative_unlock_condition: "refresh the snapshot and choose an enabled action",
          },
        },
      },
      uiLocale: "en",
    };

    const options = createFeedbackModule(state).buildGameplaySummary().fallbackTradeoffPreview;

    expect(options).toHaveLength(2);
    expect(options[0]).toEqual(expect.objectContaining({
      valueClass: "repair_now",
      available: true,
      recommended: true,
      reason: "repairable",
    }));
    expect(options[1]).toEqual(expect.objectContaining({
      valueClass: "safe_wait",
      available: false,
      recommended: false,
    }));
  });

  it("replaces an existing safe-wait option once while preserving other choices", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          fallback_tradeoff_preview: [
            { value_class: "repair_now", available: true, cost: "materials", progress_kept: "the line", opportunity_cost: "reserve", reason: "repairable", recommended: true },
            { value_class: "safe_wait", available: true, cost: "stale", progress_kept: "stale", opportunity_cost: "stale", reason: "stale", recommended: true },
          ],
          wait_resolution_quote: {
            safe_to_wait: false,
            resolution_trigger: "a committed runtime event applies the queued smelter",
            recheck_tick_or_event: "event 8",
            expected_change: "smelter construction becomes visible",
            unresolved_risk: "the action can still be blocked",
            alternative_unlock_condition: "refresh the snapshot and choose an enabled action",
          },
        },
      },
      uiLocale: "en",
    };

    const options = createFeedbackModule(state).buildGameplaySummary().fallbackTradeoffPreview;

    expect(options).toHaveLength(2);
    expect(options[0]).toEqual(expect.objectContaining({ valueClass: "repair_now", reason: "repairable" }));
    expect(options.filter((option) => option.valueClass === "safe_wait")).toEqual([
      expect.objectContaining({
        available: false,
        recommended: false,
        reason: expect.stringContaining("Trigger: a committed runtime event applies the queued smelter"),
      }),
    ]);
  });

  it("preserves English product-validation preview values and labels", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          validation_unlock_preview: {
            product_id: "iron_ingot",
            role_tag: "scale",
            tradable: false,
            required_stage: "bootstrap",
            current_stage: "bootstrap",
            stage_status: "available",
            value_summary: "Validated scale product; trading disabled.",
            next_step_hint: "Use this product in its scale role.",
          },
        },
      },
      uiLocale: "en",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().validationUnlockPreview).toEqual({
      productId: "iron_ingot",
      roleTag: "scale",
      roleLabel: "scale",
      tradable: false,
      requiredStage: "bootstrap",
      requiredStageLabel: "bootstrap",
      currentStage: "bootstrap",
      currentStageLabel: "bootstrap",
      stageStatus: "available",
      stageStatusLabel: "available",
      valueSummary: "Validated scale product; trading disabled.",
      localizedValueSummary: "Validated scale product; trading disabled.",
      nextStepHint: "Use this product in its scale role.",
      localizedNextStepHint: "Use this product in its scale role.",
    });
  });

  it("localizes product-validation preview display text while preserving raw DTO values", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          validation_unlock_preview: {
            product_id: "iron_ingot",
            role_tag: "scale",
            tradable: false,
            required_stage: "bootstrap",
            current_stage: "bootstrap",
            stage_status: "available",
            value_summary: "Validated scale product; trading disabled.",
            next_step_hint: "Use this product in its scale role.",
          },
        },
      },
      uiLocale: "zh",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().validationUnlockPreview).toEqual(expect.objectContaining({
      roleTag: "scale",
      roleLabel: "规模化",
      requiredStage: "bootstrap",
      requiredStageLabel: "起步",
      currentStage: "bootstrap",
      currentStageLabel: "起步",
      stageStatus: "available",
      stageStatusLabel: "可用",
      valueSummary: "Validated scale product; trading disabled.",
      localizedValueSummary: "已验证规模化产品；未启用交易。",
      nextStepHint: "Use this product in its scale role.",
      localizedNextStepHint: "将此产品用于规模化角色；验证不会解锁新能力。",
    }));
  });

  it("localizes a blank-gate stage as no requirement while preserving its raw value", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          validation_unlock_preview: {
            product_id: "iron_ingot",
            role_tag: "scale",
            tradable: true,
            required_stage: "none",
            current_stage: "bootstrap",
            stage_status: "available",
            value_summary: "Validated scale product; trading enabled.",
            next_step_hint: "Use this product in its scale role.",
          },
        },
      },
      uiLocale: "zh",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().validationUnlockPreview).toEqual(expect.objectContaining({
      requiredStage: "none",
      requiredStageLabel: "无要求",
      localizedValueSummary: "已验证规模化产品；已启用交易。",
    }));
  });

  it("normalizes the factory production failure disposition for player display", () => {
    const summary = createFeedbackModule({
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          available_actions: [
            {
              action_id: "schedule_recipe_smelter_iron_ingot",
              label: "Queue iron ingot run",
              protocol_action: "gameplay_action.submit",
              target_agent_id: "agent-0",
              disabled_reason: "insufficient iron_ore in site ledger",
            },
            {
              action_id: "request_snapshot",
              label: "Refresh gameplay snapshot",
              protocol_action: "request_snapshot",
            },
          ],
          factory_production_failure_disposition: {
            action_id: 19,
            requester_agent_id: "agent-0",
            factory_id: "factory.target",
            recipe_id: "recipe.smelter.iron_ingot",
            blocker_kind: "product_validation_rejected",
            blocker_detail: "product profile rejected the committed output",
            disposition_kind: "consumed_lost",
            consumed_inputs: [{ kind: "iron_ore", amount: 3 }],
            lost_inputs: [{ kind: "iron_ore", amount: 3 }],
            consumed_power: 7,
            lost_power: 7,
            next_action: "inspect_product_validation_and_reschedule",
            next_recheck: null,
          },
          wait_resolution_quote: {
            safe_to_wait: true,
            resolution_trigger: "synthetic wait should be hidden",
          },
          fallback_tradeoff_preview: [{ value_class: "safe_wait", available: true }],
        },
      },
      uiLocale: "en",
    }).buildGameplaySummary();

    expect(summary.factoryProductionFailureDisposition).toEqual({
      actionId: "19",
      requesterAgentId: "agent-0",
      factoryId: "factory.target",
      recipeId: "recipe.smelter.iron_ingot",
      blockerKind: "Product validation failed",
      blockerDetail: "product profile rejected the committed output",
      dispositionKind: "Inputs consumed and lost",
      consumedInputs: [{ kind: "iron_ore", amount: 3 }],
      lostInputs: [{ kind: "iron_ore", amount: 3 }],
      consumedPower: 7,
      lostPower: 7,
      nextAction: "Refresh gameplay snapshot",
      recoveryAction: {
        actionId: "request_snapshot",
        label: "Refresh gameplay snapshot",
        protocolAction: "request_snapshot",
        targetAgentId: null,
        disabledReason: null,
        executeKind: "request_snapshot",
      },
      recoveryActionId: "request_snapshot",
      recoveryActionLabel: "Refresh gameplay snapshot",
      recoveryActionDisabledReason: null,
      nextRecheckBoundary: "next committed snapshot",
      nextRecheck: null,
    });
    expect(summary.waitResolutionQuote).toBeNull();
    expect(summary.fallbackTradeoffPreview).toEqual([]);
    expect(JSON.stringify(summary.factoryProductionFailureDisposition)).not.toMatch(
      /product_validation_rejected|consumed_lost|inspect_product_validation_and_reschedule/,
    );
  });

  it("accepts camelCase failure receipts and keeps old snapshots receipt-free", () => {
    const summaryFor = (playerGameplay) => createFeedbackModule({
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: playerGameplay,
      },
      uiLocale: "zh",
    }).buildGameplaySummary();

    const summary = summaryFor({
      factoryProductionFailureDisposition: {
        actionId: "20",
        requesterAgentId: "agent-0",
        factoryId: "factory.camel",
        recipeId: "recipe.camel",
        blockerKind: "product_validation",
        blockerDetail: "需要检查产品验证",
        dispositionKind: "consumed_lost",
        consumedInputs: [{ kind: "copper_ore", amount: 2 }],
        lostInputs: [],
        consumedPower: 4,
        lostPower: 0,
        nextAction: "inspect_product_validation_and_reschedule",
        nextRecheck: 12,
      },
    });

    expect(summary.factoryProductionFailureDisposition).toEqual(expect.objectContaining({
      actionId: "20",
      blockerKind: "产品验证失败",
      dispositionKind: "投入已消费且损失",
      nextAction: "暂无安全恢复路径",
      recoveryAction: expect.objectContaining({ actionId: "no_safe_path", executeKind: "none" }),
      recoveryActionId: "no_safe_path",
      recoveryActionLabel: "暂无安全恢复路径",
      recoveryActionDisabledReason: "当前 committed 快照没有可执行的排程、修复、补给或刷新动作；等待下一次复查。",
      nextRecheckBoundary: "世界时刻 12",
      nextRecheck: 12,
    }));
    expect(summaryFor({}).factoryProductionFailureDisposition).toBeNull();
  });

  it("lets a fresh matching schedule recovery own the hero recommendation", () => {
    const summary = createFeedbackModule({
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          available_actions: [
            {
              action_id: "schedule_recipe_smelter_iron_ingot",
              label: "Queue iron ingot run",
              protocol_action: "gameplay_action.submit",
              target_agent_id: "agent-0",
            },
            {
              action_id: "request_snapshot",
              label: "Refresh gameplay snapshot",
              protocol_action: "request_snapshot",
            },
            {
              action_id: "advance_step",
              label: "Advance 1 step",
              protocol_action: "live_control.step",
            },
            {
              action_id: "resume_play",
              label: "Resume play",
              protocol_action: "live_control.play",
            },
          ],
          factory_production_failure_disposition: {
            action_id: 22,
            requester_agent_id: "agent-0",
            factory_id: "factory.target",
            recipe_id: "recipe.smelter.iron_ingot",
            blocker_kind: "product_validation_rejected",
            blocker_detail: "product profile rejected the committed output",
            disposition_kind: "consumed_lost",
            next_action: "inspect_product_validation_and_reschedule",
          },
        },
      },
      uiLocale: "en",
    }).buildGameplaySummary();

    expect(summary.factoryProductionFailureDisposition.recoveryAction).toEqual(expect.objectContaining({
      actionId: "schedule_recipe_smelter_iron_ingot",
      executeKind: "gameplay_action",
    }));
    expect(summary.recommendedAction).toEqual(expect.objectContaining({
      actionId: "schedule_recipe_smelter_iron_ingot",
      executeKind: "gameplay_action",
    }));
  });

  it("does not treat live step or play controls as factory failure recovery", () => {
    const failureDisposition = {
      action_id: 21,
      requester_agent_id: "agent-0",
      factory_id: "factory.target",
      recipe_id: "recipe.smelter.iron_ingot",
      blocker_kind: "product_validation_rejected",
      blocker_detail: "product profile rejected the committed output",
      disposition_kind: "consumed_lost",
      next_action: "inspect_product_validation_and_reschedule",
    };
    const summaryFor = (availableActions) => createFeedbackModule({
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          available_actions: availableActions,
          factory_production_failure_disposition: failureDisposition,
        },
      },
      uiLocale: "en",
    }).buildGameplaySummary();
    const disabledReschedule = {
      action_id: "schedule_recipe_smelter_iron_ingot",
      label: "Queue iron ingot run",
      protocol_action: "gameplay_action.submit",
      target_agent_id: "agent-0",
      disabled_reason: "insufficient iron_ore in site ledger",
    };
    const snapshot = {
      action_id: "request_snapshot",
      label: "Refresh gameplay snapshot",
      protocol_action: "request_snapshot",
    };
    const advanceStep = {
      action_id: "advance_step",
      label: "Advance 1 step",
      protocol_action: "live_control.step",
    };
    const resumePlay = {
      action_id: "resume_play",
      label: "Resume play",
      protocol_action: "live_control.play",
    };

    const snapshotSummary = summaryFor([disabledReschedule, snapshot, advanceStep, resumePlay]);
    expect(snapshotSummary.factoryProductionFailureDisposition).toEqual(
      expect.objectContaining({
        recoveryAction: expect.objectContaining({
          actionId: "request_snapshot",
          executeKind: "request_snapshot",
        }),
        recoveryActionId: "request_snapshot",
      }),
    );
    expect(snapshotSummary.recommendedAction).toEqual(expect.objectContaining({
      actionId: "request_snapshot",
      executeKind: "request_snapshot",
    }));

    const noSafePathSummary = summaryFor([disabledReschedule, advanceStep, resumePlay]);
    expect(noSafePathSummary.factoryProductionFailureDisposition).toEqual(
      expect.objectContaining({
        recoveryAction: expect.objectContaining({ actionId: "no_safe_path", executeKind: "none" }),
        recoveryActionId: "no_safe_path",
      }),
    );
    expect(noSafePathSummary.recommendedAction).toBeNull();
  });

  it("projects canonical micro depot facilities without inventing quote or ROI fields", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: { "agent-0": { id: "agent-0" } }, locations: { base: { id: "base" } } },
        player_gameplay: {
          micro_depot_facilities: [{
            facility_id: "depot-1",
            owner_claim_id: "claim-1",
            status: "active",
            location_id: "base",
            available_units_by_kind: { data: 8 },
            module_id: "micro_depot.eval.v2",
            module_version: "v2",
            wasm_hash: "wasm-hash",
            last_receipt_id: "receipt-1",
            last_proposal_hash: "proposal-1",
            available_actions: ["service_micro_depot_repair"],
          }],
        },
      },
      uiLocale: "en",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().microDepotFacilities).toEqual([
      expect.objectContaining({
        facilityId: "depot-1",
        availableUnitsByKind: { data: 8 },
        moduleId: "micro_depot.eval.v2",
        lastReceiptId: "receipt-1",
        lastProposalHash: "proposal-1",
        availableActions: ["service_micro_depot_repair"],
      }),
    ]);
  });

  it("drops malformed micro depot facility entries and normalizes display collections", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: {}, locations: {} },
        player_gameplay: {
          micro_depot_facilities: [
            null,
            "not-a-facility",
            {
              facility_id: "depot-safe",
              available_units_by_kind: "not-an-inventory-record",
              supported_resource_kinds: "data",
              available_actions: [" service_micro_depot_repair ", 7, "", { action: "unsafe" }],
            },
          ],
        },
      },
      uiLocale: "en",
    };

    expect(createFeedbackModule(state).buildGameplaySummary().microDepotFacilities).toEqual([
      expect.objectContaining({
        facilityId: "depot-safe",
        availableUnitsByKind: {},
        supportedResourceKinds: [],
        availableActions: ["service_micro_depot_repair"],
      }),
    ]);
  });

  it("uses an empty facility list when the optional snapshot field is absent or wrong-typed", () => {
    for (const microDepotFacilities of [undefined, null, {}, "depot-1"]) {
      const state = {
        lastGameplayActionFeedback: null,
        snapshot: {
          model: { agents: {}, locations: {} },
          player_gameplay: { micro_depot_facilities: microDepotFacilities },
        },
        uiLocale: "en",
      };

      expect(createFeedbackModule(state).buildGameplaySummary().microDepotFacilities).toEqual([]);
    }
  });

  it("renders published recovery options as escaped-by-the-view comparison details", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: {}, locations: {} },
        player_gameplay: {
          repair_available: true,
          rebuild_available: true,
          pivot_available: true,
          recovery_options: [
            {
              kind: "repair",
              estimated_time_class: "short",
              estimated_resource_class: "low",
              risk_class: "low",
              retained_benefit: "<existing throughput>",
              recommendation_reason: "Fastest safe recovery",
            },
            {
              kind: "rebuild",
              estimated_time_class: "medium",
              estimated_resource_class: "high",
              risk_class: "medium",
              retained_benefit: "new capacity",
              recommendation_reason: "Restores a stronger line",
            },
          ],
        },
      },
      uiLocale: "en",
    };

    const continuation = createFeedbackModule(state).buildGameplaySummary().matureWorldContinuation;

    expect(continuation.recoveryOptionComparisons).toEqual([]);
    expect(continuation.recoveryOptions).toContain("Recovery feasibility unknown");
  });

  it("keeps boolean recovery summaries when runtime recovery records are absent or invalid", () => {
    const state = {
      lastGameplayActionFeedback: null,
      snapshot: {
        model: { agents: {}, locations: {} },
        player_gameplay: {
          repair_available: true,
          rebuild_available: false,
          recovery_options: [{ estimated_time_class: "short" }, "repair"],
        },
      },
      uiLocale: "en",
    };

    const continuation = createFeedbackModule(state).buildGameplaySummary().matureWorldContinuation;

    expect(continuation.recoveryOptionComparisons).toEqual([]);
    expect(continuation.recoveryOptions).toContain("Recovery feasibility unknown");
  });
});


describe("owned Agent claim convergence", () => {
  it.each(["received", "pending", "unknown"])("keeps %s claims separate from accepted world effects", (stage) => {
    const state = {uiLocale:"en",auth:{},snapshot:{model:{agents:{},locations:{}},player_gameplay:{blocker_kind:"runtime_snapshot_empty_entities"}},
      lastGameplayActionFeedback:{kind:"gameplay_action",action:"claim_first_agent",stage,accepted:false,ok:false}};
    const summary = createFeedbackModule(state).buildGameplaySummary();
    expect(summary.executionState).toBe(stage);
    expect(summary.executionSummary).not.toMatch(/is executing|is queued/);
    expect(summary.executionCauseKind).toBe("outcome_unconfirmed");
    expect(summary.nextStepHint).not.toMatch(/submit that claim first/i);
    expect(summary.nextStepHint).toMatch(/original|authoritative/i);
    state.snapshot.player_gameplay.recent_feedback = state.lastGameplayActionFeedback;
    state.lastGameplayActionFeedback = null;
    const fromRuntime = createFeedbackModule(state).buildGameplaySummary();
    expect(fromRuntime.executionState).toBe(stage);
    expect(fromRuntime.nextStepHint).toMatch(/original|authoritative/i);
  });

  it("does not offer a second first-Agent claim while the bound owner projection catches up", () => {
    const state = { uiLocale: "en", auth: { available: true, registrationStatus: "registered", boundAgentId: "a" },
      viewerProtocol: { capabilities: ["canonical_agent_chat_v1"] }, snapshot: { model: { agents: {}, locations: {} },
        player_gameplay: { available_actions: [{ action_id: "claim_first_agent", execute_kind: "claim_first_agent", label: "Claim First Agent" }] } } };
    const summary = createFeedbackModule(state).buildGameplaySummary();
    expect(summary.availableActions.some(action => action.actionId === "claim_first_agent")).toBe(false);
    expect(summary.blockerDetail).toMatch(/sync/i);
    expect(summary.nextStepHint).not.toMatch(/submit that claim first/i);
  });
});


describe("canonical capability recovery", () => {
  it("keeps canonical status and reasons invariant across locale and stale flags", () => {
    const gameplay = {
      repair_available: true,
      gameplay_capability: {recovery: [
        {kind: "repair", status: "unknown", reason_code: "unsupported_recovery_action", action: null},
        {kind: "current_recipe_run", status: "blocked", reason_code: "canonical_recipe_admission_rejected", action: {ScheduleRecipe: {}}},
      ]},
    };
    const summary = (locale) => createFeedbackModule({
      uiLocale: locale, snapshot: {model:{agents:{},locations:{}}, player_gameplay:gameplay},
    }).buildGameplaySummary().matureWorldContinuation.recoveryOptions;
    expect(summary("en")).toBe(summary("zh"));
    expect(summary("en")).toContain("Repair: unknown");
    expect(summary("en")).toContain("Run the current recipe: blocked");
    expect(summary("en")).not.toContain("Repair: available");
  });
});
