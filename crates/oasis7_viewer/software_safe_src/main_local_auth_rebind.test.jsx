import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
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
  installMainTestLifecycleHooks,
  installMockWebSocket,
  renderViewerApp,
  renderViewerAppThroughAutoMount,
  sampleSnapshot,
  setActiveCleanup,
  setupConnectedSemanticCore,
  viewerUrl,
} from "./test_support/main_test_harness.jsx";
installMainTestLifecycleHooks();

describe("viewer local auth and starter force-rebind", () => {
  it("auto-issues local test player auth before claiming the first agent", async () => {
    const { core } = await renderViewerApp({
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

    expect(core.state.auth.available).toBe(false);

    core.sendGameplayAction({
      actionId: "claim_first_agent",
      protocolAction: "gameplay_action.submit",
      targetAgentId: "starter-agent-0",
      executeKind: "claim_first_agent",
    });

    for (let attempt = 0; attempt < 50; attempt += 1) {
      if (core.state.lastGameplayActionFeedback?.stage !== "queued") {
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    expect(core.state.lastGameplayActionFeedback?.stage).toBe("error");
    expect(core.state.auth.available).toBe(true);
    expect(core.state.auth.source).toBe("local_test_api_ephemeral");
    expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    expect(core.state.lastGameplayActionFeedback?.reason).toMatch(/viewer websocket is not connected/i);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("reuses the loopback local test player identity across module reloads", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    let core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    core.sendGameplayAction({
      actionId: "claim_first_agent",
      protocolAction: "gameplay_action.submit",
      targetAgentId: "starter-agent-0",
      executeKind: "claim_first_agent",
    });

    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    });
    const firstPlayerId = core.state.auth.playerId;
    const firstPublicKey = core.state.auth.publicKey;

    vi.resetModules();
    document.body.innerHTML = "";
    core = await import("./legacy_core.js");
    await core.initializeSoftwareSafeCore();
    core.sendGameplayAction({
      actionId: "claim_first_agent",
      protocolAction: "gameplay_action.submit",
      targetAgentId: "starter-agent-0",
      executeKind: "claim_first_agent",
    });

    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toBe(firstPlayerId);
      expect(core.state.auth.publicKey).toBe(firstPublicKey);
    });
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("force-rebinds the starter agent when local test auth finds a stale local binding", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    expect(sockets.length).toBe(1);
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });

    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    });
    const base = sampleSnapshot();
    core.injectSnapshot(sampleSnapshot({
      model: {
        ...base.model,
        agents: {
          "starter-agent-0": {
            ...base.model.agents["agent-0"],
            id: "starter-agent-0",
            name: "Starter Agent",
          },
        },
        agent_player_bindings: {
          "starter-agent-0": "local-test-player-old",
        },
        agent_player_public_key_bindings: {
          "starter-agent-0": "old-public-key",
        },
      },
    }));
    sockets[0].receive({
      type: "authoritative_recovery_ack",
      ack: {
        status: "catch_up_ready",
        player_id: core.state.auth.playerId,
        session_pubkey: core.state.auth.publicKey,
      },
    });
    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "register_session",
              request: expect.objectContaining({
                player_id: core.state.auth.playerId,
                requested_agent_id: "starter-agent-0",
                force_rebind: true,
              }),
            }),
          }),
        ]),
      );
    });
    expect(core.state.auth.pendingForceRebind).toBe(true);
    expect(core.expirePendingSessionRegisterWaiterForTest()).toBe(true);
    await Promise.resolve();
    await Promise.resolve();
    expect(core.state.auth.syncInFlight).toBe(false);
    expect(core.state.auth.pendingForceRebind).toBe(false);
    expect(core.state.auth.recoveryErrorCode).toBe("session_register_timeout");
    expect(core.state.auth.error).toMatch(/timed out waiting for ack\/error/i);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("times out a stuck reconnect sync and resumes local starter force-rebind", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });

    await Promise.resolve();
    await Promise.resolve();
    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    });
    const base = sampleSnapshot();
    core.injectSnapshot(sampleSnapshot({
      model: {
        ...base.model,
        agents: {
          "starter-agent-0": {
            ...base.model.agents["agent-0"],
            id: "starter-agent-0",
            name: "Starter Agent",
          },
        },
        agent_player_bindings: {
          "starter-agent-0": "local-test-player-old",
        },
        agent_player_public_key_bindings: {
          "starter-agent-0": "old-public-key",
        },
      },
    }));
    sockets[0].receive({
      type: "authoritative_recovery_ack",
      ack: {
        status: "catch_up_ready",
        player_id: core.state.auth.playerId,
        session_pubkey: core.state.auth.publicKey,
      },
    });

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({ mode: "reconnect_sync" }),
          }),
        ]),
      );
    });
    core.state.auth.syncInFlight = true;
    core.state.auth.registrationStatus = "registering";
    core.state.auth.runtimeStatus = "probing";

    core.expireHostedRuntimeSyncTimeoutForTest();
    await Promise.resolve();
    await Promise.resolve();

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "register_session",
              request: expect.objectContaining({
                player_id: core.state.auth.playerId,
                requested_agent_id: "starter-agent-0",
                force_rebind: true,
              }),
            }),
          }),
        ]),
      );
    });
    expect(core.state.auth.syncInFlight).toBe(true);
    expect(core.state.auth.pendingForceRebind).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps force-rebind registration active when stale reconnect reports session_not_found", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });

    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    });
    sockets[0].receive({
      type: "authoritative_recovery_ack",
      ack: {
        status: "catch_up_ready",
        player_id: core.state.auth.playerId,
        session_pubkey: core.state.auth.publicKey,
      },
    });
    core.registerPlayerSessionForTest("starter-agent-0", { forceRebind: true }).catch(() => {});

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "register_session",
              request: expect.objectContaining({
                requested_agent_id: "starter-agent-0",
                force_rebind: true,
              }),
            }),
          }),
        ]),
      );
    });
    const registerCountBeforeError = sentMessages.filter((message) => (
      message.type === "authoritative_recovery"
      && message.command?.mode === "register_session"
    )).length;

    sockets[0].receive({
      type: "authoritative_recovery_error",
      error: {
        code: "session_not_found",
        message: `session_not_found: player ${core.state.auth.playerId} has no active session_pubkey`,
        player_id: core.state.auth.playerId,
        session_pubkey: core.state.auth.publicKey,
      },
    });

    await Promise.resolve();
    expect(core.state.auth.pendingForceRebind).toBe(true);
    expect(core.state.auth.runtimeStatus).toBe("rebind_registering");
    const registerCountAfterError = sentMessages.filter((message) => (
      message.type === "authoritative_recovery"
      && message.command?.mode === "register_session"
    )).length;
    expect(registerCountAfterError).toBe(registerCountBeforeError);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps starter agent visible while local test force-rebind recovery is available", async () => {
    const base = sampleSnapshot();
    const { core } = await renderViewerApp({
      snapshot: sampleSnapshot({
        model: {
          ...base.model,
          agents: {
            "starter-agent-0": {
              ...base.model.agents["agent-0"],
              id: "starter-agent-0",
              name: "Starter Agent",
            },
          },
          agent_player_bindings: {
            "starter-agent-0": "local-test-player-old",
          },
        },
      }),
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-new",
          publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
          privateKey: "private-key-must-stay-hidden",
          source: "local_test_api_ephemeral",
          registrationStatus: "issued",
          runtimeStatus: "issued",
          boundAgentId: null,
        };
      },
    });

    expect(core.isAgentVisibleToCurrentSession("starter-agent-0")).toBe(true);
    expect(core.modelLists().agents.map((agent) => agent.id)).toContain("starter-agent-0");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("retries force-rebind when runtime reports the starter agent is bound to an old local player", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });

    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    });
    sockets[0].receive({
      type: "authoritative_recovery_ack",
      ack: {
        status: "catch_up_ready",
        player_id: core.state.auth.playerId,
        session_pubkey: core.state.auth.publicKey,
      },
    });
    core.registerPlayerSessionForTest("starter-agent-0").catch(() => {});

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "register_session",
              request: expect.objectContaining({
                player_id: core.state.auth.playerId,
                requested_agent_id: "starter-agent-0",
              }),
            }),
          }),
        ]),
      );
    });
    const registerCountBeforeError = sentMessages.filter((message) => (
      message.type === "authoritative_recovery"
      && message.command?.mode === "register_session"
    )).length;

    sockets[0].receive({
      type: "authoritative_recovery_error",
      error: {
        code: "player_bind_failed",
        message: `agent starter-agent-0 is bound to player local-test-player-old, not ${core.state.auth.playerId}`,
        agent_id: "starter-agent-0",
      },
    });

    await waitFor(() => {
      const registerMessages = sentMessages.filter((message) => (
        message.type === "authoritative_recovery"
        && message.command?.mode === "register_session"
      ));
      expect(registerMessages.length).toBeGreaterThan(registerCountBeforeError);
      expect(registerMessages.at(-1)?.command?.request).toEqual(
        expect.objectContaining({
          player_id: core.state.auth.playerId,
          requested_agent_id: "starter-agent-0",
          force_rebind: true,
        }),
      );
    });
    expect(core.state.auth.pendingForceRebind).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("recovers force-rebind target from a reconnect bind error", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });

    await waitFor(() => {
      expect(core.state.auth.source).toBe("local_test_api_ephemeral");
      expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    });
    sockets[0].receive({
      type: "authoritative_recovery_ack",
      ack: {
        status: "catch_up_ready",
        player_id: core.state.auth.playerId,
        session_pubkey: core.state.auth.publicKey,
      },
    });
    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "reconnect_sync",
            }),
          }),
        ]),
      );
    });
    const registerCountBeforeError = sentMessages.filter((message) => (
      message.type === "authoritative_recovery"
      && message.command?.mode === "register_session"
    )).length;

    sockets[0].receive({
      type: "authoritative_recovery_error",
      error: {
        code: "player_bind_failed",
        message: `agent starter-agent-0 is bound to player local-test-player-old, not ${core.state.auth.playerId}`,
      },
    });

    await waitFor(() => {
      const registerMessages = sentMessages.filter((message) => (
        message.type === "authoritative_recovery"
        && message.command?.mode === "register_session"
      ));
      expect(registerMessages.length).toBeGreaterThan(registerCountBeforeError);
      expect(registerMessages.at(-1)?.command?.request).toEqual(
        expect.objectContaining({
          player_id: core.state.auth.playerId,
          requested_agent_id: "starter-agent-0",
          force_rebind: true,
        }),
      );
    });
    expect(core.state.auth.pendingRequestedAgentId).toBe("starter-agent-0");
    expect(core.state.auth.pendingForceRebind).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("auto-issues loopback local player auth without test_api before claiming the first agent", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?connect=0&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const core = await import("./legacy_core.js");

    expect(core.state.auth.available).toBe(false);

    core.sendGameplayAction({
      actionId: "claim_first_agent",
      protocolAction: "gameplay_action.submit",
      targetAgentId: "starter-agent-0",
      executeKind: "claim_first_agent",
    });

    for (let attempt = 0; attempt < 50; attempt += 1) {
      if (core.state.lastGameplayActionFeedback?.stage !== "queued") {
        break;
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    expect(core.state.auth.available).toBe(true);
    expect(core.state.auth.source).toBe("local_test_api_ephemeral");
    expect(core.state.auth.playerId).toMatch(/^local-test-player-/);
    expect(core.state.lastGameplayActionFeedback?.stage).toBe("error");
    expect(core.state.lastGameplayActionFeedback?.reason).toMatch(/viewer websocket is not connected/i);
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
