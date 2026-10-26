import assert from "node:assert/strict";
import {
  buildTaskGame076ScenarioSnapshot,
  TASK_GAME_076_BEATS,
} from "../software_safe_src/gameplay_attraction_scenario.js";
import {
  HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
} from "../software_safe_src/software_safe_constants.js";

globalThis.__OASIS7_VISUAL_TEST__ = true;
globalThis.window = {
  location: { search: "?test_api=1", href: "http://127.0.0.1:4173/software_safe.html?ws=ws://127.0.0.1:5011&test_api=1", pathname: "/software_safe.html" },
  history: { replaceState() {} },
  localStorage: { getItem() { return null; }, setItem() {}, removeItem() {} },
  addEventListener() {},
};
globalThis.document = {
  querySelectorAll() { return [{ textContent: JSON.stringify({ deploymentMode: "hosted_public_join", viewerWsEndpoint: "ws://127.0.0.1:5011" }) }]; },
  documentElement: { lang: "en" },
  createElement() {
    return {
      getContext() {
        return null;
      },
    };
  },
};

const core = await import("../software_safe_src/legacy_core.js");

function buildBoundTaskGame076ScenarioSnapshot(options) {
  const snapshot = buildTaskGame076ScenarioSnapshot(options);
  core.state.auth = {
    ...core.state.auth,
    available: true,
    playerId: "contract-player",
    source: "hosted_browser_storage",
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: "agent-0",
    error: null,
  };
  snapshot.model = snapshot.model || {};
  snapshot.model.agent_player_bindings = {
    ...(snapshot.model.agent_player_bindings || {}),
    "agent-0": "contract-player",
  };
  return snapshot;
}

{
  const display = core.describeSemanticFeedback({
    kind: "chat",
    stage: "error",
    effect: "llm_init_failed",
    reason: "llm init failed for agent-0: llm config error: missing env variable: OASIS7_LLM_MODEL",
    response: {
      code: "llm_init_failed",
      message: "llm init failed for agent-0: llm config error: missing env variable: OASIS7_LLM_MODEL",
    },
  });
  assert.equal(display.label, "LLM unavailable");
  assert.match(display.summary, /no usable LLM configuration/i);
  assert.match(display.detail, /config\.toml|OASIS7_LLM_/);
}

{
  const display = core.describeSemanticFeedback({
    kind: "chat",
    stage: "error",
    effect: "llm_init_failed",
    reason: "llm init failed",
    response: {
      code: "llm_init_failed",
      message: "llm init failed",
    },
  }, "zh");
  assert.equal(display.label, "LLM 不可用");
  assert.match(display.summary, /没有可用的 LLM 配置/);
}

{
  const display = core.describeSemanticFeedback({
    kind: "prompt",
    stage: "rollback_ack",
    effect: "prompt rolled back via version=4 → target=2",
    response: {
      version: 4,
      rolled_back_to_version: 2,
      code: "should_not_surface_for_ack",
    },
  });
  assert.equal(display.code, null);
}

{
  core.state.promptDraft.currentVersion = 2;
  core.state.promptDraft.rollbackTargetVersion = 1;
  const versionState = core.describePromptVersionState({
    stage: "rollback_ack",
    response: {
      version: 2,
      rolled_back_to_version: 0,
    },
  });
  assert.equal(versionState.currentVersion, 2);
  assert.equal(versionState.restoredFromVersion, 0);
  assert.equal(versionState.nextRollbackTargetVersion, 1);
  assert.match(versionState.detail, /next target v1/i);
}

