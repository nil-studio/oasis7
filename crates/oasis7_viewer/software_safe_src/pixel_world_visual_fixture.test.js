import { afterEach, describe, expect, it, vi } from "vitest";

import * as core from "./legacy_core.js";
import { installPixelWorldRenderDtoProbe, installPixelWorldVisualFixtureHook } from "./pixel_world_visual_fixture.js";

afterEach(() => {
  window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__?.dispose();
  vi.useRealTimers();
  vi.restoreAllMocks();
  window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0");
  delete window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__;
  delete window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT__;
  delete window.__OASIS7_PIXEL_WORLD_RENDER_DTO__;
  core.state.pixelWorldFatal = null;
});

describe("pixel world visual fixtures", () => {
  it.each([
    ["performance_regular", 1000], ["performance_high_load", 10000],
  ])("publishes %s with complete source input and unrelated history", (name, count) => {
    window.history.replaceState({}, "", `/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=${name}`);
    expect(installPixelWorldVisualFixtureHook()).toBe(name);
    const snapshot = core.state.snapshot;
    expect(Object.keys(snapshot.model.agents).length + Object.keys(snapshot.model.locations).length).toBe(count);
    expect(snapshot.history.unrelated_payload.length).toBe(5 * 1024 * 1024);
    expect(core.modelLists().agents).toHaveLength(count - 2);
    expect(core.state.selectedId).toBe("agent-0");
    expect(snapshot.player_gameplay).toMatchObject({ goal_id: "post_onboarding.recover_capability", blocker_kind: "material_shortage" });
    expect(window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__.status()).toMatchObject({
      sourceEntityCount: count, historyBytes: 5 * 1024 * 1024, revision: 0, running: false,
    });
  });

  it("provides 2 Hz updates, a bounded 10 Hz burst, final revision and a stop control", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=performance_regular");
    installPixelWorldVisualFixtureHook();
    const control = window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__;
    // The actual initial snapshot above exercises the existing injection path;
    // capture subsequent complete snapshots without repeating unrelated core
    // transport work in fake-timer cadence assertions.
    const inject = vi.spyOn(core, "injectSnapshot").mockReturnValue({ ok: true });
    vi.spyOn(core, "requestRender").mockImplementation(() => {});
    vi.useFakeTimers();
    const initial = control.status();
    control.start();
    vi.advanceTimersByTime(1000);
    expect(control.status()).toMatchObject({ revision: 2, intervalMs: 500, running: true });
    control.stop();
    vi.advanceTimersByTime(1000);
    expect(control.status().revision).toBe(2);
    control.burst();
    vi.advanceTimersByTime(5000);
    const final = control.status();
    expect(final).toMatchObject({ revision: 52, running: false, intervalMs: null, time: initial.time + 52 });
    expect(final.selectedPosition.x_cm).toBe(initial.selectedPosition.x_cm + 52_000);
    expect(inject).toHaveBeenCalledTimes(52);
    const snapshot = inject.mock.calls.at(-1)[0];
    expect(snapshot.performance_fixture.revision).toBe(final.revision);
    expect(snapshot.model.agents["agent-0"].pos).toEqual(final.selectedPosition);
    expect(snapshot.history.unrelated_payload).toHaveLength(5 * 1024 * 1024);
    expect(() => control.start({ hz: 1000 })).toThrow("2 or 10 Hz");
    window.dispatchEvent(new Event("pagehide"));
    expect(window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__).toBeUndefined();
  });

  it("does not install performance controls for live or non-test URLs", () => {
    for (const query of ["connect=1&test_api=1", "connect=0"]) {
      window.history.replaceState({}, "", `/viewer.html?${query}&pixel_world_visual_fixture=performance_high_load`);
      expect(installPixelWorldVisualFixtureHook()).toBeNull();
      expect(window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__).toBeUndefined();
    }
  });

  it("revokes scheduled injection if the explicit offline test scope changes", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=performance_regular");
    installPixelWorldVisualFixtureHook();
    const control = window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__;
    vi.useFakeTimers();
    control.start();
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=1");
    expect(() => control.step()).toThrow("connect=0");
    expect(control.status()).toMatchObject({ running: false, revision: 0 });
    expect(() => control.start()).toThrow("connect=0");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("provides DOM controls and bounded renderer diagnostics without exposing source snapshots", () => {
    vi.useFakeTimers();
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=performance_regular");
    installPixelWorldVisualFixtureHook();
    const control = window.__OASIS7_PIXEL_WORLD_PERFORMANCE_CONTROL__;
    vi.spyOn(core, "injectSnapshot").mockReturnValue({ ok: true });
    vi.spyOn(core, "requestRender").mockImplementation(() => {});
    let cleanupProbe;
    installPixelWorldRenderDtoProbe("performance_regular", () => ({
      world_tick: control.status().time,
      agents: [{ id: "agent-0", pos: control.status().selectedPosition }],
      locations: [{ id: "loc-0" }],
      module_visual_entities: [],
      display_budget: { omitted_markers: 998 },
    }), (cleanup) => { cleanupProbe = cleanup; });
    const panel = document.getElementById("pixel-world-performance-fixture-controls");
    const click = (name) => [...panel.querySelectorAll("button")].find((button) => button.textContent === name).click();
    const read = () => JSON.parse(panel.querySelector("output").textContent);
    click("Burst 10 Hz for 5 seconds");
    vi.advanceTimersByTime(6000);
    expect(read()).toMatchObject({
      revision: 50, time: 62, running: false,
      dto_world_tick: 62, dto_marker_count: 2,
      dto_selected_position: control.status().selectedPosition,
      display_budget: { omitted_markers: 998 },
    });
    expect(read().dto_bytes).toBeGreaterThan(100);
    expect(panel.textContent).not.toContain("unrelated_payload");
    click("Start 2 Hz");
    vi.advanceTimersByTime(1000);
    click("Stop updates");
    expect(read()).toMatchObject({ revision: 52, running: false });
    core.state.pixelWorldFatal = { code: "capacity", message: "Too much display data" };
    vi.advanceTimersByTime(1000);
    expect(read().fatal).toEqual(core.state.pixelWorldFatal);
    window.dispatchEvent(new Event("pagehide"));
    cleanupProbe();
    expect(document.getElementById("pixel-world-performance-fixture-controls")).toBeNull();
    expect(vi.getTimerCount()).toBe(0);
  });
  it("installs the deterministic hotspot tooltip fixture only when live websocket connection is disabled", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=hotspot_tooltip");

    expect(installPixelWorldVisualFixtureHook()).toBe("hotspot_tooltip");
    expect(window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("hotspot_tooltip")).toEqual(expect.any(Function));
  });

  it("installs a recommendation-only fixture with an enabled rendered Agent target and no receipt inputs", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=recommended_target");

    expect(installPixelWorldVisualFixtureHook()).toBe("recommended_target");
    expect(window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("recommended_target")()).toMatchObject({
      model: { agents: { "agent-0": { id: "agent-0" } } },
      player_gameplay: {
        available_actions: [{ target_agent_id: "agent-0", disabled_reason: null }],
        recent_feedback: null,
      },
    });
    const gameplay = window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("recommended_target")().player_gameplay;
    expect(gameplay.accepted_intent_id).toBeUndefined();
    expect(gameplay.intent_target).toBeUndefined();
    expect(gameplay.last_world_change).toBeUndefined();
  });

  it("publishes an active Micro Depot at a known location for the selected-blocker renderer fixture", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=selected_blocker");

    expect(installPixelWorldVisualFixtureHook()).toBe("selected_blocker");
    expect(window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("selected_blocker")()).toMatchObject({
      model: { locations: { "loc-0": { id: "loc-0" } } },
      player_gameplay: {
        micro_depot_facilities: [{
          facility_id: "depot-fixture-loc-0",
          status: "active",
          location_id: "loc-0",
        }],
      },
    });
    expect(
      window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("selected_blocker")().player_gameplay
        .micro_depot_facilities[0].service_radius_cm,
    ).toBe(240_000);
  });

  it("uses the simulator ModuleVisualAnchor data envelope for module fixture anchors", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=module_visual_entities");

    expect(installPixelWorldVisualFixtureHook()).toBe("module_visual_entities");
    const entities = window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("module_visual_entities")().model.module_visual_entities;

    expect(entities["module-absolute"].anchor).toEqual({
      type: "absolute",
      data: { pos: { x_cm: 1_850_000, y_cm: 3_600_000, z_cm: 0 } },
    });
    expect(entities["module-agent"].anchor).toEqual({
      type: "agent",
      data: { agent_id: "agent-0" },
    });
  });

  it("registers a spatially distinct healthy/low/zero Micro Depot stock-runway fixture", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=micro_depot_stock_runway");

    expect(installPixelWorldVisualFixtureHook()).toBe("micro_depot_stock_runway");
    const fixture = window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("micro_depot_stock_runway")();
    const facilities = fixture.player_gameplay.micro_depot_facilities;

    expect(facilities).toHaveLength(3);
    expect(new Set(facilities.map((facility) => facility.facility_id)).size).toBe(3);
    expect(facilities.map((facility) => ({
      inventoryRevision: facility.inventory_revision,
      remaining: facility.available_units_by_kind?.data,
      epoch: facility.throughput_epoch,
      throughputRemaining: facility.throughput_remaining_units,
      throughputLimit: facility.throughput_limit_units_per_epoch,
    }))).toEqual(expect.arrayContaining([
      expect.objectContaining({
        inventoryRevision: expect.any(Number),
        remaining: 8,
        epoch: expect.any(Number),
        throughputRemaining: 8,
        throughputLimit: 8,
      }),
      expect.objectContaining({
        inventoryRevision: expect.any(Number),
        remaining: 2,
        epoch: expect.any(Number),
        throughputRemaining: 2,
        throughputLimit: 8,
      }),
      expect.objectContaining({
        inventoryRevision: expect.any(Number),
        remaining: 0,
        epoch: expect.any(Number),
        throughputRemaining: 0,
        throughputLimit: 8,
      }),
    ]));

    const anchors = facilities.map((facility) => {
      const location = fixture.model.locations[facility.location_id];
      expect(location).toEqual(expect.objectContaining({
        pos: expect.objectContaining({ x_cm: expect.any(Number), y_cm: expect.any(Number) }),
      }));
      return `${location.pos.x_cm}:${location.pos.y_cm}`;
    });
    expect(new Set(anchors).size).toBe(3);
  });

  it("injects the two player-visible recent-event kinds only for the glyph visual fixture", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=recent_event_glyphs");

    expect(installPixelWorldVisualFixtureHook()).toBe("recent_event_glyphs");
    expect(core.state.recentEvents).toEqual([
      expect.objectContaining({ event_id: "resource-transfer-fixture", kind: "resource_transfer" }),
      expect.objectContaining({ event_id: "build-queue-fixture", kind: "build_queue" }),
    ]);
  });

  it("refuses to install the hotspot tooltip fixture when connect is not explicitly disabled", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&pixel_world_visual_fixture=hotspot_tooltip");

    expect(installPixelWorldVisualFixtureHook()).toBeNull();
  });

  it("rebinds the fixture Agent to a later local test session without changing global visibility rules", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=hotspot_tooltip");
    installPixelWorldVisualFixtureHook();
    core.state.auth = {
      ...core.state.auth,
      available: true,
      playerId: "local-test-player-fixture",
      publicKey: "fixture-public-key",
      source: "local_test_api_ephemeral",
      boundAgentId: null,
    };

    expect(window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT__()).toBe(true);
    expect(core.state.auth.boundAgentId).toBe("agent-0");
    expect(core.modelLists().agents.map((agent) => agent.id)).toContain("agent-0");
    expect(core.state.snapshot.model.agent_player_bindings["agent-0"]).toBe("local-test-player-fixture");
  });

  it("keeps every routes-and-events relation control visible after local auth alignment", () => {
    window.history.replaceState({}, "", "/viewer.html?test_api=1&connect=0&pixel_world_visual_fixture=routes_and_events");
    expect(installPixelWorldVisualFixtureHook()).toBe("routes_and_events");
    core.state.auth = {
      ...core.state.auth,
      available: true,
      playerId: "local-test-player-routes",
      publicKey: "routes-public-key",
      source: "local_test_api_ephemeral",
      boundAgentId: null,
    };

    expect(window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURE_AUTH_ALIGNMENT__()).toBe(true);
    expect(core.modelLists().agents.map((agent) => agent.id)).toEqual(expect.arrayContaining([
      "agent-route",
      "agent-unknown-route",
      "agent-stale-route",
      "agent-zero-route",
    ]));
  });
});
