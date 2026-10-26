import { describe, it, expect } from "vitest";
import { projectGameplayReceiptFeedback, adoptRuntimeGameplayFeedback } from "./viewer_gameplay_receipt_feedback.js";
describe("authoritative gameplay result reconciliation", () => {
  it("does not regress a recovered disposition to an older admission snapshot", () => {
    const local = { action: "claim_first_agent", targetAgentId: "a", stage: "committed", requestAuthNonce: 11 };
    adoptRuntimeGameplayFeedback({ player_gameplay: { recent_feedback: { action: "world_service_intent",
      target_agent_id: "a", request_auth_nonce: 11, stage: "received" } } }, local, () => {});
    expect(local.stage).toBe("committed");
  });
  it("projects the recovered terminal result over an older admission", () => {
    const local = { kind: "gameplay_action", action: "claim_first_agent", targetAgentId: "a", stage: "committed", requestAuthNonce: 11 };
    const recent_feedback = { action: "world_service_intent", target_agent_id: "a", request_auth_nonce: 11, stage: "received" };
    expect(projectGameplayReceiptFeedback({ recent_feedback }, local, "a").recentFeedback).toMatchObject({ source: "local_gameplay_action", stage: "committed" });
  });
  for (const stage of ["executing", "completed_advanced", "rejected", "blocked"]) {
    it(`replaces temporary ack with exact request ${stage}`, () => {
      const local = { kind: "gameplay_action", action: "refine", agentId: "a", stage: "ack", requestAuthNonce: 11 };
      const recent_feedback = { action: "gameplay_action:refine", target_agent_id: "a", request_auth_nonce: 11, stage };
      expect(projectGameplayReceiptFeedback({ recent_feedback }, local, "a").recentFeedback).toMatchObject({source: "runtime", stage});
    });
  }
  it("does not replace a newer local request with the old same action result", () => {
    const local = { kind: "gameplay_action", action: "refine", agentId: "a", stage: "ack", requestAuthNonce: 12 };
    const recent_feedback = { action: "gameplay_action:refine", target_agent_id: "a", request_auth_nonce: 11, stage: "completed_advanced" };
    expect(projectGameplayReceiptFeedback({ recent_feedback }, local, "a").recentFeedback.source).toBe("local_gameplay_action");
  });
});
