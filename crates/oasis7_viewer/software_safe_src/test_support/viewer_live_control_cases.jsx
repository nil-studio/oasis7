import { fireEvent, screen } from "@solidjs/testing-library";
import { expect, it } from "vitest";

// Registered in main.test.jsx to share its app lifecycle and WebSocket fixtures.
export function registerViewerLiveControlTests({ installMockWebSocket, sampleSnapshot, renderViewerApp, HEAVY_UI_TEST_TIMEOUT_MS }) {
  it("shows confirmed Viewer scheduling state and rejects stale or conflicting observations", async () => {
    const { sockets, sentMessages } = installMockWebSocket();
    const snapshot = sampleSnapshot({ player_gameplay: { ...sampleSnapshot().player_gameplay,
      available_actions: [
        { action_id: "pause_play", label: "Pause live play", protocol_action: "live_control.pause", disabled_reason: null },
        { action_id: "resume_play", label: "Resume live play", protocol_action: "live_control.play", disabled_reason: null },
      ],
    } });
    const { core } = await renderViewerApp({
      search: "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
      autoBindSnapshotAgent: false, snapshot,
    });
    sockets[0].open();
    expect(sentMessages.find((message) => message.type === "hello_v2").capabilities).toContain("live_control_state_v1");
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world", control_profile: "live", capabilities: ["live_control_state_v1"] });
    core.injectSnapshot(snapshot);
    const epoch = "ab".repeat(32);
    const observe = (revision, paused, request_id = null, currentEpoch = epoch) => sockets[0].receive({
      type: "live_control_state", state: { scope: "viewer_new_decisions", epoch: currentEpoch, revision, paused,
        ...(request_id == null ? {} : { request_id }) },
    });
    observe(1, true);
    const status = () => screen.getByRole("status", { name: "Viewer decision scheduling" });
    expect(status()).toHaveTextContent("Viewer new decisions paused");
    expect(status()).toHaveTextContent("World jobs and requests already in flight may continue");
    const beforeTime = core.state.logicalTime;
    sentMessages.length = 0;
    fireEvent.click(screen.getAllByRole("button", { name: "Resume live play" })[0]);
    const request = sentMessages.find((message) => message.type === "live_control");
    expect(status()).toHaveTextContent("Awaiting Viewer confirmation");
    expect(status()).toHaveTextContent("Viewer new decisions paused");
    observe(2, false, request.request_id);
    expect(status()).toHaveTextContent("Viewer new decisions enabled");
    expect(core.state.lastControlFeedback.stage).toBe("completed_applied");
    expect(core.state.logicalTime).toBe(beforeTime);
    observe(1, true);
    observe(99, true, null, "cd".repeat(32));
    expect(status()).toHaveTextContent("Viewer new decisions enabled");
    observe(2, true);
    expect(status()).toHaveTextContent("Viewer scheduling state unconfirmed");
    observe(3, true);
    expect(status()).toHaveTextContent("Viewer new decisions paused");
    sockets[0].close();
    expect(status()).toHaveTextContent("Viewer scheduling state unconfirmed");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not infer Viewer scheduling state from an old peer or a local click", async () => {
    const { sockets } = installMockWebSocket();
    const { core } = await renderViewerApp({
      search: "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
      autoBindSnapshotAgent: false, snapshot: sampleSnapshot(),
    });
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "old-peer", world_id: "test-world", control_profile: "live", capabilities: [] });
    sockets[0].receive({ type: "live_control_state", state: { scope: "viewer_new_decisions", epoch: "ab".repeat(32), revision: 1, paused: true } });
    core.sendControl("pause");
    expect(screen.getByRole("status", { name: "Viewer decision scheduling" })).toHaveTextContent("Viewer scheduling state unconfirmed");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("exposes Player Pause and sends the standard live control without Director authority", async () => {
    const { sockets, sentMessages } = installMockWebSocket();
    const snapshot = sampleSnapshot({
      player_gameplay: {
        ...sampleSnapshot().player_gameplay,
        available_actions: [{
          action_id: "pause_play", label: "Pause live play", protocol_action: "live_control.pause",
          target_agent_id: null, disabled_reason: null,
        }],
      },
    });
    const { core } = await renderViewerApp({
      search: "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
      autoBindSnapshotAgent: false, snapshot,
    });
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world", control_profile: "live" });
    core.injectSnapshot(snapshot);
    sentMessages.length = 0;
    const pause = screen.getAllByRole("button", { name: "Pause live play" })[0];
    expect(pause).toHaveTextContent("Pause Live Play");
    expect(pause).toBeEnabled();
    fireEvent.click(pause);
    expect(sentMessages.filter((message) => message.type === "live_control")).toEqual([
      expect.objectContaining({ mode: { mode: "pause" }, request_id: expect.any(Number) }),
    ]);
    expect(sentMessages.some((message) => message.type === "gameplay_action" || message.type === "director_capability")).toBe(false);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it.each([
    ["provider unavailable", false],
    ["cognition allowance unavailable", false],
    ["runtime snapshot is missing agents/locations", true],
  ])("keeps global Player Pause clickable when %s without a bound Agent", async (reason, empty) => {
    const { sockets, sentMessages } = installMockWebSocket();
    const snapshot = sampleSnapshot({
      ...(empty ? { model: { agents: {}, locations: {}, agent_prompt_profiles: {}, agent_execution_debug_contexts: {} } } : {}),
      player_gameplay: {
        ...sampleSnapshot().player_gameplay,
        blocker_kind: empty ? "runtime_snapshot_empty_entities" : "runtime_unavailable",
        blocker_detail: reason,
        available_actions: [{
          action_id: "pause_play", label: "Pause live play", protocol_action: "live_control.pause",
          target_agent_id: null, disabled_reason: reason,
        }, {
          action_id: "resume_play", label: "Resume live play", protocol_action: "live_control.play",
          target_agent_id: null, disabled_reason: reason,
        }],
      },
    });
    const { core } = await renderViewerApp({
      search: "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
      autoBindSnapshotAgent: false, snapshot,
    });
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world", control_profile: "live" });
    core.injectSnapshot(snapshot);
    sentMessages.length = 0;
    const pause = screen.getAllByRole("button", { name: "Pause live play" })[0];
    expect(pause).toBeEnabled();
    expect(screen.getAllByRole("button", { name: "Resume live play" }).every((button) => button.disabled)).toBe(true);
    fireEvent.click(pause);
    expect(sentMessages.filter((message) => message.type === "live_control")).toEqual([
      expect.objectContaining({ mode: { mode: "pause" }, request_id: expect.any(Number) }),
    ]);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

}
