import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";
import { waitFor } from "@solidjs/testing-library";
import { bindLocalTestAgent } from "./test_support/viewer_player_auth_fixture.js";
import { settleLocalTestAuthStartup } from "./viewer_test_startup.js";
import { buildTaskGame076ScenarioSnapshot } from "./gameplay_attraction_scenario.js";
const custody = vi.hoisted(() => ({ enabled: false, pending: false, settle: null }));
vi.mock("./viewer_hosted_world_account_module.js", async original => {
  const actual = await original();
  return { ...actual, createHostedWorldAccount: dependencies => {
    const account = actual.createHostedWorldAccount(dependencies);
    return { ...account,
      rememberGameplayRequest: request => custody.enabled ? (custody.pending = true, Promise.resolve()) : account.rememberGameplayRequest(request),
      hasPendingGameplayRequest: () => custody.enabled ? custody.pending : account.hasPendingGameplayRequest(),
      settleGameplayRequest: (...args) => custody.enabled ? custody.settle(...args) : account.settleGameplayRequest(...args),
    };
  } };
});
const sampleSnapshot = () => buildTaskGame076ScenarioSnapshot();
let cleanup = () => {};
async function setupConnectedSemanticCore() {
  vi.resetModules();
  window.history.replaceState({}, "", "/software_safe.html?test_api=1&connect=1&hosted_bootstrap=0&locale=en&ws=ws://127.0.0.1:5011");
  window.localStorage.clear();
  document.body.innerHTML = "";
  Object.defineProperty(window, "crypto", { configurable: true, value: {
    getRandomValues: bytes => bytes.fill(42),
    subtle: { async importKey() { return {}; }, async verify() { return true; }, async sign() { return new Uint8Array(64).fill(11).buffer; } },
  }});
  const sentMessages = [], sockets = [];
  class MockWebSocket {
    static OPEN = 1; static CONNECTING = 0; static CLOSING = 2; static CLOSED = 3;
    constructor(url) { this.url = url; this.readyState = 0; this.listeners = new Map(); sockets.push(this); }
    addEventListener(type, callback) { this.listeners.set(type, [...(this.listeners.get(type) || []), callback]); }
    emit(type, event) { for (const callback of this.listeners.get(type) || []) callback(event); }
    send(value) { sentMessages.push(JSON.parse(value)); }
    close() { if (this.readyState === 3) return; this.readyState = 3; this.emit("close", {}); }
    open() { this.readyState = 1; this.emit("open", {}); }
    receive(message) { this.emit("message", { data: JSON.stringify(message) }); }
  }
  Object.defineProperty(window, "WebSocket", { configurable: true, value: MockWebSocket });
  const core = await import("./legacy_core.js");
  await core.initializeSoftwareSafeCore();
  bindLocalTestAgent(core, "agent-0");
  await (await import("./viewer_auth_session_module.js")).installSession(core.state, core.state.auth);
  sockets[0].open();
  sockets[0].receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });
  await settleLocalTestAuthStartup(core, sentMessages);
  sockets[0].receive({ type: "authoritative_recovery_ack", ack: { status: "catch_up_ready", player_id: core.state.auth.playerId, session_pubkey: core.state.auth.publicKey, agent_id: "agent-0", session_epoch: 1, binding_epoch: 1 } });
  sentMessages.length = 0;
  core.injectSnapshot(sampleSnapshot());
  core.applySelection({ kind: "agent", id: "agent-0" });
  cleanup = () => { for (const socket of sockets) socket.close(); };
  return { core, sockets, sentMessages };
}
beforeEach(() => { custody.enabled=false; custody.pending=false; custody.settle=null; vi.spyOn(globalThis, "fetch").mockResolvedValue({ ok: false, status: 404, json: async () => ({ ok: false, error_code: "not_found" }) }); });
afterEach(() => { cleanup(); cleanup = () => {}; vi.restoreAllMocks(); vi.useRealTimers(); });
describe("signed gameplay and connection recovery faults", () => {
  it("ignores quote and old same-action errors without canceling the current ACK timer", async () => {
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore();
    const action = { protocol_action: "gameplay_action.submit", action_id: "inspect_target", target_agent_id: "agent-0" };
    core.sendGameplayAction(action);
    await waitFor(() => expect(core.state.lastGameplayActionFeedback.stage).toBe("sent"));
    const oldNonce = core.state.lastGameplayActionFeedback.requestAuthNonce;
    sockets[0].receive({ type: "gameplay_action_error", request_auth_nonce: oldNonce,
      error: { code: "rejected", action_id: action.action_id, target_agent_id: "agent-0" } });
    core.sendGameplayAction(action);
    await waitFor(() => expect(core.state.lastGameplayActionFeedback.stage).toBe("sent"));
    const nonce = core.state.lastGameplayActionFeedback.requestAuthNonce;
    expect(nonce).not.toBe(oldNonce);
    const clear = vi.spyOn(window, "clearTimeout");
    sockets[0].receive({ type: "gameplay_action_error", request_auth_nonce: oldNonce,
      error: { code: "rejected", action_id: action.action_id, target_agent_id: "agent-0" } });
    sockets[0].receive({ type: "gameplay_action_error", error: { code: "quote_rejected", action_id: "quote_refine_compound" } });
    expect(clear).not.toHaveBeenCalled();
    expect(core.state.lastGameplayActionFeedback).toMatchObject({ stage: "sent", requestAuthNonce: nonce });
    clear.mockRestore();
    sockets[0].receive({ type: "gameplay_action_ack", request_auth_nonce: nonce,
      ack: { action_id: action.action_id, target_agent_id: "agent-0", runtime_action_id: 1, accepted_at_tick: 1 } });
    expect(core.state.lastGameplayActionFeedback.stage).toBe("ack");
    expect(sentMessages.filter(message => message.type === "gameplay_action")).toHaveLength(2);
  });

  it("bounds missing-session recovery to one attempt across replacement feedback", async () => {
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore();
    const action = { protocol_action: "gameplay_action.submit", action_id: "inspect_target", target_agent_id: "agent-0" };
    core.sendGameplayAction(action);
    await waitFor(() => expect(core.state.lastGameplayActionFeedback.stage).toBe("sent"));
    const rejectSession = () => sockets[0].receive({ type: "gameplay_action_error",
      request_auth_nonce: core.state.lastGameplayActionFeedback.requestAuthNonce,
      error: { code: "session_not_found", message: "missing session", action_id: action.action_id, target_agent_id: "agent-0" } });
    rejectSession();
    await waitFor(() => expect(sentMessages.filter(message => message.command?.mode === "register_session")).toHaveLength(1));
    sockets[0].receive({ type: "authoritative_recovery_ack", ack: { status: "session_registered",
      player_id: core.state.auth.playerId, session_pubkey: core.state.auth.publicKey,
      agent_id: "agent-0", session_epoch: 1, binding_epoch: 1 } });
    await waitFor(() => expect(sentMessages.filter(message => message.type === "gameplay_action")).toHaveLength(2));
    expect(core.state.lastGameplayActionFeedback.operation.sessionRefreshRetryAttempted).toBe(true);
    rejectSession();
    await Promise.resolve();
    expect(core.state.lastGameplayActionFeedback.stage).toBe("error");
    expect(sentMessages.filter(message => message.command?.mode === "register_session")).toHaveLength(1);
  });

  it("does not let an old signing failure terminate a newer gameplay request", async () => {
    const { core } = await setupConnectedSemanticCore();
    let rejectSigning;
    vi.spyOn(window.crypto.subtle, "sign").mockImplementationOnce(() => new Promise((_, reject) => { rejectSigning = reject; }));
    const action = { protocol_action: "gameplay_action.submit", action_id: "inspect_target", target_agent_id: "agent-0" };
    core.sendGameplayAction(action);
    await waitFor(() => expect(rejectSigning).toBeTypeOf("function"));
    core.sendGameplayAction(action);
    await waitFor(() => expect(core.state.lastGameplayActionFeedback.stage).toBe("sent"));
    const id = core.state.lastGameplayActionFeedback.id;
    rejectSigning(new Error("old signing failed"));
    await Promise.resolve(); await Promise.resolve();
    expect(core.state.lastGameplayActionFeedback).toMatchObject({ id, stage: "sent" });
  });

  it("rejects late chat and prompt errors from a previous request", async () => {
    const { core, sockets } = await setupConnectedSemanticCore();
    core.state.lastChatFeedback = { id: 2, kind: "chat", agentId: "agent-0", stage: "sent", requestAuthNonce: 22 };
    core.state.lastPromptFeedback = { id: 3, kind: "prompt", agentId: "agent-0", stage: "sent", requestId: "new" };
    sockets[0].receive({ type: "agent_chat_error", request_auth_nonce: 21, error: { agent_id: "agent-0", code: "control_lost" } });
    sockets[0].receive({ type: "prompt_control_error", error: { agent_id: "agent-0", request_id: "old", code: "control_lost" } });
    expect(core.state.lastChatFeedback).toMatchObject({ stage: "sent", requestAuthNonce: 22 });
    expect(core.state.lastPromptFeedback).toMatchObject({ stage: "sent", requestId: "new" });
  });

  it("requires the new connection snapshot before ready and keeps first-read retries active", async () => {
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore();
    expect(core.buildTargetSyncProgress().snapshotReceived).toBe(true);
    vi.useFakeTimers();
    sockets[0].close();
    await vi.advanceTimersByTimeAsync(1200);
    const ws = sockets.at(-1);
    expect(ws).not.toBe(sockets[0]);
    ws.open();
    ws.receive({ type: "hello_ack", server: "test-live", world_id: "test-world" });
    expect(core.buildTargetSyncProgress()).toMatchObject({ snapshotReceived: false, stage: "snapshot" });
    const reads = sentMessages.filter(message => message.type === "request_snapshot").length;
    await vi.advanceTimersByTimeAsync(4000);
    expect(sentMessages.filter(message => message.type === "request_snapshot").length).toBeGreaterThan(reads);
    expect(sockets.at(-1)).toBe(ws);
    ws.receive({ type: "snapshot", snapshot: sampleSnapshot() });
    expect(core.state.lastError).toBeNull();
    expect(core.buildTargetSyncProgress().snapshotReceived).toBe(true);
  });


  it("bounds failed first-snapshot reads and recovers on an explicit successful refresh", async () => {
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore();
    vi.useFakeTimers(); sockets[0].close(); await vi.advanceTimersByTimeAsync(1200);
    const ws=sockets.at(-1); ws.open(); ws.receive({type:"hello_ack",server:"test-live",world_id:"test-world"});
    await vi.advanceTimersByTimeAsync(60000);
    expect(core.buildTargetSyncProgress()).toMatchObject({stage:"error",snapshotReceived:false,snapshotRetryCount:8});
    const count=sentMessages.filter(message=>message.type==="request_snapshot").length;
    await vi.advanceTimersByTimeAsync(60000);
    expect(sentMessages.filter(message=>message.type==="request_snapshot")).toHaveLength(count);
    ws.receive({type:"snapshot",snapshot:sampleSnapshot()});
    expect(core.buildTargetSyncProgress().snapshotReceived).toBe(true);
    expect(core.state.lastError).toBeNull();
  });

  it("awaits durable hosted settlement then permits only one internal session recovery retry", async () => {
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore();
    core.state.auth.source = "hosted_account_cookie";
    (await import("./viewer_auth_session_module.js")).updateRegistrationGrant(core.state.auth, "fixture-grant", "device-1");
    custody.enabled = true;
    let finishSettlement, settlements=0;
    custody.settle = async () => { if (++settlements===1) await new Promise(resolve=>{finishSettlement=resolve;}); custody.pending=false; return true; };
    const action={protocol_action:"gameplay_action.submit",action_id:"inspect_target",target_agent_id:"agent-0"};
    core.sendGameplayAction(action);
    await waitFor(()=>expect(core.state.lastGameplayActionFeedback.stage).toBe("sent"));
    const firstNonce=core.state.lastGameplayActionFeedback.requestAuthNonce;
    const rejectSession=()=>sockets[0].receive({type:"gameplay_action_error",request_auth_nonce:core.state.lastGameplayActionFeedback.requestAuthNonce,error:{code:"session_not_found",action_id:action.action_id,target_agent_id:"agent-0"}});
    rejectSession();
    expect(core.sendGameplayAction(action).ok).toBe(false);
    expect(sentMessages.filter(message=>message.command?.mode==="register_session")).toHaveLength(0);
    expect(sentMessages.filter(message=>message.type==="gameplay_action")).toHaveLength(1);
    finishSettlement();
    await waitFor(()=>expect(sentMessages.filter(message=>message.command?.mode==="register_session")).toHaveLength(1));
    sockets[0].receive({type:"authoritative_recovery_ack",ack:{status:"session_registered",player_id:core.state.auth.playerId,session_pubkey:core.state.auth.publicKey,agent_id:"agent-0",session_epoch:1,binding_epoch:1}});
    await waitFor(()=>expect(sentMessages.filter(message=>message.type==="gameplay_action")).toHaveLength(2));
    expect(core.state.lastGameplayActionFeedback.requestAuthNonce).not.toBe(firstNonce);
    expect(core.state.lastGameplayActionFeedback.operation.sessionRefreshRetryAttempted).toBe(true);
    rejectSession();await Promise.resolve();await Promise.resolve();
    expect(sentMessages.filter(message=>message.command?.mode==="register_session")).toHaveLength(1);
    expect(sentMessages.filter(message=>message.type==="gameplay_action")).toHaveLength(2);
  });

  it("retains the hosted original and never issues a fresh nonce when durable settlement fails", async () => {
    const { core, sockets, sentMessages } = await setupConnectedSemanticCore();
    core.state.auth.source="hosted_account_cookie";
    (await import("./viewer_auth_session_module.js")).updateRegistrationGrant(core.state.auth,"fixture-grant","device-1");
    custody.enabled=true;custody.settle=async()=>false;
    const action={protocol_action:"gameplay_action.submit",action_id:"inspect_target",target_agent_id:"agent-0"};
    core.sendGameplayAction(action);await waitFor(()=>expect(core.state.lastGameplayActionFeedback.stage).toBe("sent"));
    const nonce=core.state.lastGameplayActionFeedback.requestAuthNonce;
    sockets[0].receive({type:"gameplay_action_error",request_auth_nonce:nonce,error:{code:"session_not_found",action_id:action.action_id,target_agent_id:"agent-0"}});
    await waitFor(()=>expect(core.state.lastGameplayActionFeedback.stage).toBe("unknown"));
    expect(core.sendGameplayAction(action).ok).toBe(false);
    expect(core.state.lastGameplayActionFeedback.requestAuthNonce).toBe(nonce);
    expect(sentMessages.filter(message=>message.command?.mode==="register_session")).toHaveLength(0);
    expect(sentMessages.filter(message=>message.type==="gameplay_action")).toHaveLength(1);
  });

  it("exhausts first-snapshot retry budget even when an OPEN socket throws on send", async () => {
    const {core,sockets}=await setupConnectedSemanticCore();vi.useFakeTimers();sockets[0].close();await vi.advanceTimersByTimeAsync(1200);
    const ws=sockets.at(-1);ws.open();ws.receive({type:"hello_ack",server:"test-live",world_id:"test-world"});
    vi.spyOn(ws,"send").mockImplementation(()=>{throw new Error("transport write failed");});
    await vi.advanceTimersByTimeAsync(60000);
    expect(ws.readyState).toBe(WebSocket.OPEN);
    expect(core.buildTargetSyncProgress()).toMatchObject({stage:"error",snapshotReceived:false,snapshotRetryCount:8});
    const attempts=ws.send.mock.calls.length;await vi.advanceTimersByTimeAsync(60000);expect(ws.send).toHaveBeenCalledTimes(attempts);
  });

});
