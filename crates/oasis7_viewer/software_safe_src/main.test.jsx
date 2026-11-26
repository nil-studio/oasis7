import { registerViewerLiveControlTests } from "./test_support/viewer_live_control_cases.jsx";
import { bindLocalTestAgent, bindFirstSnapshotAgentForTest, bindUnboundLocalPlayerForTest } from "./test_support/viewer_player_auth_fixture.js";
import { fireEvent, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildTaskGame076ScenarioSnapshot } from "./gameplay_attraction_scenario.js";
import {
  HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
  LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
} from "./software_safe_constants.js";
import { buildAuthEnvelope } from "./viewer_auth_crypto.js";
import { settleLocalTestAuthStartup } from "./viewer_test_startup.js";
vi.mock("./pixel_world_host.jsx", () => ({
  PixelWorldHost: (props) => (
    <div data-testid="pixel-world-host">
      {`pixel-world-host:${typeof props.locale === "function" ? props.locale() : props.locale}`}
    </div>
  ),
}));
import {
  HEAVY_UI_TEST_TIMEOUT_MS,
  clearActiveCleanup,
  createTestCrypto,
  setActiveCleanup,
  elementPrecedes,
  installMainTestLifecycleHooks,
  installMockWebSocket,
  renderViewerApp,
  renderViewerAppThroughAutoMount,
  sampleAgentClaimSnapshot,
  sampleHostedPublicJoinAccess,
  sampleSnapshot,
  setupConnectedSemanticCore,
  viewerUrl,
} from "./test_support/main_test_harness.jsx";
installMainTestLifecycleHooks();