{
  core.state.snapshot = buildBoundTaskGame076ScenarioSnapshot();
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.stageId, "post_onboarding");
  assert.equal(gameplaySummary.stageStatus, "blocked");
  assert.equal(gameplaySummary.executionState, "blocked");
  assert.equal(gameplaySummary.executionCauseKind, "world_constraint");
  assert.match(gameplaySummary.executionCauseLabel, /World Constraint/i);
  assert.equal(gameplaySummary.progressPercent, 68);
  assert.deepEqual(
    gameplaySummary.availableActions.map((action) => action.actionId),
    ["build_factory_smelter_mk1", "advance_step", "request_snapshot"],
  );
  assert.match(gameplaySummary.economicSurface.input, /material chain/i);
  assert.match(gameplaySummary.economicSurface.output, /smelter build request reached factory-0/i);
  assert.match(gameplaySummary.economicSurface.repairAction, /Advance 1 step/i);
  assert.match(gameplaySummary.economicSurface.nextValue, /stalled line becomes an operable capability again/i);
  assert.match(gameplaySummary.assetGovernanceHandoff, /no main token transfer form/i);
  assert.equal(gameplaySummary.controlProof.intent, "Queue build_factory_smelter_mk1 for agent-0");
  assert.match(gameplaySummary.controlProof.consequence, /World Constraint/i);
  assert.match(gameplaySummary.controlProof.recovery, /Advance 1 step/i);
  assert.match(gameplaySummary.controlProof.nextMove, /Replenish upstream materials/i);
  assert.match(gameplaySummary.controlProof.summary, /Detailed causal receipt unavailable/i);
  assert.equal(gameplaySummary.agencyMoves.interrupt, "available");
  assert.equal(gameplaySummary.agencyMoves.reprioritize, "available");
  assert.match(gameplaySummary.agencyMoves.correction, /material recovery/i);
  assert.match(gameplaySummary.progressionProof.firstWinGoal, /small_player\.first_industrial_win/i);
  assert.match(gameplaySummary.progressionProof.antiGrind, /repair_elasticity/i);
  assert.match(gameplaySummary.attractionProof.summary, /first 10\/30-minute attraction/i);
  assert.equal(gameplaySummary.attractionProof.verdict, "attraction_evidence_present");
  assert.match(gameplaySummary.attractionProof.whatICaused, /factory-0 exposed a recoverable iron shortage/i);
  assert.match(gameplaySummary.attractionProof.newOption, /repair_elasticity/i);
  assert.match(gameplaySummary.attractionProof.whyContinue, /Replenish upstream materials/i);
  assert.match(gameplaySummary.attractionProof.waitingCost, /iron input exhausted/i);
  assert.match(gameplaySummary.attractionProof.recovery, /replenish iron/i);
  assert.match(gameplaySummary.matureWorldContinuation.dependencyStatus, /independent_path_available/i);
  assert.match(gameplaySummary.matureWorldContinuation.recoveryOptions, /Recovery feasibility unknown/i);
  assert.deepEqual(gameplaySummary.matureWorldContinuation.recoveryAssessments, []);
  assert.match(gameplaySummary.shareReplay.snippet, /queued build_factory_smelter_mk1/i);
  assert.equal(core.getState().gameplaySummary.goalTitle, "Recover sustainable capability");
  assert.equal(TASK_GAME_076_BEATS.length, 10);
}

{
  core.state.snapshot = buildBoundTaskGame076ScenarioSnapshot({ variant: "weak_blocked" });
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.agencyMoves.interrupt, "unavailable");
  assert.equal(gameplaySummary.agencyMoves.reprioritize, "unavailable");
  assert.equal(gameplaySummary.agencyMoves.correction, null);
  assert.equal(gameplaySummary.agencyMoves.handoff, null);
  assert.equal(gameplaySummary.progressionProof.firstWinGoal, null);
  assert.equal(gameplaySummary.progressionProof.playerAction, null);
  assert.equal(gameplaySummary.progressionProof.worldChange, null);
  assert.match(gameplaySummary.matureWorldContinuation.recoveryOptions, /Recovery feasibility unknown/i);
  assert.deepEqual(gameplaySummary.matureWorldContinuation.recoveryAssessments, []);
  assert.equal(gameplaySummary.shareReplay.playerIntent, null);
  assert.equal(gameplaySummary.shareReplay.worldResult, null);
  assert.equal(gameplaySummary.shareReplay.snippet, null);
  assert.match(gameplaySummary.controlProof.nextMove, /Replenish upstream materials/i);
  assert.equal(gameplaySummary.attractionProof.verdict, "progression_pass_but_attraction_weak");
  assert.match(gameplaySummary.attractionProof.whatICaused, /waiting for player-caused world change/i);
  assert.match(gameplaySummary.attractionProof.newOption, /waiting for new option/i);
  assert.match(gameplaySummary.attractionProof.whyContinue, /Replenish upstream materials/i);
  assert.match(gameplaySummary.attractionProof.waitingCost, /iron input exhausted/i);
  assert.match(gameplaySummary.attractionProof.recovery, /waiting for recovery path/i);
}

