import { render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const runtimeMock = vi.hoisted(() => ({
  deriveRenderState: null,
  mountError: null,
  mountGates: [],
  mountResults: [],
  mountCalls: 0,
}));

vi.mock("./pixel_world_runtime_loader.js", async () => ({
  ...(await vi.importActual("./pixel_world_runtime_loader.js")),
  createPixelWorldRuntimeBridge: async ({ onFatal }) => ({
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
          return { status: "ready", fatal: null };
        }
        const fatal = {
          code: "pixel_world_renderer_runtime_unavailable",
          message: "pixel world wasm runtime is unavailable: missing wasm bridge",
        };
        onFatal?.(fatal);
        return { status: "fallback", fatal };
      },
      update() {
        return runtimeMock.deriveRenderState ? { status: "ready", fatal: null } : { status: "fallback" };
      },
      unmount() {
        return { status: "detached" };
      },
    },
  }),
}));

let activeCleanup = null;
let canvasContextSpy = null;
const HEAVY_UI_TEST_TIMEOUT_MS = 60000;

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function sampleSnapshot() {
  return {
    time: 12,
    config: {
      space: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 },
    },
    model: {
      agents: {
        "agent-0": { id: "agent-0", name: "Agent 0", location_id: "loc-0", resources: {} },
      },
      locations: {
        "loc-0": {
          id: "loc-0",
          name: "Factory Anchor",
          pos: { x_cm: 5_000_000, y_cm: 2_500_000, z_cm: 0 },
          resources: {},
        },
      },
      agent_prompt_profiles: {},
      agent_execution_debug_contexts: {},
      agent_player_bindings: { "agent-0": "player-one" },
      agent_player_public_key_bindings: { "agent-0": "abcdef0123456789abcdef0123456789" },
    },
    player_gameplay: {
      stage_id: "post_onboarding",
      stage_status: "blocked",
      execution_state: "blocked",
      intent_target: "agent-0",
      goal_title: "Recover sustainable capability",
      objective: "Stabilize the first production line before expanding.",
      progress_detail: "The primary line is blocked by missing material input.",
      progress_percent: 68,
      blocker_kind: "material_shortage",
      blocker_detail: "iron input exhausted at factory-0",
      causality_kind: "world_constraint",
      last_world_change: "Smelter build request reached factory-0; iron shortage blocks construction.",
      available_actions: [{
        action_id: "build_factory_smelter_mk1",
        target_agent_id: "agent-0",
        label: "Build smelter mk1",
        protocol_action: "gameplay_action.submit",
        disabled_reason: null,
      }],
      recent_feedback: {
        action: "build_factory_smelter_mk1",
        stage: "completed_no_progress",
        effect: "Smelter build request reached factory-0; iron shortage blocks construction.",
        reason: "iron input exhausted at factory-0",
        hint: "Replenish upstream materials, then advance again.",
        delta_logical_time: 1,
        delta_event_seq: 2,
      },
    },
  };
}

function renderStateFor(input) {
  const snapshot = input.snapshot || {};
  const snake = value => Object.fromEntries(Object.entries(value || {}).map(([key, item]) => [key.replace(/[A-Z]/g, char => `_${char.toLowerCase()}`), item]));
  const gameplay = snake(input.gameplay);
  gameplay.intent_target = gameplay.accepted_intent_target;
  gameplay.intent_summary = gameplay.accepted_intent_summary;
  gameplay.stage_status = gameplay.execution_state;
  gameplay.available_actions = (gameplay.available_actions || []).map(snake);
  gameplay.recent_feedback = gameplay.recent_feedback ? snake(gameplay.recent_feedback) : null;
  const locations = (input.lists?.locations || []).map((location) => ({
    id: location.id,
    label: location.name || location.label || location.id,
    pos: location.pos || { x_cm: 0, y_cm: 0, z_cm: 0 },
  }));
  const agents = (input.lists?.agents || []).map((agent, index) => ({
    id: agent.id,
    name: agent.name,
    label: agent.name || agent.label || agent.id,
    pos: agent.pos || { x_cm: 5_020_000 + index * 15_000, y_cm: 2_510_000, z_cm: 0 },
    positionSource: "location_derived",
  }));
  const firstAction = gameplay.available_actions?.[0] || {};
  const activeAgentId = gameplay.intent_target || agents[0]?.id || null;
  const receiptPresent = Boolean(gameplay.recent_feedback || gameplay.last_world_change);
  const receipt = {
    present: receiptPresent,
    state: receiptPresent ? "blocked" : "waiting_for_intent",
    confidence: receiptPresent ? "world_delta" : "none",
    title: receiptPresent ? "Action blocked" : "No action receipt yet",
    summary: receiptPresent ? "Action blocked" : "No receipt",
    detail: gameplay.last_world_change || "No player-caused world change has been confirmed yet.",
    target_agent_id: receiptPresent ? activeAgentId : null,
  };
  return {
    locale: input.locale || "en",
    worldBounds: snapshot.config?.space,
    world_bounds: snapshot.config?.space,
    locations,
    fragmentTerrain: [],
    fragment_terrain: [],
    agents,
    links: agents.filter((agent) => agent.location_id || locations.length).map((agent) => ({ id: `link:${agent.id}`, kind: "agent_assignment" })),
    selection: activeAgentId ? { kind: "agent", id: activeAgentId } : null,
    goalHighlight: { title: gameplay.goal_title || "Current Objective", objective: gameplay.objective || "" },
    blockerHighlight: gameplay.blocker_kind ? { kind: gameplay.blocker_kind, label: "Missing Material" } : null,
    recentEventHotspots: [],
    visualHotspots: [],
    commercial_surface: {
      objective: {
        title: gameplay.goal_title || "Current Objective",
        detail: gameplay.objective || "No current objective.",
        progress_percent: gameplay.progress_percent ?? null,
      },
      next_action: {
        label: firstAction.label || "Build smelter mk1",
        detail: gameplay.intent_summary || null,
        target_agent_id: firstAction.target_agent_id || activeAgentId,
        execute_kind: firstAction.action_id === "claim_starter_oc" ? "claim_starter_oc" : firstAction.action_id === "claim_first_agent" ? "claim_first_agent" : "gameplay_action",
      },
      active_agent_id: activeAgentId,
      player_leverage: {
        state: gameplay.stage_status || "waiting_for_intent",
        label: receiptPresent ? "Blocked" : "Waiting for Intent",
        summary: gameplay.progress_detail || "Waiting",
        detail: null,
      },
      action_receipt: receipt,
      blocker: { label: gameplay.blocker_kind ? "Missing Material" : null, detail: gameplay.blocker_detail || null },
      world_read: { tick: 12, agents: agents.length, routes: agents.length, fragments: 0, hotspots: 0 },
    },
    presentation: { world_bounds_label: "bounds", marker_truth_note: "truth" },
  };
}

