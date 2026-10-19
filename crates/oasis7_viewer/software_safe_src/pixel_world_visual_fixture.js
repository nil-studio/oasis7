import * as core from "./legacy_core.js";
import {
  pixelWorldMicroDepotStockRunwayVisualFixture,
  pixelWorldRecommendedTargetVisualFixture,
  pixelWorldSelectedBlockerVisualFixture,
  pixelWorldModuleVisualEntitiesFixture,
  pixelWorldRoutesAndEventsVisualFixture,
  pixelWorldPerformanceVisualFixture,
} from "./pixel_world_visual_fixture_data.js";

const PIXEL_WORLD_VISUAL_FIXTURE_GLOBAL = "__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__";
const PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT_GLOBAL = "__OASIS7_PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT__";
const PERFORMANCE_CONTROL_GLOBAL = "__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__";

function installPerformanceFixturePanel(control) {
  const panel = document.createElement("section");
  panel.id = "pixel-world-performance-fixture-controls";
  panel.setAttribute("aria-label", "Performance fixture controls");
  panel.style.cssText = "position:fixed;right:8px;top:8px;z-index:2147483647;width:min(360px,calc(100vw - 16px));max-height:45vh;overflow:auto;padding:8px;background:#111827;color:#f9fafb;border:1px solid #94a3b8;font:12px monospace";
  const title = document.createElement("strong");
  title.textContent = "Offline performance fixture";
  panel.append(title);
  const actions = document.createElement("div");
  const output = document.createElement("output");
  output.setAttribute("aria-label", "Performance fixture diagnostics");
  output.setAttribute("data-testid", "performance-fixture-diagnostics");
  output.style.cssText = "display:block;white-space:pre-wrap;overflow-wrap:anywhere;margin-top:6px";
  const refresh = () => {
    let diagnostics;
    try {
      const dto = window.__OASIS7_PIXEL_WORLD_RENDER_DTO__?.() || null;
      const agents = dto?.agents || [];
      diagnostics = {
        ...control.status(),
        dto_marker_count: agents.length + (dto?.locations?.length || 0) + (dto?.module_visual_entities?.length || 0),
        dto_bytes: dto ? new TextEncoder().encode(JSON.stringify(dto)).byteLength : null,
        display_budget: dto?.display_budget || null,
        dto_world_tick: dto?.world_tick ?? null,
        dto_selected_position: agents.find((agent) => agent.id === "agent-0")?.pos || null,
        fatal: core.state.pixelWorldFatal ? {
          code: String(core.state.pixelWorldFatal.code || "").slice(0, 128),
          message: String(core.state.pixelWorldFatal.message || "").slice(0, 512),
        } : null,
      };
    } catch (error) {
      diagnostics = { ...control.status(), diagnostic_error: String(error?.message || error).slice(0, 512) };
    }
    output.textContent = JSON.stringify(diagnostics, null, 2);
  };
  for (const [label, action] of [
    ["Start 2 Hz", () => control.start()],
    ["Burst 10 Hz for 5 seconds", () => control.burst()],
    ["Stop updates", () => control.stop()],
  ]) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.style.cssText = "margin:6px 6px 0 0;padding:6px";
    button.addEventListener("click", () => {
      try { action(); } catch (error) { output.textContent = String(error?.message || error); return; }
      refresh();
    });
    actions.append(button);
  }
  panel.append(actions, output);
  document.body.append(panel);
  refresh();
  const timer = window.setInterval(refresh, 1000);
  return () => {
    window.clearInterval(timer);
    panel.remove();
  };
}

