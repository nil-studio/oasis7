import { LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE } from "./software_safe_constants.js";
import { fireEvent, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { buildAuthEnvelope } from "./viewer_auth_crypto.js";
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
  installMainTestLifecycleHooks,
  installMockWebSocket,
  renderViewerApp,
  sampleSnapshot,
  setActiveCleanup,
  setupConnectedSemanticCore,
  viewerUrl,
} from "./test_support/main_test_harness.jsx";
installMainTestLifecycleHooks();

describe("viewer semantic timeouts and session registration", () => {
  it("times out queued chat commands that never reach ack or error", async () => {
    const { core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-bound",
          publicKey: "09".repeat(32),
          privateKey: "07".repeat(32),
          source: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
          registrationStatus: "registered",
          runtimeStatus: "registered",
          boundAgentId: "agent-0",
          syncInFlight: false,
        };
      },
    });
    Object.defineProperty(window, "crypto", {
      configurable: true,
      value: {
        subtle: {
          ...createTestCrypto().subtle,
          sign() {
            return new Promise(() => {});
          },
        },
      },
    });

    expect(core.sendAgentChat("agent-0", "hello while signer hangs")).toEqual(
      expect.objectContaining({ ok: true }),
    );
    await Promise.resolve();
    await Promise.resolve();

	    expect(core.state.lastChatFeedback.stage).toMatch(/queued|signing|registering|sent/);
	    expect(core.sendAgentChat("agent-0", "second click while first hangs")).toEqual(
	      expect.objectContaining({
	        ok: false,
	        reason: expect.stringContaining("already in flight"),
	      }),
	    );
	    expect(core.expirePendingAgentChatOverallTimeoutForTest()).toBe(true);

	    expect(core.state.lastChatFeedback).toEqual(
      expect.objectContaining({
        stage: "error",
        ok: false,
        accepted: false,
        effect: "agent_chat overall timeout",
        reason: "agent_chat timed out before live server ack/error completed",
      }),
    );
	    expect(screen.getAllByText("Chat failed").length).toBeGreaterThan(0);
	    expect(screen.getAllByText(/agent_chat timed out before live server ack\/error completed/i).length)
	      .toBeGreaterThan(0);

	    Object.defineProperty(window, "crypto", {
	      configurable: true,
	      value: {
	        subtle: {
	          async importKey() {
	            return { kind: "test-key" };
	          },
	          async sign() {
	            return new Uint8Array(64).fill(12).buffer;
	          },
	        },
	      },
	    });

	    expect(core.sendAgentChat("agent-0", "second message after timeout")).toEqual(
	      expect.objectContaining({ ok: true }),
	    );
	    await Promise.resolve();
	    await Promise.resolve();
	    expect(core.state.lastChatFeedback).toEqual(
	      expect.objectContaining({
	        pendingMessage: "second message after timeout",
	      }),
	    );
	    expect(core.state.lastChatFeedback.effect).not.toBe("agent_chat overall timeout");
	  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not start agent chat overall timeout while queued behind another semantic command", async () => {
    vi.useFakeTimers();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en&agent_chat_overall_timeout_ms=1000",
    );
    const { core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-bound",
          publicKey: "09".repeat(32),
          privateKey: "07".repeat(32),
          source: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
          registrationStatus: "registered",
          runtimeStatus: "registered",
          boundAgentId: "agent-0",
          syncInFlight: false,
        };
      },
    });
    const pendingSign = new Promise(() => {});
    Object.defineProperty(window, "crypto", {
      configurable: true,
      value: {
        subtle: {
          ...createTestCrypto().subtle,
          sign() {
            return pendingSign;
          },
        },
      },
    });

    expect(core.sendPromptControl("preview", { agentId: "agent-0" })).toEqual(
      expect.objectContaining({ ok: true }),
    );
    await Promise.resolve();
    await Promise.resolve();

    const queued = core.sendAgentChat("agent-0", "queued behind prompt signing");
    expect(queued).toEqual(expect.objectContaining({ ok: true }));
    const chatId = queued.feedback.id;
    expect(core.state.lastChatFeedback).toEqual(
      expect.objectContaining({
        id: chatId,
        stage: "queued",
        effect: "queued for signing and send",
      }),
    );

    await vi.advanceTimersByTimeAsync(1500);
    await Promise.resolve();
    await Promise.resolve();

    expect(core.state.lastChatFeedback).toEqual(
      expect.objectContaining({
        id: chatId,
        stage: "queued",
        effect: "queued for signing and send",
      }),
    );
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("times out prompt_control requests that were sent but never acked", async () => {
    const { core, sentMessages } = await setupConnectedSemanticCore();

    expect(core.sendPromptControl("preview", {
      agentId: "agent-0",
      shortTermGoal: "keep the forge queue moving",
    })).toEqual(expect.objectContaining({ ok: true }));

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "prompt_control",
            command: expect.objectContaining({ mode: "preview" }),
          }),
        ]),
      );
    });
    expect(core.state.lastPromptFeedback).toEqual(
      expect.objectContaining({
        stage: "sent",
        effect: "prompt preview request sent; waiting for ack",
      }),
    );

    expect(core.expirePendingPromptControlAckTimeoutForTest()).toBe(true);

    expect(core.state.lastPromptFeedback).toEqual(
      expect.objectContaining({
        stage: "error",
        ok: false,
        accepted: false,
        effect: "prompt_control ack timeout",
        reason: "prompt_control timed out waiting for ack/error from live server",
      }),
    );
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not let stale prompt_control overall timeout overwrite newer prompt feedback", async () => {
    vi.useFakeTimers();
    const { core } = await renderViewerApp({
      selection: { kind: "agent", id: "agent-0" },
      setupAfterMount(core) {
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "local-test-player-bound",
          publicKey: "09".repeat(32),
          privateKey: "07".repeat(32),
          source: LEGACY_VIEWER_AUTH_BOOTSTRAP_SOURCE,
          registrationStatus: "registered",
          runtimeStatus: "registered",
          boundAgentId: "agent-0",
          syncInFlight: false,
        };
      },
    });
    const pendingSign = new Promise(() => {});
    Object.defineProperty(window, "crypto", {
      configurable: true,
      value: {
        subtle: {
          ...createTestCrypto().subtle,
          sign() {
            return pendingSign;
          },
        },
      },
    });

    const first = core.sendPromptControl("preview", {
      agentId: "agent-0",
      shortTermGoal: "first hanging prompt",
    });
    expect(first).toEqual(expect.objectContaining({ ok: true }));
    await Promise.resolve();
    await Promise.resolve();

    const second = core.sendPromptControl("preview", {
      agentId: "agent-0",
      shortTermGoal: "newer queued prompt",
    });
    expect(second).toEqual(expect.objectContaining({ ok: true }));
    expect(core.state.lastPromptFeedback).toEqual(
      expect.objectContaining({
        id: second.feedback.id,
        effect: "queued for signing and send",
      }),
    );

    await vi.advanceTimersToNextTimerAsync();
    await Promise.resolve();
    await Promise.resolve();

    expect(core.state.lastPromptFeedback).toEqual(
      expect.objectContaining({
        id: second.feedback.id,
      }),
    );
    expect(core.state.lastPromptFeedback.effect).not.toBe("prompt_control overall timeout");
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("times out gameplay_action requests that were sent but never acked", async () => {
    const { core, sentMessages } = await setupConnectedSemanticCore();

    expect(core.sendGameplayAction({
      protocol_action: "gameplay_action.submit",
      action_id: "inspect_target",
      target_agent_id: "agent-0",
    })).toEqual(expect.objectContaining({ ok: true }));

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "gameplay_action",
            request: expect.objectContaining({
              action_id: "inspect_target",
              target_agent_id: "agent-0",
            }),
          }),
        ]),
      );
    });
    expect(core.state.lastGameplayActionFeedback).toEqual(
      expect.objectContaining({
        stage: "sent",
        effect: "gameplay action sent; waiting for ack",
      }),
    );

    expect(core.expirePendingGameplayActionAckTimeoutForTest()).toBe(true);

    expect(core.state.lastGameplayActionFeedback).toEqual(
      expect.objectContaining({
        stage: "unknown",
        ok: false,
        accepted: false,
        effect: "gameplay_action ack timeout",
        reason: "gameplay_action timed out waiting for ack/error from live server",
      }),
    );
    expect(core.sendGameplayAction({ protocol_action: "gameplay_action.submit", action_id: "inspect_target", target_agent_id: "agent-0" })).toMatchObject({ ok: false });
    expect(sentMessages.filter(message => message.type === "gameplay_action")).toHaveLength(1);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("signs actor-scoped gameplay actions with the Rust auth payload field order", async () => {
    const signSpy = vi.spyOn(window.crypto.subtle, "sign");
    const { core, sentMessages } = await setupConnectedSemanticCore();

    expect(core.sendGameplayAction({
      protocol_action: "gameplay_action.submit",
      action_id: "claim_agent",
      target_agent_id: "agent-claim-target",
      actor_agent_id: "agent-0",
    })).toEqual(expect.objectContaining({ ok: true }));

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "gameplay_action",
            request: expect.objectContaining({
              action_id: "claim_agent",
              target_agent_id: "agent-claim-target",
              actor_agent_id: "agent-0",
            }),
          }),
        ]),
      );
    });
    const request = sentMessages.find((message) => message.type === "gameplay_action").request;
    const actualSigningPayload = new Uint8Array(signSpy.mock.calls.at(-1)[2]);
    const expectedSigningPayload = buildAuthEnvelope({
      operation: "gameplay_action",
      action_id: "claim_agent",
      target_agent_id: "agent-claim-target",
      actor_agent_id: "agent-0",
      player_id: request.auth.player_id,
      public_key: request.auth.public_key,
      nonce: request.auth.nonce,
    });

    expect(Array.from(actualSigningPayload)).toEqual(Array.from(expectedSigningPayload));
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps first-agent claim pending without binding or advancing on transport ack", async () => {
    vi.useFakeTimers();
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore({
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
          agent_player_bindings: {},
          agent_player_public_key_bindings: {},
        },
        player_gameplay: {
          ...sampleSnapshot().player_gameplay,
          blocker_kind: "runtime_snapshot_empty_entities",
          available_actions: [
            {
              action_id: "request_snapshot",
              label: "Refresh gameplay snapshot",
              protocol_action: "request_snapshot",
              disabled_reason: null,
            },
            {
              action_id: "advance_step",
              label: "Advance 1 step",
              protocol_action: "live_control.step",
              disabled_reason: null,
            },
          ],
        },
      }),
      agentId: "starter-agent-0",
    });
    sentMessages.length = 0;

    core.state.auth.boundAgentId = null;
    core.state.auth.pendingRequestedAgentId = null;
    core.state.lastGameplayActionFeedback = { action: "claim_first_agent", agentId: "starter-agent-0", stage: "sent" };
    sockets[0].receive({
      type: "gameplay_action_ack",
      ack: {
        action_id: "claim_first_agent",
        target_agent_id: "starter-agent-0",
        player_id: "local-test-player-bound",
        accepted_at_tick: 1,
        message: "queued gameplay action claim_first_agent for starter-agent-0",
      },
    });

    expect(core.state.auth.boundAgentId).toBeNull();
    expect(core.state.lastGameplayActionFeedback).toMatchObject({ stage: "pending", ok: false, accepted: false });
    expect(sentMessages).toContainEqual({ type: "request_snapshot" });
    expect(sentMessages.some((message) => message.type === "live_control")).toBe(false);

    vi.advanceTimersByTime(1200);
    expect(sentMessages.some((message) => message.type === "live_control")).toBe(false);
    expect(core.state.auth.boundAgentId).toBeNull();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("resolves pending session registration when runtime returns session_registered", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://198.51.100.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    core.injectSnapshot(sampleSnapshot());
    core.applySelection({ kind: "agent", id: "agent-0" });
    core.state.auth = {
      ...core.state.auth,
      available: true,
      playerId: "local-test-player-bound",
      publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
      privateKey: "07".repeat(32),
      source: "local_test_api_ephemeral",
      registrationStatus: "registered",
      runtimeStatus: "issued",
      boundAgentId: "agent-0",
      syncInFlight: false,
    };

    await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
    expect((await import("./viewer_auth_session_module.js")).hasSigningIdentity(core.state.auth)).toBe(true);
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });
    const registerPromise = core.registerPlayerSessionForTest("agent-0");

    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "register_session",
              request: expect.objectContaining({
                player_id: "local-test-player-bound",
                requested_agent_id: "agent-0",
              }),
            }),
          }),
        ]),
      );
    });

    sockets[0].receive({
      type: "authoritative_recovery_ack",
      ack: {
        status: "session_registered",
        player_id: "local-test-player-bound",
        session_pubkey: "abcdef0123456789abcdef0123456789".repeat(2),
        agent_id: "agent-0",
      },
    });
    await expect(registerPromise).resolves.toEqual(
      expect.objectContaining({ status: "session_registered" }),
    );

  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not resolve registration from an ordinary runtime action ACK", async () => {
    clearActiveCleanup();
    vi.resetModules();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://198.51.100.1:5011",
    );
    window.localStorage.clear();
    document.body.innerHTML = "";
    const { sockets, sentMessages } = installMockWebSocket();
    const core = await import("./legacy_core.js");

    await core.initializeSoftwareSafeCore();
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });
    core.injectSnapshot(sampleSnapshot());
    core.applySelection({ kind: "agent", id: "agent-0" });
    core.state.auth = {
      ...core.state.auth,
      available: true,
      playerId: "local-test-player-bound",
      publicKey: "abcdef0123456789abcdef0123456789".repeat(2),
      privateKey: "07".repeat(32),
      source: "local_test_api_ephemeral",
      registrationStatus: "registered",
      runtimeStatus: "issued",
      boundAgentId: "agent-0",
      syncInFlight: false,
    };

    await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
    const registerPromise = core.registerPlayerSessionForTest("agent-0");
    await waitFor(() => {
      expect(sentMessages).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            type: "authoritative_recovery",
            command: expect.objectContaining({
              mode: "register_session",
              request: expect.objectContaining({
                player_id: "local-test-player-bound",
                requested_agent_id: "agent-0",
              }),
            }),
          }),
        ]),
      );
    });

    core.state.connectionStatus = "error";
    core.state.lastError = "window.unhandledrejection: player session registration timed out waiting for ack/error from live server";
    core.state.auth.runtimeStatus = "error";
    core.state.auth.error = "player session registration timed out waiting for ack/error from live server";
    core.state.auth.recoveryErrorCode = "session_register_timeout";
    core.state.auth.recoveryErrorMessage = core.state.auth.error;

    sockets[0].receive({
      type: "agent_chat_ack",
      ack: {
        agent_id: "agent-0",
        player_id: "local-test-player-bound",
        accepted_at_tick: 12,
        intent_seq: 99,
      },
    });

    let settled = false;
    void registerPromise.then(() => { settled = true; });
    await Promise.resolve();
    expect(settled).toBe(false);
    sockets[0].receive({ type: "authoritative_recovery_ack", ack: { status: "session_registered", agent_id: "agent-0", player_id: core.state.auth.playerId, session_pubkey: core.state.auth.publicKey } });
    await expect(registerPromise).resolves.toEqual(
      expect.objectContaining({
        agent_id: "agent-0",
        player_id: "local-test-player-bound",
      }),
    );
    expect(core.state.connectionStatus).toBe("connected");
    expect(core.state.lastError).toBeNull();
    expect(core.state.auth.error).toBeNull();
    expect(core.state.auth.recoveryErrorCode).toBeNull();
    expect(core.state.auth.recoveryErrorMessage).toBeNull();
    expect(core.expirePendingSessionRegisterWaiterForTest()).toBe(false);
  }, HEAVY_UI_TEST_TIMEOUT_MS);
});
