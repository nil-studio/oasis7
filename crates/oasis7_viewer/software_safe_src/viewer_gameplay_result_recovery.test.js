import { describe, it, expect, vi } from "vitest";
import { adoptRuntimeGameplayFeedback } from "./viewer_gameplay_receipt_feedback.js";
import { requestGameplayResult, receiveGameplayResult } from "./viewer_gameplay_result_recovery.js";

const scope = { endpointId: "wss://same/runtime", worldId: "world" };
function fixture() {
  const original = { action_id: "claim_first_agent", target_agent_id: "starter-agent-0", player_id: "p",
    public_key: "key", auth: { nonce: 71, player_id: "p", public_key: "key", signature: "original" } };
  const feedback = { action: original.action_id, targetAgentId: original.target_agent_id, requestAuthNonce: 71,
    originalRequest: original, gameplayScope: scope, stage: "unknown" };
  const state = { auth: { playerId: "p", publicKey: "key", registrationStatus: "registered" },
    viewerProtocol: { capabilities: ["gameplay_result_lookup_v1"] }, lastGameplayActionFeedback: feedback };
  return { state, feedback, original };
}
describe("original gameplay result recovery after both processes restart", () => {
  it("queries the saved original without a submit, and settles only its correlated result", async () => {
    const { state, original, feedback } = fixture();
    const send = vi.fn();
    expect(requestGameplayResult(state, scope, send, 100)).toBe(true);
    expect(send).toHaveBeenCalledExactlyOnceWith({ type: "lookup_gameplay_action", request: original });
    const account = { settleGameplayRequest: vi.fn(async () => true) };
    expect(receiveGameplayResult(state, scope, account, { player_id: "p", action_id: "claim_first_agent",
      target_agent_id: "starter-agent-0", request_auth_nonce: 71, status: "committed", receipt: { action_id: 101 } })).toBe(true);
    expect(feedback.stage).toBe("committed");
    expect(feedback.response.receipt).toEqual({ action_id: 101 });
    await Promise.resolve();
    expect(account.settleGameplayRequest).toHaveBeenCalledWith(71, scope);
    expect(original.auth.nonce).toBe(71);
  });
  for (const status of ["unknown", "history_unavailable", "unavailable", "unauthorized"]) {
    it(`keeps the original unresolved for ${status}`, () => {
      const { state, feedback } = fixture(); const account = { settleGameplayRequest: vi.fn() };
      requestGameplayResult(state, scope, vi.fn(), 100);
      expect(receiveGameplayResult(state, scope, account, { player_id: "p", action_id: "claim_first_agent",
        target_agent_id: "starter-agent-0", request_auth_nonce: 71, status })).toBe(true);
      expect(feedback.stage).toBe("unknown"); expect(account.settleGameplayRequest).not.toHaveBeenCalled();
      expect(feedback.originalRequest.auth.nonce).toBe(71);
    });
  }
  it("rejects foreign results, stale scope and missing negotiated support", () => {
    const { state } = fixture(); const account = { settleGameplayRequest: vi.fn() }; const send = vi.fn();
    expect(receiveGameplayResult(state, scope, account, { player_id: "other", request_auth_nonce: 71, status: "committed" })).toBe(false);
    expect(requestGameplayResult(state, { ...scope, worldId: "other" }, send, 100)).toBe(false);
    state.viewerProtocol.capabilities = [];
    expect(requestGameplayResult(state, scope, send, 100)).toBe(false);
    expect(send).not.toHaveBeenCalled();
  });
  it("keeps a terminal snapshot result when an earlier lookup finishes late", () => {
    const { state, feedback } = fixture(); const account = { settleGameplayRequest: vi.fn() };
    requestGameplayResult(state, scope, vi.fn(), 100);
    adoptRuntimeGameplayFeedback({ player_gameplay: { recent_feedback: { action: "world_service_intent",
      target_agent_id: "starter-agent-0", request_auth_nonce: 71, stage: "committed" } } }, feedback, vi.fn());
    expect(receiveGameplayResult(state, scope, account, { player_id: "p", action_id: "claim_first_agent",
      target_agent_id: "starter-agent-0", request_auth_nonce: 71, status: "unavailable" })).toBe(false);
    expect(feedback.stage).toBe("committed");
  });
  it("queries retained feedback across runtime epochs but rejects an outstanding old-epoch reply", () => {
    const { state, feedback } = fixture(); const send = vi.fn();
    feedback.gameplayScope = { ...scope, authorityEpoch: "old" };
    const current = { ...scope, authorityEpoch: "new" };
    expect(requestGameplayResult(state, current, send, 100)).toBe(true);
    expect(receiveGameplayResult(state, { ...current, authorityEpoch: "newer" }, {}, {
      player_id: "p", action_id: "claim_first_agent", target_agent_id: "starter-agent-0",
      request_auth_nonce: 71, status: "committed" })).toBe(false);
    expect(feedback.stage).toBe("unknown");
  });
  it("bounds repeated snapshot queries and retries the same identity after the interval", () => {
    const { state } = fixture(); const send = vi.fn();
    requestGameplayResult(state, scope, send, 100);
    expect(requestGameplayResult(state, scope, send, 101)).toBe(false);
    expect(requestGameplayResult(state, scope, send, 10100)).toBe(true);
    expect(send).toHaveBeenCalledTimes(2);
  });
});
