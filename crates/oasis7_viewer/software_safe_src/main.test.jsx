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
function viewerUrl() {
  return "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en";
}
let activeCleanup = null;
const HEAVY_UI_TEST_TIMEOUT_MS = 60000;
const TEST_ED25519_PKCS8_PREFIX = new Uint8Array([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
  0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);
function createTestCrypto() {
  const privateBytes = new Uint8Array(32).fill(7);
  const publicBytes = new Uint8Array(32).fill(9);
  return {
    getRandomValues(bytes) { return bytes.fill(42); },
    subtle: {
      async generateKey() {
        return {
          privateKey: { kind: "test-ed25519-private" },
          publicKey: { kind: "test-ed25519-public" },
        };
      },
      async exportKey(format) {
        if (format === "pkcs8") {
          const out = new Uint8Array(TEST_ED25519_PKCS8_PREFIX.length + privateBytes.length);
          out.set(TEST_ED25519_PKCS8_PREFIX, 0);
          out.set(privateBytes, TEST_ED25519_PKCS8_PREFIX.length);
          return out.buffer;
        }
        if (format === "raw") {
          return publicBytes.buffer.slice(0);
        }
        throw new Error(`unsupported test key export: ${format}`);
      },
      async verify() { return true; },
      async importKey() {
        return { kind: "test-ed25519-imported" };
      },
      async sign() {
        return new Uint8Array(64).fill(11).buffer;
      },
    },
  };
}
function installMockWebSocket() {
  const sentMessages = [];
  const sockets = [];
  class MockWebSocket {
    static CONNECTING = 0;
    static OPEN = 1;
    static CLOSING = 2;
    static CLOSED = 3;
    constructor(url) {
      this.url = url;
      this.readyState = MockWebSocket.CONNECTING;
      this.listeners = new Map();
      sockets.push(this);
    }
    addEventListener(type, listener) {
      const listeners = this.listeners.get(type) || [];
      listeners.push(listener);
      this.listeners.set(type, listeners);
    }
    send(payload) {
      sentMessages.push(JSON.parse(payload));
    }
    close() {
      if (this.readyState === MockWebSocket.CLOSED) return;
      this.readyState = MockWebSocket.CLOSED;
      this.emit("close", {});
    }
    open() {
      this.readyState = MockWebSocket.OPEN;
      this.emit("open", {});
    }
    receive(message) {
      this.emit("message", { data: JSON.stringify(message) });
    }
    emit(type, event) {
      for (const listener of this.listeners.get(type) || []) {
        listener(event);
      }
    }
  }
  Object.defineProperty(window, "WebSocket", {
    configurable: true,
    value: MockWebSocket,
  });
  return { MockWebSocket, sockets, sentMessages };
}
function elementPrecedes(first, second) {
  return Boolean(first?.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING);
}
function sampleSnapshot(overrides = {}) {
  const base = buildTaskGame076ScenarioSnapshot();
  return {
    ...base,
    ...overrides,
    config: {
      ...base.config,
      ...(overrides.config || {}),
    },
    model: {
      ...base.model,
      agent_player_bindings: {
        "agent-0": "local-test-player-bound",
      },
      agent_player_public_key_bindings: {
        "agent-0": "abcdef0123456789abcdef0123456789".repeat(2),
      },
      ...(overrides.model || {}),
    },
    player_gameplay: {
      ...base.player_gameplay,
      ...(overrides.player_gameplay || {}),
    },
  };
}
function sampleAgentClaimSnapshot() {
  const base = sampleSnapshot();
  return sampleSnapshot({
    model: {
      ...base.model,
      agents: {
        ...base.model.agents,
        "agent-claim-target": {
          id: "agent-claim-target",
          name: "Claim Target",
          location_id: "loc-0",
          resources: {},
        },
      },
    },
    player_gameplay: {
      ...base.player_gameplay,
      agent_claim: {
        claimer_agent_id: "agent-0",
        current_epoch: 0,
        reputation_tier: 0,
        claim_cap: 1,
        owned_claim_count: 0,
        liquid_main_token_balance: 0,
        restricted_starter_claim_balance: 0,
        slot_1_auto_restricted_starter_claim_amount: 325,
        slot_1_eligible_claim_balance: 325,
        next_claim_quote: {
          slot_index: 1,
          reputation_tier: 0,
          claim_cap: 1,
          owned_claim_count: 0,
          activation_fee_amount: 100,
          claim_bond_amount: 200,
          upkeep_per_epoch: 25,
          total_upfront_amount: 325,
          transferable_liquid_balance: 0,
          restricted_starter_claim_balance: 0,
          auto_restricted_starter_claim_amount: 325,
          eligible_claim_balance: 325,
          release_cooldown_epochs: 2,
          grace_epochs: 4,
          idle_warning_epochs: 8,
          forced_idle_reclaim_epochs: 10,
          forced_reclaim_penalty_bps: 2000,
        },
        owned_claims: [],
      },
    },
  });
}
function sampleHostedPublicJoinAccess(overrides = {}) {
  return {
    deployment_mode: HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE,
    action_matrix: [
      {
        action_id: "prompt_control_apply",
        required_auth: "strong_auth",
        availability: "public_player_plane_with_backend_reauth_preview",
        reason: "prompt_control_apply is available through browser-local player auth plus backend re-authorization",
      },
      {
        action_id: "main_token_transfer",
        required_auth: "strong_auth",
        availability: "blocked_until_strong_auth",
        reason: "main_token_transfer remains blocked until a higher-trust hosted strong-auth lane exists",
      },
    ],
    ...overrides,
  };
}
async function renderViewerApp({
  snapshot = sampleSnapshot(),
  selection = null,
  search = viewerUrl(),
  setupCore = null,
  setupAfterMount = null,
  autoBindSnapshotAgent = true,
  starterOcOnboardingComplete = true,
} = {}) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", search);
  window.localStorage.clear();
  document.body.innerHTML = "";
  const core = await import("./legacy_core.js");
  const main = await import("./main.jsx");
  const { mountViewerApp } = main;
  const appRoot = document.createElement("div");
  appRoot.id = "app";
  document.body.appendChild(appRoot);
  await core.initializeSoftwareSafeCore();
  core.setViewerLocale("en");
  if (snapshot) {
    core.injectSnapshot(snapshot);
    if (autoBindSnapshotAgent) {
      bindFirstSnapshotAgentForTest(core, snapshot);
    }
  }
  if (selection) {
    core.applySelection(selection);
  }
  if (setupCore) {
    setupCore(core);
  }
  if (autoBindSnapshotAgent) {
    bindFirstSnapshotAgentForTest(core, core.state.snapshot);
  }
  if (setupAfterMount) {
    setupAfterMount(core);
  }
  if (starterOcOnboardingComplete) {
    main.__markStarterOcOnboardingCompleteForTest(core.state.auth.boundAgentId);
  }
  if (/^[0-9a-f]{64}$/i.test(core.state.auth.privateKey || "")) {
    await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
  }
  const dispose = mountViewerApp(appRoot);
  const cleanup = () => {
    dispose();
    if (activeCleanup === cleanup) {
      activeCleanup = null;
    }
  };
  activeCleanup = cleanup;
  return {
    core,
    cleanup,
    container: appRoot,
  };
}
async function renderViewerAppThroughAutoMount({ snapshot = sampleSnapshot(), search }) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", search);
  window.localStorage.clear();
  document.body.innerHTML = "";
  const core = await import("./legacy_core.js");
  await core.initializeSoftwareSafeCore();
  core.setViewerLocale("en");
  if (snapshot) {
    core.injectSnapshot(snapshot);
    bindFirstSnapshotAgentForTest(core, snapshot);
  }
  const appRoot = document.createElement("div");
  appRoot.id = "app";
  document.body.appendChild(appRoot);
  await import("./main.jsx");
  const cleanup = () => {
    appRoot.textContent = "";
    if (activeCleanup === cleanup) {
      activeCleanup = null;
    }
  };
  activeCleanup = cleanup;
  return {
    core,
    cleanup,
    container: appRoot,
  };
}
async function setupConnectedSemanticCore({
  snapshot = sampleSnapshot(),
  agentId = "agent-0",
  agentChatOverallTimeoutMs = null,
} = {}) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  const agentChatTimeoutParam = agentChatOverallTimeoutMs == null
    ? ""
    : `&agent_chat_overall_timeout_ms=${encodeURIComponent(String(agentChatOverallTimeoutMs))}`;
  window.history.replaceState(
    {},
    "",
    `/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011${agentChatTimeoutParam}`,
  );
  window.localStorage.clear();
  document.body.innerHTML = "";
  const { sockets, sentMessages } = installMockWebSocket();
  const core = await import("./legacy_core.js");
  await core.initializeSoftwareSafeCore();
  bindLocalTestAgent(core, agentId);
  await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
  sockets[0].open();
  sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });
  await settleLocalTestAuthStartup(core, sentMessages);
  sockets[0].receive({ type: "authoritative_recovery_ack", ack: { status: "catch_up_ready", player_id: core.state.auth.playerId, session_pubkey: core.state.auth.publicKey, agent_id: agentId, session_epoch: 1, binding_epoch: 1 } });
  sentMessages.length = 0;
  core.injectSnapshot(snapshot);
  core.applySelection({ kind: "agent", id: agentId });
  activeCleanup = () => {
    for (const socket of sockets) {
      if (socket.readyState !== socket.CLOSED) {
        socket.close();
      }
    }
    activeCleanup = null;
  };
  return { core, sockets, sentMessages };
}
beforeEach(() => {
  vi.restoreAllMocks();
  // Legacy fixtures explicitly advertise v2 absence; outages must never permit fallback.
  vi.spyOn(globalThis, "fetch").mockResolvedValue({ ok: false, status: 404, json: async () => ({ ok: false, error_code: "not_found" }) });
  window.history.replaceState({}, "", viewerUrl());
  window.localStorage.clear();
  Object.defineProperty(window, "crypto", {
    configurable: true,
    value: createTestCrypto(),
  });
  document.body.innerHTML = "";
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  activeCleanup?.();
  activeCleanup = null;
  document.body.innerHTML = "";
});

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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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
      ...within(stagePanel).getAllByRole("button", { name: /Claim First Agent|Waiting for result|Result unknown/ }),
      ...within(targetsPanel).getAllByRole("button", { name: /Claim First Agent|Waiting for result|Result unknown/ }),
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
    activeCleanup?.();
    activeCleanup = null;
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
    activeCleanup?.();
    activeCleanup = null;
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

  it("shows a cookie-authenticated account waiting for world entry without granting control", async () => {
    let account = { account_id: "synthetic-cookie-account", display_name: "Synthetic Cookie Player" };
    globalThis.fetch.mockImplementation(async url => {
      const result = String(url).endsWith("/capabilities") ? { ok: true, csrf_token: "synthetic-csrf", passkey: { available: true } }
        : String(url).endsWith("/session") ? { ok: true, account }
        : String(url).endsWith("/credentials") ? { ok: true, credentials: [] }
        : String(url).endsWith("/sessions") ? { ok: true, sessions: [] }
        : String(url).endsWith("/logout") ? (account = null, { ok: true })
        : { ok: true };
      return { ok: true, status: 200, json: async () => result };
    });
    const { core } = await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = { available: false, playerId: null, publicKey: null, boundAgentId: null, source: "guest_only", registrationStatus: "guest", runtimeStatus: "guest" };
      },
    });
    const dialog = await screen.findByRole("dialog", { name: "Enter the World to Play" });
    expect(within(dialog).getByText("Signed in · world entry pending")).toBeInTheDocument();
    expect(within(dialog).queryByText("Not signed in")).not.toBeInTheDocument();
    expect(core.state.auth.available).toBe(false);
    expect(core.state.auth.boundAgentId).toBeNull();
    expect(globalThis.fetch.mock.calls.some(([url]) => String(url).endsWith("/world/join"))).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "退出登录" }));
    expect(await screen.findByRole("dialog", { name: "Sign In to Play" })).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("surfaces hosted recovery and preview strong-auth truth without not-implemented drift", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth.error = "session_revoked";
        core.state.auth.revokeReason = "qa-kick";
        core.state.auth.revokedBy = "qa";
        core.state.hostedLogin.handle = "player@example.com";
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(await screen.findByRole("dialog", { name: "Sign In to Play" })).toBeInTheDocument();
    expect((await screen.findAllByRole("button", { name: "Request Login Code" })).length).toBeGreaterThan(0);
    expect(screen.getAllByLabelText("Email").length).toBeGreaterThan(0);
    expect(screen.getByText("upgrade_after_player_session")).toBeInTheDocument();
    expect(
      screen.getAllByText(
        "The runtime or operator revoked this browser session by qa. Reason: qa-kick. You need to re-login to the hosted account and acquire a fresh player session before gameplay, chat, or prompt actions can continue.",
      ).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText("not_implemented")).not.toBeInTheDocument();
    expect(screen.queryByText(/not implemented yet/i)).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("does not show the hosted login gate after player session registration", async () => {
    await renderViewerApp({
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "hosted-player-1",
          publicKey: "oc:pk:test-player",
          privateKey: "ed25519-secret",
          releaseToken: "hosted-release-1",
          source: "hosted_browser_storage",
          registrationStatus: "registered",
          runtimeStatus: "registered",
        };
      },
    });

    expect(screen.queryByRole("dialog", { name: "Sign In to Play" })).not.toBeInTheDocument();
  });

  it("surfaces hosted login retry-after guidance when OTP resend is throttled", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.hostedLogin.handle = "player@example.com";
        core.state.hostedLogin.error = "a login code was just sent for this email; retry in 21 seconds (retry in 21s)";
        core.state.hostedLogin.retryAfterSeconds = 21;
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(
      (await screen.findAllByText("a login code was just sent for this email; retry in 21 seconds (retry in 21s)")).length,
    ).toBeGreaterThan(0);
    expect((await screen.findAllByText("retry_after=21s")).length).toBeGreaterThan(0);
  });

  it("clears hosted login retry guidance immediately when the player edits the email", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.hostedLogin.handle = "player@example.com";
        core.state.hostedLogin.error = "a login code was just sent for this email; retry in 21 seconds (retry in 21s)";
        core.state.hostedLogin.retryAfterSeconds = 21;
      },
    });

    expect((await screen.findAllByText("retry_after=21s")).length).toBeGreaterThan(0);

    fireEvent.input(document.getElementById("gate-hosted-login-handle"), {
      target: { value: "next-player@example.com" },
    });

    expect(screen.queryByText("retry_after=21s")).not.toBeInTheDocument();
    expect(screen.queryByText(/retry in 21 seconds/i)).not.toBeInTheDocument();
  });

  it("keeps keyboard focus inside the hosted login gate while it is modal", async () => {
    await renderViewerApp({
      autoBindSnapshotAgent: false,
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.hostedLogin.handle = "player@example.com";
      },
    });

    const dialog = await screen.findByRole("dialog", { name: "Sign In to Play" });
    const emailInput = await within(dialog).findByLabelText("Email");
    const requestButton = await within(dialog).findByRole("button", { name: "Request Login Code" });

    await waitFor(() => {
      expect(document.activeElement).toBe(emailInput);
    });

    requestButton.focus();
    const tabEvent = new KeyboardEvent("keydown", {
      key: "Tab",
      bubbles: true,
      cancelable: true,
    });
    dialog.dispatchEvent(tabEvent);

    expect(tabEvent.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(emailInput);
  });

  it("marks hosted backend reauth as available once a browser player session is registered", async () => {
    await renderViewerApp({
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "hosted-player-1",
          publicKey: "oc:pk:test-player",
          privateKey: "ed25519-secret",
          releaseToken: "hosted-release-1",
          source: "hosted_browser_storage",
          registrationStatus: "registered",
          runtimeStatus: "registered",
        };
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(screen.getAllByRole("button", { name: "Release Player Session" }).length).toBeGreaterThan(0);
    expect(screen.getByText("active_hosted_session")).toBeInTheDocument();
    expect(screen.getByText("preview_backend_reauth_available")).toBeInTheDocument();
    expect(
      screen.getByText(
        "hosted preview backend reauth is available after the browser device-session-backed player_session has completed runtime registration for prompt_control",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("page reload will reuse the hosted device session, mint a fresh browser session key, and attempt reconnect_sync first"),
    ).toBeInTheDocument();
    const assetLane = screen.getByText("Asset / Governance Lane").closest("section");
    expect(assetLane).toBeTruthy();
    expect(
      within(assetLane).getAllByText(/main_token_transfer remains blocked until a higher-trust hosted strong-auth lane exists/i).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText("not_implemented")).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps hosted backend reauth pending until runtime registration finishes", async () => {
    await renderViewerApp({
      setupAfterMount(core) {
        core.state.hostedAccess = sampleHostedPublicJoinAccess();
        core.state.auth = {
          ...core.state.auth,
          available: true,
          playerId: "hosted-player-2",
          publicKey: "oc:pk:test-player-2",
          privateKey: "ed25519-secret-2",
          releaseToken: "hosted-release-2",
          source: "hosted_browser_storage",
          registrationStatus: "issued",
          runtimeStatus: "issued",
        };
      },
    });

    screen.getByText("Runtime Diagnostics").click();
    expect(screen.getAllByText("issued_pending_register").length).toBeGreaterThan(0);
    expect(screen.queryByText("preview_backend_reauth_available")).not.toBeInTheDocument();
    expect(
      screen.getByText(
        "hosted preview backend reauth stays pending until the browser device-session-backed player_session finishes runtime registration",
      ),
    ).toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("installs visual fixture states for screenshot parity without production semantics", async () => {
    const states = [
      "shell_selected_blocker",
      "agent_chat_history",
      "gameplay_diagnostics_expanded",
      "factory_production_failure_disposition",
      "hosted_login_gate",
      "empty_world_recovery",
      "major_world_event_crisis",
    ];

    for (const fixtureName of states) {
      const { cleanup, container } = await renderViewerApp({
        snapshot: null,
        search: `${viewerUrl()}&viewer_visual_fixture=${fixtureName}`,
      });

      expect(window.__OASIS7_VIEWER_VISUAL_FIXTURES__).toBeTruthy();
      expect(container).toHaveAttribute("data-viewer-visual-fixture", fixtureName);
      expect(document.body).toHaveAttribute("data-viewer-visual-fixture", fixtureName);

      if (fixtureName === "factory_production_failure_disposition") {
        const card = within(container).getByTestId("viewer-factory-production-failure-disposition");
        expect(within(card).getByText("Production result failed validation")).toBeInTheDocument();
        expect(card).toHaveAttribute("role", "status");
        expect(card).toHaveAttribute("aria-live", "polite");
        expect(card).toHaveTextContent("Factory: factory.target · Recipe: recipe.smelter.iron_ingot");
        expect(card).toHaveTextContent("Consumed inputs");
        expect(card).toHaveTextContent("iron_ore × 3");
        expect(card).toHaveTextContent("Consumed power");
        expect(card).toHaveTextContent("Refresh gameplay snapshot");
        expect(card).toHaveTextContent("Recovery action: request_snapshot");
        expect(card).toHaveTextContent("Next recheck: next committed snapshot");
        expect(card.textContent).not.toMatch(
          /product_validation_rejected|consumed_lost|inspect_product_validation_and_reschedule/,
        );
        await waitFor(() => expect(container.querySelector("#viewer-gameplay-details")).toHaveAttribute("open"));
        expect(elementPrecedes(
          card,
          within(container.querySelector("#viewer-gameplay-details")).getByText("Control Proof").closest(".event-card"),
        )).toBe(true);
      }

      cleanup();
    }
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("keeps visual fixture query parameters inert without test_api", async () => {
    const { container } = await renderViewerAppThroughAutoMount({
      snapshot: null,
      search: "/software_safe.html?connect=0&hosted_bootstrap=0&locale=en&viewer_visual_fixture=gameplay_diagnostics_expanded",
    });

    expect(window.__OASIS7_VIEWER_VISUAL_FIXTURES__).toBeUndefined();
    expect(container).not.toHaveAttribute("data-viewer-visual-fixture");
    expect(document.body).not.toHaveAttribute("data-viewer-visual-fixture");
    expect(container.querySelector("#viewer-gameplay-details")).not.toHaveAttribute("open");
    expect(container.querySelector("#viewer-gameplay-details")).toHaveProperty("open", false);
    expect(container.querySelector("#viewer-diagnostics-panel")).not.toHaveAttribute("open");
    expect(elementPrecedes(
      container.querySelector(".stage-hero"),
      container.querySelector("#viewer-gameplay-details"),
    )).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders the agent chat fixture with history and collapsed prompt controls", async () => {
    const { container } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=agent_chat_history`,
    });

    const detailsPanel = container.querySelector("#viewer-details-panel");
    const commandSurface = detailsPanel.querySelector(".command-surface");
    expect(commandSurface).toHaveAttribute("data-command-agent", "agent-0");
    expect(commandSurface).toHaveAttribute("data-command-chat-history", "3");
    const diagnosticStrip = commandSurface.querySelector(".command-surface__diagnostic-strip");
    expect(diagnosticStrip).toBeTruthy();
    expect(within(diagnosticStrip).getByText("prompt=enabled")).toHaveClass("badge--good");
    expect(within(diagnosticStrip).getByText("chat=enabled")).toHaveClass("badge--good");
    expect(within(diagnosticStrip).getByText(/mainToken=/)).toHaveClass("badge--good");
    expect(commandSurface.querySelector(".command-surface__chat-panel")).toBeTruthy();
    expect(elementPrecedes(
      commandSurface.querySelector(".command-surface__chat-panel"),
      commandSurface.querySelector(".command-surface__advanced-panel"),
    )).toBe(true);
    expect(within(detailsPanel).getByText("Chat Ready")).toBeInTheDocument();
    expect(within(detailsPanel).getByText("Awaiting material recovery before the smelter can proceed.")).toBeInTheDocument();
    expect(within(detailsPanel).getByText("Hold position and confirm the blocker.")).toBeInTheDocument();
    expect(commandSurface.querySelectorAll(".event-card--chat-player").length).toBeGreaterThan(0);
    expect(commandSurface.querySelectorAll(".event-card--chat-agent").length).toBeGreaterThan(0);
    expect(within(detailsPanel).getByText("state=hidden_by_default")).toBeInTheDocument();
    expect(within(detailsPanel).queryByLabelText("System Prompt Override")).not.toBeInTheDocument();
    expect(within(detailsPanel).getAllByText(/no transfer form/i).length).toBeGreaterThan(0);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("opens gameplay details and diagnostics for the diagnostics visual fixture", async () => {
    const { container } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=gameplay_diagnostics_expanded`,
    });

    await waitFor(() => {
      expect(container.querySelector("#viewer-gameplay-details")).toHaveAttribute("open");
      expect(container.querySelector("#viewer-diagnostics-panel")).toHaveAttribute("open");
    });
    expect(within(container.querySelector("#viewer-gameplay-details")).getByText("Formal Gameplay Summary")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-gameplay-details")).getByText("Capability Economics")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-diagnostics-panel")).getByText("Execution Lanes")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-diagnostics-panel")).getByText("state sync")).toBeInTheDocument();
    expect(elementPrecedes(
      container.querySelector("#viewer-gameplay-details"),
      container.querySelector(".stage-hero"),
    )).toBe(true);
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("forces a hosted login gate fixture with reserved retry/error guidance", async () => {
    await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=hosted_login_gate`,
    });

    const dialog = await screen.findByRole("dialog", { name: "Sign In to Play" });
    expect(dialog).toHaveAttribute("data-viewer-fixture-state", "hosted_login_gate");
    expect(await within(dialog).findByDisplayValue("player@example.com")).toBeInTheDocument();
    expect(within(dialog).getByText("challenge=fixture-challenge")).toBeInTheDocument();
    expect(within(dialog).getByText("Enter the latest verification code to continue.")).toBeInTheDocument();
    expect(within(dialog).getByText("retry_after=18s")).toBeInTheDocument();
    expect(screen.queryByText(/wallet/i)).not.toBeInTheDocument();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

  it("renders empty-world recovery as intentional recovery instead of selected-agent acceptance", async () => {
    const { container } = await renderViewerApp({
      snapshot: null,
      search: `${viewerUrl()}&viewer_visual_fixture=empty_world_recovery`,
    });

    const state = window.__AW_TEST__.getState();
    expect(state.selectedKind).toBe(null);
    expect(state.selectedId).toBe(null);
    expect(state.gameplaySummary.blockerKind).toBe("runtime_snapshot_empty_entities");
    expect(within(container.querySelector("#viewer-targets-panel")).getByText("No agents in current snapshot.")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-stage-panel")).getByText("Recover World Snapshot")).toBeInTheDocument();
    expect(within(container.querySelector("#viewer-stage-panel")).getAllByText("No published entities yet").length).toBeGreaterThan(0);
    expect(within(container.querySelector("#viewer-stage-panel")).getAllByText(/Request a fresh snapshot|Reload the authoritative snapshot|first published entity/i).length).toBeGreaterThan(0);
    expect(within(container.querySelector("#viewer-stage-panel")).getAllByText("Request snapshot").length).toBeGreaterThan(0);
    expect(within(container.querySelector("#viewer-details-panel")).getByText("Claim Your First Agent")).toBeInTheDocument();
    expect(container.querySelector("[data-callout-kind='empty_world_recovery']")).toBeTruthy();
  }, HEAVY_UI_TEST_TIMEOUT_MS);

});
