import { render } from "@solidjs/testing-library";
import { afterEach, beforeEach, vi } from "vitest";

const runtimeMock = vi.hoisted(() => ({
  deriveRenderState: null,
  mountError: null,
  mountGates: [],
  mountResults: [],
  mountCalls: 0,
  updateCalls: 0,
  onEvent: null,
}));
vi.mock("./pixel_world_runtime_loader.js", async () => ({
  ...(await vi.importActual("./pixel_world_runtime_loader.js")),
  createPixelWorldRuntimeBridge: async ({ onEvent, onFatal }) => {
    runtimeMock.onEvent = onEvent;
    return {
      source: runtimeMock.deriveRenderState ? "test_rust_runtime" : "wasm_import_failed",
      moduleUrl: "http://127.0.0.1:4173/pixel-world-bridge/pixel_world_bridge.js",
      deriveRenderState: runtimeMock.deriveRenderState,
      bridge: {
        mount() {
          runtimeMock.mountCalls += 1;
          if (runtimeMock.mountError) {
            throw runtimeMock.mountError;
          }
          if (runtimeMock.mountGates.length) {
            const gate = runtimeMock.mountGates.shift();
            return gate.then(() => runtimeMock.mountResults.shift() || { status: "ready", fatal: null });
          }
          if (runtimeMock.deriveRenderState) {
            return {
              status: "ready",
              fatal: null,
            };
          }
          const fatal = {
            code: "pixel_world_renderer_runtime_unavailable",
            message: "pixel world wasm runtime is unavailable: missing wasm bridge",
          };
          onFatal?.(fatal);
          return {
            status: "fallback",
            fatal,
          };
        },
        update() {
          runtimeMock.updateCalls += 1;
          if (runtimeMock.deriveRenderState) {
            return {
              status: "ready",
              fatal: null,
            };
          }
          return {
            status: "fallback",
          };
        },
        unmount() {
          return {
            status: "detached",
          };
        },
      },
    };
  },
}));
let activeCleanup = null;
let canvasContextSpy = null;
const testHarness = {
  get activeCleanup() { return activeCleanup; },
  set activeCleanup(value) { activeCleanup = value; },
  get canvasContextSpy() { return canvasContextSpy; },
  set canvasContextSpy(value) { canvasContextSpy = value; },
};
const HEAVY_UI_TEST_TIMEOUT_MS = 60000;
function clone(value) {
  return JSON.parse(JSON.stringify(value));
}
function fieldValue(source, snake, camel, fallback = null) {
  if (!source || typeof source !== "object") {
    return fallback;
  }
  if (source[snake] != null) {
    return source[snake];
  }
  if (source[camel] != null) {
    return source[camel];
  }
  return fallback;
}
function dominantCompound(block) {
  const ppm = block?.compounds?.ppm || {};
  const entries = Object.entries(ppm);
  if (!entries.length) {
    return "unknown";
  }
  return entries.sort((left, right) => Number(right[1] || 0) - Number(left[1] || 0))[0][0];
}
function locationPos(location) {
  return location?.pos || { x_cm: 0, y_cm: 0, z_cm: 0 };
}
function buildTestRustRenderState(input) {
  const snapshot = input.snapshot || {};
  const snake = value => Object.fromEntries(Object.entries(value || {}).map(([key, item]) => [key.replace(/[A-Z]/g, char => `_${char.toLowerCase()}`), item]));
  const gameplay = snake(input.gameplay);
  gameplay.intent_target = gameplay.accepted_intent_target;
  gameplay.intent_summary = gameplay.accepted_intent_summary;
  gameplay.stage_status = gameplay.execution_state;
  gameplay.causality_kind = gameplay.execution_cause_kind;
  gameplay.available_actions = (gameplay.available_actions || []).map(snake);
  gameplay.recent_feedback = gameplay.recent_feedback ? snake(gameplay.recent_feedback) : null;
  const locations = input.lists?.locations || [];
  const agents = input.lists?.agents || [];
  const model = { locations: Object.fromEntries(locations.map(item => [item.id, item])) };
  const fragments = locations.flatMap((location) => {
    const base = locationPos(location);
    return (location.fragment_profile?.blocks?.blocks || []).map((block, index) => ({
      id: `fragment:${location.id}:${index}`,
      locationId: location.id,
      pos: {
        x_cm: base.x_cm + Number(block.origin_cm?.x_cm || 0),
        y_cm: base.y_cm + Number(block.origin_cm?.z_cm || block.origin_cm?.y_cm || 0),
        z_cm: base.z_cm + Number(block.origin_cm?.y_cm || 0),
      },
      dominantCompound: dominantCompound(block),
      footprintCm: Math.max(Number(block.size_cm?.x_cm || 12_000), Number(block.size_cm?.z_cm || block.size_cm?.y_cm || 12_000)),
    }));
  });
  const firstAction = (gameplay.available_actions || [])[0] || {};
  const activeAgentId = gameplay.intent_target || agents[0]?.id || null;
  const receiptPresent = Boolean(gameplay.recent_feedback || gameplay.last_world_change);
  const blockerLabel = gameplay.blocker_kind === "material_shortage" ? "Missing Material" : gameplay.blocker_kind || null;
  const renderState = {
    locale: input.locale || "en",
    worldBounds: snapshot.config?.space || { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 },
    world_bounds: snapshot.config?.space || { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 },
    locations: locations.map((location) => ({
      id: location.id,
      label: location.name || location.id,
      pos: locationPos(location),
      markerRole: "logic_anchor",
      markerAlpha: 0.32,
    })),
    fragmentTerrain: fragments,
    fragment_terrain: fragments,
    agents: agents.map((agent, index) => {
      const base = locationPos(model.locations?.[agent.location_id]);
      return {
        id: agent.id,
        label: agent.name || agent.id,
        pos: agent.pos || {
          x_cm: base.x_cm + 20_000 + index * 15_000,
          y_cm: base.y_cm + 10_000 + index * 12_000,
          z_cm: base.z_cm,
        },
        positionSource: agent.pos ? "runtime_agent" : "location_derived",
      };
    }),
    links: agents
      .filter((agent) => agent.location_id && model.locations?.[agent.location_id])
      .map((agent) => {
        const from = agent.pos || {
          x_cm: locationPos(model.locations[agent.location_id]).x_cm + 20_000,
          y_cm: locationPos(model.locations[agent.location_id]).y_cm + 10_000,
          z_cm: 0,
        };
        return {
          id: `link:${agent.id}:${agent.location_id}`,
          kind: "agent_assignment",
          from,
          to: locationPos(model.locations[agent.location_id]),
          emphasis: 0.72,
        };
      }),
    selection: activeAgentId ? { kind: "agent", id: activeAgentId } : null,
    goalHighlight: {
      title: gameplay.goal_title || "Current Objective",
      objective: gameplay.objective || gameplay.progress_detail || "",
    },
    blockerHighlight: blockerLabel
      ? { kind: gameplay.blocker_kind, label: blockerLabel, detail: gameplay.blocker_detail || null }
      : null,
    recentEventHotspots: [],
    visualHotspots: [],
    commercial_surface: {
      objective: {
        title: gameplay.goal_title || "Current Objective",
        detail: gameplay.objective || gameplay.progress_detail || "No current objective.",
        progress_percent: gameplay.progress_percent ?? null,
      },
      next_action: {
        label: fieldValue(firstAction, "label", "label", "Unassigned"),
        detail: gameplay.intent_summary || null,
        target_agent_id: fieldValue(firstAction, "target_agent_id", "targetAgentId", activeAgentId),
        execute_kind: fieldValue(firstAction, "execute_kind", "executeKind", "gameplay_action"),
      },
      active_agent_id: activeAgentId,
      player_leverage: {
        state: gameplay.stage_status || "waiting_for_intent",
        label: receiptPresent ? "Blocked" : "Waiting for Intent",
        summary: gameplay.progress_detail || "Waiting",
        detail: gameplay.next_step_hint || null,
      },
      action_receipt: {
        present: receiptPresent,
        state: receiptPresent ? "blocked" : "waiting_for_intent",
        confidence: receiptPresent ? "world_delta" : "none",
        title: receiptPresent ? "Action blocked" : "No action receipt yet",
        summary: receiptPresent ? "Action blocked" : "No receipt",
        detail: gameplay.last_world_change || gameplay.recent_feedback?.effect || "No player-caused world change has been confirmed yet.",
        target_agent_id: receiptPresent ? activeAgentId : null,
        effect_kind: gameplay.causality_kind || null,
        delta_logical_time: gameplay.recent_feedback?.delta_logical_time ?? null,
        delta_event_seq: gameplay.recent_feedback?.delta_event_seq ?? null,
      },
      blocker: {
        label: blockerLabel,
        detail: gameplay.next_step_hint || gameplay.blocker_detail || null,
      },
      world_read: {
        agents: agents.length,
        routes: agents.filter((agent) => agent.location_id).length,
        fragments: fragments.length,
        hotspots: 0,
      },
    },
    presentation: input.presentation || { world_bounds_label: "bounds", marker_truth_note: "truth" },
  };
  return renderState;
}
function useTestRustRenderState() {
  runtimeMock.deriveRenderState = vi.fn((input) => buildTestRustRenderState(input));
}
function sampleSnapshot() {
  return {
    time: 12,
    config: {
      space: {
        width_cm: 10_000_000,
        depth_cm: 5_000_000,
        height_cm: 1_000_000,
      },
    },
    model: {
      agents: {
        "agent-0": {
          id: "agent-0",
          name: "Agent 0",
          location_id: "loc-0",
          resources: {},
        },
      },
      locations: {
        "loc-0": {
          id: "loc-0",
          name: "Factory Anchor",
          pos: { x_cm: 5_000_000, y_cm: 2_500_000, z_cm: 0 },
          profile: { radius_cm: 25_000, radiation_emission_per_tick: 0, material: "silicate" },
          fragment_profile: {
            blocks: {
              blocks: [
                {
                  origin_cm: { x_cm: 0, y_cm: 0, z_cm: 0 },
                  size_cm: { x_cm: 12_000, y_cm: 7_500, z_cm: 8_000 },
                  density_kg_per_m3: 3200,
                  compounds: {
                    ppm: {
                      silicate_matrix: 800_000,
                      water_ice: 200_000,
                    },
                  },
                },
                {
                  origin_cm: { x_cm: 20_000, y_cm: 1_000, z_cm: 18_000 },
                  size_cm: { x_cm: 20_000, y_cm: 8_000, z_cm: 10_000 },
                  density_kg_per_m3: 7800,
                  compounds: {
                    ppm: {
                      iron_nickel_alloy: 900_000,
                      sulfide_ore: 100_000,
                    },
                  },
                },
              ],
            },
          },
          resources: {},
        },
      },
      agent_prompt_profiles: {},
      agent_execution_debug_contexts: {},
      agent_player_bindings: {
        "agent-0": "player-one",
      },
      agent_player_public_key_bindings: {
        "agent-0": "abcdef0123456789abcdef0123456789",
      },
    },
    player_gameplay: {
      stage_id: "post_onboarding",
      stage_status: "blocked",
      execution_state: "blocked",
      accepted_intent_id: "gameplay_action:build_factory_smelter_mk1",
      intent_summary: "Queue build_factory_smelter_mk1 for agent-0",
      intent_scope: "gameplay_action",
      intent_target: "agent-0",
      goal_id: "post_onboarding.recover_capability",
      goal_kind: "RecoverCapability",
      goal_title: "Recover sustainable capability",
      objective: "Stabilize the first production line before expanding.",
      progress_detail: "The primary line is blocked by missing material input.",
      progress_percent: 68,
      blocker_kind: "material_shortage",
      blocker_detail: "iron input exhausted at factory-0",
      causality_kind: "world_constraint",
      causality_detail: "iron input exhausted at factory-0",
      last_world_change: "Smelter build request reached factory-0; iron shortage blocks construction.",
      blocker_supplemental_detail: null,
      next_step_hint: "Replenish upstream materials, then advance again to confirm the line resumes.",
      branch_hint: null,
      available_actions: [
        {
          action_id: "build_factory_smelter_mk1",
          target_agent_id: "agent-0",
          label: "Build smelter mk1",
          protocol_action: "gameplay_action.submit",
          disabled_reason: null,
        },
      ],
      recent_feedback: {
        action: "build_factory_smelter_mk1",
        stage: "completed_no_progress",
        effect: "Smelter build request reached factory-0; iron shortage blocks construction.",
        reason: "iron input exhausted at factory-0",
        hint: "Replenish upstream materials, then advance again.",
        delta_logical_time: 1,
        delta_event_seq: 2,
      },
      agent_claim: null,
    },
  };
}
function acceptedOnlySnapshot() {
  const snapshot = clone(sampleSnapshot());
  const gameplay = snapshot.player_gameplay;
  gameplay.stage_status = "executing";
  gameplay.execution_state = "accepted";
  gameplay.blocker_kind = null;
  gameplay.blocker_detail = null;
  gameplay.causality_kind = null;
  gameplay.causality_detail = null;
  gameplay.last_world_change = null;
  gameplay.recent_feedback = {
    action: "build_factory_smelter_mk1",
    stage: "accepted",
    effect: null,
    reason: null,
    hint: "Build request queued for agent-0.",
    delta_logical_time: 0,
    delta_event_seq: 1,
  };
  return snapshot;
}
function noReceiptSnapshot() {
  const snapshot = clone(sampleSnapshot());
  const gameplay = snapshot.player_gameplay;
  gameplay.stage_status = "running";
  delete gameplay.execution_state;
  delete gameplay.accepted_intent_id;
  delete gameplay.intent_summary;
  delete gameplay.intent_scope;
  delete gameplay.intent_target;
  delete gameplay.blocker_kind;
  delete gameplay.blocker_detail;
  delete gameplay.causality_kind;
  delete gameplay.causality_detail;
  delete gameplay.last_world_change;
  gameplay.progress_detail = "The first production line is waiting for a player command.";
  gameplay.recent_feedback = null;
  return snapshot;
}
function emptyWorldSnapshot() {
  const snapshot = clone(noReceiptSnapshot());
  snapshot.model.agents = {};
  snapshot.model.locations = {};
  snapshot.model.agent_prompt_profiles = {};
  snapshot.model.agent_execution_debug_contexts = {};
  snapshot.model.agent_player_bindings = {};
  snapshot.model.agent_player_public_key_bindings = {};
  return snapshot;
}
function bindFirstSnapshotAgentForTest(core, snapshot) {
  const agentId = Object.keys(snapshot?.model?.agents || {})[0];
  const playerId = snapshot?.model?.agent_player_bindings?.[agentId];
  if (!agentId || !playerId) {
    return;
  }
  core.state.auth = {
    ...core.state.auth,
    available: true,
    playerId,
    publicKey: snapshot?.model?.agent_player_public_key_bindings?.[agentId] || "abcdef0123456789abcdef0123456789",
    privateKey: "private-key-must-stay-hidden",
    source: "local_test_api_ephemeral",
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: agentId,
  };
}
async function renderPixelWorldHost(snapshot = sampleSnapshot(), search = "?test_api=1&connect=0&locale=en", locale = "en", { injectionSearch = search, initializeCore = false, setupCore = null } = {}) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", `/software_safe.html${injectionSearch}`);
  window.localStorage.clear();
  document.body.innerHTML = "";
  const core = await import("./legacy_core.js");
  const { PixelWorldHost } = await import("./pixel_world_host.jsx");
  if (initializeCore) await core.initializeSoftwareSafeCore();
  core.setViewerLocale(locale);
  core.injectSnapshot(snapshot);
  bindFirstSnapshotAgentForTest(core, snapshot);
  setupCore?.(core);
  window.history.replaceState({}, "", `/software_safe.html${search}`);
  const view = render(() => <PixelWorldHost locale={locale} />);
  activeCleanup = view.unmount;
  return {
    core,
    ...view,
  };
}
function cleanupMountedHost() {
  activeCleanup?.();
  activeCleanup = null;
}
beforeEach(() => {
  runtimeMock.deriveRenderState = null;
  runtimeMock.mountError = null;
  runtimeMock.mountGates = [];
  runtimeMock.mountResults = [];
  runtimeMock.mountCalls = 0;
  runtimeMock.updateCalls = 0;
  runtimeMock.onEvent = null;
  canvasContextSpy = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({});
  window.history.replaceState({}, "", "/software_safe.html?test_api=1&connect=0&locale=en");
  window.localStorage.clear();
  document.body.removeAttribute("data-viewer-visual-fixture");
  document.body.innerHTML = "";
});
afterEach(() => {
  activeCleanup?.();
  activeCleanup = null;
  canvasContextSpy?.mockRestore();
  canvasContextSpy = null;
  document.body.removeAttribute("data-viewer-visual-fixture");
  document.body.innerHTML = "";
});

export {
  HEAVY_UI_TEST_TIMEOUT_MS,
  buildTestRustRenderState,
  cleanupMountedHost,
  clone,
  emptyWorldSnapshot,
  noReceiptSnapshot,
  acceptedOnlySnapshot,
  bindFirstSnapshotAgentForTest,
  renderPixelWorldHost,
  runtimeMock,
  sampleSnapshot,
  testHarness,
  useTestRustRenderState,
};
