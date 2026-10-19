import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { verifyHotspotCameraAndFocus } from "./pixel_world_hotspot_test_helpers.js";
import {
  HEAVY_UI_TEST_TIMEOUT_MS,
  buildTestRustRenderState,
  cleanupMountedHost,
  emptyWorldSnapshot,
  noReceiptSnapshot,
  renderPixelWorldHost,
  runtimeMock,
  sampleSnapshot,
  testHarness,
  useTestRustRenderState,
} from "./pixel_world_host_test_support.jsx";
describe("pixel world host", () => {
  it("keeps world focus stage resets scoped away from nested command panels", () => {
    const html = readFileSync("viewer.html", "utf8");

    expect(html).toContain("body.pixel-world-focus-active .panel--stage > .panel__body");
    expect(html).toContain(".panel--stage > .panel__body");
    expect(html).not.toContain("body.pixel-world-focus-active .panel--stage .panel__body");
    expect(html).not.toContain(".panel--stage .panel__body {");
    expect(html).toMatch(/\.pixel-world-focus-hud__cell--tick\s*\{[^}]*grid-column:\s*3;/s);
    expect(html).toMatch(/\.pixel-world-focus-hud__cell--tick strong,[\s\S]*?\.pixel-world-focus-hud__cell--tick em\s*\{[^}]*white-space:\s*nowrap;/s);
    expect(html).toMatch(/\.pixel-world-focus-hud__cell--blocker::after,[\s\S]*?\.pixel-world-focus-hud__cell--receipt::after\s*\{[^}]*width:\s*3px;/s);
    expect(html).toMatch(/\.pixel-world-focus-hud__cell--blocker\[data-blocker-present="true"\]::after\s*\{[^}]*background:\s*var\(--bad\);/s);
    expect(html).toMatch(/\.pixel-world-focus-hud__cell--receipt\[data-hud-priority="receipt"\]::after\s*\{[^}]*background:\s*var\(--good\);/s);
    expect(html).toMatch(/\.pixel-world-focus-rail\s*\{[^}]*top:\s*112px;/s);
    expect(html).toContain("max-height: min(42vh, 340px);");
    expect(html).toContain(".pixel-world-focus-command-tray");
    expect(html).toMatch(/\.pixel-world-focus-minimap__node::before\s*\{[^}]*width:\s*7px;[^}]*height:\s*7px;/s);
    expect(html).toMatch(/\.pixel-world-focus-minimap__node--target::before\s*\{[^}]*background:\s*var\(--good\);/s);
    expect(html).toMatch(/\.pixel-world-focus-minimap__node--agent::before\s*\{[^}]*background:\s*var\(--accent\);/s);
    expect(html).toMatch(/\.pixel-world-focus-minimap__node--selected\s*\{[^}]*border-color:\s*rgba\(208,\s*168,\s*91,\s*0\.58\);/s);
    expect(html).toMatch(/\.pixel-world-focus-minimap__node--selected::before\s*\{[^}]*width:\s*18px;[^}]*height:\s*18px;[^}]*border:\s*1px solid rgba\(208,\s*168,\s*91,\s*0\.78\);/s);
  });

  it("exposes one decision area with one primary action, receipt, and canvas legend", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker",
    );

    await waitFor(() => {
      expect(document.querySelector('[data-viewer-decision-area="true"]')).toBeInTheDocument();
    });

    const decisionArea = document.querySelector('[data-viewer-decision-area="true"]');
    expect(decisionArea).toHaveAttribute("id", "viewer-decision-area");
    expect(decisionArea.querySelectorAll('[data-primary-action="true"]')).toHaveLength(1);
    expect(decisionArea.querySelector("#viewer-action-receipt")).toBeInTheDocument();
    expect(document.querySelectorAll('[data-pixel-world-legend="true"]')).toHaveLength(1);
    expect(document.querySelector('[data-pixel-world-legend="true"]')).toHaveTextContent(/route/i);
    expect(document.querySelector('[data-pixel-world-legend="true"]')).toHaveTextContent(/goal/i);
    expect(document.querySelector('[data-pixel-world-legend="true"]')).toHaveTextContent(/blocker/i);
    expect(document.querySelector('[data-pixel-world-legend="true"]')).toHaveTextContent(/resource/i);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders stable terse marker codes and exposes the selected full label in a safe callout", async () => {
    const snapshot = sampleSnapshot();
    snapshot.model.agents = {
      "agent-builder": { id: "agent-builder", name: "Shared Name", location_id: "loc-0", resources: {} },
      "agent-factory": { id: "agent-factory", name: "Shared Name", location_id: "loc-0", resources: {} },
    };
    snapshot.model.agent_player_bindings = {"agent-builder":"player-one", "agent-factory":"player-one"};
    snapshot.player_gameplay.intent_target = "agent-builder";
    useTestRustRenderState();
    await renderPixelWorldHost(snapshot);

    await waitFor(() => {
      expect(document.querySelectorAll('[data-pixel-world-agent-marker="true"]').length).toBeGreaterThanOrEqual(2);
    });
    const markers = [...document.querySelectorAll('[data-pixel-world-agent-marker="true"]')];
    const codes = markers.map((marker) => marker.querySelector(".pixel-world-entity__code")?.textContent);
    expect(new Set(codes).size).toBe(codes.length);
    expect(markers[0]).not.toHaveTextContent("Shared Name");
    expect(markers[0].querySelector(".pixel-world-entity__label")).not.toBeInTheDocument();
    expect(markers[0]).toHaveAttribute("title", "Shared Name");
    expect(markers[0]).toHaveAttribute("aria-label", "Select Agent Shared Name");
    expect(document.querySelector(".pixel-world-canvas__selection")).toHaveTextContent("Selected: Shared Name");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("selects keyboard reachable module markers and exposes readable current details", async () => {
    const snapshot = sampleSnapshot();
    snapshot.model.module_visual_entities = {
      "module-relay": {
        entity_id: "module-relay",
        module_id: "module-7",
        kind: "relay",
        label: "Relay Seven",
        anchor: {
          type: "absolute",
          data: { pos: { x_cm: 7_100_000, y_cm: 1_200_000, z_cm: 80 } },
        },
      },
    };
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      moduleVisualEntities: [{
        id: "module-relay",
        moduleId: "module-7",
        kind: "relay",
        label: "Relay Seven",
        anchor: snapshot.model.module_visual_entities["module-relay"].anchor,
        pos: { x_cm: 7_100_000, y_cm: 1_200_000, z_cm: 80 },
      }],
      selection: input.selectedKind && input.selectedId
        ? { kind: input.selectedKind, id: input.selectedId }
        : { kind: "agent", id: "agent-0" },
    }));

    const { core } = await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");
    await waitFor(() => {
      expect(document.querySelector('[data-pixel-world-module-marker="true"]')).toBeInTheDocument();
    });
    const detailsPanel = document.createElement("section");
    detailsPanel.id = "viewer-details-panel";
    detailsPanel.tabIndex = -1;
    document.body.appendChild(detailsPanel);
    const marker = document.querySelector('[data-pixel-world-module-marker="true"]');
    expect(marker).toBeInstanceOf(HTMLButtonElement);
    expect(marker).not.toBeDisabled();
    expect(marker).toHaveAttribute("aria-label", "Select Module Relay Seven");

    fireEvent.click(marker);
    await waitFor(() => expect(core.state.selectedKind).toBe("module_visual"));
    expect(core.state.selectedId).toBe("module-relay");
    expect(core.state.selectedObject).toEqual(expect.objectContaining({
      id: "module-relay",
      module_id: "module-7",
      kind: "relay",
      label: "Relay Seven",
    }));
    await waitFor(() => expect(document.querySelector(".pixel-world-canvas__selection")).toHaveTextContent("Relay Seven"));
    expect(core.state.selectedObject.anchor).toEqual(snapshot.model.module_visual_entities["module-relay"].anchor);
    expect(window.location.hash).toBe("#viewer-details-panel");
    expect(document.activeElement).toBe(detailsPanel);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("highlights only routes with explicit selected endpoint ids", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      selection: { kind: "agent", id: "agent-0" },
      links: [
        { id: "link-associated", kind: "agent_assignment", from: { x_cm: 1, y_cm: 1 }, to: { x_cm: 2, y_cm: 2 }, agent_id: "agent-0", location_id: "loc-0", status: "active", source_class: "runtime_projection", freshness: "current" },
        { id: "link-unassociated", kind: "agent_assignment", from: { x_cm: 3, y_cm: 3 }, to: { x_cm: 4, y_cm: 4 } },
      ],
    }));
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker",
    );

    await waitFor(() => {
      expect(document.querySelectorAll(".pixel-world-route")).toHaveLength(2);
    });
    const associated = document.querySelector('[data-route-id="link-associated"]');
    const unassociated = document.querySelector('[data-route-id="link-unassociated"]');
    expect(associated).toHaveAttribute("data-associated", "true");
    expect(unassociated).toHaveAttribute("data-associated", "false");
    expect(unassociated).toHaveClass("pixel-world-route--muted");
    expect(document.querySelectorAll(".pixel-world-route[role='button'], .pixel-world-route button")).toHaveLength(0);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("associates only known current runtime links when matching endpoint ids are present", async () => {
    const authority = { status: "active", source_class: "runtime_projection", freshness: "current" };
    const endpoint = { agent_id: "agent-0", location_id: "loc-0" };
    const geometry = { from: { x_cm: 1, y_cm: 1 }, to: { x_cm: 2, y_cm: 2 } };
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      selection: { kind: "agent", id: "agent-0" },
      links: [
        { id: "link-assignment", kind: "agent_assignment", ...geometry, ...endpoint, ...authority },
        { id: "link-route", kind: "route", ...geometry, ...endpoint, ...authority },
        { id: "link-logistics-route", kind: "logistics_route", ...geometry, ...endpoint, ...authority },
        { id: "link-resource-flow", kind: "resource_flow", ...geometry, ...endpoint, ...authority },
        { id: "link-unknown", kind: "unknown", ...geometry, ...endpoint, ...authority },
        { id: "link-stale", kind: "route", ...geometry, ...endpoint, ...authority, freshness: "stale" },
        { id: "link-local-pending", kind: "resource_flow", ...geometry, ...endpoint, ...authority, source_class: "local_pending" },
        { id: "link-nonruntime", kind: "logistics_route", ...geometry, ...endpoint, ...authority, source_class: "viewer_cache" },
      ],
    }));
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker",
    );

    await waitFor(() => {
      expect(document.querySelectorAll(".pixel-world-route")).toHaveLength(8);
    });
    for (const id of ["link-assignment", "link-route", "link-logistics-route", "link-resource-flow"]) {
      expect(document.querySelector(`[data-route-id="${id}"]`)).toHaveAttribute("data-associated", "true");
    }
    for (const id of ["link-unknown", "link-stale", "link-local-pending", "link-nonruntime"]) {
      expect(document.querySelector(`[data-route-id="${id}"]`)).toHaveAttribute("data-associated", "false");
      expect(document.querySelector(`[data-route-id="${id}"]`)).toHaveClass("pixel-world-route--muted");
    }
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("associates current generic relation links by their canonical agent-location id", async () => {
    const authority = { kind: "logistics_route", status: "active", source_class: "runtime_projection", freshness: "current" };
    runtimeMock.deriveRenderState = vi.fn((input) => {
      const state = buildTestRustRenderState(input);
      state.agents = [{ ...state.agents[0], location_id: "loc-0", relation: authority }];
      return {
        ...state,
        selection: { kind: "agent", id: "agent-0" },
        links: [{
          id: "link:agent-0:loc-0",
          kind: authority.kind,
          from: { x_cm: 1, y_cm: 1 },
          to: { x_cm: 2, y_cm: 2 },
          status: authority.status,
          source_class: authority.source_class,
          freshness: authority.freshness,
        }],
      };
    });
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker",
    );

    await waitFor(() => {
      expect(document.querySelector('[data-route-id="link:agent-0:loc-0"]')).toHaveAttribute("data-associated", "true");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("associates authoritative host relation projections by their canonical link id", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => {
      const authoritative = { kind: "agent_assignment", status: "active", source_class: "runtime_projection", freshness: "current" };
      const state = buildTestRustRenderState(input);
      state.agents = [
        { ...state.agents[0], location_id: "loc-0", relation: authoritative },
        { id: "agent-1", location_id: "loc-1", pos: { x_cm: 3, y_cm: 3 }, relation: authoritative },
      ];
      return {
      ...state,
      selection: { kind: "agent", id: "agent-0" },
      links: [
        {
          id: "link:agent-0:loc-0",
          kind: "agent_assignment",
          from: { x_cm: 1, y_cm: 1 },
          to: { x_cm: 2, y_cm: 2 },
          status: "active",
          source_class: "runtime_projection",
          freshness: "current",
        },
        {
          id: "link:agent-1:loc-1",
          kind: "agent_assignment",
          from: { x_cm: 3, y_cm: 3 },
          to: { x_cm: 4, y_cm: 4 },
          status: "active",
          source_class: "runtime_projection",
          freshness: "current",
        },
      ],
      };
    });
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");
    document.body.setAttribute("data-viewer-visual-fixture", "authoritative-relation");
    core.requestRender();

    await waitFor(() => {
      expect(document.querySelectorAll(".pixel-world-route")).toHaveLength(2);
    });
    expect(document.querySelector('[data-route-id="link:agent-0:loc-0"]')).toHaveAttribute("data-associated", "true");
    expect(document.querySelector('[data-route-id="link:agent-1:loc-1"]')).toHaveAttribute("data-associated", "false");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps accepted intent wording out of a blocked receipt", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => {
      const state = buildTestRustRenderState(input);
      state.commercial_surface.action_receipt = {
        present: true,
        state: "blocked",
        confidence: "accepted_intent",
        title: "Action blocked",
        summary: "The action was blocked by a material shortage.",
        detail: "No world change was confirmed.",
        target_agent_id: "agent-0",
      };
      return state;
    });
    await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");

    await waitFor(() => {
      expect(document.querySelector("#viewer-action-receipt")).toHaveTextContent("Action blocked");
    });
    const receipt = document.querySelector("#viewer-action-receipt");
    expect(receipt).not.toHaveTextContent("Action accepted");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps marker codes unique across the first-screen agent and location caps", async () => {
    const snapshot = sampleSnapshot();
    snapshot.model.agents = Object.fromEntries(Array.from({ length: 10 }, (_, index) => [
      `agent-${index}`,
      { id: `agent-${index}`, name: index % 2 ? "Shared Long Agent Name" : "共享长名称行动体", location_id: `loc-${index % 8}`, resources: {} },
    ]));
    snapshot.model.locations = Object.fromEntries(Array.from({ length: 8 }, (_, index) => [
      `loc-${index}`,
      { id: `loc-${index}`, name: index % 2 ? "Shared Long Location Name" : "共享长名称地点", pos: { x_cm: 100_000 + index * 80_000, y_cm: 100_000 + index * 80_000, z_cm: 0 }, resources: {} },
    ]));
    snapshot.model.agent_player_bindings = Object.fromEntries(Object.keys(snapshot.model.agents).map(id => [id,"player-one"]));
    snapshot.player_gameplay.intent_target = "agent-0";
    useTestRustRenderState();
    const { core } = await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");
    document.body.setAttribute("data-viewer-visual-fixture", "long-identity");
    core.requestRender();

    await waitFor(() => {
      expect(document.querySelectorAll('[data-pixel-world-agent-marker="true"]:not(.pixel-world-entity--canvas-hit-target)')).toHaveLength(10);
      expect(document.querySelectorAll('[data-pixel-world-location-marker="true"]')).toHaveLength(8);
    });
    const codes = [...document.querySelectorAll(".pixel-world-entity:not(.pixel-world-entity--canvas-hit-target) .pixel-world-entity__code")].map((node) => node.textContent);
    expect(new Set(codes).size).toBe(codes.length);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps the primary action hit area at least 44px on mobile and exposes pending state in place", async () => {
    const css = readFileSync("viewer_terminal_shell.css", "utf8");
    expect(css).toMatch(/@media\s*\(max-width:\s*1240px\)[\s\S]*?\.pixel-world-command-cell__action\s*\{[^}]*min-height:\s*44px;/);

    useTestRustRenderState();
    await renderPixelWorldHost(sampleSnapshot());
    await waitFor(() => expect(document.querySelector('[data-primary-action="true"]')).toBeInTheDocument());
    const primaryAction = document.querySelector('[data-primary-action="true"]');
    expect(primaryAction).toHaveAttribute("aria-live", "polite");
    expect(primaryAction.closest('[data-viewer-decision-area="true"]')).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("resolves claim onboarding next moves to executable gameplay actions", async () => {
    const { resolvePixelWorldDirectNextMoveAction } = await import("./pixel_world_host.jsx");
    const gameplay = {
      availableActions: [
        {
          actionId: "claim_first_agent",
          executeKind: "claim_first_agent",
          label: "Claim First Agent",
        },
      ],
    };

    expect(resolvePixelWorldDirectNextMoveAction(gameplay, "claim_first_agent")).toEqual(
      gameplay.availableActions[0],
    );
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not directly execute disabled or generic pixel world next moves", async () => {
    const { resolvePixelWorldDirectNextMoveAction } = await import("./pixel_world_host.jsx");
    const gameplay = {
      availableActions: [
        {
          actionId: "claim_first_agent",
          executeKind: "claim_first_agent",
          disabledReason: "already claimed",
        },
        {
          actionId: "build_factory_smelter_mk1",
          executeKind: "gameplay_action",
        },
      ],
    };

    expect(resolvePixelWorldDirectNextMoveAction(gameplay, "claim_first_agent")).toBeNull();
    expect(resolvePixelWorldDirectNextMoveAction(gameplay, "gameplay_action")).toBeNull();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses authoritative world_read for the optional player HUD without ambient topology/debug counts", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      commercial_surface: {
        ...buildTestRustRenderState(input).commercial_surface,
        world_read: {
          tick: 12,
          agents: 7,
          routes: 99,
          fragments: 101,
          hotspots: 103,
        },
      },
    }));

    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const worldHud = document.querySelector('[data-viewer-overlay="world-hud"]');
    expect(worldHud).toBeTruthy();
    const readout = worldHud.querySelector(".pixel-world-readout");
    expect(readout).toHaveTextContent("agents=7");
    expect(readout).not.toHaveTextContent(/routes=|fragments=|hotspots=|renderer=|runtime=/i);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("refreshes connection and feed badges across updates without remounting", async () => {
    useTestRustRenderState();
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(document.querySelector(".pixel-world-readout")).not.toBeNull());
    const connection = document.querySelector("[data-world-connection-status]");
    const feed = document.querySelector("[data-world-feed-readout-status]");
    for (const [connectionStatus, status, stale, connectionLabel, feedLabel] of [
      ["connecting", "loading", false, "CONNECTING", "SYNCING"],
      ["connected", "ready", false, "ONLINE", "LIVE"],
      ["closed", "ready", true, "CLOSED", "STALE"],
    ]) {
      core.state.connectionStatus = connectionStatus;
      Object.assign(core.state.worldFeed, { status, stale });
      core.requestRender();
      await waitFor(() => {
        expect(document.querySelector("[data-world-connection-status]")).toBe(connection);
        expect(document.querySelector("[data-world-feed-readout-status]")).toBe(feed);
        expect(connection).toHaveTextContent(`World connection: ${connectionLabel}`);
        expect(feed).toHaveTextContent(`Feed freshness: ${feedLabel}`);
        expect(connection).toHaveClass(`pixel-world-readout__connection--${connectionLabel.toLowerCase()}`);
        expect(feed).toHaveClass(`pixel-world-readout__feed--${feedLabel.toLowerCase()}`);
        expect(connection).toHaveAttribute("data-world-connection-status", connectionLabel.toLowerCase());
        expect(feed).toHaveAttribute("data-world-feed-readout-status", status);
      });
    }
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not label a disconnected or stale world as LIVE", async () => {
    useTestRustRenderState();
    const { core } = await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });
    core.state.connectionStatus = "connecting";
    core.state.worldFeed.stale = true;
    core.requestRender();

    await waitFor(() => {
      const readout = document.querySelector(".pixel-world-readout");
      expect(readout).not.toHaveTextContent(/\bLIVE\b/);
      expect(readout.querySelector(".badge--good")).toBeNull();
      expect(readout).toHaveTextContent(/stale|syncing|offline|reconnecting|unavailable/i);
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("flattens Shell Lite into one dominant Next Move and grouped supporting context", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");

    await waitFor(() => { expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument(); });

    const strip = document.querySelector('[data-viewer-overlay="next-move"]'); expect(strip).toBeTruthy();
    const primary = strip.querySelector('[data-shell-region="next-move-primary"]');
    const supporting = strip.querySelector('[data-shell-region="supporting-context"]');
    expect(primary).toBeTruthy(); expect(supporting).toBeTruthy();
    expect(primary.parentElement).toBe(strip); expect(supporting.parentElement).toBe(strip);
    expect(strip.children).toHaveLength(2);

    expect(primary).toHaveTextContent("Next Move"); expect(primary).toHaveTextContent("Build smelter mk1"); expect(primary).toHaveTextContent("Missing Material");
    const ctas = primary.querySelectorAll("a,button");
    expect(ctas).toHaveLength(1); expect(ctas[0]).toHaveAttribute("aria-label", expect.stringContaining("Next Move"));
    expect(strip.querySelectorAll("a,button")).toHaveLength(1);

    expect(supporting).toHaveTextContent("Objective"); expect(supporting).toHaveTextContent("Recover sustainable capability");
    expect(supporting).toHaveTextContent("Player Leverage"); expect(supporting).toHaveTextContent("Agent 0");
    expect(supporting.querySelectorAll("a,button")).toHaveLength(0);
    expect(supporting).not.toHaveTextContent(/Power|Data|Consensus|Trust|Economy/i);
    expect(strip).not.toHaveTextContent(/material_shortage|world_constraint|completed_no_progress/i);
    expect(screen.getByText("Action Receipt")).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("shows the explicit unavailable surface when renderer deferral is requested", async () => {
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en&pixel_world_renderer=defer");
    await waitFor(() => { expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument(); });
    expect(runtimeMock.mountCalls).toBe(0); expect(screen.getByText("World Command Board")).toBeInTheDocument();
    expect(screen.getAllByText(/pixel_world_renderer_deferred/i).length).toBeGreaterThan(0);
    expect(document.querySelector('[data-viewer-overlay="renderer-unavailable"]')).toHaveTextContent("Graphics unavailable in this browser");
    expect(document.querySelector('.pixel-world-render-diagnostics[data-renderer-state="unavailable"]')).toHaveTextContent("pixel_world_renderer_deferred");
    expect(screen.queryByText("Recover sustainable capability")).not.toBeInTheDocument(); expect(screen.queryByText("Build smelter mk1")).not.toBeInTheDocument(); expect(screen.queryByText("Action Receipt")).not.toBeInTheDocument();
    const diagnostics = screen.getByText("Renderer Diagnostics").closest("details"); expect(diagnostics.open).toBe(false); expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument();
    screen.getByRole("button", { name: "Reattach Embedded Renderer" }).click();
    await waitFor(() => { expect(screen.getAllByText(/pixel_world_render_state_unavailable/i).length).toBeGreaterThan(0); });
    expect(runtimeMock.mountCalls).toBe(0); expect(document.querySelectorAll(".pixel-world-fragment-terrain")).toHaveLength(0);
    expect(core.state.lastError).toContain("pixel world Rust render-state derivation is unavailable");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("fails closed when the embedded WebGL2 mount rejects instead of exposing a ready canvas", async () => {
    useTestRustRenderState();
    runtimeMock.mountError = new Error("canvas.getContext() returned null; webgl2 not available");

    const { core } = await renderPixelWorldHost();

    await waitFor(() => {
      expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument();
    });

    expect(document.querySelector(".pixel-world-render-diagnostics[data-renderer-state='unavailable']")).toHaveTextContent(
      /canvas\.getContext\(\) returned null; webgl2 not available/i,
    );
    expect(document.querySelector("[data-renderer-ready='true']")).toBeNull();
    expect(document.querySelector("#pixel-world-embedded-runtime-canvas")).toBeNull();
    expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable");
    expect(core.state.pixelWorldFatal).toEqual(expect.objectContaining({
      code: "pixel_world_webgl2_unavailable",
    }));
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("fails closed before bridge mount when the canvas cannot create a WebGL2 surface", async () => {
    useTestRustRenderState();
    testHarness.canvasContextSpy.mockReturnValue(null);

    try {
      const { core } = await renderPixelWorldHost();

      await waitFor(() => {
        expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument();
      });

      expect(runtimeMock.mountCalls).toBe(0);
      expect(document.querySelector("[data-renderer-ready='true']")).toBeNull();
      expect(document.querySelector("#pixel-world-embedded-runtime-canvas")).toBeNull();
      expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable");
      expect(core.state.pixelWorldFatal).toEqual(expect.objectContaining({
        code: "pixel_world_webgl2_unavailable",
      }));
    } finally {
      testHarness.canvasContextSpy.mockReturnValue({});
    }
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps unavailable copy player-readable while raw fatal details stay folded", async () => {
    useTestRustRenderState();
    testHarness.canvasContextSpy.mockReturnValue(null);

    const { container } = await renderPixelWorldHost();

    await waitFor(() => {
      expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument();
    });

    const fallback = container.querySelector('[data-viewer-overlay="renderer-unavailable"]');
    expect(fallback).toHaveTextContent("Graphics unavailable in this browser");
    expect(fallback).not.toHaveTextContent("pixel_world_webgl2_unavailable");

    const diagnostics = container.querySelector(".pixel-world-render-diagnostics");
    expect(diagnostics).toHaveAttribute("data-renderer-state", "unavailable");
    expect(diagnostics).toHaveTextContent("pixel_world_webgl2_unavailable");
    expect(diagnostics).toHaveProperty("open", false);

    const cinematicButton = screen.getByRole("button", { name: /Cinematic View.*unavailable/i });
    expect(cinematicButton).toBeDisabled();
    expect(cinematicButton).toHaveAttribute("aria-describedby", "pixel-world-renderer-unavailable-message");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders player-readable receipt state without leaking internal confidence enums", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(sampleSnapshot());

    await waitFor(() => {
      expect(screen.getByText("Action Receipt")).toBeInTheDocument();
    });

    const receipt = document.querySelector('[data-viewer-overlay="receipt"]');
    await waitFor(() => {
      expect(receipt).toHaveAttribute("data-receipt-confidence", "world_delta");
    });
    expect(receipt).not.toHaveTextContent(/\b(world_delta|accepted_intent|none)\b/i);
    expect(receipt).toHaveTextContent("Agent 0");
    expect(receipt).not.toHaveTextContent("agent=agent-0");
    expect(receipt).toHaveTextContent(/blocked|confirmed|waiting|queued|accepted/i);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps rejected receipts truthful on normal and compact surfaces", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => {
      const state = buildTestRustRenderState(input);
      state.commercial_surface.action_receipt = { present: true, state: "rejected", confidence: "none", title: "Request outcome", summary: "No world change was applied.", detail: "Runtime policy declined the request.", target_agent_id: null };
      return state;
    });
    await renderPixelWorldHost(sampleSnapshot());

    await waitFor(() => {
      const receipt = document.querySelector("#viewer-action-receipt");
      expect(receipt).toHaveTextContent("Action rejected");
      expect(receipt).not.toHaveTextContent("Waiting for confirmation");
    });

    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      const receipt = document.querySelector(".pixel-world-focus-receipt .pixel-world-action-receipt");
      expect(receipt).toHaveTextContent("Action rejected");
      expect(receipt).not.toHaveTextContent("Waiting for confirmation");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not bind raw receipt confidence enums directly into player-visible markup", () => {
    const source = readFileSync("software_safe_src/pixel_world_host.jsx", "utf8");
    expect(source).not.toMatch(/<span>\{receipt\(\)\.confidence\}<\/span>/);
    expect(source).not.toMatch(/<em>\{surface\(\)\.action_receipt\.confidence\}<\/em>/);
  });

  it("exposes renderer recovery in the unavailable surface instead of Diagnostics only", async () => {
    const { core } = await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_renderer=defer",
    );

    await waitFor(() => {
      expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument();
    });

    const fallback = document.querySelector('[data-viewer-overlay="renderer-unavailable"]');
    const retryButton = fallback?.querySelector("button");
    expect(retryButton).not.toBeNull();
    expect(retryButton).toHaveTextContent(/Retry Renderer/i);
    expect(retryButton.closest("details")).toBeNull();
    expect(core.state.pixelWorldRuntimeStatus).toBe("unavailable");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("reattaches the deferred renderer only after explicit retry", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => buildTestRustRenderState(input));
    const { core } = await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en&pixel_world_renderer=defer");
    await waitFor(() => { expect(screen.getByText("Graphics unavailable in this browser")).toBeInTheDocument(); });
    expect(runtimeMock.mountCalls).toBe(0); expect(document.querySelector("#pixel-world-embedded-runtime-canvas")).toBeNull();
    screen.getByRole("button", { name: "Retry Renderer" }).click();
    await waitFor(() => { expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument(); });
    expect(runtimeMock.mountCalls).toBe(1); expect(core.state.pixelWorldRuntimeStatus).toBe("ready");
    expect(document.querySelector("#pixel-world-embedded-runtime-canvas")).toBeTruthy();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("auto-attaches the embedded renderer for test_api pages unless deferral is explicit", async () => {
    const { core } = await renderPixelWorldHost();

    await waitFor(() => {
      expect(screen.getAllByText(/pixel_world_render_state_unavailable/i).length).toBeGreaterThan(0);
    });

    expect(runtimeMock.mountCalls).toBe(0);
    expect(core.state.lastError).toContain("pixel world Rust render-state derivation is unavailable");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses Rust-derived render state from the runtime module when available", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      locale: input.locale,
      worldBounds: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 },
      locations: [{
        id: "loc-0",
        label: "Factory Anchor",
        pos: { x_cm: 5_000_000, y_cm: 2_500_000, z_cm: 0 },
        markerRole: "logic_anchor",
        markerAlpha: 0.32,
      }],
      fragmentTerrain: [{
        id: "fragment:loc-0:0",
        locationId: "loc-0",
        pos: { x_cm: 5_000_000, y_cm: 2_500_000, z_cm: 0 },
        dominantCompound: "silicate_matrix",
        footprintCm: 12_000,
        color: [126, 144, 99],
      }],
      agents: [{
        id: "agent-0",
        label: "Agent 0",
        pos: { x_cm: 5_020_000, y_cm: 2_510_000, z_cm: 0 },
        positionSource: "location_derived",
      }, {
        id: "agent-1",
        label: "Agent 1",
        pos: { x_cm: 5_020_000, y_cm: 2_510_000, z_cm: 0 },
        positionSource: "location_derived",
      }],
      links: [{
        id: "link:agent-0:loc-0",
        kind: "agent_assignment",
        from: { x_cm: 5_020_000, y_cm: 2_510_000, z_cm: 0 },
        to: { x_cm: 5_000_000, y_cm: 2_500_000, z_cm: 0 },
        emphasis: 0.72,
      }],
      selection: { kind: "agent", id: "agent-0" },
      goalHighlight: {
        title: "Rust derived goal",
        objective: "Rust objective detail",
      },
      blockerHighlight: null,
      recentEventHotspots: [],
      visualHotspots: [],
      commercial_surface: {
        objective: {
          title: "Rust derived goal",
          detail: "Rust objective detail",
          progress_percent: null,
        },
        next_action: {
          label: "Rust next move",
          detail: null,
          target_agent_id: null,
          execute_kind: null,
        },
        active_agent_id: null,
        player_leverage: {
          state: "waiting_for_intent",
          label: "Waiting for Intent",
          summary: "Rust leverage summary",
          detail: null,
        },
        action_receipt: {
          present: false,
          state: "waiting_for_intent",
          confidence: "none",
          title: "No action receipt yet",
          summary: "Rust receipt summary",
          detail: null,
          target_agent_id: null,
          effect_kind: null,
          delta_logical_time: null,
          delta_event_seq: null,
        },
        blocker: {
          label: null,
          detail: null,
        },
        world_read: {
          agents: 0,
          routes: 0,
          fragments: 0,
          hotspots: 0,
        },
      },
      presentation: {
        world_bounds_label: "rust bounds",
        marker_truth_note: "rust truth",
      },
    }));

    await renderPixelWorldHost();
    screen.getByRole("button", { name: "Reattach Embedded Renderer" }).click();

    await waitFor(() => {
      expect(screen.getByText("Rust derived goal")).toBeInTheDocument();
    });
    expect(screen.getByText("Rust next move")).toBeInTheDocument();
    expect(screen.getByText("Rust leverage summary")).toBeInTheDocument();
    expect(document.querySelector(".pixel-world-readout")).toHaveTextContent("tick=12");
    expect(document.querySelector(".pixel-world-readout [data-world-tick='12']")).toHaveTextContent("tick=12");
    expect(document.querySelector(".pixel-world-readout [data-world-connection-status]")).toHaveTextContent(/World connection:/i);
    expect(document.querySelector(".pixel-world-readout [data-world-feed-readout-status]")).toHaveTextContent(/Feed freshness:/i);
    await waitFor(() => {
      expect(document.querySelector(".pixel-world-canvas--rendered")).toBeInTheDocument();
    });
    const canvas = document.querySelector(".pixel-world-canvas--rendered");
    expect(canvas.querySelectorAll(".pixel-world-fragment-terrain")).toHaveLength(0);
    expect(canvas.querySelector(".pixel-world-entity--location")).toBeNull();
    const agentMarker = canvas.querySelector("[data-pixel-world-agent-marker='true'][data-agent-id='agent-0']");
    const secondAgentMarker = canvas.querySelector("[data-pixel-world-agent-marker='true'][data-agent-id='agent-1']");
    expect(agentMarker).not.toBeNull();
    expect(secondAgentMarker).not.toBeNull();
    expect(agentMarker).toHaveAttribute("aria-label", "Select Agent Agent 0");
    expect(secondAgentMarker).toHaveAttribute("aria-label", "Select Agent Agent 1");
    expect(agentMarker.style.transform).not.toEqual(secondAgentMarker.style.transform);
    agentMarker.click();
    expect(canvas.querySelector(".pixel-world-canvas__selection")).toHaveTextContent("Selected: Agent 0");
    expect(canvas.querySelector(".pixel-world-canvas__selection")).not.toHaveTextContent("agent/agent-0");
    expect(canvas.querySelector(".pixel-world-route")).toBeNull();
    expect(canvas.querySelector(".pixel-world-canvas__selection")).toHaveTextContent("Selected: Agent 0");
    expect(runtimeMock.deriveRenderState).toHaveBeenCalled();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps sparse-scene guidance honest and read-only", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      links: [],
      fragmentTerrain: [],
      fragment_terrain: [],
      locations: [{ id: "loc-0", label: "Origin", pos: { x_cm: 1, y_cm: 2, z_cm: 0 } }],
      agents: [{ id: "agent-0", label: "Agent 0", pos: { x_cm: 3, y_cm: 4, z_cm: 0 } }],
    }));
    await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(document.querySelector("[data-pixel-world-sparse-guidance='true']")).toBeInTheDocument());
    const guidance = document.querySelector("[data-pixel-world-sparse-guidance='true']");
    expect(guidance).toHaveTextContent("No published routes in this snapshot");
    expect(guidance).toHaveTextContent("No published terrain in this snapshot");
    expect(guidance).toHaveTextContent("Published bounds:");
    expect(guidance.querySelectorAll("button, a")).toHaveLength(0);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses a safe generic label when an unknown blocker reaches the host", async () => {
    useTestRustRenderState();
    const snapshot = sampleSnapshot();
    snapshot.player_gameplay.blocker_kind = "unknown_internal_code";
    snapshot.player_gameplay.blocker_detail = "diagnostic only";
    await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");
    await waitFor(() => expect(document.querySelector("[data-shell-region='next-move-primary']")).toBeInTheDocument());
    const primary = document.querySelector("[data-shell-region='next-move-primary']");
    expect(primary).toHaveTextContent("Current blocker");
    expect(primary).not.toHaveTextContent("unknown_internal_code");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps unknown blocker codes out of the Cinematic command chip", async () => {
    useTestRustRenderState();
    const snapshot = sampleSnapshot();
    snapshot.player_gameplay.blocker_kind = "unknown_internal_code";
    snapshot.player_gameplay.blocker_detail = "diagnostic only";
    await renderPixelWorldHost(snapshot, "?test_api=1&connect=0&locale=en");

    await waitFor(() => expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument());
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => expect(document.querySelector(".pixel-world-host")).toHaveAttribute("data-world-focus", "true"));
    screen.getByRole("button", { name: "Command & Target" }).click();

    const commandChip = document.querySelector(".pixel-world-focus-command-chip--blocker");
    expect(commandChip).toHaveTextContent("Current blocker");
    expect(commandChip).not.toHaveTextContent("unknown_internal_code");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps read-only hotspot controls available in production and resolves tooltip locale", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      visualHotspots: [{
        id: "hotspot-blocker",
        kind: "blocker",
        label: "缺料阻塞",
        pos: { x_cm: 5_020_000, y_cm: 2_510_000, z_cm: 0 },
        sizeHintPx: 20,
      }, { id: "hotspot-goal", kind: "goal", label: "稳定生产", pos: { x_cm: 6_000_000, y_cm: 2_000_000, z_cm: 0 } }],
    }));

    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=zh-CN",
      "zh-CN",
    );

    const marker = await screen.findByRole("button", { name: /阻塞热点：缺料阻塞/ });
    expect(marker).toHaveProperty("tabIndex", 0);
    await verifyHotspotCameraAndFocus(marker, runtimeMock.onEvent);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps hotspot controls painted above decorative route waypoints", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      ...buildTestRustRenderState(input),
      visualHotspots: [{ id: "hotspot-goal", kind: "goal", label: "stabilize the first production line", pos: { x_cm: 5_020_000, y_cm: 2_510_000, z_cm: 0 } }],
    }));

    await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker");

    const marker = await screen.findByRole("button", { name: /Goal hotspot/ });
    const waypoint = document.querySelector(".pixel-world-route-waypoint--target");
    expect(waypoint).not.toBeNull();
    expect(Boolean(waypoint.compareDocumentPosition(marker) & Node.DOCUMENT_POSITION_FOLLOWING)).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("makes the rendered canvas focusable with a read-only accessible world description", async () => {
    runtimeMock.deriveRenderState = vi.fn((input) => ({
      locale: input.locale,
      worldBounds: { width_cm: 10_000_000, depth_cm: 5_000_000, height_cm: 1_000_000 },
      locations: [],
      fragmentTerrain: [],
      agents: [],
      links: [],
      selection: null,
      goalHighlight: null,
      blockerHighlight: null,
      recentEventHotspots: [],
      visualHotspots: [],
      commercial_surface: {
        objective: { title: "Accessible canvas goal", detail: "Readable adjacent HUD", progress_percent: null },
        next_action: { label: "Inspect world", detail: null, target_agent_id: null, execute_kind: null },
        active_agent_id: null,
        player_leverage: { state: "waiting_for_intent", label: "Waiting for Intent", summary: "Waiting", detail: null },
        action_receipt: {
          present: false,
          state: "waiting_for_intent",
          confidence: "none",
          title: "No action receipt yet",
          summary: "No receipt",
          detail: null,
          target_agent_id: null,
          effect_kind: null,
          delta_logical_time: null,
          delta_event_seq: null,
        },
        blocker: { label: null, detail: null },
        world_read: { agents: 0, routes: 0, fragments: 0, hotspots: 0 },
      },
      presentation: { world_bounds_label: "bounds", marker_truth_note: "truth" },
    }));

    await renderPixelWorldHost();
    screen.getByRole("button", { name: "Reattach Embedded Renderer" }).click();

    const canvas = await screen.findByRole("img", { name: "World canvas overview" });
    expect(canvas).toHaveAttribute("tabindex", "0");
    expect(canvas).toHaveAttribute("aria-describedby", "pixel-world-canvas-accessible-summary");
    expect(document.getElementById("pixel-world-canvas-accessible-summary")).toHaveTextContent(/read-only overview/i);
    expect(document.getElementById("pixel-world-canvas-accessible-summary")).toHaveTextContent(/adjacent HUD/i);
    canvas.focus();
    expect(document.activeElement).toBe(canvas);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders the no-receipt fallback without implying an active agent caused progress", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(noReceiptSnapshot());

    await waitFor(() => {
      expect(screen.getByText("No action receipt yet")).toBeInTheDocument();
    });

    const receipt = document.querySelector(".pixel-world-action-receipt");
    expect(screen.getByText("Action Receipt")).toBeInTheDocument();
    expect(screen.getByText("No action receipt yet")).toBeInTheDocument();
    expect(screen.getByText("No player-caused world change has been confirmed yet.")).toBeInTheDocument();
    expect(receipt).toHaveAttribute("data-receipt-present", "false");
    expect(receipt).toHaveAttribute("data-receipt-state", "waiting_for_intent");
    expect(receipt).toHaveAttribute("data-receipt-confidence", "none");
    expect(receipt.querySelector(".pixel-world-action-receipt__meta")).toBeNull();
    expect(receipt.textContent).not.toContain("agent=agent-0");
  });

  it("publishes the stable Action Receipt anchor for the player-causality surface", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(noReceiptSnapshot());

    await waitFor(() => {
      expect(screen.getByText("No action receipt yet")).toBeInTheDocument();
    });

    expect(document.querySelectorAll("#viewer-action-receipt")).toHaveLength(1);
    expect(document.querySelector("#viewer-action-receipt")).toHaveClass("pixel-world-action-receipt");
  });

  it("publishes stable Focus exit, command-console, and drawer anchors", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(document.querySelector(".pixel-world-host")).toHaveAttribute("data-world-focus", "true");
    });

    expect(document.querySelector("#viewer-focus-exit")).toBeInTheDocument();
    expect(document.querySelector("#viewer-focus-command-drawer")).toBeInTheDocument();
    expect(document.querySelector("#viewer-focus-diagnostics-drawer")).toBeInTheDocument();
    expect(document.querySelector("#viewer-command-console")).toBeInTheDocument();
    expect(document.querySelector("#viewer-command-console").closest("#viewer-focus-command-drawer")).toBeTruthy();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses Cinematic View / Exit Cinematic as the player-facing presentation labels", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(document.querySelector(".pixel-world-host")).toHaveAttribute("data-world-focus", "true");
    });
    expect(screen.getByRole("button", { name: "Exit Cinematic" })).toBeInTheDocument();
    expect(screen.queryByText("World Focus")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("closes a local Cinematic drawer before Escape exits the presentation", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    const entry = screen.queryByRole("button", { name: "Cinematic View" })
      || screen.getByRole("button", { name: "Cinematic View" });
    entry.click();
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });

    const commandInvoker = screen.getByRole("button", { name: "Command & Target" });
    const commandDrawer = document.querySelector(".pixel-world-focus-drawer--command");
    commandInvoker.click();
    expect(commandDrawer).toHaveProperty("open", true);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(host).toHaveAttribute("data-world-focus", "true");
    expect(commandDrawer).toHaveProperty("open", false);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "false");
    });
    expect(screen.getByRole("button", { name: "Cinematic View" })).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("closes an open local Focus drawer before exiting Focus and restores the drawer invoker", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });

    const commandInvoker = screen.getByRole("button", { name: "Command & Target" });
    const commandDrawer = document.querySelector(".pixel-world-focus-drawer--command");
    commandInvoker.focus();
    commandInvoker.click();
    expect(commandDrawer).toHaveProperty("open", true);

    commandInvoker.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(host).toHaveAttribute("data-world-focus", "true");
    expect(commandDrawer).toHaveProperty("open", false);
    expect(document.activeElement).toBe(commandInvoker);

    commandInvoker.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "false");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not close a local Focus drawer when Escape is consumed by an IME composition", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });

    const commandDrawer = document.querySelector(".pixel-world-focus-drawer--command");
    const commandInvoker = screen.getByRole("button", { name: "Command & Target" });
    commandInvoker.focus();
    commandInvoker.click();
    expect(commandDrawer).toHaveProperty("open", true);

    commandInvoker.dispatchEvent(new KeyboardEvent("keydown", {
      key: "Escape",
      isComposing: true,
      bubbles: true,
    }));

    expect(host).toHaveAttribute("data-world-focus", "true");
    expect(commandDrawer).toHaveProperty("open", true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("returns keyboard focus to the Cinematic View invoker after presentation exits", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    const focusInvoker = screen.getByRole("button", { name: "Cinematic View" });
    focusInvoker.focus();
    focusInvoker.click();
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });

    const exitButton = screen.getByRole("button", { name: "Exit Cinematic" });
    exitButton.focus();
    exitButton.click();
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "false");
    });
    expect(document.activeElement).toBe(focusInvoker);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps secondary focus controls in a collapsed native mobile disclosure without changing their actions", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });

    const controls = document.querySelector(".pixel-world-focus-controls");
    const commandButton = screen.getByRole("button", { name: "Command & Target" });
    const moreControls = screen.getByText("More controls").closest("details");
    expect(moreControls).toBeTruthy();
    expect(moreControls).not.toHaveAttribute("open");
    expect(controls).toContainElement(commandButton);
    expect(commandButton).toHaveClass("pixel-world-focus-control--primary");
    expect(moreControls).toContainElement(screen.getByRole("button", { name: "World Status" }));
    expect(moreControls).toContainElement(screen.getByRole("button", { name: "Maximize" }));
    expect(controls).toContainElement(screen.getByRole("button", { name: "Exit Cinematic" }));
    expect(moreControls).not.toContainElement(screen.getByRole("button", { name: "Exit Cinematic" }));

    screen.getByRole("button", { name: "World Status" }).click();
    const diagnosticsDrawer = document.querySelector(".pixel-world-focus-drawer--diagnostics");
    expect(diagnosticsDrawer).toHaveProperty("open", true);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(host).toHaveAttribute("data-world-focus", "true");
    expect(diagnosticsDrawer).toHaveProperty("open", false);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "false");
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("offers an app-level world focus mode with command and diagnostics drawers", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    expect(host).toHaveAttribute("data-world-focus", "false");
    expect(screen.queryByText("World Focus")).not.toBeInTheDocument();

    const worldFocusButton = screen.getByRole("button", { name: "Cinematic View" });
    expect(screen.getByText("Pan, zoom, and inspect the world")).toBeInTheDocument();
    expect(worldFocusButton).toHaveAccessibleDescription("Pan, zoom, and inspect the world");

    worldFocusButton.click();

    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });
    expect(document.body).toHaveClass("pixel-world-focus-active");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Cinematic View");
    expect(screen.queryByText("No blocker")).not.toBeInTheDocument();
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Current Objective");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Recover sustainable capability");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Build smelter mk1");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Missing Material");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Mission Progress");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("World Tick");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("tick=12");
    expect(document.querySelector(".pixel-world-focus-hud")).not.toHaveTextContent("Receipt");
    expect(document.querySelector(".pixel-world-focus-hud")).not.toHaveTextContent("Action blocked");
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("68%");
    expect(document.querySelector(".pixel-world-focus-hud")).not.toHaveTextContent("Next Move");
    expect(document.querySelector(".pixel-world-focus-rail")).toHaveTextContent("Agent 0");
    expect(document.querySelector(".pixel-world-focus-rail")).not.toHaveTextContent("agent-0");
    expect(document.querySelector(".pixel-world-focus-rail")).toHaveTextContent("Routes");
    expect(document.querySelector(".pixel-world-focus-rail")).toHaveTextContent("Missing Material");
    expect(document.querySelectorAll(".pixel-world-focus-rail__item")[0]).toHaveClass("pixel-world-focus-rail__item--blocker");
    expect(document.querySelector(".pixel-world-focus-rail__item--blocker")).toHaveAttribute("data-focus-priority", "blocker");
    expect(document.querySelector(".pixel-world-focus-rail__item--blocker")).toHaveTextContent("Missing Material");
    expect(
      document.querySelector(".pixel-world-focus-rail__item--blocker").compareDocumentPosition(
        Array.from(document.querySelectorAll(".pixel-world-focus-rail__item")).find((item) => item.textContent.includes("Routes")),
      ) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(document.querySelector(".pixel-world-focus-receipt")).toHaveTextContent("Action blocked");
    expect(document.querySelector(".pixel-world-focus-receipt .pixel-world-action-receipt")).toHaveClass("pixel-world-action-receipt--focus-compact");
    expect(document.querySelector(".pixel-world-focus-receipt .pixel-world-action-receipt")).toHaveAttribute("data-receipt-confidence", "world_delta");
    expect(host).toHaveAttribute("data-focus-comparable", "true");
    expect(document.querySelector('[data-focus-cinematic="true"]')).toBeNull();
    expect(document.querySelector('[data-renderer-state="unavailable"]')).toBeNull();
    expect(document.querySelector(".pixel-world-canvas--rendered")).not.toBeNull();
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("Mission Map");
    expect(document.querySelector('[data-focus-minimap="true"]')).not.toHaveTextContent("ref: Factory Anchor");
    expect(document.querySelector(".pixel-world-focus-fallback-map__reference-marker")).toBeNull();
    expect(document.querySelector('[data-focus-minimap="true"] .sr-only')).toHaveTextContent("Reference: Factory Anchor");
    expect(document.querySelector(".pixel-world-focus-minimap__node--target")).not.toHaveTextContent("Anchor");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("Build smelter mk1");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("Agent 0");
    expect(document.querySelector('[data-focus-minimap="true"]')).not.toHaveTextContent("agent-0");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("agents=1");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("targets=1");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("routes=1");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("fragments=2");
    expect(document.querySelector(".pixel-world-focus-controls")).toHaveAttribute("aria-label", "Cinematic controls");
    expect(document.querySelector(".pixel-world-focus-controls")).toContainElement(screen.getByRole("button", { name: "Command & Target" }));
    expect(document.querySelector(".pixel-world-focus-hud__cell--prompt")).toHaveTextContent("Current Objective");
    expect(document.querySelector(".pixel-world-focus-hud__cell--tick")).toHaveAttribute("data-world-tick", "12");
    expect(document.querySelector(".pixel-world-focus-hud__cell--tick")).toHaveAttribute("data-hud-priority", "telemetry");
    expect(document.querySelector(".pixel-world-focus-hud__cell--blocker")).toHaveAttribute("data-blocker-present", "true");
    expect(document.querySelector(".pixel-world-focus-hud__cell--blocker")).toHaveAttribute("data-hud-priority", "critical");
    expect(document.querySelector(".pixel-world-focus-hud__cell--receipt")).toBeNull();
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveClass("pixel-world-focus-minimap");

    const commandDrawer = document.querySelector(".pixel-world-focus-drawer--command");
    const focusDiagnosticsDrawer = document.querySelector(".pixel-world-focus-drawer--diagnostics");
    expect(commandDrawer).toHaveProperty("open", false);
    expect(focusDiagnosticsDrawer).toHaveProperty("open", false);

    screen.getByRole("button", { name: "Command & Target" }).click();
    expect(commandDrawer).toHaveProperty("open", true);
    expect(focusDiagnosticsDrawer).toHaveProperty("open", false);
    expect(commandDrawer.querySelector(".pixel-world-focus-command-tray")).toHaveAttribute("data-chat-ready", "true");
    expect(commandDrawer.querySelector(".pixel-world-focus-command-chip--target")).toHaveTextContent("Agent 0");
    expect(commandDrawer).not.toHaveTextContent("agent=agent-0");
    expect(commandDrawer.querySelector(".pixel-world-focus-command-chip--blocker")).toHaveAttribute("data-blocker-present", "true");
    expect(commandDrawer.querySelector(".pixel-world-focus-command-chip--receipt")).toHaveTextContent("Blocked");
    expect(commandDrawer).toHaveTextContent("Agent Chat");
    expect(commandDrawer).toHaveTextContent("Command Surface");
    expect(commandDrawer).toHaveTextContent("Current Target");
    expect(commandDrawer).toHaveTextContent("Message");
    expect(commandDrawer).toHaveTextContent("Send Chat");
    expect(commandDrawer).toHaveTextContent("No chat feedback yet.");
    expect(commandDrawer).toHaveTextContent("No chat history for this agent yet.");
    expect(commandDrawer.querySelector("#agent-chat-message")).not.toBeNull();
    expect(commandDrawer.querySelector("[data-chat-send='1']")).not.toBeNull();

    expect(screen.getByRole("button", { name: "Command & Target" })).toHaveClass("pixel-world-focus-control--primary");
    expect(screen.getByRole("button", { name: "World Status" })).toHaveClass("pixel-world-focus-control--secondary");
    expect(screen.getByRole("button", { name: "Maximize" })).toHaveClass("pixel-world-focus-control--secondary");
    expect(screen.getByRole("button", { name: "Exit Cinematic" })).toHaveClass("pixel-world-focus-control--quiet");

    expect(commandDrawer).toHaveTextContent("Agent Chat");

    screen.getByRole("button", { name: "Maximize" }).click();
    expect(host).toHaveAttribute("data-world-focus-maximized", "true");
    expect(document.body).toHaveClass("pixel-world-focus-maximized");
    expect(document.querySelector(".pixel-world-host__summary")).toBeNull();
    expect(document.querySelector(".pixel-world-focus-rail")).toBeNull();
    expect(document.querySelector('[data-focus-cinematic="true"]')).toBeNull();
    expect(document.querySelector('[data-focus-minimap="true"]')).toBeNull();
    expect(document.querySelector(".pixel-world-focus-receipt")).not.toBeNull();
    expect(document.querySelectorAll('[data-viewer-overlay="receipt"]')).toHaveLength(1);
    expect(document.querySelector(".pixel-world-render-diagnostics")).toBeNull();
    expect(document.querySelector(".pixel-world-focus-hud")).toHaveTextContent("Restore Layout");
    expect(document.querySelector(".pixel-world-focus-hud")).not.toHaveTextContent("Action blocked");
    expect(document.querySelector(".pixel-world-focus-receipt")).toHaveTextContent("Action blocked");
    expect(document.querySelector(".pixel-world-focus-drawer--command")?.open).toBe(true);

    screen.getByRole("button", { name: "Restore Layout" }).click();
    expect(host).toHaveAttribute("data-world-focus-maximized", "false");
    expect(document.body).not.toHaveClass("pixel-world-focus-maximized");
    expect(document.querySelector(".pixel-world-host__summary")).not.toBeNull();
    expect(document.querySelector('[data-focus-cinematic="true"]')).toBeNull();

    screen.getByRole("button", { name: "World Status" }).click();
    const diagnosticsDrawer = document.querySelector(".pixel-world-focus-drawer--diagnostics");
    expect(commandDrawer.open).toBe(false);
    expect(diagnosticsDrawer.open).toBe(true);
    expect(diagnosticsDrawer).toHaveTextContent("renderer=ready");
    expect(diagnosticsDrawer).toHaveTextContent("runtime=test_rust_runtime");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(host).toHaveAttribute("data-world-focus", "true");
    expect(diagnosticsDrawer).toHaveProperty("open", false);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "false");
    });
    expect(document.body).not.toHaveClass("pixel-world-focus-active");
    expect(document.querySelector(".pixel-world-focus-drawer--diagnostics")).toBeNull();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it.each([["blocked", "world_delta", true], ["rejected", "none", true], ["accepted", "accepted_intent", true], ["waiting_for_intent", "none", false]])("renders exactly one Action Receipt surface in Cinematic View for %s", async (state, confidence, present) => {
    runtimeMock.deriveRenderState = vi.fn((input) => {
      const renderState = buildTestRustRenderState(input);
      renderState.commercial_surface.action_receipt = { ...renderState.commercial_surface.action_receipt, state, confidence, present };
      return renderState;
    });
    await renderPixelWorldHost(sampleSnapshot(), "?test_api=1&connect=0&locale=en");
    await waitFor(() => { expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument(); });
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => { expect(document.querySelector(".pixel-world-host")).toHaveAttribute("data-world-focus", "true"); });
    expect(document.querySelectorAll('[data-viewer-overlay="receipt"]')).toHaveLength(1);
    expect(document.querySelector(".pixel-world-focus-hud__cell--receipt")).toBeNull();
    expect(document.querySelector(".pixel-world-focus-receipt .pixel-world-action-receipt")).toHaveAttribute("data-receipt-state", state);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("provides a test-only selected blocker visual fixture for comparable focus screenshots", async () => {
    useTestRustRenderState();
    const { core } = await renderPixelWorldHost(
      emptyWorldSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker",
    );
    core.state.snapshot.model.agent_player_bindings["agent-1"] = "player-one";
    core.requestRender();
    await waitFor(() => expect(document.querySelector("[data-pixel-world-agent-marker=\"true\"][data-agent-id=\"agent-1\"]")).not.toBeNull());

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const host = document.querySelector(".pixel-world-host");
    expect(host).toHaveAttribute("data-visual-fixture", "selected_blocker");
    expect(window.__OASIS7_PIXEL_WORLD_VISUAL_FIXTURES__.get("selected_blocker")()).toMatchObject({
      player_gameplay: {
        blocker_kind: "material_shortage",
        intent_target: "agent-0",
      },
    });

    screen.getByRole("button", { name: "Cinematic View" }).click();

    await waitFor(() => {
      expect(host).toHaveAttribute("data-world-focus", "true");
    });
    expect(host).toHaveAttribute("data-focus-comparable", "true");
    expect(document.querySelector('[data-focus-cinematic="true"]')).toBeNull();
    expect(document.querySelector(".pixel-world-focus-rail")).toHaveTextContent("Agent 0");
    expect(document.querySelector(".pixel-world-focus-rail")).not.toHaveTextContent("agent-0");
    expect(document.querySelector(".pixel-world-focus-hud__cell--blocker")).toHaveAttribute("data-hud-priority", "critical");
    expect(document.querySelector(".pixel-world-focus-hud__cell--receipt")).toBeNull();
    expect(document.querySelector(".pixel-world-focus-receipt")).toHaveTextContent("Action blocked");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("agents=2");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("routes=2");
    expect(document.querySelector('[data-focus-minimap="true"]')).toHaveTextContent("fragments=4");
    expect(document.querySelector(".pixel-world-focus-drawer--command")).toHaveTextContent("Agent 0");
    expect(document.querySelector(".pixel-world-focus-drawer--command")).not.toHaveTextContent("agent=agent-0");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("projects the shared non-color beacon for selected agents and locations only", async () => {
    useTestRustRenderState();
    const { core } = await renderPixelWorldHost(
      emptyWorldSnapshot(),
      "?test_api=1&connect=0&locale=en&pixel_world_visual_fixture=selected_blocker",
    );
    core.state.snapshot.model.agent_player_bindings["agent-1"] = "player-one";
    core.requestRender();
    await waitFor(() => expect(document.querySelector("[data-pixel-world-agent-marker=\"true\"][data-agent-id=\"agent-1\"]")).not.toBeNull());

    await waitFor(() => {
      expect(document.querySelector("[data-pixel-world-agent-marker='true'][data-agent-id='agent-0']")).not.toBeNull();
    });

    const selectedAgent = document.querySelector(".pixel-world-entity--agent:not(.pixel-world-entity--canvas-hit-target)[data-agent-id='agent-0']");
    const unselectedAgent = document.querySelector(".pixel-world-entity--agent:not(.pixel-world-entity--canvas-hit-target)[data-agent-id='agent-1']");
    const selectedLocation = document.querySelector("[data-pixel-world-location-marker='true'][data-location-id='loc-0']");
    const unselectedLocation = document.querySelector("[data-pixel-world-location-marker='true'][data-location-id='loc-1']");

    expect(selectedAgent).toHaveAttribute("data-selected", "true");
    expect(unselectedAgent).toHaveAttribute("data-selected", "false");
    expect(selectedLocation).toHaveAttribute("data-selected", "false");
    expect(unselectedLocation).toHaveAttribute("data-selected", "false");

    core.applySelection({ kind: "location", id: "loc-0" });
    core.requestRender();

    await waitFor(() => {
      expect(selectedLocation).toHaveAttribute("data-selected", "true");
    });
    expect(selectedAgent).toHaveAttribute("data-selected", "false");
    expect(unselectedAgent).toHaveAttribute("data-selected", "false");
    expect(unselectedLocation).toHaveAttribute("data-selected", "false");

    const html = readFileSync("viewer.html", "utf8");
    expect(html).toMatch(/\.pixel-world-entity\[data-selected="true"\]\s*\{[^}]*min-width:\s*42px;[^}]*min-height:\s*42px;/s);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("demotes raw focus command feedback and chat history behind diagnostics", async () => {
    useTestRustRenderState();
    const { core } = await renderPixelWorldHost(
      sampleSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );
    core.applySelection({ kind: "agent", id: "agent-0" });
    core.state.lastChatFeedback = {
      id: "feedback-1",
      kind: "agent_chat",
      action: "agent_chat",
      agentId: "agent-0",
      accepted: true,
      stage: "accepted",
      ok: true,
      reason: null,
      effect: "Message accepted by agent-0.",
      response: { message: "Agent acknowledged the recovery plan.", code: "chat_ok" },
    };
    core.state.chatHistory = [
      {
        id: "chat-1",
        source: "player",
        agentId: "agent-0",
        targetAgentId: "agent-0",
        playerId: "player-one",
        speaker: "player-one",
        locationId: "loc-0",
        message: "Restore the smelter before expanding.",
        tick: 44,
      },
    ];
    core.requestRender();

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(document.querySelector(".pixel-world-focus-drawer--command")).not.toBeNull();
    });

    const commandDrawer = document.querySelector(".pixel-world-focus-drawer--command");
    expect(commandDrawer).toHaveTextContent("Message accepted by agent-0.");
    expect(commandDrawer).toHaveTextContent("Restore the smelter before expanding.");
    expect(commandDrawer).toHaveTextContent("Player -> agent-0");
    expect(commandDrawer).toHaveTextContent("player-one · loc-0 · tick=44");
    expect(commandDrawer).toHaveTextContent("Raw diagnostics");
    expect(commandDrawer).not.toHaveTextContent('"message": "Restore the smelter before expanding."');
    expect(commandDrawer).not.toHaveTextContent('"code": "chat_ok"');

    commandDrawer.querySelector("summary").click();
    commandDrawer.querySelectorAll("details.diagnostic summary")[0].click();
    await waitFor(() => {
      expect(commandDrawer).toHaveTextContent('"code": "chat_ok"');
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps empty focus rail collapsed while preserving fallback world summary", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost(
      emptyWorldSnapshot(),
      "?test_api=1&connect=0&locale=en",
    );

    await waitFor(() => {
      expect(screen.getByText("No action receipt yet")).toBeInTheDocument();
    });

    expect(document.querySelector('[data-focus-fallback-map="true"]')).toBeNull();

    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(document.querySelector('[data-focus-minimap="true"]')).not.toBeNull();
    });

    expect(document.querySelector(".pixel-world-focus-rail")).toBeNull();
    const minimap = document.querySelector('[data-focus-minimap="true"]');
    expect(minimap).not.toBeNull();
    expect(minimap).toHaveTextContent("agents=0");
    expect(minimap).toHaveTextContent("targets=0");
    expect(minimap).toHaveTextContent("routes=0");
    expect(minimap).toHaveTextContent("fragments=0");
    expect(minimap).toHaveTextContent("Unassigned");
    expect(minimap).not.toHaveTextContent("Selected");
  });

  it("preserves world focus UI state across host remounts", async () => {
    useTestRustRenderState();
    cleanupMountedHost();
    vi.resetModules();
    window.history.replaceState({}, "", "/software_safe.html?test_api=1&connect=0&locale=en");
    window.localStorage.clear();
    document.body.innerHTML = "";

    const core = await import("./legacy_core.js");
    const { PixelWorldHost } = await import("./pixel_world_host.jsx");
    core.setViewerLocale("en");
    core.injectSnapshot(sampleSnapshot());

    const firstView = render(() => <PixelWorldHost locale="en" />);
    testHarness.activeCleanup = firstView.unmount;

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const firstHost = document.querySelector(".pixel-world-host");
    screen.getByRole("button", { name: "Cinematic View" }).click();
    await waitFor(() => {
      expect(firstHost).toHaveAttribute("data-world-focus", "true");
    });
    screen.getByRole("button", { name: "World Status" }).click();

    expect(firstHost).toHaveAttribute("data-world-focus", "true");
    expect(document.body).toHaveClass("pixel-world-focus-active");
    expect(document.querySelector(".pixel-world-focus-drawer--diagnostics")?.open).toBe(true);

    firstView.unmount();
    cleanupMountedHost();

    const secondView = render(() => <PixelWorldHost locale="en" />);
    testHarness.activeCleanup = secondView.unmount;

    const secondHost = document.querySelector(".pixel-world-host");
    expect(secondHost).toHaveAttribute("data-world-focus", "true");
    expect(document.body).toHaveClass("pixel-world-focus-active");
    await waitFor(() => {
      expect(document.querySelector(".pixel-world-focus-drawer--diagnostics")).not.toBeNull();
    });
    expect(document.querySelector(".pixel-world-focus-drawer--diagnostics").open).toBe(true);
    expect(document.querySelector(".pixel-world-focus-drawer--command")).toHaveProperty("open", false);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps the Rust canvas primary while preserving readable agent hit targets", async () => {
    useTestRustRenderState();
    await renderPixelWorldHost();

    await waitFor(() => {
      expect(screen.getByText("Recover sustainable capability")).toBeInTheDocument();
    });

    const canvas = document.querySelector(".pixel-world-canvas");
    const fragments = Array.from(canvas.querySelectorAll(".pixel-world-fragment-terrain"));
    const route = canvas.querySelector(".pixel-world-route");
    const location = canvas.querySelector(".pixel-world-entity--location");
    const agent = screen.getByRole("button", { name: "Select Agent Agent 0" });

    expect(screen.getByRole("img", { name: "World canvas overview" })).toBeInTheDocument();
    expect(fragments).toHaveLength(0);
    expect(route).toBeNull();
    expect(location).toBeNull();
    expect(agent).toHaveAttribute("data-pixel-world-agent-marker", "true");
    expect(agent).toHaveAttribute("data-position-source", "location_derived");
    expect(agent).toHaveAttribute("aria-label", "Select Agent Agent 0");
  });

});
