import { registerViewerLiveControlTests } from "./test_support/viewer_live_control_cases.jsx";
import { fireEvent, screen, waitFor, within } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { buildTaskGame076ScenarioSnapshot } from "./gameplay_attraction_scenario.js";
import { bindFirstSnapshotAgentForTest, bindLocalTestAgent, bindUnboundLocalPlayerForTest } from "./test_support/viewer_player_auth_fixture.js";
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
  elementPrecedes,
  installMainTestLifecycleHooks,
  installMockWebSocket,
  renderViewerApp,
  sampleAgentClaimSnapshot,
  sampleSnapshot,
  setActiveCleanup,
  setupConnectedSemanticCore,
  viewerUrl,
} from "./test_support/main_test_harness.jsx";
installMainTestLifecycleHooks();

describe("viewer claim and recovery flows", () => {
  it("routes the empty-world Claim First Agent button into gameplay action feedback", async () => {
    const { core, container } = await renderViewerApp({
      setupCore: bindUnboundLocalPlayerForTest,
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
          available_actions: [
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
    });

    const stagePanel = container.querySelector("#viewer-stage-panel"); const claimButtons = within(stagePanel).getAllByRole("button", { name: "Claim First Agent" });
    expect(claimButtons).toHaveLength(1); expect(container.querySelector('[data-testid="viewer-playthrough-action-claim-first-agent"]')).toBeInTheDocument();

    const dispatchSpy = vi.spyOn(core, "sendGameplayAction"); fireEvent.click(claimButtons[0]);
    expect(dispatchSpy).toHaveBeenCalledTimes(1);
    expect(dispatchSpy).toHaveBeenCalledWith(expect.objectContaining({ actionId: "claim_first_agent", executeKind: "claim_first_agent" }));

    expect(core.state.auth.available).toBe(true);
    expect(core.state.lastGameplayActionFeedback?.action).toBe("claim_first_agent");
    await waitFor(() => expect(core.state.lastGameplayActionFeedback?.stage).toBe("error"));
    expect(core.state.lastGameplayActionFeedback?.reason).toMatch(/viewer websocket is not connected/i);
    expect(core.buildGameplaySummary().recentFeedback).toMatchObject({
      action: "claim_first_agent",
      stage: "error",
      source: "local_gameplay_action",
    });
    await waitFor(() => {
      expect(within(stagePanel).getAllByText(/viewer websocket is not connected/i).length)
        .toBeGreaterThan(0);
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it.each(["ack", "received", "pending", "unknown"])("disables first-agent claim while result is %s", async (stage) => {
    const { container, core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        model: {
          agents: {},
          locations: {},
          agent_prompt_profiles: {},
          agent_execution_debug_contexts: {},
        },
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          blocker_kind: "runtime_snapshot_empty_entities",
          blocker_detail: "runtime exposed an empty new-user world",
          available_actions: [
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
      setupCore(core) {
        core.state.lastGameplayActionFeedback = {
          kind: "gameplay_action",
          action: "claim_first_agent",
          stage,
          accepted: true,
          ok: true,
          effect: "submitted gameplay action claim_first_agent for starter-agent-0",
          reason: null,
          response: null,
        };
      },
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const targetsPanel = container.querySelector("#viewer-targets-panel");
    const claimButtons = [
      ...within(stagePanel).getAllByRole("button", { name: /Claim First Agent|Waiting for result|Result unknown/i }),
      ...within(targetsPanel).getAllByRole("button", { name: /Claim First Agent|Waiting for result|Result unknown/i }),
    ];
    expect(claimButtons.length).toBeGreaterThan(1);
    claimButtons.forEach((button) => expect(button).toBeDisabled());
    expect(within(stagePanel).getAllByText(/waiting for the committed chain snapshot/i).length)
      .toBeGreaterThan(0);
    expect(within(targetsPanel).getAllByText(/waiting for the committed chain snapshot/i).length)
      .toBeGreaterThan(0);
    expect(within(targetsPanel).getByText(/Do not idle through sync/i)).toBeInTheDocument();
    expect(within(targetsPanel).getAllByText(/Starter budget/i).length).toBeGreaterThan(0);
    expect(within(targetsPanel).getByText(/the OC button appears automatically after the Agent syncs/i))
      .toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  registerViewerLiveControlTests({ installMockWebSocket, sampleSnapshot, renderViewerApp, HEAVY_UI_TEST_TIMEOUT_MS });

  it("keeps published live-control recovery actions clickable while first-agent sync is pending", async () => {
    await renderViewerApp({
      snapshot: sampleSnapshot({
        model: {
          agents: {},
          locations: {
            origin: {
              id: "origin",
              name: "Origin",
              radius_cm: 1,
              resources: {},
            },
          },
          agent_prompt_profiles: {},
          agent_execution_debug_contexts: {},
        },
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          blocker_kind: "runtime_snapshot_empty_entities",
          blocker_detail: "runtime exposed an empty new-user world",
          available_actions: [
            {
              action_id: "request_snapshot",
              label: "Refresh gameplay snapshot",
              protocol_action: "request_snapshot",
              disabled_reason: null,
            },
            {
              action_id: "advance_step",
              label: "Advance 1 step to create the first world feedback",
              protocol_action: "live_control.step",
              disabled_reason: null,
            },
            {
              action_id: "resume_play",
              label: "Resume live play",
              protocol_action: "live_control.play",
              disabled_reason: null,
            },
            {
              action_id: "pause_play",
              label: "Pause live play",
              protocol_action: "live_control.pause",
              disabled_reason: null,
            },
          ],
        },
      }),
      setupCore(core) {
        core.state.lastGameplayActionFeedback = {
          kind: "gameplay_action",
          action: "claim_first_agent",
          stage: "ack",
          accepted: true,
          ok: true,
          effect: "submitted gameplay action claim_first_agent for starter-agent-0",
          reason: null,
          response: null,
        };
      },
    });

    const stepButtons = screen.getAllByTestId("viewer-available-action-step");
    expect(stepButtons.some((button) => !button.disabled)).toBe(true);
    const playButtons = screen.getAllByTestId("viewer-available-action-play");
    expect(playButtons.some((button) => !button.disabled)).toBe(true);
    const pauseButtons = screen.getAllByTestId("viewer-available-action-pause");
    expect(pauseButtons.some((button) => !button.disabled)).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("moves presentation notes into the stage help tip instead of the right details rail", async () => {
    const { container } = await renderViewerApp();

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const detailsPanel = container.querySelector("#viewer-details-panel");
    const helpButton = within(stagePanel).getByRole("button", { name: /open presentation scale guidance/i });

    expect(helpButton).toHaveAttribute("aria-expanded", "false");
    expect(within(detailsPanel).queryByText("Do not trust marker size")).not.toBeInTheDocument();

    fireEvent.click(helpButton);

    expect(helpButton).toHaveAttribute("aria-expanded", "true");
    expect(within(stagePanel).getByText("Presentation Notes")).toBeInTheDocument();
    expect(within(stagePanel).getByText(/do not read on-screen diameter as real geometry size/i)).toBeInTheDocument();
    expect(helpButton).toHaveAttribute("aria-describedby", "viewer-stage-scale-tip");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("prefers recovery proof controls over generic gameplay actions when blocked", async () => {
    const { container, core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          next_step_hint: "Advance one committed step to prove the material recovery before queuing another build.",
          available_actions: [
            {
              action_id: "build_factory_smelter_mk1",
              target_agent_id: "agent-0",
              label: "Build smelter mk1",
              protocol_action: "gameplay_action.submit",
              disabled_reason: null,
            },
            {
              action_id: "live_control.step",
              label: "Advance recovery proof",
              protocol_action: "live_control.step",
              disabled_reason: null,
            },
            {
              action_id: "request_snapshot",
              label: "Request snapshot",
              protocol_action: "world.request_snapshot",
              disabled_reason: null,
            },
          ],
        },
      }),
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const recommendedCard = within(stagePanel).getByText("Recommended Action").closest(".callout");
    expect(recommendedCard).toBeTruthy();
    expect(within(recommendedCard).getByText("Advance recovery proof")).toBeInTheDocument();
    expect(within(recommendedCard).getByText(/Advance one committed step, then inspect the committed result/i)).toBeInTheDocument();
    expect(within(recommendedCard).getByRole("button", { name: "Advance One Step" })).toHaveAttribute(
      "data-testid",
      "viewer-playthrough-action-recommended",
    );
    expect(within(stagePanel).getAllByTestId("viewer-playthrough-action-recommended")).toHaveLength(1);
    ["step", "request-snapshot"].forEach((suffix) => expect(within(stagePanel).queryByTestId(`viewer-playthrough-action-${suffix}`)).not.toBeInTheDocument());
    expect(within(stagePanel).getByText(/Refresh the snapshot to confirm whether the blocker is still present/i)).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("uses snapshot refresh as the recommended recovery action when the blocker asks for fresh state", async () => {
    const { container, core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          next_step_hint: "Refresh the snapshot first, then prove whether the blocker cleared.",
          available_actions: [
            {
              action_id: "request_snapshot",
              label: "Request snapshot",
              protocol_action: "world.request_snapshot",
              disabled_reason: null,
            },
          ],
        },
      }),
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    const recommendedCard = within(stagePanel).getByText("Recommended Action").closest(".callout");
    expect(recommendedCard).toBeTruthy();
    expect(within(recommendedCard).getByText("Request snapshot")).toBeInTheDocument();
    expect(within(recommendedCard).getByRole("button", { name: "Refresh Snapshot" })).toHaveAttribute(
      "data-testid",
      "viewer-playthrough-action-recommended",
    );
    expect(within(stagePanel).getAllByTestId("viewer-playthrough-action-recommended")).toHaveLength(1);
    ["viewer-playthrough-action-step", "viewer-primary-action-preview"].forEach((testId) => expect(within(stagePanel).queryByTestId(testId)).not.toBeInTheDocument());
    expect(within(recommendedCard).getByText(/Refresh the snapshot to confirm whether the blocker is still present/i)).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("auto-refreshes empty-entity recovery snapshots until first-agent claim appears", async () => {
    vi.useFakeTimers();
    vi.resetModules();
    const { sockets, sentMessages } = installMockWebSocket();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";

    const core = await import("./legacy_core.js");
    await core.initializeSoftwareSafeCore();
    expect(sockets.length).toBe(1);
    sockets[0].open();
    sockets[0].receive({
      type: "hello_ack",
      server: { name: "mock-runtime" },
      world_id: "test-world",
      control_profile: "interactive",
    });

    const blockedSnapshot = sampleSnapshot({
      model: {
        agents: {},
        locations: {
          "frag-0-0-0-2": {
            id: "frag-0-0-0-2",
            name: "Starter Fragment",
            radius_cm: 26900,
            resources: {},
          },
        },
        agent_prompt_profiles: {},
        agent_execution_debug_contexts: {},
        agent_player_bindings: {},
        agent_player_public_key_bindings: {},
      },
      player_gameplay: {
        ...sampleSnapshot().player_gameplay,
        blocker_kind: "runtime_snapshot_empty_entities",
        blocker_detail: "runtime exposed an empty new-user world",
        available_actions: [
          {
            action_id: "request_snapshot",
            label: "Refresh gameplay snapshot",
            protocol_action: "world.request_snapshot",
            disabled_reason: null,
          },
        ],
      },
    });
    core.injectSnapshot(blockedSnapshot);
    expect(core.needsEmptyEntitySnapshotRefreshForTest()).toBe(true);
    expect(core.isEmptyEntitySnapshotRefreshPendingForTest()).toBe(true);
    expect(vi.getTimerCount()).toBeGreaterThan(0);

    const sentBeforeRefresh = sentMessages.length;
    vi.advanceTimersByTime(2499);
    expect(sentMessages).toHaveLength(sentBeforeRefresh);
    vi.advanceTimersByTime(1);
    await Promise.resolve();
    expect(core.needsEmptyEntitySnapshotRefreshForTest()).toBe(true);
    expect(core.state.connectionStatus).toBe("connected");
    expect(core.isEmptyEntitySnapshotRefreshPendingForTest()).toBe(true);
    expect(sentMessages.slice(sentBeforeRefresh)).toContainEqual({ type: "request_snapshot" });

    const claimSnapshot = sampleSnapshot({
      model: blockedSnapshot.model,
      player_gameplay: {
        ...blockedSnapshot.player_gameplay,
        available_actions: [
          {
            action_id: "claim_first_agent",
            label: "Claim first Agent",
            protocol_action: "gameplay_action.submit",
            target_agent_id: "starter-agent-0",
            disabled_reason: null,
          },
          {
            action_id: "request_snapshot",
            label: "Refresh gameplay snapshot",
            protocol_action: "world.request_snapshot",
            disabled_reason: null,
          },
        ],
      },
    });
    core.injectSnapshot(claimSnapshot);
    expect(core.needsEmptyEntitySnapshotRefreshForTest()).toBe(false);
    expect(core.isEmptyEntitySnapshotRefreshPendingForTest()).toBe(false);
    const sentAfterClaimAppears = sentMessages.length;
    vi.advanceTimersByTime(7500);
    await Promise.resolve();
    expect(sentMessages).toHaveLength(sentAfterClaimAppears);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("publishes stable player-visible locators for actual-click attraction playthrough", async () => {
    await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          next_step_hint: "Advance one committed step, then refresh the snapshot.",
          available_actions: [
            {
              action_id: "live_control.step",
              label: "Advance recovery proof",
              protocol_action: "live_control.step",
              disabled_reason: null,
            },
            {
              action_id: "request_snapshot",
              label: "Request snapshot",
              protocol_action: "world.request_snapshot",
              disabled_reason: null,
            },
          ],
        },
      }),
    });

    expect(screen.getByTestId("viewer-playthrough-select-agent")).toHaveAccessibleName(/agent/i);
    expect(screen.getByTestId("viewer-playthrough-action-recommended")).toHaveAccessibleName("Advance One Step");
    ["step", "request-snapshot"].forEach((suffix) => expect(screen.queryByTestId(`viewer-playthrough-action-${suffix}`)).not.toBeInTheDocument());
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders claim-agent quote and owned claim guidance without inventing claim controls", async () => {
    const { container } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          agent_claim: {
            next_claim_quote: {
              slot_index: 2,
              reputation_tier: "trusted",
              owned_claim_count: 1,
              claim_cap: 3,
              total_upfront_amount: 120,
              activation_fee_amount: 20,
              claim_bond_amount: 80,
              upkeep_per_epoch: 5,
              eligible_claim_balance: 200,
              transferable_liquid_balance: 140,
              restricted_starter_claim_balance: 60,
              auto_restricted_starter_claim_amount: 25,
              release_cooldown_epochs: 2,
              grace_epochs: 1,
              idle_warning_epochs: 4,
              forced_idle_reclaim_epochs: 6,
              forced_reclaim_penalty_bps: 500,
              blocked_reason: null,
            },
            owned_claims: [
              {
                target_agent_id: "agent-0",
                status: "release_ready",
                upkeep_paid_through_epoch: 17,
                upfront_restricted_spent_amount: 30,
                upfront_liquid_spent_amount: 90,
                claim_bond_locked_restricted_amount: 20,
                claim_bond_locked_liquid_amount: 60,
                release_ready_at_epoch: 18,
                release_ready_in_epochs: 0,
                grace_remaining_epochs: 1,
                idle_warning_in_epochs: 2,
                forced_reclaim_in_epochs: 6,
              },
            ],
          },
        },
      }),
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getByText("Claim-Agent Choice")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Slot")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText("2").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Owned / cap")).toBeInTheDocument();
    expect(within(stagePanel).getByText("1 / 3")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Total upfront")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Eligible balance")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Liquid balance")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Owned / cap").closest(".metric")).toHaveClass("metric--claim-primary");
    expect(within(stagePanel).getByText("Total upfront").closest(".metric")).toHaveClass("metric--claim-primary", "metric--claim-total");
    expect(within(stagePanel).getByText("Eligible balance").closest(".metric")).toHaveClass("metric--claim-primary");
    expect(within(stagePanel).getByText("Liquid balance").closest(".metric")).not.toHaveClass("metric--claim-primary");
    expect(within(stagePanel).getByText("Restricted starter")).toBeInTheDocument();
    expect(within(stagePanel).getByText("agent-0")).toBeInTheDocument();
    expect(within(stagePanel).getByText(/upkeep paid through epoch 17/i)).toBeInTheDocument();
    expect(within(stagePanel).getByText(/release ready in 0/i)).toBeInTheDocument();
    expect(within(stagePanel).getByText(/bond restricted=20 liquid=60/i)).toBeInTheDocument();
    expect(within(stagePanel).getByText(/Maintain by keeping control and upkeep healthy/i)).toBeInTheDocument();
    expect(within(stagePanel).queryByRole("button", { name: /claim/i })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not describe disabled release claim actions as executable", async () => {
    const { container } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          agent_claim: {
            next_claim_quote: {
              slot_index: 2,
              owned_claim_count: 1,
              claim_cap: 3,
              blocked_reason: null,
            },
            owned_claims: [
              {
                target_agent_id: "agent-0",
                status: "release_ready",
                release_ready_in_epochs: 0,
              },
            ],
          },
          available_actions: [
            {
              action_id: "release_agent_claim",
              label: "Release agent claim",
              protocol_action: "gameplay_action.submit",
              target_agent_id: "agent-0",
              disabled_reason: "cooldown active",
            },
          ],
        },
      }),
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getByText(/Release is published but currently disabled/i)).toBeInTheDocument();
    expect(within(stagePanel).queryByText(/execute it from the available actions list/i)).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps blocked claim reasons as diagnostics instead of fake claim buttons", async () => {
    const { container } = await renderViewerApp({
      snapshot: sampleSnapshot({
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          agent_claim: {
            next_claim_quote: {
              slot_index: 3,
              total_upfront_amount: 180,
              eligible_claim_balance: 90,
              transferable_liquid_balance: 90,
              blocked_reason: "insufficient reputation",
            },
            owned_claims: [],
          },
        },
      }),
    });

    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getByText("Wait before claiming")).toBeInTheDocument();
    expect(within(stagePanel).getByText(/needs waiting, funding, or eligibility first/i)).toBeInTheDocument();
    expect(within(stagePanel).getByText("Claim blocker diagnostics")).toBeInTheDocument();
    expect(within(stagePanel).queryByText("insufficient reputation")).not.toBeInTheDocument();
    expect(within(stagePanel).queryByRole("button", { name: /claim/i })).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("wires snapshot branch commitments into the stage panel", async () => {
    const { container } = await renderViewerApp({ snapshot: sampleSnapshot({ player_gameplay: {
      ...sampleSnapshot().player_gameplay,
      goal_kind: "ChooseFirstExpansionTradeoff",
      goal_title: "Choose the first expansion tradeoff",
      branch_hint: "Compare the published consequences before committing.",
      next_step_hint: "Choose only after reading the blocker.",
      branch_recommendations: [{ action_id: "build_alloy_factory", route_label: "Scale alloy throughput",
        immediate_gain: "Adds a second alloy production lane", future_beats: ["The next expansion starts with spare capacity", "New throughput requires a steadier structural-frame supply"],
        risk_or_lockin: "Consumes the current structural-frame reserve", next_session_hook: "Return to route the first bulk alloy order" }],
      available_actions: [{ action_id: "build_alloy_factory", label: "Build alloy factory core",
        protocol_action: "gameplay_action.submit", disabled_reason: "missing structural frames" }],
    } }) });
    const stagePanel = container.querySelector("#viewer-stage-panel");
    expect(within(stagePanel).getByText("Expansion Tradeoffs")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Scale alloy throughput")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Immediate gain")).toBeInTheDocument();
    expect(within(stagePanel).getAllByText(/^Future beat(?: [12])?$/)).toHaveLength(2);
    expect(within(stagePanel).getByText("The next expansion starts with spare capacity")).toBeInTheDocument();
    expect(within(stagePanel).getByText("New throughput requires a steadier structural-frame supply")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Risk or lock-in")).toBeInTheDocument();
    expect(within(stagePanel).getByText("Next-session hook")).toBeInTheDocument();
    expect(within(stagePanel).queryByText("Incomplete recommendation")).not.toBeInTheDocument();
    expect(within(stagePanel).getAllByText("Compare the published consequences before committing.").length).toBeGreaterThan(0);
    expect(within(stagePanel).getByText("Build alloy factory core: missing structural frames")).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
