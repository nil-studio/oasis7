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
describe("pixel world host focus and cinematic mode", () => {
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

    const html = readFileSync("viewer_base.css", "utf8");
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
