import { it, expect, vi } from "vitest";
import { createOwnerSetupClient, CUSTODY_PATHS } from "./viewer_custody_owner_setup_client.js";
const parent={custody_account_id:"account",provision_id:"provision",environment:"local",world_id:"world"};
const cap={api_version:1,available:true,audience:"custody",environment:"local",...CUSTODY_PATHS};
function fixture({badContract=false,lose=false,proxy=false,mutateContract=null}={}) {
  const calls=[]; let prepared;
  const original={api_version:2,approval_request_id:"request",plan:{api_version:2,model_budget:null,
    write_deadline:"9000",actions:[{tool:"move",position_cm:[0,0]}],allowed_resource_refs:[],business_limit:"0",
    termination:{session_id:"session",provision_id:"provision",terminal_mode:"literal_persistent_closed"}},
    scope:{session_id:"session",client_pairing:{target:{custody_account_id:"account",provision_id:"provision"}}}};
  let contract={api_version:2,contract:"full_test_session_v2",summary_digest:"ab".repeat(32),model_budget_contract:null};
  if (proxy) {
    const budget={api_version:1,approval_contract:"native_custody_model_proxy_v2",native_account_id:"native",native_public_key:"11".repeat(32),native_instance_public_key:"22".repeat(32),
      ledger_id:"ledger",preset_id:"fixed",preset_revision:"1",model:"model",max_calls:"3",max_input_tokens:"100",max_output_tokens:"50",max_total_tokens:"450",max_concurrent:"1",
      probe_physical_http:["10","10"],inference_physical_http:"10",deadline:"9000",physical_graph_digest:"77".repeat(32),
      price_policy:{currency:"USD",valid_until:"9000",max_total_micro_units:"600"}};
    original.plan.model_budget=budget;
    contract={api_version:2,contract:"full_test_session_proxy_v2",basis_digest:"60".repeat(32),
      full_session:{...contract,model_budget_contract:{api_version:1,summary_digest:"ef".repeat(32)}},
      basis:{api_version:2,approval_request_id:"request",intent_digest:"cd".repeat(32),context_digest:"55".repeat(32),budget_digest:"56".repeat(32),
        ledger_id:budget.ledger_id,native_account_id:budget.native_account_id,native_instance_public_key:budget.native_instance_public_key,
        connection_ref_digest:"61".repeat(32),connection_generation:"1",preset_id:budget.preset_id,preset_revision:budget.preset_revision,model:budget.model,
        endpoint_policy_digest:"62".repeat(32),physical_graph_digest:budget.physical_graph_digest,price_policy_digest:"63".repeat(32),issued_at:"1000",expires_at:"7000"}};
    mutateContract?.(contract);
  }

  const fetch=vi.fn(async(url,options)=>{
    const body=options.body&&JSON.parse(options.body);calls.push({url,body,raw:options.body,headers:options.headers});
    let value;
    if(url.endsWith("/capabilities"))value={csrf_token:"csrf",custody_management:cap};
    else if(url.endsWith("/prepare"))value=prepared={approval_request_id:"request",context:{custody_account_id:"account",environment:"local",audience:"custody",policy_version:"1",security_version:"1",service_epoch:"1"},intent:{kind:"test_session_v2",plan:body.plan,scope:body.scope}};
    else if(url===CUSTODY_PATHS.options_path)value={api_version:1,phase:"pending",approval_id:"approval",ceremony_id:"ceremony",operation_id:"operation",intent_digest:"cd".repeat(32),audience:"custody",ceremony_expires_at:"8000",options:{},model_budget_summary:original.plan.model_budget,model_budget_summary_contract:badContract?null:contract,custody_session_summary_v2:{api_version:2,plan:original.plan,scope:original.scope}};
    else if(url===CUSTODY_PATHS.complete_path){if(lose){lose=false;throw Error("network");}value={api_version:1,phase:"completed",approval:{signature:"signature",proof:{approval_id:"approval",intent_digest:"cd".repeat(32),context:prepared.context,model_budget_summary_contract:contract}}};}
    else if(url.endsWith("/approve"))value={api_version:2,custody_account_id:"account",provision_id:"provision",session_id:"session",phase:"completed"};
    else throw Error("unexpected fixed route");
    return new Response(JSON.stringify(value));
  });
  const get=vi.fn(async()=>({}));const client=createOwnerSetupClient({fetch,credentials:{get},browserPasskeyOptions:x=>x,serializePasskey:()=>({}),now:()=>1000});
  client.setSession({account_id:"manager"},{method:"webauthn"},cap);
  return {client,calls,get,original,contract};
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

// These mock/DOM surface cases do not prove Native signatures or N dispatch.
// The separate Rust-produced three-case wire gate covers the exact DTO shape.
it("confirms and retains the complete proxy wrapper before get",async()=>{
  const f=fixture({proxy:true});await f.client.prepareSessionV2(parent,f.original);
  const confirmSession=vi.fn(async summary=>{
    expect(f.get).not.toHaveBeenCalled();expect(summary.contract).toEqual(f.contract);
    expect(summary.contract.basis.connection_generation).toBe("1");
    summary.contract.basis.connection_generation="2"; // caller receives a copy
  });
  await f.client.approve({confirmSession});expect(confirmSession).toHaveBeenCalledTimes(1);expect(f.get).toHaveBeenCalledTimes(1);
  const approved=f.calls.find(x=>x.url.endsWith("/approve")).body;
  expect(approved.input.approval.proof.model_budget_summary_contract).toEqual(f.contract);
  const complete=f.calls.find(x=>x.url===CUSTODY_PATHS.complete_path);expect(complete.headers["X-Oasis7-CSRF"]).toBe("csrf");
  expect(complete.body).toMatchObject({approval_id:"approval",ceremony_id:"ceremony",operation_id:"operation"});
  expect(f.calls.some(x=>x.url.includes("continue"))).toBe(false);
});
it("rejects visible proxy basis mismatches before get or complete",async()=>{
  const changes=[c=>c.basis.native_account_id="other",c=>c.basis.native_instance_public_key="23".repeat(32),
    c=>c.basis.intent_digest="24".repeat(32),c=>c.basis.physical_graph_digest="25".repeat(32),
    c=>c.basis.preset_revision="2",c=>c.basis.connection_generation="01",c=>c.basis.connection_generation=0,c=>delete c.basis.connection_generation,c=>c.basis.expires_at="1000",
    c=>c.basis.budget_digest="invalid",c=>c.basis.extra=true,c=>c.extra=true];
  for(const mutateContract of changes){
    const f=fixture({proxy:true,mutateContract});await f.client.prepareSessionV2(parent,f.original);
    await expect(f.client.approve({confirmSession:async()=>{}})).rejects.toMatchObject({code:"custody_budget_summary_required"});
    expect(f.get).not.toHaveBeenCalled();expect(f.calls.some(x=>x.url===CUSTODY_PATHS.complete_path||x.url.endsWith("/approve"))).toBe(false);
  }
});
it("retains the full proxy unknown complete original without new get/options",async()=>{
  const f=fixture({proxy:true,lose:true});await f.client.prepareSessionV2(parent,f.original);
  await expect(f.client.approve({confirmSession:async()=>{}})).rejects.toMatchObject({code:"custody_outcome_unknown"});
  await f.client.retry();const sent=f.calls.filter(x=>x.url===CUSTODY_PATHS.complete_path);
  expect(sent[0].raw).toBe(sent[1].raw);expect(f.get).toHaveBeenCalledTimes(1);
  expect(f.calls.filter(x=>x.url===CUSTODY_PATHS.options_path)).toHaveLength(1);
  expect(f.calls.find(x=>x.url.endsWith("/approve")).body.input.approval.proof.model_budget_summary_contract).toEqual(f.contract);
});

it("preserves real Draft generation zero without marking a connection ready",async()=>{
  const f=fixture({proxy:true,mutateContract:c=>{c.basis.connection_generation="0";}});
  await f.client.prepareSessionV2(parent,f.original);
  await f.client.approve({confirmSession:async summary=>{
    expect(f.get).not.toHaveBeenCalled();expect(summary.contract.basis.connection_generation).toBe("0");
  }});
  expect(f.calls.find(x=>x.url.endsWith("/approve")).body.input.approval.proof.model_budget_summary_contract.basis.connection_generation).toBe("0");
  expect(f.calls.some(x=>x.url.includes("continue"))).toBe(false);
});