function bindFirstSnapshotAgentForTest(core, snapshot) {
  const agentId = Object.keys(snapshot?.model?.agents || {})[0];
  const playerId = snapshot?.model?.agent_player_bindings?.[agentId];
  if (!agentId || !playerId) return;
  core.state.auth = {
    ...core.state.auth,
    available: true,
    playerId,
    publicKey: snapshot.model.agent_player_public_key_bindings?.[agentId] || "abcdef0123456789abcdef0123456789",
    privateKey: "private-key-must-stay-hidden",
    source: "local_test_api_ephemeral",
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: agentId,
  };
}

async function renderPixelWorldHost(snapshot = sampleSnapshot(), search = "?test_api=1&connect=0&locale=en") {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", `/software_safe.html${search}`);
  window.localStorage.clear();
  document.body.innerHTML = "";
  const core = await import("./legacy_core.js");
  const { PixelWorldHost } = await import("./pixel_world_host.jsx");
  core.setViewerLocale("en");
  core.injectSnapshot(snapshot);
  bindFirstSnapshotAgentForTest(core, snapshot);
  const view = render(() => <PixelWorldHost locale="en" />);
  activeCleanup = view.unmount;
  return { core, ...view };
}

beforeEach(() => {
  runtimeMock.deriveRenderState = null;
  runtimeMock.mountError = null;
  runtimeMock.mountGates = [];
  runtimeMock.mountResults = [];
  runtimeMock.mountCalls = 0;
  canvasContextSpy = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({});
  window.history.replaceState({}, "", "/software_safe.html?test_api=1&connect=0&locale=en");
  window.localStorage.clear();
  document.body.innerHTML = "";
});

afterEach(() => {
  activeCleanup?.();
  activeCleanup = null;
  canvasContextSpy?.mockRestore();
  canvasContextSpy = null;
  document.body.innerHTML = "";
});