{
  core.state.snapshot = {
    model: {
      agents: { "agent-0": { id: "agent-0" } },
      locations: { "loc-0": { id: "loc-0" } },
    },
    player_gameplay: {
      stage_id: "post_onboarding",
      stage_status: "blocked",
      execution_state: "blocked",
      goal_title: "Recover sustainable capability",
      player_action: "queued build_factory_smelter_mk1",
      causality_detail: "iron input exhausted at factory-0",
      last_world_change: "storm damaged an unrelated storage yard",
    },
  };
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.progressionProof.worldChange, null);
  assert.equal(gameplaySummary.shareReplay.playerIntent, "queued build_factory_smelter_mk1");
  assert.equal(gameplaySummary.shareReplay.worldResult, null);
  assert.equal(gameplaySummary.shareReplay.snippet, null);
}

{
  core.state.snapshot = buildBoundTaskGame076ScenarioSnapshot({ variant: "weak_high_progress" });
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.progressPercent, 92);
  assert.equal(gameplaySummary.attractionProof.verdict, "progression_pass_but_attraction_weak");
  assert.match(gameplaySummary.attractionProof.summary, /progression can pass while attraction is weak/i);
  assert.match(gameplaySummary.attractionProof.whatICaused, /waiting for player-caused world change/i);
  assert.match(gameplaySummary.attractionProof.newOption, /waiting for new option/i);
  assert.match(gameplaySummary.attractionProof.whyContinue, /waiting for next branch/i);
  assert.match(gameplaySummary.attractionProof.waitingCost, /repeat=4.*grind_only/i);
  assert.match(gameplaySummary.attractionProof.recovery, /waiting for recovery path/i);
  core.state.snapshot.player_gameplay.stage_status = "blocked";
  core.state.snapshot.player_gameplay.execution_state = "blocked";
}

{
  const gameplaySummary = core.buildGameplaySummary("zh");
  assert.match(gameplaySummary.assetGovernanceHandoff, /资产 \/ 治理动作/);
  assert.equal(gameplaySummary.executionStateLabel, "已阻塞");
}

{
  core.state.hostedAccess = {
    deployment_mode: HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
    action_matrix: [
      {
        action_id: "prompt_control_apply",
        required_auth: "strong_auth",
        availability: "public_player_plane_with_backend_reauth_preview",
        reason: "backend re-authorization preview is available for prompt control on this hosted lane",
      },
      {
        action_id: "main_token_transfer",
        required_auth: "strong_auth",
        availability: "blocked_until_strong_auth",
        reason: "main token transfer still requires a stronger hosted custody lane",
      },
    ],
  };
  core.state.auth = {
    ...core.state.auth,
    available: true,
    playerId: "player-123",
    publicKey: "abcd1234efgh5678",
    privateKey: "priv",
    releaseToken: "release-1",
    source: "hosted_browser_storage",
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: "agent-0",
    error: null,
  };
  const authSurface = core.buildAuthSurfaceModel();
  assert.equal(authSurface.tiers[1].status, "active_hosted_session");
  assert.equal(authSurface.tiers[2].status, "preview_backend_reauth_available");
  assert.match(authSurface.tiers[2].reason, /device-session-backed player_session/i);
  assert.match(authSurface.reconnect, /mint a fresh browser session key/i);
  assert.equal(authSurface.capabilities.prompt_control.enabled, true);
  assert.equal(authSurface.capabilities.main_token_transfer.enabled, false);
  assert.equal(authSurface.capabilities.main_token_transfer.code, "strong_auth_required");
  assert.match(authSurface.capabilities.main_token_transfer.reason, /stronger hosted custody lane/i);
}