function installPerformanceFixtureControl(fixture) {
  let revision = 0;
  let timer = null;
  let remainingUpdates = null;
  let intervalMs = null;
  let disposePanel = null;
  const initialTime = fixture.time;
  const initialX = fixture.model.agents["agent-0"].pos.x_cm;
  const status = () => ({
    running: timer !== null,
    revision,
    time: initialTime + revision,
    sourceEntityCount: fixture.performance_fixture.source_entity_count,
    historyBytes: fixture.history.unrelated_payload.length,
    intervalMs,
    remainingUpdates,
    selectedPosition: { ...fixture.model.agents["agent-0"].pos },
  });
  const stop = () => {
    if (timer !== null) window.clearInterval(timer);
    timer = null;
    remainingUpdates = null;
    intervalMs = null;
    return status();
  };
  const step = () => {
    if (!pixelWorldTestApiEnabled() || !liveConnectionDisabledForFixture()) {
      dispose();
      throw new Error("Performance controls require test_api=1 and connect=0");
    }
    // Deliberately inject complete source snapshots through the existing test
    // transport. No renderer DTO or action/receipt path is overridden here.
    revision += 1;
    fixture.time = initialTime + revision;
    fixture.performance_fixture.revision = revision;
    fixture.model.agents["agent-0"].pos.x_cm = initialX + revision * 1000;
    const playerId = core.state.auth.playerId || "player-one";
    const publicKey = core.state.auth.publicKey || "abcdef0123456789abcdef0123456789";
    for (const id of Object.keys(fixture.model.agents)) {
      fixture.model.agent_player_bindings[id] = playerId;
      fixture.model.agent_player_public_key_bindings[id] = publicKey;
    }
    core.injectSnapshot(fixture, { returnState: false });
    core.requestRender();
    return status();
  };
  const start = ({ hz = 2, updates = null } = {}) => {
    if (!pixelWorldTestApiEnabled() || !liveConnectionDisabledForFixture()) {
      dispose();
      throw new Error("Performance controls require test_api=1 and connect=0");
    }
    if (![2, 10].includes(hz) || (updates !== null && (!Number.isInteger(updates) || updates < 1))) {
      throw new Error("Use 2 or 10 Hz and a positive integer update count");
    }
    stop();
    intervalMs = 1000 / hz;
    remainingUpdates = updates;
    timer = window.setInterval(() => {
      step();
      if (remainingUpdates !== null && --remainingUpdates === 0) stop();
    }, intervalMs);
    return status();
  };
  const dispose = () => {
    stop();
    disposePanel?.();
    disposePanel = null;
    window.removeEventListener("pagehide", dispose);
    if (window[PERFORMANCE_CONTROL_GLOBAL] === control) delete window[PERFORMANCE_CONTROL_GLOBAL];
  };
  const control = { status, step, start, burst: () => start({ hz: 10, updates: 50 }), stop, dispose };
  window.addEventListener("pagehide", dispose);
  window[PERFORMANCE_CONTROL_GLOBAL] = control;
  disposePanel = installPerformanceFixturePanel(control);
}

export function pixelWorldTestApiEnabled() {
  if (__OASIS7_VISUAL_TEST__ !== true || typeof window === "undefined" || !window.location) {
    return false;
  }
  const value = String(new URLSearchParams(window.location.search || "").get("test_api") || "").trim().toLowerCase();
  return value === "1" || value === "true" || value === "yes" || value === "on";
}

function requestedVisualFixtureName() {
  if (__OASIS7_VISUAL_TEST__ !== true || typeof window === "undefined" || !window.location) {
    return null;
  }
  return String(new URLSearchParams(window.location.search || "").get("pixel_world_visual_fixture") || "").trim();
}

function liveConnectionDisabledForFixture() {
  if (__OASIS7_VISUAL_TEST__ !== true || typeof window === "undefined" || !window.location) {
    return false;
  }
  return String(new URLSearchParams(window.location.search || "").get("connect") || "").trim() === "0";
}