describe("pixel world host remediation contracts", () => {
  it("renders exactly one visible receipt representation in Cinematic View", async () => {
    runtimeMock.deriveRenderState = vi.fn(renderStateFor);
    await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument());
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => expect(document.querySelector(".pixel-world-host")).toHaveAttribute("data-world-focus", "true"));
    const visibleReceipts = [...document.querySelectorAll('[data-viewer-overlay="receipt"]')];
    expect(visibleReceipts).toHaveLength(1);
    expect(document.querySelector(".pixel-world-focus-hud__cell--receipt")).toBeNull();
    screen.getByRole("button", { name: "Maximize" }).click();
    await waitFor(() => expect(document.querySelector(".pixel-world-host")).toHaveAttribute("data-world-focus-maximized", "true"));
    const maximizedReceipts = [...document.querySelectorAll('[data-viewer-overlay="receipt"]')];
    expect(maximizedReceipts).toHaveLength(1);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses Agent name before label before id in the Cinematic focus surfaces", async () => {
    const snapshot = clone(sampleSnapshot());
    snapshot.model.agents["agent-0"].name = "Surveyor Seven";
    runtimeMock.deriveRenderState = vi.fn((input) => {
      const state = renderStateFor(input);
      state.agents[0] = { ...state.agents[0], name: "Surveyor Seven", label: "Fallback Label" };
      state.selection = { kind: "agent", id: "agent-0" };
      return state;
    });
    await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument());
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => expect(document.querySelector('[data-focus-minimap="true"]')).not.toBeNull());
    const railAgent = Array.from(document.querySelectorAll(".pixel-world-focus-rail__item"))
      .find((item) => item.querySelector("span")?.textContent === "Agent");
    expect(railAgent).toBeTruthy();
    expect(railAgent.querySelector("strong")).toHaveTextContent("Surveyor Seven");
    expect(document.querySelector(".pixel-world-focus-minimap__node--agent strong")).toHaveTextContent("Surveyor Seven");
    const railSelected = Array.from(document.querySelectorAll(".pixel-world-focus-rail__item"))
      .find((item) => item.querySelector("span")?.textContent === "Selected");
    expect(railSelected.querySelector("strong")).toHaveTextContent("Surveyor Seven");
    expect(document.querySelector(".pixel-world-focus-minimap__node--selected strong")).toHaveTextContent("Surveyor Seven");
    screen.getByRole("button", { name: "Command & Target" }).click();
    expect(document.querySelector(".pixel-world-focus-command-chip--target strong")).toHaveTextContent("Surveyor Seven");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("labels an enabled direct Next Move with the gameplay verb it submits", async () => {
    const snapshot = clone(sampleSnapshot());
    snapshot.player_gameplay.available_actions = [{
      action_id: "claim_starter_oc",
      target_agent_id: "agent-0",
      label: "Claim starter OC",
      protocol_action: "gameplay_action.submit",
      disabled_reason: null,
    }];
    runtimeMock.deriveRenderState = vi.fn(renderStateFor);
    const { core } = await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(screen.getAllByText("Claim starter OC")).toHaveLength(2));
    const dispatchSpy = vi.spyOn(core, "sendGameplayAction");
    const nextMove = screen.getByRole("link", { name: "Next Move: Claim starter OC" });
    expect(nextMove).toHaveTextContent("Claim starter OC");
    nextMove.click();
    expect(dispatchSpy).toHaveBeenCalledTimes(1);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("guards a pending or out-of-bound Next Move against repeat activation", async () => {
    const snapshot = clone(sampleSnapshot());
    snapshot.model.agents["agent-1"] = { id: "agent-1", name: "Other Agent", location_id: "loc-0", resources: {} };
    snapshot.model.agent_player_bindings["agent-1"] = "player-one";
    snapshot.model.agent_player_public_key_bindings["agent-1"] = "1234567890abcdef1234567890abcdef";
    snapshot.player_gameplay.intent_target = "agent-1";
    snapshot.player_gameplay.available_actions = [{
      action_id: "claim_starter_oc",
      target_agent_id: "agent-1",
      label: "Claim starter OC",
      protocol_action: "gameplay_action.submit",
      disabled_reason: null,
    }];
    runtimeMock.deriveRenderState = vi.fn(renderStateFor);
    const { core } = await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(screen.getAllByText("Claim starter OC")).toHaveLength(2));
    const dispatchSpy = vi.spyOn(core, "sendGameplayAction");
    core.state.lastGameplayActionFeedback = { kind: "gameplay_action", action: "claim_starter_oc", agentId: "agent-1", stage: "registering", accepted: false };
    core.requestRender();
    const nextMove = screen.getByRole("link", { name: "Next Move: Claim starter OC" });
    expect(nextMove).toHaveAttribute("aria-disabled", "true");
    nextMove.click();
    nextMove.click();
    expect(dispatchSpy).not.toHaveBeenCalled();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("shows connected world-feed unavailability separately from stale data", async () => {
    runtimeMock.deriveRenderState = vi.fn(renderStateFor);
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument());
    core.state.connectionStatus = "connected";
    core.state.worldFeed = { ...core.state.worldFeed, status: "unavailable", stale: true, unavailableReason: "source_unavailable" };
    core.requestRender();
    await waitFor(() => {
      expect(document.querySelector(".pixel-world-readout")).toHaveTextContent("UNAVAILABLE");
      expect(document.querySelector(".pixel-world-readout")).not.toHaveTextContent("STALE");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps a stale renderer mount result from replacing the current retry", async () => {
    runtimeMock.deriveRenderState = vi.fn(renderStateFor);
    let resolveFirst;
    let resolveSecond;
    runtimeMock.mountGates = [
      new Promise((resolve) => { resolveFirst = resolve; }),
      new Promise((resolve) => { resolveSecond = resolve; }),
    ];
    runtimeMock.mountResults = [
      { status: "unavailable", fatal: { code: "stale_attempt", message: "stale renderer attempt" } },
      { status: "ready", fatal: null },
    ];
    const { core } = await renderPixelWorldHost(sampleSnapshot());
    await waitFor(() => expect(runtimeMock.mountCalls).toBe(1));
    screen.getByRole("button", { name: "Reattach Embedded Renderer" }).click();
    await waitFor(() => expect(runtimeMock.mountCalls).toBe(2));
    resolveFirst();
    resolveSecond();
    await waitFor(() => expect(core.state.pixelWorldRuntimeStatus).toBe("ready"));
    expect(screen.queryByText("Graphics unavailable in this browser")).toBeNull();
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