describe("viewer web ui automation baseline", () => {
  it("renders the world-target-command structure and diagnostics anchors", async () => {
    const { container } = await renderViewerApp();

    const nav = screen.getByRole("navigation", { name: /primary entry section navigation/i });
    expect(within(nav).getByRole("link", { name: "World" })).toHaveAttribute("href", "#viewer-stage-panel");
    expect(within(nav).getByRole("link", { name: "Targets" })).toHaveAttribute("href", "#viewer-targets-panel");
    expect(within(nav).getByRole("link", { name: "Command" })).toHaveAttribute("href", "#viewer-details-panel");
    expect(screen.getByRole("button", { name: "More" })).toHaveAttribute("aria-controls", "viewer-diagnostics-panel");

    const targetsPanel = container.querySelector("#viewer-targets-panel");
    const stagePanel = container.querySelector("#viewer-stage-panel");
    const detailsPanel = container.querySelector("#viewer-details-panel");
    const diagnosticsPanel = container.querySelector("#viewer-diagnostics-panel");

    expect(targetsPanel).toBeTruthy();
    expect(stagePanel).toBeTruthy();
    expect(detailsPanel).toBeTruthy();
    expect(diagnosticsPanel).toBeTruthy();
    expect(targetsPanel).toHaveAttribute("data-viewer-surface", "targets");
    expect(stagePanel).toHaveAttribute("data-viewer-surface", "stage");
    expect(detailsPanel).toHaveAttribute("data-viewer-surface", "command");
    expect(diagnosticsPanel).toHaveAttribute("data-viewer-surface", "diagnostics");
    expect(within(targetsPanel).getByText("Targets")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Industrial World Command Desk")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Recover sustainable capability").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Control Proof")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Player Intent")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Actual world result")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Recovery Move")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Next Move").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Attraction Proof")).toBeInTheDocument();
    expect(within(stagePanel).getByText("What I caused")).toBeInTheDocument();
    expect(within(stagePanel).getByText("New option")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Why continue")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Waiting cost")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Recovery").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Agency Moves")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Interrupt")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Reprioritize")).toBeInTheDocument();
    expect(within(stagePanel).getByText(/Switch from stalled smelter build to material recovery/i)).toBeInTheDocument();
    expect(within(stagePanel).getByText("First Win & Anti-Grind")).toBeInTheDocument();
    expect(within(stagePanel).getByText("First Win")).toBeInTheDocument();
    expect(within(stagePanel).getByText("small_player.first_industrial_win")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText(/repair_elasticity/i).length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Mature-World Continuation")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Recovery Options")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText(/Recovery feasibility unknown/i).length).toBeGreaterThan(0);
    expect(within(stagePanel).queryByText("repair: available / rebuild: available / pivot: available")).not.toBeInTheDocument();
    expect(within(stagePanel).queryByText("independent_path_available")).not.toBeInTheDocument();
    expect(within(stagePanel).getByText("Share Replay")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText(/queued build_factory_smelter_mk1/i).length).toBeGreaterThan(0);
    expect(within(stagePanel).getAllByText("Accepted Intent").length).toBeGreaterThan(0);
    expect(within(stagePanel).getAllByText("Queue build_factory_smelter_mk1 for agent-0").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Goal Execution")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Capability Economics")).toBeInTheDocument();
    expect(within(stagePanel).getByText("What The Next Move Changes")).toBeInTheDocument();
    expect(within(stagePanel).queryByText("Why This Step Is Worth Continuing")).not.toBeInTheDocument();
    expect(within(stagePanel).getByText("Next Action Guidance")).toBeInTheDocument();
    expect(within(stagePanel).getByText(/The gating input is the material chain/i)).toBeInTheDocument();
    expect(within(stagePanel).getByText("World Constraint")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Missing Material")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Recommended Action")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Runtime Diagnostics")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Session Ladder")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Runtime Diagnostics")).toBeInTheDocument();
    expect(screen.getByTestId("pixel-world-host")).toHaveTextContent("pixel-world-host:en");
  }, 60000);

  it("shows target list loading affordances before the first snapshot arrives", async () => {
    const { container } = await renderViewerApp({ snapshot: null });

    const targetsPanel = container.querySelector("#viewer-targets-panel");
    expect(targetsPanel).toBeTruthy();
    expect(within(targetsPanel).getByText("Syncing agents…")).toBeInTheDocument();
    expect(within(targetsPanel).getByText("Syncing locations…")).toBeInTheDocument();
    expect(within(targetsPanel).getAllByText("Connection: connecting").length).toBeGreaterThan(0);
    expect(within(targetsPanel).getAllByText("Handshake: waiting for server hello").length).toBeGreaterThan(0);
    expect(within(targetsPanel).getAllByText("Snapshot: waiting for first world snapshot").length).toBeGreaterThan(0);
    expect(within(targetsPanel).queryByText("No agents in current snapshot.")).not.toBeInTheDocument();
    expect(within(targetsPanel).queryByText("No locations in current snapshot.")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders the factory production failure disposition before generic wait and recovery choices", async () => {
    const { container } = await renderViewerApp({ snapshot: sampleSnapshot({ player_gameplay: {
      ...sampleSnapshot().player_gameplay,
      available_actions: [
        { action_id: "schedule_recipe_smelter_iron_ingot", label: "Queue iron ingot run", protocol_action: "gameplay_action.submit", target_agent_id: "agent-0", disabled_reason: "insufficient iron_ore in site ledger" },
        { action_id: "request_snapshot", label: "Refresh gameplay snapshot", protocol_action: "request_snapshot" },
      ],
      factory_production_failure_disposition: {
        action_id: "19",
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
    } }) });

    const details = container.querySelector("#viewer-gameplay-details");
    const card = within(details).getByTestId("viewer-factory-production-failure-disposition");
    const cardText = card.textContent;
    for (const text of ["factory.target", "recipe.smelter.iron_ingot", "Detail: product profile rejected the committed output", "Refresh gameplay snapshot", "Recovery action: request_snapshot", "Next recheck: next committed snapshot"]) expect(cardText).toContain(text);
    expect(within(card).getAllByText(/iron_ore × 3/)).toHaveLength(2); expect(within(card).getAllByText("7").length).toBeGreaterThanOrEqual(2);
    expect(card.querySelector("button")).toBeEnabled();
    expect(card.textContent).not.toMatch(/product_validation_rejected|consumed_lost|inspect_product_validation_and_reschedule/);
    expect(within(details).queryByText("synthetic wait should be hidden")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("marks branch-hint-only guidance as legacy and incomplete", async () => {
    const { container } = await renderViewerApp({ snapshot: sampleSnapshot({ player_gameplay: {
      ...sampleSnapshot().player_gameplay, goal_kind: "ChooseFirstExpansionTradeoff",
      branch_hint: "Choose whether the next branch buys throughput, resilience, or reach.", branch_recommendations: [],
    } }) });
    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getByText("Legacy / Incomplete")).toBeInTheDocument();
    expect(within(stagePanel).getByText(/structured branch recommendations are unavailable/i)).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("marks branch commitments with fewer than two or duplicate future beats incomplete", async () => {
    for (const futureBeats of [
      ["Only one published future beat"],
      ["The capacity opens", "The capacity opens"],
    ]) {
      const { container } = await renderViewerApp({ snapshot: sampleSnapshot({ player_gameplay: {
        ...sampleSnapshot().player_gameplay, goal_kind: "ChooseFirstExpansionTradeoff",
        branch_recommendations: [{ action_id: "build_alloy_factory", route_label: "Scale alloy throughput",
          immediate_gain: "Adds a production lane", future_beats: futureBeats, risk_or_lockin: "Consumes reserves", next_session_hook: "Return to inspect output" }],
      } }) });
      expect(within(container.querySelector("#viewer-stage-panel")).getByText("Incomplete recommendation")).toBeInTheDocument();
    }
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps a singular legacy future beat visible but incomplete", async () => {
    const { container } = await renderViewerApp({ snapshot: sampleSnapshot({ player_gameplay: {
      ...sampleSnapshot().player_gameplay, goal_kind: "ChooseFirstExpansionTradeoff",
      branch_recommendations: [{ action_id: "build_alloy_factory", route_label: "Scale alloy throughput",
        immediate_gain: "Adds a production lane", future_beat_changed: "Legacy capacity opens next", risk_or_lockin: "Consumes reserves", next_session_hook: "Return to inspect output" }],
    } }) });
    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getByText("Legacy capacity opens next")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Incomplete recommendation")).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("compresses world scale details into a one-line details-rail summary", async () => {
    const { container } = await renderViewerApp();

    const detailsPanel = container.querySelector("#viewer-details-panel");
    expect(within(detailsPanel).getByText("World Scale")).toBeInTheDocument();
    expect(within(detailsPanel).getByText("snapshot.config.space")).toBeInTheDocument();
    expect(within(detailsPanel).getByText(/World Bounds 100 km × 50 km × 10 km/i)).toBeInTheDocument();

    expect(within(detailsPanel).queryByText("Nearest Distance Samples")).not.toBeInTheDocument();
    expect(within(detailsPanel).queryByText("Selected location anchor")).not.toBeInTheDocument();
    expect(
      within(detailsPanel).queryByText(/The main runtime state already lives in World Summary/i),
    ).not.toBeInTheDocument();
  });

  it("dismisses the stage help tip on escape and outside click", async () => {
    const { container } = await renderViewerApp();

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const helpButton = within(stagePanel).getByRole("button", { name: /open presentation scale guidance/i });

    fireEvent.click(helpButton);
    expect(helpButton).toHaveAttribute("aria-expanded", "true");

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(helpButton).toHaveAttribute("aria-expanded", "false"));

    fireEvent.click(helpButton);
    expect(helpButton).toHaveAttribute("aria-expanded", "true");

    fireEvent.pointerDown(document.body);
    await waitFor(() => expect(helpButton).toHaveAttribute("aria-expanded", "false"));
  });

  it("renders a selected Location as readable identity with explicit context unavailability", async () => {
    const { container, core } = await renderViewerApp({
      selection: { kind: "location", id: "loc-0" },
    });

    const detailsPanel = container.querySelector("#viewer-details-panel");
    const context = within(detailsPanel).getByRole("region", { name: "Entity Context" });
    expect(core.state.selectedKind).toBe("location");
    expect(context).toHaveAttribute("data-agent-context-kind", "location");
    expect(within(context).getByText("Factory Anchor")).toBeInTheDocument();
    expect(context).toHaveTextContent(/Context unavailable: this entity has no published context projection yet/i);
    expect(context).not.toHaveTextContent("Agent Context");
    expect(context).not.toHaveTextContent("Executing");
    expect(context.querySelectorAll('[data-agent-context-group="truth"], [data-agent-context-group="decision"], [data-agent-context-group="intent"]')).toHaveLength(0);
    expect(context).toHaveAttribute("data-agent-context-intent", "unavailable");
    expect(context).toHaveAttribute("data-agent-context-receipt", "none");
    expect(detailsPanel.querySelector("[data-command-agent]")).toBeNull();
    expect(within(detailsPanel).queryByText("Agent Chat")).not.toBeInTheDocument();
    expect(within(detailsPanel).queryByLabelText("Message")).not.toBeInTheDocument();
    expect(within(detailsPanel).queryByText("Advanced Prompt Settings")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);
  it("switches Command context between Agent and Location without leaking Agent controls or context", async () => {
    const { container, core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
    });

    const detailsPanel = container.querySelector("#viewer-details-panel");
    expect(within(detailsPanel).getByRole("region", { name: "Agent Context" })).toBeInTheDocument();
    expect(within(detailsPanel).getByText("Agent Chat")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("viewer-select-location-loc-0"));
    await waitFor(() => {
      expect(core.state.selectedKind).toBe("location");
      expect(within(detailsPanel).getByRole("region", { name: "Entity Context" })).toBeInTheDocument();
    });
    const locationContext = within(detailsPanel).getByRole("region", { name: "Entity Context" });
    expect(within(locationContext).getByText("Factory Anchor")).toBeInTheDocument();
    expect(within(detailsPanel).queryByText("Agent Chat")).not.toBeInTheDocument();
    expect(within(detailsPanel).queryByLabelText("Message")).not.toBeInTheDocument();
    expect(within(detailsPanel).queryByText("Advanced Prompt Settings")).not.toBeInTheDocument();
    expect(within(detailsPanel).queryByRole("region", { name: "Agent Context" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByTestId("viewer-playthrough-select-agent"));
    await waitFor(() => {
      expect(core.state.selectedKind).toBe("agent");
      expect(within(detailsPanel).getByRole("region", { name: "Agent Context" })).toBeInTheDocument();
    });
    expect(within(detailsPanel).getByText("Agent Chat")).toBeInTheDocument();
    expect(within(detailsPanel).queryByRole("region", { name: "Entity Context" })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("unlocks agent chat and prompt override surfaces for the current bound agent", async () => {
    const { core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
      },
    });

    expect(screen.getByText("Agent Chat")).toBeInTheDocument();
    expect(screen.getByLabelText("Message")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send Chat" })).toBeInTheDocument();

    expect(screen.getByText("Advanced Prompt Settings")).toBeInTheDocument();
    expect(screen.queryByLabelText("System Prompt Override")).not.toBeInTheDocument();

    core.togglePromptOverridesVisible();

    await waitFor(() => {
      expect(screen.getByLabelText("System Prompt Override")).toBeInTheDocument();
    });
    expect(screen.getByLabelText("Short-Term Goal Override")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Preview Prompt" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Apply Prompt" })).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("hides unbound agents from the current account agent list", async () => {
    const base = sampleSnapshot();
    const { container, core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        model: {
          ...base.model,
          agent_player_bindings: {},
          agent_player_public_key_bindings: {},
        },
      }),
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-fresh",
          publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
          privateKey: "private-key-must-stay-hidden",
          source: "local_test_api_ephemeral",
          registrationStatus: "registered",
          runtimeStatus: "registered_unbound",
          boundAgentId: null,
        };
      },
    });

    const targetsPanel = container.querySelector("#viewer-targets-panel");
    const detailsPanel = container.querySelector("#viewer-details-panel");
    expect(within(targetsPanel).getByText("No agents in current snapshot.")).toBeInTheDocument();
    expect(within(targetsPanel).queryByText("agent-0")).not.toBeInTheDocument();
    expect(within(targetsPanel).queryByText("World Visible")).not.toBeInTheDocument();
    expect(within(detailsPanel).getByText(/no controllable Agent yet/i)).toBeInTheDocument();
    expect(within(detailsPanel).queryByText("Agent Chat")).not.toBeInTheDocument();
    expect(core.sendAgentChat("agent-0", "hello")).toEqual(
      expect.objectContaining({ ok: false, reason: expect.stringMatching(/bound Agent/i) }),
    );
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("hides agents bound to another account from a fresh account view", async () => {
    const base = sampleSnapshot();
    const { container, core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        model: {
          ...base.model,
          agent_player_bindings: {
            "agent-0": "old-player",
          },
          agent_player_public_key_bindings: {
            "agent-0": "old-public-key",
          },
        },
      }),
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-fresh",
          publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
          privateKey: "private-key-must-stay-hidden",
          source: "local_test_api_ephemeral",
          registrationStatus: "registered",
          runtimeStatus: "registered_unbound",
          boundAgentId: null,
        };
      },
    });

    const targetsPanel = container.querySelector("#viewer-targets-panel");
    const detailsPanel = container.querySelector("#viewer-details-panel");
    expect(within(targetsPanel).getByText("No agents in current snapshot.")).toBeInTheDocument();
    expect(within(targetsPanel).queryByText("agent-0")).not.toBeInTheDocument();
    expect(within(targetsPanel).queryByText("Other Account")).not.toBeInTheDocument();
    expect(within(targetsPanel).queryByText("World Visible")).not.toBeInTheDocument();
    expect(within(detailsPanel).getByText(/no controllable Agent yet/i)).toBeInTheDocument();
    expect(within(detailsPanel).queryByText("Agent Chat")).not.toBeInTheDocument();
    expect(core.sendAgentChat("agent-0", "hello")).toEqual(
      expect.objectContaining({ ok: false, reason: expect.stringMatching(/bound Agent/i) }),
    );
    expect(core.sendPromptControl("preview", { agentId: "agent-0" })).toEqual(
      expect.objectContaining({ ok: false, reason: expect.stringMatching(/bound Agent/i) }),
    );
    expect(core.sendGameplayAction({
      protocol_action: "gameplay_action.submit",
      action_id: "build_factory_smelter_mk1",
      target_agent_id: "agent-0",
    })).toEqual(
      expect.objectContaining({ ok: false, reason: expect.stringMatching(/bound Agent/i) }),
    );
    expect(core.sendGameplayAction({
      protocol_action: "gameplay_action.submit",
      action_id: "claim_agent",
      actor_agent_id: "agent-0",
      target_agent_id: "agent-claim-target",
    })).toEqual(
      expect.objectContaining({ ok: false, reason: expect.stringMatching(/bound Agent/i) }),
    );
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("surfaces backend agent chat rejection details without requiring diagnostics", async () => {
    await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.lastChatFeedback = {
          id: "chat-error-test",
          kind: "chat",
          action: "agent_chat",
          agentId: "agent-0",
          accepted: false,
          ok: false,
          stage: "error",
          reason: "claim starter OC before using LLM/agent chat for this Agent",
          effect: "insufficient_oc_budget",
          response: {
            code: "insufficient_oc_budget",
            message: "claim starter OC before using LLM/agent chat for this Agent",
            agent_id: "agent-0",
          },
        };
      },
    });

    const agentChatPanel = screen.getByText("Agent Chat").closest("section");
    expect(agentChatPanel).toBeTruthy();
    expect(within(agentChatPanel).getByText("Chat failed")).toBeInTheDocument();
    expect(within(agentChatPanel).getByText("code=insufficient_oc_budget")).toBeInTheDocument();
    expect(within(agentChatPanel).getByText(/Agent chat did not complete.*insufficient_oc_budget.*claim starter OC/i))
      .toBeInTheDocument();
    expect(within(agentChatPanel).getByText("claim starter OC before using LLM/agent chat for this Agent"))
      .toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("persists acknowledged chat messages for the current world", async () => {
    const { core } = await renderViewerApp({
      snapshot: null,
      setupCore(core) {
        core.state.worldId = "world-chat-cache";
        core.state.wsUrl = "ws://127.0.0.1:5011";
        core.injectSnapshot(sampleSnapshot());
        core.applySelection({ kind: "agent", id: "agent-0" });
      },
    });

    core.pushChatHistory({
      id: "chat-cache-1",
      source: "player",
      agentId: "agent-0",
      targetAgentId: "agent-0",
      playerId: "player-one",
      speaker: "player-one",
      message: "Keep this message visible after refresh.",
      tick: 44,
    });

    const stored = JSON.parse(window.localStorage.getItem(core.chatHistoryStorageKey()));
    expect(stored).toEqual([
      expect.objectContaining({
        id: "chat-cache-1",
        source: "player",
        message: "Keep this message visible after refresh.",
      }),
    ]);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("hydrates cached chat history after a viewer refresh", async () => {
    await renderViewerApp({
      snapshot: null,
      selection: null,
      setupCore(core) {
        core.state.worldId = "world-chat-cache";
        core.state.wsUrl = "ws://127.0.0.1:5011";
        window.localStorage.setItem(
          core.chatHistoryStorageKey(),
          JSON.stringify([
            {
              id: "chat-cache-reload-1",
              source: "player",
              agentId: "agent-0",
              targetAgentId: "agent-0",
              playerId: "player-one",
              speaker: "player-one",
              message: "This message survived refresh.",
              tick: 45,
            },
          ]),
        );
        core.injectSnapshot(sampleSnapshot());
        core.applySelection({ kind: "agent", id: "agent-0" });
      },
    });

    expect(screen.getByText("This message survived refresh.")).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders chat history as readable messages with raw diagnostics behind disclosure", async () => {
    await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.chatHistory = [
          {
            id: "chat-test-1",
            source: "player",
            agentId: "agent-0",
            targetAgentId: "agent-0",
            playerId: "player-one",
            speaker: "player-one",
            locationId: null,
            message: "Please restore the smelter line before expanding.",
            tick: 42,
            intentSeq: 7,
          },
        ];
      },
    });

    const messageFlow = screen.getByText("Message Flow").closest("div");
    expect(messageFlow).toBeTruthy();
    expect(screen.getByText("Player -> agent-0")).toBeInTheDocument();
    expect(screen.getByText("player-one · unknown location")).toBeInTheDocument();
    expect(screen.getByText("Please restore the smelter line before expanding.")).toBeInTheDocument();
    expect(screen.getByText("Please restore the smelter line before expanding.").closest(".event-card")).toHaveClass("event-card--chat-player");
    expect(screen.getByText("Raw diagnostics")).toBeInTheDocument();
    expect(screen.queryByText(/"message": "Please restore the smelter line before expanding."/)).not.toBeInTheDocument();
    const commandPanel = document.querySelector("#viewer-details-panel"); expect(within(commandPanel).getByText("Agent Chat")).toBeInTheDocument(); expect(commandPanel.querySelector(".command-surface__diagnostic-strip")?.closest("details")).toHaveProperty("open", false);
    for (const label of ["Advanced Prompt Settings", "Asset / Governance Lane"]) expect(within(commandPanel).getByText(label).closest("details"), `${label} must be a closed disclosure`).toHaveProperty("open", false);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders provider chat failures in message flow", async () => {
    await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.pushChatHistory({
          id: "chat-error-test",
          source: "error",
          agentId: "agent-0",
          targetAgentId: "agent-0",
          speaker: "runtime",
          code: "provider_unreachable",
          message: "provider request /v1/world-simulator/agent-chat failed",
          tick: 849,
          response: {
            code: "provider_unreachable",
            message: "provider request /v1/world-simulator/agent-chat failed",
            agent_id: "agent-0",
          },
        });
      },
    });

    expect(screen.getByText("agent-0 reply failed")).toBeInTheDocument();
    expect(screen.getByText("agent-0 reply failed").closest(".event-card")).toHaveClass("event-card--chat-error");
    expect(screen.getByText("runtime · code=provider_unreachable · tick=849")).toBeInTheDocument();
    expect(screen.getByText("Agent reply did not complete: provider request /v1/world-simulator/agent-chat failed"))
      .toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps mounted dom stable across requestRender", async () => {
    const { core, container } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
    });

    const searchInput = screen.getByRole("searchbox");
    const selectedBadge = screen.getByText("Current Selection");

    core.state.connectionStatus = "connected";
    core.requestRender();

    expect(screen.getByRole("searchbox")).toBe(searchInput);
    expect(screen.getByText("Current Selection")).toBe(selectedBadge);
    expect(container.querySelector("#viewer-stage-panel")).toBeTruthy();
  });

  it("keeps diagnostics visually demoted behind the player path surface", async () => {
    const { container } = await renderViewerApp();

    const summary = screen.getByText("Runtime Diagnostics").closest("summary");
    expect(summary).toBeTruthy();
    expect(summary).toHaveClass("diagnostic-surface__summary");
    const diagnosticsPanel = container.querySelector("#viewer-diagnostics-panel");
    expect(diagnosticsPanel).toBeTruthy();
    expect(diagnosticsPanel).toHaveClass("diagnostic-surface");
    expect(diagnosticsPanel).not.toHaveAttribute("open");
    const summaryBadges = summary.querySelectorAll(".badge");
    expect(summaryBadges.length).toBeGreaterThan(0);
    summaryBadges.forEach((badge) => {
      expect(badge).toHaveClass("badge--diagnostic");
      expect(badge).not.toHaveClass("badge--good");
      expect(badge).not.toHaveClass("badge--warn");
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(stagePanel).toBeTruthy();
    expect(within(stagePanel).getByText("Formal Gameplay Summary")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Accepted Intent").length).toBeGreaterThan(0);
    expect(within(stagePanel).getAllByText("Next Step").length).toBeGreaterThan(0);
    const nextStepCard = within(stagePanel).getByTestId("viewer-next-step-card");
    expect(nextStepCard).toHaveClass("hero-focus-card--next-step");
    expect(within(nextStepCard).getByText("Next Step")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Actions Not Exposed On This Page")).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("shows current player identity on the primary world surface", async () => {
    const { container } = await renderViewerApp({
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-visible",
          publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
          privateKey: "07".repeat(32),
          source: "local_test_api_ephemeral",
          registrationStatus: "registered",
          runtimeStatus: "registered",
          boundAgentId: "starter-agent-0",
        };
      },
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(stagePanel).toBeTruthy();
    const identityCard = within(stagePanel).getByTestId("viewer-identity-card");

    expect(within(identityCard).getByText("Current Identity")).toBeInTheDocument();
    expect(within(identityCard).getByText("Local Test Identity")).toBeInTheDocument();
    expect(within(identityCard).getByText(/player=local-test-player-visible/)).toBeInTheDocument();
    expect(within(identityCard).getByText(/pubkey=abcdef012345/)).toBeInTheDocument();
    expect(within(identityCard).getByText(/not an email login account/)).toBeInTheDocument();
    expect(identityCard).not.toHaveTextContent("07".repeat(32));
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("shows guest identity before local test auth is generated", async () => {
    const { container } = await renderViewerApp({ autoBindSnapshotAgent: false });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(stagePanel).toBeTruthy();
    const identityCard = within(stagePanel).getByTestId("viewer-identity-card");

    expect(within(identityCard).getByText("Current Identity")).toBeInTheDocument();
    expect(within(identityCard).getByText("Guest / Not Signed In")).toBeInTheDocument();
    expect(within(identityCard).getByText(/No player session is active yet/)).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("surfaces the first agent claim entry from gameplay snapshot", async () => {
    const { container } = await renderViewerApp({
      snapshot: sampleAgentClaimSnapshot(),
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
      },
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(stagePanel).toBeTruthy();
    expect(within(stagePanel).getByText("Agent Claim")).toBeInTheDocument();
    expect(within(stagePanel).getByLabelText("Target Agent")).toHaveValue("agent-claim-target");
    expect(within(stagePanel).getByRole("button", { name: "Claim Agent" })).toBeInTheDocument();
    expect(within(stagePanel).getByText("claimer=agent-0")).toBeInTheDocument();
    expect(within(stagePanel).getByText("eligible=325")).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps other-account claim-agent hidden while surfacing first-agent claim for a fresh account", async () => {
    let sendGameplayAction;
    const { container } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleAgentClaimSnapshot().player_gameplay,
          available_actions: [
            ...sampleAgentClaimSnapshot().player_gameplay.available_actions,
            {
              action_id: "claim_first_agent",
              label: "Claim first Agent",
              protocol_action: "gameplay_action.submit",
              target_agent_id: "starter-agent-0",
              disabled_reason: null,
            },
          ],
        },
      }),
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-fresh",
          publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
          privateKey: "private-key-must-stay-hidden",
          source: "local_test_api_ephemeral",
          registrationStatus: "registered",
          runtimeStatus: "registered_unbound",
          boundAgentId: null,
        };
        sendGameplayAction = vi.spyOn(core, "sendGameplayAction").mockReturnValue({ ok: true });
      },
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const targetsPanel = container.querySelector("#viewer-targets-panel");
    expect(within(stagePanel).getByText("Current Account Has No Bound Agent")).toBeInTheDocument();
    expect(within(stagePanel).getByText("claimer=agent-0")).toBeInTheDocument();
    expect(within(stagePanel).queryByRole("button", { name: "Claim Agent" })).not.toBeInTheDocument();
    expect(within(stagePanel).getAllByRole("button", { name: "Claim First Agent" }).length).toBeGreaterThan(0);
    expect(within(targetsPanel).getByRole("button", { name: "Claim First Agent" })).toBeInTheDocument();
    expect(within(targetsPanel).queryByText("agent-0")).not.toBeInTheDocument();

    fireEvent.click(within(targetsPanel).getByRole("button", { name: "Claim First Agent" }));
    expect(sendGameplayAction).toHaveBeenCalledWith(
      expect.objectContaining({
        actionId: "claim_first_agent",
        targetAgentId: "starter-agent-0",
        executeKind: "claim_first_agent",
      }),
    );
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("surfaces starter OC claim before first agent chat", async () => {
    let sendGameplayAction;
    const { container, core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          available_actions: [
            {
              action_id: "advance_step",
              label: "Advance 1 step",
              protocol_action: "live_control.step",
              disabled_reason: null,
            },
            {
              action_id: "request_snapshot",
              label: "Refresh snapshot",
              protocol_action: "world.request_snapshot",
              disabled_reason: null,
            },
            {
              action_id: "claim_starter_oc",
              label: "Claim starter OC",
              protocol_action: "gameplay_action.submit",
              target_agent_id: "agent-0",
              disabled_reason: null,
            },
            {
              action_id: "chat_first_agent",
              label: "Send one chat/command to the first available agent",
              protocol_action: "agent_chat",
              target_agent_id: "agent-0",
              disabled_reason: "claim starter OC before using LLM/agent chat for this Agent",
            },
          ],
        },
      }),
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
        sendGameplayAction = vi.spyOn(core, "sendGameplayAction").mockImplementation((action) => {
          if (action?.actionId === "claim_starter_oc") {
            core.state.lastGameplayActionFeedback = {
              kind: "gameplay_action",
              action: "claim_starter_oc",
              stage: "ack",
              accepted: true,
              effect: "queued gameplay action claim_starter_oc for agent-0",
            };
          }
          return { ok: true };
        });
      },
      starterOcOnboardingComplete: false,
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(stagePanel).toBeTruthy();
    const starterOcDialog = screen.getByRole("dialog", { name: "Claim Your First OC" });
    expect(within(stagePanel).getAllByText("Claim starter OC").length).toBeGreaterThan(0);
    expect(within(stagePanel).getAllByText(/one-time starter OC/i).length).toBeGreaterThan(0);
    expect(starterOcDialog).toBeInTheDocument();
    expect(within(starterOcDialog).getAllByText(/Starter budget/i).length).toBeGreaterThan(0);
    expect(within(starterOcDialog).getAllByText(/Unlock Agent chat/i).length).toBeGreaterThan(0);
    expect(core.buildGameplaySummary().recommendedAction).toMatchObject({
      actionId: "claim_starter_oc",
      executeKind: "claim_starter_oc",
    });
    const claimButton = within(starterOcDialog).getByRole("button", { name: "Claim Starter OC" });
    expect(claimButton).toBeInTheDocument();
    fireEvent.click(claimButton);
    expect(sendGameplayAction).toHaveBeenCalledWith(
      expect.objectContaining({
        actionId: "claim_starter_oc",
        targetAgentId: "agent-0",
        executeKind: "claim_starter_oc",
      }),
    );
    let confirmingDialog;
    await waitFor(() => {
      confirmingDialog = screen.getByRole("dialog", { name: "Confirming OC Credit" });
      expect(confirmingDialog).toBeInTheDocument();
    });
    expect(stagePanel.querySelector('[role="status"]')).toBeNull();
    const feedbackStatus = within(confirmingDialog).getByRole("status");
    expect(feedbackStatus).toHaveAttribute("aria-live", "polite");
    expect(feedbackStatus).toHaveAttribute("data-feedback-stage", "ack");
    expect(within(feedbackStatus).getByText("Gameplay action accepted")).toBeInTheDocument();
    expect(within(feedbackStatus).getByText(/entered the runtime queue/i)).toBeInTheDocument();
    expect(within(confirmingDialog).getAllByText(/Starter budget/i).length).toBeGreaterThan(0);
    expect(within(confirmingDialog).getAllByText(/Unlock Agent chat/i).length).toBeGreaterThan(0);
    expect(within(confirmingDialog).getAllByText(/starter budget/i).length).toBeGreaterThan(0);
    const retryButton = within(confirmingDialog).getByRole("button", { name: "Retry Confirmation" });
    expect(retryButton).toBeEnabled();
    fireEvent.click(retryButton);
    const busyRetryButton = within(confirmingDialog).getByRole("button", { name: "Advancing..." });
    expect(busyRetryButton).toBeDisabled();
    expect(busyRetryButton).toHaveAttribute("aria-busy", "true");
    expect(within(confirmingDialog).getByText("Waiting for manual confirmation")).toBeInTheDocument();
    expect(within(confirmingDialog).getByText("Manual check 1")).toBeInTheDocument();
    expect(sendGameplayAction).toHaveBeenLastCalledWith(
      expect.objectContaining({
        actionId: "advance_step",
        executeKind: "step",
      }),
    );
    expect(screen.queryByRole("dialog", { name: "OC Credited" })).not.toBeInTheDocument();
    expect(within(starterOcDialog).queryByRole("button", { name: "Start First Agent Chat" })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not invent a starter OC claim action after reload when runtime no longer advertises it", async () => {
    const snapshot = sampleSnapshot({
      player_gameplay: {
        ...sampleSnapshot().player_gameplay,
        available_actions: [
          {
            action_id: "chat_first_agent",
            label: "Send one chat/command to the first available agent",
            protocol_action: "agent_chat",
            target_agent_id: "agent-0",
            disabled_reason: "claim starter OC before using LLM/agent chat for this Agent",
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
    });
    const { core } = await renderViewerApp({
      snapshot,
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
      },
      starterOcOnboardingComplete: false,
    });

    expect(core.buildGameplaySummary().availableActions.some((action) => action.actionId === "claim_starter_oc")).toBe(false);
    expect(core.buildGameplaySummary().recommendedAction?.actionId).not.toBe("claim_starter_oc");
    expect(screen.queryByRole("dialog", { name: "Claim Your First OC" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Claim Starter OC" })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not open starter OC gate before the claimed Agent exists in the snapshot", async () => {
    const base = sampleSnapshot();
    const snapshot = sampleSnapshot({
      model: {
        ...base.model,
        agent_player_bindings: {
          ...base.model.agent_player_bindings,
          "starter-agent-0": "local-test-player-bound",
        },
        agent_player_public_key_bindings: {
          ...base.model.agent_player_public_key_bindings,
          "starter-agent-0": "abcdef0123456789abcdef0123456789".repeat(2),
        },
      },
      player_gameplay: {
        ...base.player_gameplay,
        available_actions: [
          {
            action_id: "advance_step",
            label: "Advance 1 step",
            protocol_action: "live_control.step",
            disabled_reason: null,
          },
          {
            action_id: "claim_starter_oc",
            label: "Claim starter OC",
            protocol_action: "gameplay_action.submit",
            target_agent_id: "starter-agent-0",
            disabled_reason: null,
          },
          {
            action_id: "chat_first_agent",
            label: "Send one chat/command to the first available agent",
            protocol_action: "agent_chat",
            target_agent_id: "starter-agent-0",
            disabled_reason: "claim starter OC before using LLM/agent chat for this Agent",
          },
        ],
      },
    });
    const { core } = await renderViewerApp({
      snapshot,
      setupAfterMount(core) {
        bindLocalTestAgent(core, "starter-agent-0");
      },
      starterOcOnboardingComplete: false,
    });

    const starterOcAction = core.buildGameplaySummary().availableActions.find((action) => action.actionId === "claim_starter_oc");
    expect(starterOcAction?.disabledReason).toMatch(/waiting for the committed snapshot/i);
    expect(core.buildGameplaySummary().recommendedAction?.actionId).not.toBe("claim_starter_oc");
    expect(screen.queryByRole("dialog", { name: "Claim Your First OC" })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("closes starter OC confirmation when committed snapshot exposes runtime state credit", async () => {
    const base = sampleSnapshot();
    const snapshot = sampleSnapshot({
      model: {
        ...base.model,
        state: {
          ...(base.model.state || {}),
          starter_oc_claims: {
            "agent-0": {
              agent_id: "agent-0",
              player_id: "local-test-player-bound",
              amount: 100000000,
              claimed_at: 14,
            },
          },
        },
      },
      player_gameplay: {
        ...base.player_gameplay,
        available_actions: [
          {
            action_id: "advance_step",
            label: "Advance 1 step",
            protocol_action: "live_control.step",
            target_agent_id: "agent-0",
            disabled_reason: null,
          },
          {
            action_id: "chat_first_agent",
            label: "Send one chat/command to the first available agent",
            protocol_action: "agent_chat",
            target_agent_id: "agent-0",
            disabled_reason: null,
          },
        ],
        recent_feedback: {
          action: "claim_starter_oc",
          stage: "accepted",
        },
      },
    });
    await renderViewerApp({
      snapshot,
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
      },
      starterOcOnboardingComplete: false,
    });

    expect(screen.queryByRole("dialog", { name: "Confirming OC Credit" })).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Claim Your First OC" })).not.toBeInTheDocument();
    expect(screen.getByTestId("viewer-playthrough-action-chat-first-agent")).toBeEnabled();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("closes starter OC confirmation when committed gameplay unlocks first agent chat", async () => {
    const base = sampleSnapshot();
    const snapshot = sampleSnapshot({
      player_gameplay: {
        ...base.player_gameplay,
        available_actions: [
          {
            action_id: "advance_step",
            label: "Advance 1 step",
            protocol_action: "live_control.step",
            target_agent_id: "agent-0",
            disabled_reason: null,
          },
          {
            action_id: "chat_first_agent",
            label: "Send one chat/command to the first available agent",
            protocol_action: "agent_chat",
            target_agent_id: "agent-0",
            disabled_reason: null,
          },
        ],
        recent_feedback: {
          action: "claim_starter_oc",
          stage: "accepted",
        },
      },
    });
    await renderViewerApp({
      snapshot,
      setupAfterMount(core) {
        bindLocalTestAgent(core, "agent-0");
      },
      starterOcOnboardingComplete: false,
    });

    expect(screen.queryByRole("dialog", { name: "Confirming OC Credit" })).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Claim Your First OC" })).not.toBeInTheDocument();
    expect(screen.getByTestId("viewer-playthrough-action-chat-first-agent")).toBeEnabled();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("forces goal execution blocked when the empty-entity guard trips", async () => {
    let sendGameplayAction;
    const { core, container } = await renderViewerApp({
      snapshot: sampleSnapshot({
        model: {
          agents: {},
          locations: {},
          agent_prompt_profiles: {},
          agent_execution_debug_contexts: {},
          agent_player_bindings: {},
          agent_player_public_key_bindings: {},
        },
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          stage_status: "active",
          execution_state: "completed",
          blocker_kind: null,
          blocker_detail: null,
          available_actions: [
            {
              action_id: "request_snapshot",
              label: "Request snapshot",
              protocol_action: "request_snapshot",
              disabled_reason: null,
            },
            {
              action_id: "claim_first_agent",
              label: "Claim first Agent",
              protocol_action: "gameplay_action.submit",
              target_agent_id: "starter-agent-0",
              disabled_reason: null,
            },
          ],
        },
      }),
      setupAfterMount(core) {
        sendGameplayAction = vi.spyOn(core, "sendGameplayAction").mockReturnValue({ ok: true });
      },
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const targetsPanel = container.querySelector("#viewer-targets-panel");
    expect(stagePanel).toBeTruthy();
    expect(targetsPanel).toBeTruthy();
    expect(within(stagePanel).getByText("Goal Execution")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Blocked").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("World Constraint")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Claim first Agent").length).toBeGreaterThan(0);
    expect(core.buildGameplaySummary().recommendedAction).toMatchObject({
      actionId: "claim_first_agent",
      executeKind: "claim_first_agent",
    });
    const claimButton = within(stagePanel).getAllByRole("button", { name: "Claim First Agent" })[0];
    expect(claimButton).toBeInTheDocument();
    fireEvent.click(claimButton);
    expect(sendGameplayAction).toHaveBeenCalledWith(
      expect.objectContaining({
        actionId: "claim_first_agent",
        targetAgentId: "starter-agent-0",
        executeKind: "claim_first_agent",
      }),
    );
    const recoveryClaimButton = within(targetsPanel).getByRole("button", { name: "Claim First Agent" });
    expect(recoveryClaimButton).toBeInTheDocument();
    fireEvent.click(recoveryClaimButton);
    expect(sendGameplayAction).toHaveBeenCalledTimes(2);
    expect(sendGameplayAction).toHaveBeenLastCalledWith(
      expect.objectContaining({
        actionId: "claim_first_agent",
        targetAgentId: "starter-agent-0",
        executeKind: "claim_first_agent",
      }),
    );
    expect(within(stagePanel).getAllByText(/New-user empty-world entry/i).length).toBeGreaterThan(0);
    expect(within(targetsPanel).getByText("No agents in current snapshot.")).toBeInTheDocument();
    expect(within(targetsPanel).getByText("No locations in current snapshot.")).toBeInTheDocument();
    expect(within(targetsPanel).queryByText("Syncing agents…")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
