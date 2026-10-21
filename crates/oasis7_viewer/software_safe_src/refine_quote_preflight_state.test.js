import { quoteDependencyFingerprint } from "./viewer_quote_dependencies.js";
import { describe, expect, it } from "vitest";
import { createRefineQuotePreflightStateModule } from "./refine_quote_preflight_state.js";
describe("refine quote error correlation", () => {
  it("retains a quote business code without turning the rejection into an action receipt", () => {
    const state = { refineQuoteRequest: {status:"pending",requestAuthNonce:7}, lastGameplayActionFeedback: { action: "build", stage: "completed" } };
    state.refineQuoteRequest.dependencyFingerprint = quoteDependencyFingerprint(state,"refine");
    const module = createRefineQuotePreflightStateModule({ state, clone: structuredClone });
    expect(module.handleRefineQuoteError({ action_id: "build", code: "denied" })).toBe(false);
    expect(module.handleRefineQuoteError({ action_id: "quote_refine_compound", request_auth_nonce:7, code: "refine_quote_unavailable", message: "private detail" })).toBe(true);
    expect(state.refineQuoteRequest).toMatchObject({ status: "error", errorCode: "refine_quote_unavailable" });
    expect(state.lastGameplayActionFeedback).toEqual({ action: "build", stage: "completed" });
    expect(state.refineQuotePreflight).toBeUndefined();
  });
});

it("fences late refine replies after a same-tick resource change and preserves unrelated updates", () => {
 const state={auth:{playerId:"p",publicKey:"key",boundAgentId:"a"},snapshot:{time:5,config:{economy:{}},model:{agents:{a:{resources:{amounts:{data:100,electricity:80}}}}}}};
 const module=createRefineQuotePreflightStateModule({state,clone:structuredClone});
 state.refineQuoteRequest={status:"pending",requestAuthNonce:1};
 module.handleRefineQuotePreflight({request_auth_nonce:9,value:"old"});
 expect(state.refineQuotePreflight).toBeUndefined();
});