{
  core.state.hostedAccess = {
    deployment_mode: HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
    action_matrix: [],
  };
  core.state.auth = {
    ...core.state.auth,
    available: true,
    playerId: "player-legacy",
    publicKey: "legacy1234efgh5678",
    privateKey: "priv-legacy",
    releaseToken: "release-legacy",
    source: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: "agent-0",
    error: null,
  };
  const authSurface = core.buildAuthSurfaceModel();
  assert.equal(authSurface.deploymentHint, "hosted_public_join_contract_with_legacy_bootstrap");
  assert.equal(authSurface.capabilities.main_token_transfer.enabled, false);
  assert.equal(authSurface.capabilities.main_token_transfer.code, "strong_auth_required");
  assert.match(authSurface.capabilities.main_token_transfer.reason, /legacy preview player_session/i);
}

{
  core.state.snapshot.player_gameplay.execution_state = "completed";
  core.state.snapshot.player_gameplay.causality_kind = "agent_override";
  core.state.snapshot.player_gameplay.causality_detail = "rule module policy.guard redirected the accepted action before execution";
  core.state.snapshot.player_gameplay.recent_feedback = {
    action: "step",
    stage: "completed_advanced",
    effect: "world advanced: logicalTime +1, eventSeq +2",
    reason: null,
    hint: null,
    delta_logical_time: 1,
    delta_event_seq: 2,
  };
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.executionState, "completed");
  assert.equal(gameplaySummary.executionCauseKind, "agent_override");
  assert.match(gameplaySummary.executionCauseLabel, /Agent Chose Differently/i);
  assert.match(gameplaySummary.executionCauseDetail, /policy\.guard/i);
  assert.match(gameplaySummary.controlProof.consequence, /Agent Chose Differently/i);
  assert.match(gameplaySummary.controlProof.summary, /Detailed causal receipt unavailable/i);
  assert.doesNotMatch(gameplaySummary.controlProof.summary, /Control proved/i);
}

{
  core.state.snapshot.model.agents = {};
  core.state.snapshot.model.locations = {};
  core.state.snapshot.player_gameplay.execution_state = "completed";
  core.state.snapshot.player_gameplay.stage_status = "active";
  core.state.snapshot.player_gameplay.blocker_kind = null;
  core.state.snapshot.player_gameplay.blocker_detail = null;
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.stageStatus, "blocked");
  assert.equal(gameplaySummary.executionState, "blocked");
  assert.equal(gameplaySummary.executionCauseKind, "world_constraint");
  assert.equal(gameplaySummary.blockerKind, "runtime_snapshot_empty_entities");
}

