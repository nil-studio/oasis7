// This adapter only replaces the bridge module in the test server. The DOM
// and Viewer source remain the real bundle under test; the canvas is rendered
// by the repo-owned JS bridge and its derived state is deterministic.
export const bridgeModule = String.raw`import { createPixelWorldBevyBridge } from "/software_safe_src/pixel_world_bevy_bridge.js";
export const PIXEL_WORLD_RUNTIME_SOURCE = "qa_browser_fixture_bridge";
export async function createPixelWorldBridge(options) { return createPixelWorldBevyBridge(options); }
const field = (value, snake, camel, fallback = null) => value?.[snake] ?? value?.[camel] ?? fallback;
const pos = (entity) => entity?.pos || { x_cm: 0, y_cm: 0, z_cm: 0 };
export function derivePixelWorldRenderState(input) {
  const snapshot = input?.snapshot || {};
  const model = snapshot.model || {};
  const gameplay = input?.gameplay || snapshot.player_gameplay || {};
  const locations = Object.values(model.locations || {});
  const agents = Object.values(model.agents || {});
  const bounds = snapshot.config?.space || { width_cm: 10000000, depth_cm: 5000000, height_cm: 1000000 };
  const selectedId = input?.selectedId || gameplay.intent_target || agents[0]?.id || null;
  const selectedKind = input?.selectedKind || (selectedId ? "agent" : null);
  const fragments = locations.flatMap((location) => (location.fragment_profile?.blocks?.blocks || []).map((block, index) => ({
    id: "fragment:" + location.id + ":" + index, location_id: location.id,
    pos: { x_cm: pos(location).x_cm + Number(block.origin_cm?.x_cm || 0), y_cm: pos(location).y_cm + Number(block.origin_cm?.z_cm || block.origin_cm?.y_cm || 0), z_cm: pos(location).z_cm + Number(block.origin_cm?.y_cm || 0) },
    dominant_compound: Object.entries(block.compounds?.ppm || {}).sort((a, b) => Number(b[1]) - Number(a[1]))[0]?.[0] || "unknown",
    footprint_cm: Math.max(Number(block.size_cm?.x_cm || 12000), Number(block.size_cm?.z_cm || block.size_cm?.y_cm || 12000)),
  })));
  const normalizedAgents = agents.map((agent, index) => {
    const location = model.locations?.[agent.location_id];
    return { id: agent.id, label: agent.name || agent.id, pos: agent.pos || { x_cm: pos(location).x_cm + 20000 + index * 15000, y_cm: pos(location).y_cm + 10000 + index * 12000, z_cm: pos(location).z_cm }, position_source: agent.pos ? "runtime_agent" : "location_derived" };
  });
  const normalizedLocations = locations.map((location) => ({ id: location.id, label: location.name || location.id, pos: pos(location), marker_role: "logic_anchor", marker_alpha: 0.32 }));
  const firstAction = (gameplay.available_actions || gameplay.availableActions || [])[0] || {};
  const hasReceipt = Boolean(gameplay.recent_feedback || gameplay.recentFeedback || gameplay.last_world_change || gameplay.lastWorldChange);
  const blocker = gameplay.blocker_kind || gameplay.blockerKind || null;
  const blockerDetail = gameplay.blocker_detail || gameplay.blockerDetail || null;
  const objective = gameplay.objective || gameplay.progress_detail || gameplay.progressDetail || "Stabilize the first production line before expanding.";
  return {
    world_bounds: bounds, locations: normalizedLocations, agents: normalizedAgents, fragment_terrain: fragments,
    links: normalizedAgents.filter((agent) => model.locations?.[agents.find((candidate) => candidate.id === agent.id)?.location_id]).map((agent) => ({ id: "link:" + agent.id, kind: "agent_assignment", from: agent.pos, to: pos(model.locations?.[agents.find((candidate) => candidate.id === agent.id)?.location_id]) })),
    selection: selectedKind && selectedId ? { kind: selectedKind, id: selectedId } : null,
    goal_highlight: { title: gameplay.goal_title || gameplay.goalTitle || "Recover sustainable capability", objective },
    blocker_highlight: blocker ? { kind: blocker, label: blocker === "material_shortage" ? "Missing Material" : blocker, detail: blockerDetail } : null,
    // Three read-only fixture explanations exercise blocker/goal/info paths.
    visual_hotspots: [
      { id: "fixture-hotspot-blocker", kind: "blocker", label: "iron input is exhausted", pos: { x_cm: 2900000, y_cm: 3450000, z_cm: 0 } },
      { id: "fixture-hotspot-goal", kind: "goal", label: String(gameplay.objective || "").length > 120 ? gameplay.objective : "stabilize the first production line", pos: { x_cm: 7150000, y_cm: 2200000, z_cm: 0 } },
      { id: "fixture-hotspot-info", kind: "info", label: "read-only world context", pos: { x_cm: 4550000, y_cm: 1200000, z_cm: 0 } },
    ],
    commercial_surface: {
      objective: { title: gameplay.goal_title || gameplay.goalTitle || "Recover sustainable capability", detail: objective, progress_percent: gameplay.progress_percent ?? gameplay.progressPercent ?? 68 },
      next_action: { label: field(firstAction, "label", "label", "Build smelter mk1"), detail: gameplay.intent_summary || gameplay.intentSummary || "Replenish upstream materials, then advance again to confirm the line resumes.", target_agent_id: field(firstAction, "target_agent_id", "targetAgentId", selectedId), execute_kind: field(firstAction, "execute_kind", "executeKind", "gameplay_action") },
      active_agent_id: selectedId,
      player_leverage: { state: gameplay.stage_status || gameplay.stageStatus || "blocked", label: hasReceipt ? "Blocked" : "Waiting for Intent", summary: gameplay.progress_detail || gameplay.progressDetail || "The primary line is blocked by missing material input.", detail: gameplay.next_step_hint || gameplay.nextStepHint || "Replenish upstream materials, then advance again to confirm the line resumes." },
      action_receipt: { present: hasReceipt, state: hasReceipt ? "blocked" : "waiting_for_intent", confidence: hasReceipt ? "world_delta" : "none", title: hasReceipt ? "Action blocked" : "No action receipt yet", summary: hasReceipt ? "Smelter build request reached factory-0; iron shortage blocks construction." : "No receipt", detail: gameplay.last_world_change || gameplay.lastWorldChange || "No player-caused world change has been confirmed yet.", target_agent_id: hasReceipt ? selectedId : null },
      blocker: { label: blocker === "material_shortage" ? "Missing Material" : blocker, detail: gameplay.next_step_hint || gameplay.nextStepHint || blockerDetail },
      world_read: { agents: normalizedAgents.length, routes: normalizedAgents.length, fragments: fragments.length, hotspots: 3 },
    },
  };
}`;