export function installPixelWorldVisualFixtureHook() {
  if (typeof window !== "undefined") window[PERFORMANCE_CONTROL_GLOBAL]?.dispose?.();
  if (typeof window === "undefined" || !pixelWorldTestApiEnabled() || !liveConnectionDisabledForFixture()) {
    return null;
  }
  const fixtures = new Map(Object.entries({
    selected_blocker: () => core.clone(pixelWorldSelectedBlockerVisualFixture()),
    hotspot_tooltip: () => core.clone(pixelWorldSelectedBlockerVisualFixture()),
    recent_event_glyphs: () => core.clone(pixelWorldSelectedBlockerVisualFixture()),
    routes_and_events: () => core.clone(pixelWorldRoutesAndEventsVisualFixture()),
    recommended_target: () => core.clone(pixelWorldRecommendedTargetVisualFixture()),
    module_visual_entities: () => core.clone(pixelWorldModuleVisualEntitiesFixture()),
    micro_depot_stock_runway: () => core.clone(pixelWorldMicroDepotStockRunwayVisualFixture()),
    performance_regular: () => pixelWorldPerformanceVisualFixture(1000),
    performance_high_load: () => pixelWorldPerformanceVisualFixture(10000),
  }));
  window[PIXEL_WORLD_VISUAL_FIXTURE_GLOBAL] = fixtures;

  const fixtureName = requestedVisualFixtureName();
  if (!fixtureName || !fixtures.has(fixtureName)) {
    return null;
  }
  const fixture = fixtures.get(fixtureName)();
  core.injectSnapshot(fixture, { returnState: false });
  if (fixtureName === "module_visual_entities") {
    const moduleFixtureEvents = [
      {
        event_seq: 101,
        kind: "ModuleVisualEntityUpserted",
        summary: "Relay marker published",
        detail: "The relay marker is available on the world map.",
        receipt_ref: null,
        module_visual_entity_id: "module-relay",
      },
      {
        event_seq: 100,
        kind: "ModuleVisualEntityRemoved",
        summary: "Removed marker reference",
        detail: "The referenced marker is no longer in the current snapshot.",
        receipt_ref: null,
        module_visual_entity_id: "module-deleted",
      },
    ];
    core.state.worldFeed = {
      status: "ready",
      schemaVersion: "world_feed/v1",
      worldId: "fixture-world",
      reorgEpoch: "0",
      cursor: "101",
      events: moduleFixtureEvents,
      stale: false,
      gapReason: null,
      unavailableReason: null,
      snapshotReloadRequired: false,
      requestInFlight: false,
      requestCursor: null,
      requestLimit: 50,
      dedupedCount: 0,
      lastError: null,
    };
    // This is test-api-only and uses whole snapshots so it cannot expose a
    // production action path. It lets a browser smoke prove update/removal.
    window.__OASIS7_MODULE_VISUAL_FIXTURE_CONTROL__ = {
      update(entities) {
        const next = core.clone(fixture);
        next.model.module_visual_entities = core.clone(entities || {});
        core.injectSnapshot(next, { returnState: false });
        core.requestRender();
        return true;
      },
      publishEvent(event) {
        core.state.worldFeed.events = [core.clone(event)];
        core.state.worldFeed.status = "ready";
        core.state.worldFeed.stale = false;
        core.requestRender();
        return true;
      },
      publishStaleEvent(event) {
        core.state.worldFeed.events = [core.clone(event)];
        core.state.worldFeed.status = "gap";
        core.state.worldFeed.stale = true;
        core.requestRender();
        return true;
      },
      clear() {
        return this.update({});
      },
    };
  }
  if (["recent_event_glyphs", "routes_and_events"].includes(fixtureName)) {
    // Test-only input for the real WASM renderer smoke. These event kinds are
    // projected by the bridge into two independent, hoverable visual hotspots.
    core.state.recentEvents = [
      { event_id: "resource-transfer-fixture", title: "Resource transfer completed", kind: "resource_transfer" },
      { event_id: "build-queue-fixture", title: "Build queue updated", kind: "build_queue" },
    ];
    core.state.eventCount = core.state.recentEvents.length;
  }
  const alignFixtureAuth = () => {
    const playerId = String(core.state.auth.playerId || "player-one").trim() || "player-one";
    const publicKey = String(core.state.auth.publicKey || "abcdef0123456789abcdef0123456789").trim();
    const model = core.state.snapshot?.model || {};
    const alignedAgentIds = fixtureName === "routes_and_events" || fixture.performance_fixture
      ? Object.keys(model.agents || {})
      : ["agent-0"];
    model.agent_player_bindings = {
      ...(model.agent_player_bindings || {}),
    };
    model.agent_player_public_key_bindings = {
      ...(model.agent_player_public_key_bindings || {}),
    };
    for (const agentId of alignedAgentIds) {
      model.agent_player_bindings[agentId] = playerId;
      model.agent_player_public_key_bindings[agentId] = publicKey;
    }
    core.state.auth = {
    ...core.state.auth,
    available: true,
      playerId,
      publicKey,
      privateKey: null,
      releaseToken: null,
      registrationGrant: null,
    source: "visual_fixture_projection",
    registrationStatus: "registered",
    runtimeStatus: "registered",
    boundAgentId: "agent-0",
    };
    core.applySelection({ kind: "agent", id: "agent-0" });
    core.requestRender();
    return true;
  };
  window[PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT_GLOBAL] = alignFixtureAuth;
  alignFixtureAuth();
  if (fixture.performance_fixture) installPerformanceFixtureControl(fixture);
  return fixtureName;
}

export function installPixelWorldRenderDtoProbe(fixtureName, getRenderState, onCleanup) {
  if (!fixtureName || !pixelWorldTestApiEnabled()) {
    return;
  }
  window.__OASIS7_PIXEL_WORLD_RENDER_DTO__ = () => core.clone(getRenderState());
  onCleanup(() => {
    delete window.__OASIS7_PIXEL_WORLD_RENDER_DTO__;
  });
}
