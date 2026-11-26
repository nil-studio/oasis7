import { afterEach, beforeEach, expect, vi } from "vitest";
import { buildTaskGame076ScenarioSnapshot } from "../gameplay_attraction_scenario.js";
import { HOSTED_PUBLIC_JOIN_DEPLOYMENT_MODE } from "../software_safe_constants.js";
import { bindFirstSnapshotAgentForTest, bindLocalTestAgent } from "./viewer_player_auth_fixture.js";
import { settleLocalTestAuthStartup } from "../viewer_test_startup.js";
export function viewerUrl() {
  return "/software_safe.html?test_api=1&connect=0&hosted_bootstrap=0&locale=en";
}
let activeCleanup = null;
export function clearActiveCleanup() {
  activeCleanup?.();
  activeCleanup = null;
}
export function setActiveCleanup(fn) {
  activeCleanup = fn;
}
export const HEAVY_UI_TEST_TIMEOUT_MS = 60000;
const TEST_ED25519_PKCS8_PREFIX = new Uint8Array([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
  0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);
export function createTestCrypto() {
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
export function installMockWebSocket() {
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
export function elementPrecedes(first, second) {
  return Boolean(first?.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING);
}
export function sampleSnapshot(overrides = {}) {
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
export function sampleAgentClaimSnapshot() {
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
export function sampleHostedPublicJoinAccess(overrides = {}) {
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
export async function renderViewerApp({
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
  const core = await import("../legacy_core.js");
  const main = await import("../main.jsx");
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
    await (await import("../viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
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
export async function renderViewerAppThroughAutoMount({ snapshot = sampleSnapshot(), search }) {
  activeCleanup?.();
  activeCleanup = null;
  vi.resetModules();
  window.history.replaceState({}, "", search);
  window.localStorage.clear();
  document.body.innerHTML = "";
  const core = await import("../legacy_core.js");
  await core.initializeSoftwareSafeCore();
  core.setViewerLocale("en");
  if (snapshot) {
    core.injectSnapshot(snapshot);
    bindFirstSnapshotAgentForTest(core, snapshot);
  }
  const appRoot = document.createElement("div");
  appRoot.id = "app";
  document.body.appendChild(appRoot);
  await import("../main.jsx");
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
export async function setupConnectedSemanticCore({
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
  const core = await import("../legacy_core.js");
  await core.initializeSoftwareSafeCore();
  bindLocalTestAgent(core, agentId);
  await (await import("../viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
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
export function installMainTestLifecycleHooks() {
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
}
