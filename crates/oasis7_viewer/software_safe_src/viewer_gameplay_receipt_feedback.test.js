import { describe, it, expect } from "vitest";
import { projectGameplayReceiptFeedback } from "./viewer_gameplay_receipt_feedback.js";
describe("authoritative gameplay result reconciliation", () => {
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
