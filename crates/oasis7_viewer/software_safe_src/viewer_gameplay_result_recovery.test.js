import { describe, it, expect, vi } from "vitest";
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
  it("bounds repeated snapshot queries and retries the same identity after the interval", () => {
    const { state } = fixture(); const send = vi.fn();
    requestGameplayResult(state, scope, send, 100);
    expect(requestGameplayResult(state, scope, send, 101)).toBe(false);
    expect(requestGameplayResult(state, scope, send, 10100)).toBe(true);
    expect(send).toHaveBeenCalledTimes(2);
  });
});