// Production-overlay regression data is sent as a normal snapshot message by
// the deterministic transport below. It intentionally does not use the
// pixel-world visual-fixture query or body marker that enables the historical
// fixture shell.
export const productionSnapshot = {
  time: 12,
  config: { space: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 } },
  model: {
    agents: { "agent-0": { id: "agent-0", name: "Agent 0", location_id: "loc-0", pos: { x_cm: 2_400_000, y_cm: 1_800_000, z_cm: 0 } } },
    locations: { "loc-0": { id: "loc-0", name: "Factory Anchor", pos: { x_cm: 2_000_000, y_cm: 1_500_000, z_cm: 0 } } },
  },
  player_gameplay: {
    stage_status: "blocked",
    goal_title: "Recover sustainable capability",
    objective: "Stabilize the first production line before expanding.",
    progress_detail: "The primary line is blocked by missing material input.",
    blocker_kind: "material_shortage",
    blocker_detail: "iron input exhausted at factory-0",
    next_step_hint: "Replenish upstream materials, then advance again to confirm the line resumes.",
    available_actions: [{ action_id: "request_snapshot", label: "Request snapshot", protocol_action: "world.request_snapshot", execute_kind: "request_snapshot" }],
    recent_feedback: { action: "build_factory_smelter_mk1", stage: "completed_no_progress", effect: "Smelter build request reached factory-0; iron shortage blocks construction." },
    last_world_change: "Smelter build request reached factory-0; iron shortage blocks construction.",
  },
};