{
  const injectedState = core.injectSnapshot({
    time: 12,
    config: {
      space: {
        width_cm: 10_000_000,
        depth_cm: 5_000_000,
        height_cm: 1_000_000,
      },
    },
    model: {
      agents: { "agent-0": { id: "agent-0", location_id: "loc-0", resources: {} } },
      locations: {
        "loc-0": {
          id: "loc-0",
          name: "Loc 0",
          pos: { x_cm: 0, y_cm: 0, z_cm: 0 },
          profile: { radius_cm: 25_000, radiation_emission_per_tick: 0, material: "silicate" },
          resources: {},
        },
        "loc-1": {
          id: "loc-1",
          name: "Loc 1",
          pos: { x_cm: 100_000, y_cm: 0, z_cm: 0 },
          profile: { radius_cm: 10_000, radiation_emission_per_tick: 0, material: "silicate" },
          resources: {},
        },
        "loc-bad": {
          id: "loc-bad",
          name: "Broken Location",
          pos: { x_cm: null, y_cm: 0, z_cm: 0 },
          profile: { radius_cm: 5_000, radiation_emission_per_tick: 0, material: "silicate" },
          resources: {},
        },
      },
    },
    player_gameplay: {
      stage_id: "first_session_loop",
      stage_status: "active",
      goal_id: "first_session_loop.create_first_world_feedback",
      goal_kind: "CreateFirstWorldFeedback",
      goal_title: "Claim your first agent slot",
      objective: "Select a target and submit your first slot-1 claim.",
      progress_detail: "The dedicated starter pool can auto-fund the first slot-1 claim when you confirm it.",
      progress_percent: 24,
      blocker_kind: null,
      blocker_detail: null,
      next_step_hint: "Pick an unclaimed target, review the canonical quote, then confirm ClaimAgent.",
      branch_hint: null,
      available_actions: [],
      recent_feedback: null,
      agent_claim: {
        claimer_agent_id: "agent-0",
        current_epoch: 3,
        reputation_tier: 0,
        claim_cap: 1,
        owned_claim_count: 0,
        liquid_main_token_balance: 0,
        restricted_starter_claim_balance: 0,
        slot_1_auto_restricted_starter_claim_amount: 325,
        slot_1_eligible_claim_balance: 325,
        next_claim_quote: {
          slot_index: 1,
          reputation_tier: 0,
          claim_cap: 1,
          owned_claim_count: 0,
          activation_fee_amount: 100,
          claim_bond_amount: 200,
          upkeep_per_epoch: 25,
          total_upfront_amount: 325,
          transferable_liquid_balance: 0,
          restricted_starter_claim_balance: 0,
          auto_restricted_starter_claim_amount: 325,
          eligible_claim_balance: 325,
          release_cooldown_epochs: 3,
          grace_epochs: 2,
          idle_warning_epochs: 7,
          forced_idle_reclaim_epochs: 10,
          forced_reclaim_penalty_bps: 2000,
          blocked_reason: null,
        },
        owned_claims: [],
      },
    },
  });
  assert.equal(injectedState.logicalTime, 12);
  assert.equal(injectedState.gameplaySummary.agentClaim.next_claim_quote.auto_restricted_starter_claim_amount, 325);
  assert.equal(injectedState.gameplaySummary.agentClaim.slot_1_auto_restricted_starter_claim_amount, 325);
  core.select({ kind: "location", id: "loc-0" });
  const worldScale = core.buildWorldScaleSurface();
  assert.equal(worldScale.physicalTruth.canonicalUnitLabel, "1 cm");
  assert.match(worldScale.physicalTruth.worldBoundsLabel, /100 km × 50 km × 10 km/);
  assert.equal(worldScale.physicalTruth.anchor.id, "loc-0");
  assert.equal(worldScale.physicalTruth.anchor.radiusLabel, "250 m");
  assert.equal(worldScale.physicalTruth.nearestLocations[0].id, "loc-1");
  assert.equal(worldScale.physicalTruth.nearestLocations[0].distanceLabel, "1 km");
  assert.equal(
    worldScale.physicalTruth.nearestLocations.some((location) => location.id === "loc-bad"),
    false,
  );
  assert.match(worldScale.presentationScale.markerTruthNote, /do not read on-screen diameter/i);
  assert.match(worldScale.presentationScale.zoomTruthNote, /zoom tiers/i);
}

{
  assert.equal(core.setSoftwareSafeLocale("zh"), "zh");
  assert.equal(core.state.uiLocale, "zh");
  assert.equal(globalThis.document.documentElement.lang, "zh-CN");
}

{
  core.state.snapshot = {
    model: {
      agents: { "agent-0": { id: "agent-0" } },
      locations: { "loc-0": { id: "loc-0" } },
    },
    player_gameplay: {
      next_step_hint: "Enable LLM access before retrying world controls.",
      available_actions: [
        {
          action_id: "advance_step",
          label: "Advance 1 step",
          protocol_action: "live_control.step",
          target_agent_id: null,
          disabled_reason: "missing env variable: OASIS7_LLM_MODEL",
        },
        {
          action_id: "resume_play",
          label: "Resume live play",
          protocol_action: "live_control.play",
          target_agent_id: null,
          disabled_reason: "missing env variable: OASIS7_LLM_MODEL",
        },
        {
          action_id: "request_snapshot",
          label: "Request snapshot",
          protocol_action: "world.request_snapshot",
          target_agent_id: null,
          disabled_reason: null,
        },
      ],
    },
  };
  const gameplaySummary = core.buildGameplaySummary();
  assert.deepEqual(
    gameplaySummary.availableActions.map((action) => action.actionId),
    ["advance_step", "resume_play", "request_snapshot"],
  );
  core.state.controlProfile = "live";
  const feedback = core.sendControl("step");
  assert.equal(feedback.accepted, false);
  assert.equal(feedback.stage, "blocked");
  assert.equal(feedback.reason, "missing env variable: OASIS7_LLM_MODEL");
  assert.match(feedback.effect, /control blocked by gameplay gate/i);
  assert.equal(feedback.hint, "Enable LLM access before retrying world controls.");
}

