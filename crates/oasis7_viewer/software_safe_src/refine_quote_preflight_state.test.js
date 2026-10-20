import { describe, expect, it } from "vitest";
import { createRefineQuotePreflightStateModule } from "./refine_quote_preflight_state.js";
describe("refine quote error correlation", () => {
  it("retains a quote business code without turning the rejection into an action receipt", () => {
    const state = { lastGameplayActionFeedback: { action: "build", stage: "completed" } };
    const module = createRefineQuotePreflightStateModule({ state, clone: structuredClone });
    expect(module.handleRefineQuoteError({ action_id: "build", code: "denied" })).toBe(false);
    expect(module.handleRefineQuoteError({ action_id: "quote_refine_compound", code: "refine_quote_unavailable", message: "private detail" })).toBe(true);
    expect(state.refineQuoteRequest).toMatchObject({ status: "error", errorCode: "refine_quote_unavailable" });
    expect(state.lastGameplayActionFeedback).toEqual({ action: "build", stage: "completed" });
    expect(state.refineQuotePreflight).toBeUndefined();
  });
});