export const fakeWebSocket = String.raw`(() => {
  const status = new URLSearchParams(location.search).get("feed_status") || "ready";
  const sendProductionSnapshot = !new URLSearchParams(location.search).has("pixel_world_visual_fixture");
  const incomingSnapshot = ${JSON.stringify(productionSnapshot)};
  if (new URLSearchParams(location.search).get("long_hotspot") === "1") incomingSnapshot.player_gameplay.objective = "Long runtime explanation: " + "Replenish upstream materials and inspect sustainable production constraints before expansion. ".repeat(32) + "QA_LABEL_END";
  const listeners = new WeakMap();
  const emit = (socket, type, payload = {}) => {
    const callbacks = listeners.get(socket)?.[type] || [];
    for (const callback of callbacks) callback({ type, ...payload });
  };
  const feed = (value) => ({ schema_version: "world_feed/v1", world_id: "fixture-world", reorg_epoch: "0", cursor: value === "empty" ? "" : "1", status: value, events: ["ready", "replay"].includes(value) ? [{ event_seq: "1", kind: "resource_change", summary: "Fixture ambient resource update", detail: "QA fixture event; no player action receipt.", receipt_ref: null }] : [], gap_reason: value === "gap" ? "cursor_gap" : null, unavailable_reason: value === "unavailable" ? "source_unavailable" : null, snapshot_reload_required: value === "gap" });
  class FixtureWebSocket {
    static OPEN = 1;
    static CONNECTING = 0;
    static CLOSING = 2;
    static CLOSED = 3;
    constructor(url) { this.url = url; this.readyState = FixtureWebSocket.CONNECTING; listeners.set(this, {}); queueMicrotask(() => { this.readyState = FixtureWebSocket.OPEN; emit(this, "open"); }); }
    addEventListener(type, callback) { const table = listeners.get(this); table[type] ||= []; table[type].push(callback); }
    removeEventListener(type, callback) { const list = listeners.get(this)?.[type] || []; const index = list.indexOf(callback); if (index >= 0) list.splice(index, 1); }
    send(raw) { const message = JSON.parse(raw); if (message.type === "hello") { if (sendProductionSnapshot) queueMicrotask(() => emit(this, "message", { data: JSON.stringify({ type: "snapshot", snapshot: incomingSnapshot }) })); queueMicrotask(() => emit(this, "message", { data: JSON.stringify({ type: "hello_ack", server: "qa-fixture", world_id: "fixture-world", control_profile: "fixture" }) })); } if (message.type === "request_world_feed") queueMicrotask(() => emit(this, "message", { data: JSON.stringify({ type: "world_feed", feed: feed(status) }) })); }
    close() { this.readyState = FixtureWebSocket.CLOSED; emit(this, "close"); }
  }
  window.WebSocket = FixtureWebSocket;
})();`;

// The deterministic repo-owned JS bridge below paints with a 2D context,
// while the production host performs a WebGL2 surface probe first. A real
// canvas cannot claim both context types, so this fixture satisfies only the
// host probe and leaves the bridge's 2D context untouched. This keeps the
// browser coverage focused on the real Viewer DOM/shell without implying a
// live WebGPU/WebGL renderer or playability.
export const fixtureCanvasCompatibility = String.raw`(() => {
  const nativeGetContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function getContext(type, ...args) {
    if (type === "webgl2") return { __qaWebgl2ProbeOnly: true };
    return nativeGetContext.call(this, type, ...args);
  };
})();`;

export const fixtureVisualOverlayBootstrap = String.raw`(() => {
  document.body.setAttribute("data-viewer-visual-fixture", "shell_selected_blocker");
})();`;
