import { it, expect, vi } from "vitest";
import { createOwnerSetupClient, CUSTODY_PATHS } from "./viewer_custody_owner_setup_client.js";
const parent={custody_account_id:"account",provision_id:"provision",environment:"local",world_id:"world"};
const cap={api_version:1,available:true,audience:"custody",environment:"local",...CUSTODY_PATHS};
function fixture({badContract=false,lose=false}={}) {
  const calls=[]; let prepared;
  const original={api_version:2,approval_request_id:"request",plan:{api_version:2,model_budget:null,
    write_deadline:"9000",actions:[{tool:"move",position_cm:[0,0]}],allowed_resource_refs:[],business_limit:"0",
    termination:{session_id:"session",provision_id:"provision",terminal_mode:"literal_persistent_closed"}},
    scope:{session_id:"session",client_pairing:{target:{custody_account_id:"account",provision_id:"provision"}}}};
  const contract={api_version:2,contract:"full_test_session_v2",summary_digest:"ab".repeat(32),model_budget_contract:null};
  const fetch=vi.fn(async(url,options)=>{
    const body=options.body&&JSON.parse(options.body);calls.push({url,body,raw:options.body,headers:options.headers});
    let value;
    if(url.endsWith("/capabilities"))value={csrf_token:"csrf",custody_management:cap};
    else if(url.endsWith("/prepare"))value=prepared={approval_request_id:"request",context:{custody_account_id:"account",environment:"local",audience:"custody",policy_version:"1",security_version:"1",service_epoch:"1"},intent:{kind:"test_session_v2",plan:body.plan,scope:body.scope}};
    else if(url===CUSTODY_PATHS.options_path)value={api_version:1,phase:"pending",approval_id:"approval",ceremony_id:"ceremony",operation_id:"operation",intent_digest:"cd".repeat(32),audience:"custody",ceremony_expires_at:"8000",options:{},model_budget_summary_contract:badContract?null:contract,custody_session_summary_v2:{api_version:2,plan:original.plan,scope:original.scope}};
    else if(url===CUSTODY_PATHS.complete_path){if(lose){lose=false;throw Error("network");}value={api_version:1,phase:"completed",approval:{signature:"signature",proof:{approval_id:"approval",intent_digest:"cd".repeat(32),context:prepared.context,model_budget_summary_contract:contract}}};}
    else if(url.endsWith("/approve"))value={api_version:2,custody_account_id:"account",provision_id:"provision",session_id:"session",phase:"completed"};
    else throw Error("unexpected fixed route");
    return new Response(JSON.stringify(value));
  });
  const get=vi.fn(async()=>({}));const client=createOwnerSetupClient({fetch,credentials:{get},browserPasskeyOptions:x=>x,serializePasskey:()=>({}),now:()=>1000});
  client.setSession({account_id:"manager"},{method:"webauthn"},cap);
  return {client,calls,get,original};
}
it("requires full session confirmation even with no model budget before actual get",async()=>{
  const f=fixture();await f.client.prepareSessionV2(parent,f.original);
  const confirmSession=vi.fn(async s=>{expect(f.get).not.toHaveBeenCalled();expect(s.plan.termination).toEqual(f.original.plan.termination);expect(s.budget).toBeNull();});
  const result=await f.client.approve({confirmSession});expect(confirmSession).toHaveBeenCalledTimes(1);expect(result.execution_started).toBe(false);
  const complete=f.calls.find(x=>x.url===CUSTODY_PATHS.complete_path);expect(complete.body).toMatchObject({approval_id:"approval",ceremony_id:"ceremony",operation_id:"operation"});
  expect(complete.headers["X-Oasis7-CSRF"]).toBe("csrf");
  const approved=f.calls.find(x=>x.url.endsWith("/approve")).body;expect(approved.input.operation_id).toBe("session");expect(approved).not.toHaveProperty("plan");
  expect(f.calls.some(x=>x.url.includes("continue"))).toBe(false);
});
it("does not let budget-null bypass the Native full contract",async()=>{
  const f=fixture({badContract:true});await f.client.prepareSessionV2(parent,f.original);
  await expect(f.client.approve({confirmSession:vi.fn()})).rejects.toMatchObject({code:"custody_session_summary_required"});expect(f.get).not.toHaveBeenCalled();
});
it("retains exact full session unknown complete without a new get or options",async()=>{
  const f=fixture({lose:true});await f.client.prepareSessionV2(parent,f.original);
  await expect(f.client.approve({confirmSession:async()=>{}})).rejects.toMatchObject({code:"custody_outcome_unknown"});
  await f.client.retry();const c=f.calls.filter(x=>x.url===CUSTODY_PATHS.complete_path);expect(c[0].raw===c[1].raw).toBe(true);expect(f.get).toHaveBeenCalledTimes(1);
  expect(f.calls.filter(x=>x.url===CUSTODY_PATHS.options_path)).toHaveLength(1);
});
