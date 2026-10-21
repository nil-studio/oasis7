import { beforeEach, describe, expect, it, vi } from "vitest";
import { buildAuthEnvelope } from "./viewer_auth_crypto.js";

function installMockWebSocket() {
  const sentMessages = [];
  const sockets = [];
  class MockWebSocket {
    static OPEN = 1;
    static CONNECTING = 0;

    constructor() {
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
      this.readyState = 3;
    }

    open() {
      this.readyState = MockWebSocket.OPEN;
      for (const listener of this.listeners.get("open") || []) listener({});
    }
  }
  Object.defineProperty(window, "WebSocket", { configurable: true, value: MockWebSocket });
  return { MockWebSocket, sentMessages, sockets };
}

function installTestCrypto() {
  Object.defineProperty(window, "crypto", {
    configurable: true,
    value: {
      getRandomValues: (array) => array.fill(1),
      subtle: {
        async verify() { return true; },
        async importKey() { return { kind: "test-private-key" }; },
        async sign() { return new Uint8Array(64).fill(7).buffer; },
      },
    },
  });
}

describe("requestRefineQuote", () => {
  beforeEach(() => {
    vi.resetModules();
    window.history.replaceState({}, "", "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0");
    installTestCrypto();
  });

  it("sends the Rust-compatible signed read-only quote request and rejects invalid or disconnected calls", async () => {
    const signSpy = vi.spyOn(window.crypto.subtle, "sign");
    const { sentMessages, sockets } = installMockWebSocket();
    const core = await import("./legacy_core.js");
    await core.initializeSoftwareSafeCore();

    expect(await core.requestRefineQuote(0)).toEqual(expect.objectContaining({
      ok: false,
      reason: "refine quote requires a positive whole-number compound mass in grams",
    }));
    expect(core.state.refineQuoteRequest).toEqual(expect.objectContaining({ status: "error" }));
    expect(await core.requestRefineQuote(40)).toEqual(expect.objectContaining({
      ok: false,
      reason: "refine quote requires a connected viewer websocket",
    }));

    sockets[0].open();
    core.state.auth = {
      ...core.state.auth,
      available: true,
      playerId: "player-quote-test",
      publicKey: "09".repeat(32),
      privateKey: "07".repeat(32),
      registrationStatus: "registered",
      runtimeStatus: "registered",
      boundAgentId: "agent-0",
    };
    await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);

    expect(await window.__AW_TEST__.requestRefineQuote(40)).toEqual(expect.objectContaining({
      ok: true,
      request: expect.objectContaining({ compound_mass_g: 40 }),
    }));
    expect(core.state.refineQuoteRequest).toMatchObject({ status: "pending", error: null, requestAuthNonce: expect.any(Number), dependencyFingerprint: expect.any(String) });
    const message = sentMessages.find((entry) => entry.type === "quote_refine_compound");
    expect(message).toEqual(expect.objectContaining({
      type: "quote_refine_compound",
      request: expect.objectContaining({
        compound_mass_g: 40,
        player_id: "player-quote-test",
        public_key: "09".repeat(32),
      }),
    }));
    const actualSigningPayload = new Uint8Array(signSpy.mock.calls.at(-1)[2]);
    const expectedSigningPayload = buildAuthEnvelope({
      operation: "gameplay_action",
      action_id: "quote_refine_compound",
      target_agent_id: "compound_mass_g:40",
      player_id: message.request.auth.player_id,
      public_key: message.request.auth.public_key,
      nonce: message.request.auth.nonce,
    });
    expect(Array.from(actualSigningPayload)).toEqual(Array.from(expectedSigningPayload));
  });
});
