import { afterEach, describe, expect, it, vi } from "vitest";
import { buildTaskGame076ScenarioSnapshot } from "./gameplay_attraction_scenario.js";
import { createViewerAgentChatAuthModule } from "./viewer_agent_chat_auth_module.js";
import { settleLocalTestAuthStartup } from "./viewer_test_startup.js";

const originalCryptoDescriptor = Object.getOwnPropertyDescriptor(window, "crypto");
const originalWebSocketDescriptor = Object.getOwnPropertyDescriptor(window, "WebSocket");
const ED25519_PKCS8_PREFIX = new Uint8Array([
  0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06,
  0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
]);

function makeRequest() {
  return {
    agent_id: "agent-0",
    message: "Keep the forge queue moving.",
    player_id: "player-1",
    public_key: "ab".repeat(32),
  };
}

function installMockWebSocket() {
  const sentMessages = [];
  const sockets = [];
  class MockWebSocket {
    static OPEN = 1;
    static CONNECTING = 0;
    static CLOSED = 3;

    constructor() {
      this.readyState = MockWebSocket.CONNECTING;
      this.listeners = new Map();
      sockets.push(this);
    }

    addEventListener(type, listener) {
      this.listeners.set(type, [...(this.listeners.get(type) || []), listener]);
    }

    send(payload) {
      sentMessages.push(JSON.parse(payload));
    }

    open() {
      this.readyState = MockWebSocket.OPEN;
      this.emit("open", {});
    }

    receive(message) {
      this.emit("message", { data: JSON.stringify(message) });
    }

    close() {
      this.readyState = MockWebSocket.CLOSED;
      this.emit("close", {});
    }

    emit(type, event) {
      for (const listener of this.listeners.get(type) || []) listener(event);
    }
  }
  Object.defineProperty(window, "WebSocket", { configurable: true, value: MockWebSocket });
  return { sentMessages, sockets };
}
afterEach(() => {
  vi.restoreAllMocks();
  if (originalCryptoDescriptor) {
    Object.defineProperty(window, "crypto", originalCryptoDescriptor);
  }
  if (originalWebSocketDescriptor) {
    Object.defineProperty(window, "WebSocket", originalWebSocketDescriptor);
  }
  document.body.innerHTML = "";
});
describe("viewer agent chat auth", () => {
  it("projects current world-feed authority into the request and exact signing payload", async () => {
    const state = {
      worldFeed: {
        status: "ready",
        stale: false,
        worldId: "runtime-world-7",
        reorgEpoch: 12,
      },
    };
    const buildAuthEnvelope = vi.fn((payload) => payload);
    const signAuthPayload = vi.fn(async () => "awviewauth:v1:signed");
    const module = createViewerAgentChatAuthModule({
      buildAuthEnvelope,
      nextAuthNonce: () => 41,
      signAuthPayload,
      state,
    });
    const request = makeRequest();
    request.intent_tick = 18;
    request.intent_seq = 41;

    await expect(module.buildAuthProof(request, {
      playerId: request.player_id,
      publicKey: request.public_key,
    })).resolves.toEqual({
      scheme: "ed25519",
      player_id: request.player_id,
      public_key: request.public_key,
      nonce: 41,
      signature: "awviewauth:v1:signed",
    });
    expect(request).toMatchObject({
      world_id: "runtime-world-7",
      reorg_epoch: 12,
      authority_scope: "player_agent_chat",
    });
    expect(Object.keys(buildAuthEnvelope.mock.calls[0][0])).toEqual([
      "operation",
      "agent_id",
      "player_id",
      "public_key",
      "nonce",
      "message",
      "intent_tick",
      "intent_seq",
      "world_id",
      "reorg_epoch",
      "authority_scope",
    ]);
    expect(buildAuthEnvelope).toHaveBeenCalledWith(expect.objectContaining({
      operation: "agent_chat",
      agent_id: "agent-0",
      player_id: "player-1",
      public_key: "ab".repeat(32),
      nonce: 41,
      message: "Keep the forge queue moving.",
      intent_tick: 18,
      intent_seq: 41,
      world_id: "runtime-world-7",
      reorg_epoch: 12,
      authority_scope: "player_agent_chat",
    }));
    expect(signAuthPayload).toHaveBeenCalledWith(buildAuthEnvelope.mock.calls[0][0], expect.anything());
  });

  it("signs canonical owner authority after the replacement and rejects foreign metadata", async () => {
    const canonical = { agent_id: "agent-0", player_id: "player-1", public_key: "ab".repeat(32),
      world_id: "runtime-world-7", reorg_epoch: 12, authority_scope: "player_agent_chat",
      canonical_authority: { branch_id: "branch-1", agent_identity_generation: 2 }, current_intent_id: "intent-1" };
    const state = { auth: {playerId:"player-1",publicKey:"ab".repeat(32),boundAgentId:"agent-0"}, logicalTime: 12, worldFeed: { status: "ready", worldId: "runtime-world-7", reorgEpoch: 12 },
      viewerProtocol: { capabilities: ["canonical_agent_chat_v1"] },
      canonicalAgentOwnerView: canonical, snapshot: { player_gameplay: { canonical_agent_chat: canonical } } };
    const buildAuthEnvelope = vi.fn((payload) => payload);
    const signAuthPayload = vi.fn(async () => "signed");
    const module = createViewerAgentChatAuthModule({ state, buildAuthEnvelope, signAuthPayload, nextAuthNonce: () => 42 });
    const auth = { playerId: "player-1", publicKey: "ab".repeat(32) };
    const request = makeRequest();
    await module.buildAuthProof(request, auth);
    expect(request.replaces_intent_id).toBe("intent-1");
    expect(Object.keys(buildAuthEnvelope.mock.calls[0][0]).slice(-2)).toEqual(["replaces_intent_id", "canonical_authority"]);
    expect(buildAuthEnvelope.mock.calls[0][0].canonical_authority).toEqual(canonical.canonical_authority);
    canonical.public_key = "cd".repeat(32);
    await expect(module.buildAuthProof(makeRequest(), auth)).rejects.toThrow(/authenticated canonical owner/);
    state.canonicalAgentOwnerView = null;
    await expect(module.buildAuthProof(makeRequest(), auth)).rejects.toThrow(/authenticated canonical owner/);
    expect(signAuthPayload).toHaveBeenCalledTimes(1);
  });

  it("fails closed until the current runtime authority identity is available", async () => {
    const signAuthPayload = vi.fn();
    const module = createViewerAgentChatAuthModule({
      buildAuthEnvelope: vi.fn(),
      nextAuthNonce: () => 41,
      signAuthPayload,
      state: {
        worldFeed: {
          status: "loading",
          stale: false,
          worldId: "runtime-world-7",
          reorgEpoch: null,
        },
      },
    });

    await expect(module.buildAuthProof(makeRequest(), {
      playerId: "player-1",
      publicKey: "ab".repeat(32),
    })).rejects.toThrow(/current runtime world authority/i);
    expect(signAuthPayload).not.toHaveBeenCalled();
  });

  it("sends the projected authority fields through the real Viewer chat request path", async () => {
    Object.defineProperty(window, "crypto", {
      configurable: true,
      value: {
        getRandomValues: (array) => array.fill(1),
        subtle: {
          async generateKey() {
            return { privateKey: "test-private", publicKey: "test-public" };
          },
          async exportKey(format) {
            if (format === "pkcs8") {
              return new Uint8Array([...ED25519_PKCS8_PREFIX, ...new Uint8Array(32).fill(7)]).buffer;
            }
            return new Uint8Array(32).fill(9).buffer;
          },
          async verify() { return true; },
          async importKey() {
            return { kind: "test-signing-key" };
          },
          async sign() {
            return new Uint8Array(64).fill(11).buffer;
          },
        },
      },
    });
    const { sockets, sentMessages } = installMockWebSocket();
    window.history.replaceState(
      {},
      "",
      "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011",
    );
    vi.resetModules();
    const core = await import("./legacy_core.js");
    await core.initializeSoftwareSafeCore();
    sockets[0].open();
    sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "runtime-world-7" });
    await settleLocalTestAuthStartup(core, sentMessages);
    sentMessages.length = 0;
    core.injectSnapshot(buildTaskGame076ScenarioSnapshot());
    core.applySelection({ kind: "agent", id: "agent-0" });
    core.state.auth = {
      ...core.state.auth,
      available: true,
      playerId: "player-1",
      publicKey: "ab".repeat(32),
      privateKey: "07".repeat(32),
      source: "local_test_api_ephemeral",
      registrationStatus: "registered",
      runtimeStatus: "registered",
      boundAgentId: "agent-0",
    };
    await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
    core.state.worldFeed = {
      status: "ready",
      stale: false,
      schemaVersion: "world_feed/v1",
      worldId: "runtime-world-7",
      reorgEpoch: 12,
      cursor: "runtime-world-7:12:0",
      events: [],
      gapReason: null,
      unavailableReason: null,
      snapshotReloadRequired: false,
      requestInFlight: false,
    };

    expect(core.sendAgentChat("agent-0", "Keep the forge queue moving.")).toEqual(
      expect.objectContaining({ ok: true }),
    );
    for (let attempt = 0; attempt < 20; attempt += 1) {
      if (sentMessages.some((message) => message.type === "agent_chat")) break;
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    const chatMessage = sentMessages.find((message) => message.type === "agent_chat");
    expect(chatMessage?.request).toEqual(expect.objectContaining({
      world_id: "runtime-world-7",
      reorg_epoch: 12,
      authority_scope: "player_agent_chat",
      auth: expect.objectContaining({
        player_id: "player-1",
        public_key: "ab".repeat(32),
        signature: expect.stringMatching(/^awviewauth:v1:/),
      }),
    }));
    sockets[0].receive({
      type: "agent_chat_ack",
      request_auth_nonce: chatMessage.request.auth.nonce,
      ack: { agent_id: "agent-0", player_id: "player-1", accepted_at_tick: 12 },
    });
    core.state.viewerProtocol.capabilities.push("canonical_agent_chat_v1");
    const canonical = {
      agent_id: "agent-0", player_id: "player-1", public_key: "ab".repeat(32),
      world_id: "runtime-world-7", reorg_epoch: 12, authority_scope: "player_agent_chat",
      canonical_authority: { branch_id: "branch-1", agent_identity_generation: 2 },
      current_intent_id: null, goal: null,
    };
    core.state.snapshot.player_gameplay.canonical_agent_chat = canonical; core.state.canonicalAgentOwnerView = canonical;
    sentMessages.length = 0;
    expect(core.sendAgentChat("agent-0", "Persist this owner goal.").ok).toBe(true);
    for (let attempt = 0; attempt < 20; attempt += 1) {
      const contextRequest = sentMessages.find((message) => message.type === "request_canonical_agent_owner_read_context");
      if (contextRequest && !sentMessages.some((message) => message.type === "canonical_agent_owner_read")) {
        const commit = {world: {world_id: "runtime-world-7", genesis_digest: "fixture-genesis"}, binding: {branch_id: "branch-1", reorg_generation: 12, provider_world_id:"runtime-world-7",finality_ref:"f",governing_manifest_ref:"m",authority_generation:1,permission_generation:1}, position: 1, execution_block_hash:"block",state_root_ref:"root"};
        const request = {contract_version:1,world:commit.world,scope_id:"agent:agent-0",min_commit:commit,fixed_commit:null,deadline_unix_ms:null};
        const {cborCanonicalEncode} = await import("./viewer_auth_crypto.js");
        const bytes=cborCanonicalEncode(["oasis7.world-service.v1","/v1/world/view",request]);
        sockets[0].receive({type:"canonical_agent_owner_read_context",context:{request,fence:canonical,expected:{...canonical,branch_id:canonical.canonical_authority.branch_id},signing_domain:"/v1/world/view",signing_bytes_hex:Array.from(bytes,x=>x.toString(16).padStart(2,"0")).join("")}});
      }
      const read = sentMessages.find((message) => message.type === "canonical_agent_owner_read");
      if (read) sockets[0].receive({type:"canonical_agent_owner_view",request:read.request,request_digest:"blake3:"+"aa".repeat(32),view:canonical,version:{commit:read.request.request.min_commit}});
      if (sentMessages.some((message) => message.type === "agent_chat")) break;
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    const canonicalMessage = sentMessages.find((message) => message.type === "agent_chat");
    expect(canonicalMessage.request.canonical_authority).toEqual(canonical.canonical_authority);
    sockets[0].receive({ type: "agent_chat_ack", ack: { agent_id: "agent-0", player_id: "player-1", auth_nonce: canonicalMessage.request.auth.nonce, intent_seq: canonicalMessage.request.intent_seq, intent_tick: canonicalMessage.request.intent_tick, status: "pending" } });
    expect(core.state.lastChatFeedback).toMatchObject({ stage: "pending", accepted: false, ok: false });
    expect(core.state.chatHistory.some((entry) => entry.message === "Persist this owner goal.")).toBe(false);
    canonical.current_intent_id = "canonical-intent-1";
    canonical.goal = { intent_id: "canonical-intent-1", message: "Persist this owner goal.", status: "accepted", event_seq: 1, logical_time: 12 };
    core.state.canonicalAgentOwnerView = { ...canonical };
    expect((await import("./viewer_canonical_goal_module.js")).authenticatedCanonicalGoal(core.state, "agent-0")?.message).toBe("Persist this owner goal.");
    sockets[0].receive({ type: "agent_chat_ack", ack: { agent_id: "agent-0", player_id: "player-1", auth_nonce: canonicalMessage.request.auth.nonce, intent_seq: canonicalMessage.request.intent_seq, intent_tick: canonicalMessage.request.intent_tick, status: "accepted", intent_id: "canonical-intent-1", accepted_at_tick: 12 } });
    expect(core.state.lastChatFeedback.accepted).toBe(true);
    core.state.auth = { ...core.state.auth, publicKey: "cd".repeat(32) };
    core.injectSnapshot(core.state.snapshot);
    expect((await import("./viewer_canonical_goal_module.js")).authenticatedCanonicalGoal(core.state, "agent-0")).toBeNull();
    sockets[0].close();
  });
});
