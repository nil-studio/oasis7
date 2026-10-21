import { describe, expect, it, vi } from "vitest";
import { quoteDependencyFingerprint, invalidateDependentQuote } from "./viewer_quote_dependencies.js";
import { createProductValidationQuoteStateModule } from "./product_validation_quote_state.js";
import { createRefineQuotePreflightStateModule } from "./refine_quote_preflight_state.js";
import { createProductValidationQuoteRequestModule } from "./product_validation_quote_request.js";
function fixture() {
 return {auth:{available:true,playerId:"p",publicKey:"key",boundAgentId:"a",registrationStatus:"registered",bindingEpoch:1},snapshot:{time:5,config:{economy:{refine_hardware_yield_ppm:500000}},model:{agents:{a:{id:"a",resources:{amounts:{data:40,electricity:80}}},other:{id:"other"}},agent_player_bindings:{a:"p"}},runtime_snapshot:{state:{industry_progress:{stage:"bootstrap"},product_profiles:{sensor:{product_id:"sensor",role_tag:"build",tradable:true,unlock_stage:"scale_out"}}}}}};
}
function pending(state,kind,nonce=1){ const inputs=kind==="product"?{product_id:"sensor",amount:2}:{};return {status:"pending",requestAuthNonce:nonce,dependencyInputs:inputs,dependencyFingerprint:quoteDependencyFingerprint(state,kind,inputs)}; }
describe("quote input and response fences",()=>{
 for(const kind of ["refine","product"]) it(`${kind} accepts only the current nonce and dependencies`,()=>{
  const state=fixture(); const requestKey=kind==="refine"?"refineQuoteRequest":"productValidationQuoteRequest",quoteKey=kind==="refine"?"refineQuotePreflight":"productValidationQuote";
  const module=kind==="refine"?createRefineQuotePreflightStateModule({state,clone:structuredClone}):createProductValidationQuoteStateModule({state,clone:structuredClone});
  const handle=kind==="refine"?module.handleRefineQuotePreflight:module.handleProductValidationQuote;
  const error=kind==="refine"?module.handleRefineQuoteError:module.handleProductValidationQuoteError;
  const action=kind==="refine"?"quote_refine_compound":"quote_validate_product";
  state[requestKey]=pending(state,kind,2);
  handle({request_auth_nonce:1,value:"old"}); error({request_auth_nonce:1,action_id:action,code:"denied"});
  expect(state[requestKey].status).toBe("pending");expect(state[quoteKey]).toBeUndefined();
  const unrelated=structuredClone(state.snapshot);unrelated.time=9;unrelated.model.agents.other.pos={x:9};
  invalidateDependentQuote(state,kind,unrelated);state.snapshot=unrelated;
  expect(state[requestKey].status).toBe("pending");
  handle({request_auth_nonce:2,value:"current"});expect(state[requestKey].status).toBe("received");expect(state[quoteKey].value).toBe("current");
  state.auth.bindingEpoch=2;invalidateDependentQuote(state,kind,state.snapshot);
  expect(state[quoteKey]).toBeNull();handle({request_auth_nonce:2,value:"late"});expect(state[quoteKey]).toBeNull();
 });
 it("tracks exact resource/config and product-profile/stage dependencies at the same tick",()=>{
  const state=fixture(),refine=quoteDependencyFingerprint(state,"refine"),product=quoteDependencyFingerprint(state,"product",{product_id:"sensor",amount:2});
  state.snapshot.model.agents.a.resources.amounts.electricity=0;
  expect(quoteDependencyFingerprint(state,"refine")).not.toBe(refine);
  expect(quoteDependencyFingerprint(state,"product",{product_id:"sensor",amount:2})).toBe(product);
  state.snapshot.runtime_snapshot.state.product_profiles.sensor.tradable=false;
  expect(quoteDependencyFingerprint(state,"product",{product_id:"sensor",amount:2})).not.toBe(product);
 });
 it("fences old responses even when dependencies change back before a late reply",()=>{
  const state=fixture();state.refineQuoteRequest=pending(state,"refine");
  const module=createRefineQuotePreflightStateModule({state,clone:structuredClone});
  const changed=structuredClone(state.snapshot);changed.model.agents.a.resources.amounts.data=0;
  invalidateDependentQuote(state,"refine",changed);
  module.handleRefineQuotePreflight({request_auth_nonce:1,value:"late"});expect(state.refineQuotePreflight).toBeNull();
 });
 it("stores the real signed nonce and refuses a dependency change during signing",async()=>{
  const state=fixture(),sendJson=vi.fn(),socket={readyState:WebSocket.OPEN};let complete;
  const module=createProductValidationQuoteRequestModule({state,sendJson,getSocket:()=>socket,clone:structuredClone,nextAuthNonce:()=>7,buildAuthEnvelope:v=>v,ensureHostedPlayerAuthAvailable:async()=>{},ensureRegisteredPlayerSession:async()=>{},signAuthPayload:()=>new Promise(resolve=>{complete=resolve;})});
  const operation=module.requestProductValidationQuote("sensor",2);await vi.waitFor(()=>expect(complete).toBeTypeOf("function"));
  state.auth.bindingEpoch=2;complete("signature");expect((await operation).ok).toBe(false);expect(sendJson).not.toHaveBeenCalled();
 });
});