{
  core.state.snapshot = {
    model: {
      agents: {},
      locations: {},
    },
    player_gameplay: {
      stage_id: "post_onboarding",
      stage_status: "active",
      goal_id: "post_onboarding.establish_first_capability",
      goal_kind: "EstablishFirstCapability",
      goal_title: "Establish your first sustainable capability",
      objective: "Do not stall after onboarding.",
      progress_detail: "Progress exists, but entities are missing from the viewer snapshot.",
      progress_percent: 20,
      blocker_kind: null,
      blocker_detail: null,
      next_step_hint: "this will be replaced by the empty-entity guard",
      branch_hint: null,
      available_actions: [
        {
          action_id: "advance_step",
          label: "Advance 1 step",
          protocol_action: "live_control.step",
          target_agent_id: null,
          disabled_reason: null,
        },
        {
          action_id: "request_snapshot",
          label: "Request snapshot",
          protocol_action: "request_snapshot",
          target_agent_id: null,
          disabled_reason: null,
        },
      ],
      recent_feedback: null,
      agent_claim: null,
    },
  };
  const gameplaySummary = core.buildGameplaySummary();
  assert.equal(gameplaySummary.stageStatus, "blocked");
  assert.equal(gameplaySummary.blockerKind, "runtime_snapshot_empty_entities");
  assert.match(gameplaySummary.blockerDetail, /no agents\/locations|没有 Agent \/ 地点/i);
  assert.match(gameplaySummary.nextStepHint, /fresh snapshot|刷新快照/i);
  assert.equal(gameplaySummary.availableActions[0].disabledReason !== null, true);
  assert.equal(gameplaySummary.availableActions[1].disabledReason, null);
}

{
  core.state.snapshot = {
    player_gameplay: {
      next_step_hint: "Recover the lane before retrying world controls.",
      available_actions: [],
    },
  };
  core.state.lastControlFeedback = {
    id: 41,
    action: "step",
    accepted: true,
    stage: "queued",
    reason: null,
    hint: null,
    effect: "queued",
    baselineLogicalTime: 7,
    baselineEventSeq: 3,
    deltaLogicalTime: 0,
    deltaEventSeq: 0,
    requestId: 41,
  };
  core.handleControlCompletionAck({
    request_id: 41,
    status: "blocked",
    delta_logical_time: 0,
    delta_event_seq: 0,
    error_code: "llm_init_failed",
    error_message: "gameplay requires a configured and reachable LLM provider",
  });
  // A manually-created last feedback is not an outstanding request. An
  // unsolicited completion must not adopt status, errors, or world deltas.
  assert.equal(core.state.lastControlFeedback.stage, "queued");
  assert.equal(core.state.lastControlFeedback.reason, null);
  assert.equal(core.state.lastControlFeedback.hint, null);
  assert.equal(core.state.lastControlFeedback.effect, "queued");
}

{
  core.state.lastControlFeedback = {
    id: 42,
    action: "step",
    accepted: true,
    stage: "queued",
    reason: null,
    hint: null,
    effect: "queued",
    baselineLogicalTime: 7,
    baselineEventSeq: 3,
    deltaLogicalTime: 0,
    deltaEventSeq: 0,
    requestId: 42,
  };
  core.handleControlCompletionAck({
    request_id: 42,
    status: "timeout_no_progress",
    delta_logical_time: 0,
    delta_event_seq: 0,
  });
  assert.equal(core.state.lastControlFeedback.stage, "queued");
  assert.equal(core.state.lastControlFeedback.reason, null);
  assert.equal(core.state.lastControlFeedback.effect, "queued");
}

