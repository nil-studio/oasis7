import { afterEach, describe, expect, it, vi } from "vitest";
import { authenticatedCanonicalGoal, scheduleCanonicalGoalPending, canonicalAckMatches } from "./viewer_canonical_goal_module.js";

afterEach(() => vi.useRealTimers());
describe("canonical goal receipts", () => {
  it("rejects stale ACKs from a previous signed goal tuple", () => {
    const request = { agent_id: "a", player_id: "p", intent_seq: 9, intent_tick: 12, auth: { nonce: 11 } };
    expect(canonicalAckMatches({ ...request, auth_nonce: 11 }, request)).toBe(true);
    expect(canonicalAckMatches({ ...request, auth_nonce: 12 }, request)).toBe(false);
    for (const field of ["agent_id", "player_id", "intent_seq", "intent_tick"]) {
      expect(canonicalAckMatches({ ...request, auth_nonce: 11, [field]: "stale" }, request)).toBe(false);
    }
  });
  it("retries the original signed request without publishing acceptance and stops after identity changes", () => {
    vi.useFakeTimers();
    const request = { agent_id: "agent-1", world_id: "w", reorg_epoch: 1, canonical_authority: { branch_id: "b", agent_identity_generation: 1 }, message: "private owner goal", auth: { player_id: "player-1", public_key: "key-1", nonce: 9, signature: "original" } };
    const feedback = { canonicalRequest: request, canonicalRetryCount: 0 };
    const state = { auth: { playerId: "player-1", publicKey: "key-1", boundAgentId: "agent-1" }, lastChatFeedback: feedback, worldFeed: { worldId: "w", reorgEpoch: "1" },
      snapshot: { player_gameplay: { canonical_agent_chat: { canonical_authority: request.canonical_authority } } } };
    const sendJson = vi.fn(); const failPendingAgentChatAck = vi.fn();
    const options = { ack: { status: "pending" }, feedback, state, clone: structuredClone,
      requestSnapshotSafe: vi.fn(), sameAgentChatFeedback: (a, b) => a === b,
      failPendingAgentChatAck, sendJson, scheduleAgentChatAckTimeout: vi.fn(),
      getSocket: () => ({ readyState: WebSocket.OPEN }) };
    scheduleCanonicalGoalPending(options);
    expect(feedback.accepted).toBe(false);
    vi.advanceTimersByTime(1500);
    expect(sendJson).toHaveBeenCalledWith({ type: "agent_chat", request });
    expect(sendJson.mock.calls[0][0].request).toBe(request);
    scheduleCanonicalGoalPending(options);
    state.auth.publicKey = "new-key";
    vi.advanceTimersByTime(1500);
    expect(sendJson).toHaveBeenCalledTimes(1);
    expect(failPendingAgentChatAck).toHaveBeenCalledWith(expect.stringContaining("identity"));
  });
  it("restores only the current owner's authenticated goal and never reads local chat history", () => {
    const goal = { intent_id: "goal-1", status: "accepted", message: "private" };
    const state = { auth: { playerId: "p", publicKey: "k" }, chatHistory: [{ message: "sidecar-only" }],
      worldFeed: { worldId: "w", reorgEpoch: "1" },
      snapshot: { player_gameplay: { canonical_agent_chat: { world_id: "w", reorg_epoch: 1, agent_id: "a", player_id: "p", public_key: "k", current_intent_id: "goal-1", goal } } } };
    expect(authenticatedCanonicalGoal(state, "a")).toBe(goal);
    expect(authenticatedCanonicalGoal(state, "other")).toBeNull();
    state.snapshot.player_gameplay.canonical_agent_chat = null;
    expect(authenticatedCanonicalGoal(state, "a")).toBeNull();
  });
});