{
  const snapshotWithProfiles = (version, agent0Goal) => ({
    time: version,
    model: {
      agents: {
        "agent-0": { id: "agent-0", location_id: "loc-0", resources: { energy: version } },
        "agent-1": { id: "agent-1", location_id: "loc-0", resources: {} },
      },
      locations: { "loc-0": { id: "loc-0", name: "Loc 0", resources: {} } },
      agent_prompt_profiles: {
        "agent-0": {
          agent_id: "agent-0",
          version,
          system_prompt_override: `system-${version}`,
          short_term_goal_override: agent0Goal,
          long_term_goal_override: `long-${version}`,
        },
        "agent-1": {
          agent_id: "agent-1",
          version: 3,
          system_prompt_override: "agent-1-system",
          short_term_goal_override: "agent-1-goal",
          long_term_goal_override: "agent-1-long",
        },
      },
    },
  });

  core.injectSnapshot(snapshotWithProfiles(1, "authoritative-v1"));
  core.applySelection({ kind: "agent", id: "agent-0" });
  core.state.promptDraft.shortTermGoal = "dirty-player-goal";
  core.state.promptDraft.dirty = true;
  core.state.chatDraft.message = "dirty player chat";
  core.state.chatDraft.dirty = true;

  core.injectSnapshot(snapshotWithProfiles(2, "authoritative-v2"));
  assert.equal(core.state.selectedObject.resources.energy, 2, "same-selection snapshot still refreshes selected object");
  assert.equal(core.state.promptDraft.shortTermGoal, "dirty-player-goal", "same-selection snapshot preserves dirty prompt draft");
  assert.equal(core.state.promptDraft.dirty, true);
  assert.equal(core.state.chatDraft.message, "dirty player chat", "same-selection snapshot preserves dirty chat draft");
  assert.equal(core.state.chatDraft.dirty, true);

  core.applySelection({ kind: "agent", id: "agent-1" });
  assert.equal(core.state.promptDraft.shortTermGoal, "agent-1-goal", "explicit selection resets prompt draft to selected agent authority");
  assert.equal(core.state.promptDraft.dirty, false);
  assert.equal(core.state.chatDraft.message, "", "explicit selection resets chat draft for a different agent");
  assert.equal(core.state.chatDraft.dirty, false);

  core.applySelection({ kind: "agent", id: "agent-0" });
  core.state.promptDraft.dirty = false;
  core.injectSnapshot(snapshotWithProfiles(4, "authoritative-v4"));
  assert.equal(core.state.promptDraft.shortTermGoal, "authoritative-v4", "clean draft still reconciles from authoritative snapshot");
  assert.equal(core.state.promptDraft.currentVersion, 4);
}

// Exercise the real legacy_core summary entry point. Presentation-era flags
// cannot override a canonical assessment, including when locale/text changes.
for (const status of ["available", "blocked"]) {
  const reasonCode = status === "available"
    ? "canonical_recipe_quote_available"
    : "canonical_recipe_admission_rejected";
  const recovery = [
    {kind: "current_recipe_run", status, reason_code: reasonCode,
      action: {ScheduleRecipe: {factory_id: "factory-0", recipe_id: "recipe.smelter.iron_ingot"}}},
    ...["repair", "rebuild", "pivot"].map(kind => ({kind, status: "unknown",
      reason_code: "unsupported_recovery_action", action: null})),
  ];
  const expected = recovery.map(({kind, status, reason_code}) => ({kind, status, reasonCode: reason_code}));
  for (const locale of ["en", "zh"]) {
    core.state.uiLocale = locale;
    core.state.snapshot = buildBoundTaskGame076ScenarioSnapshot({overrides: {player_gameplay: {
      repair_available: true, rebuild_available: true, pivot_available: true,
      blocker_detail: locale === "en" ? "only a sponsor must repair this" : "当前恢复证据",
      gameplay_capability: {goal_id: "current_industrial_capability", recovery},
    }}});
    const continuation = core.buildGameplaySummary().matureWorldContinuation;
    assert.deepEqual(continuation.recoveryAssessments, expected);
    assert.match(continuation.recoveryOptions, new RegExp(`Run the current recipe: ${status}`));
    assert.match(continuation.recoveryOptions, /Repair: unknown/);
    assert.match(continuation.recoveryOptions, /Rebuild: unknown/);
    assert.match(continuation.recoveryOptions, /Pivot: unknown/);
    assert.doesNotMatch(continuation.recoveryOptions, /Repair: available/);
    assert.deepEqual(continuation.recoveryOptionComparisons, []);
  }
}
core.state.uiLocale = "en";

console.log("software-safe feedback contract tests passed");
