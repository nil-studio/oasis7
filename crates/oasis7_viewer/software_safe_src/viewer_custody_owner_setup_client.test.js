import { describe, it, expect, vi } from "vitest";
import { createOwnerSetupPanel } from "./viewer_custody_owner_setup_panel.js";
import { createOwnerSetupClient, CUSTODY_PATHS } from "./viewer_custody_owner_setup_client.js";
import { browserPasskeyOptions, serializePasskey, mountPasskeyAccount } from "./viewer_passkey_account_module.js";
const cap = { api_version:1, available:true, audience:"custody", environment:"fixture", ...CUSTODY_PATHS };
const parent = { custody_account_id:"custody-account", provision_id:"provision", environment:"fixture", world_id:"world" };
const world = { world_id:"world", genesis_digest:"genesis", branch_id:"branch", reorg_epoch:"1", service_public_key:"ab".repeat(32) };
const credential = () => ({ id:"credential", type:"public-key", rawId:new Uint8Array([1]).buffer,
  response:{clientDataJSON:new Uint8Array([2]).buffer,authenticatorData:new Uint8Array([3]).buffer,
    signature:new Uint8Array([4]).buffer,userHandle:null},getClientExtensionResults:()=>({}) });
function fixture(overrides={}) {
  const calls=[]; let prepared; let plan; let nextUnknown=false;
  const fetch=vi.fn(async(url,options)=>{
    const body=options.body ? JSON.parse(options.body) : undefined;calls.push({url,options,body});
    if(overrides.fetch) {const value=await overrides.fetch(url,options,body);if(value!==undefined)return value;}
    let value;
    if(url.endsWith("/capabilities")) value={csrf_token:"real-csrf",custody_management:cap};
    else if(url===CUSTODY_PATHS.parents_query_path)value={api_version:1,directory_revision:"revision",frozen:false,inventory_complete:true,items:[parent],next_after_custody_account_id:null};
    else if(url.endsWith("/basis"))value={api_version:1,...parent,world_identity:world,owner_key:{key_id:"owner"},executor_key:{key_id:"executor",public_key:"12".repeat(32)},
      participant_receipt_digest:"34".repeat(32),bootstrap_config_version:"1"};
    else if(url.endsWith("/prepare")) {
      plan=body.plan; prepared={approval_request_id:body.approval_request_id,context:{environment:"fixture",custody_account_id:parent.custody_account_id,audience:"custody",policy_version:"1",security_version:"1",service_epoch:"1"},
        intent:{kind:"owner_setup",setup_id:plan.setup_id,provision_id:plan.provision_id,participant_receipt_digest:plan.participant_receipt_digest,world_identity:plan.world_identity,
          owner_key_ref:plan.owner_key_ref,executor_key_ref:plan.executor_key_ref,setup_plan_digest:"56".repeat(32),bootstrap_config_version:"1",deadline:plan.deadline,request_limit:"6",transport_limit:"584",write_limit:"4"}};value=prepared;
    } else if(url===CUSTODY_PATHS.options_path)value={api_version:1,phase:"pending",approval_id:"approval-A",ceremony_id:"ceremony-B",operation_id:"operation-C",
      intent_digest:"78".repeat(32),audience:"custody",ceremony_expires_at:"9999999999999",options:{publicKey:{challenge:"AQ",allowCredentials:[]}}};
    else if(url===CUSTODY_PATHS.complete_path) {
      if(nextUnknown){nextUnknown=false;throw new Error("private network detail must not render");}
      const context=overrides.approvalContext?overrides.approvalContext(prepared.context):prepared.context;
      value={api_version:1,phase:"completed",approval:{proof:{approval_id:"approval-A",intent_digest:"78".repeat(32),context},signature:"signature"},proof_bytes:"private-proof-bytes"};
    } else if(url.endsWith("/approve"))value={api_version:1,setup_id:plan.setup_id,...parent,stage:"participant_bound"};
    else if(url.endsWith("/progress"))value={stage:"participant_bound"};
    else throw new Error("unexpected route");
    return new Response(JSON.stringify(value),{status:200,headers:{"content-type":"application/json"}});
  });
  const get=overrides.get||vi.fn(async()=>credential());
  const client=createOwnerSetupClient({fetch,credentials:{get},browserPasskeyOptions,serializePasskey,now:overrides.now||(()=>1000)});
  client.setSession({account_id:"manager"},{method:"webauthn"},cap);
  return {client,calls,fetch,get,loseComplete(){nextUnknown=true;}};
}
describe("dedicated OwnerSetup custody browser client",()=>{
  it("uses real Rust prepare intact, exact three distinct Native IDs, CSRF and the signed approval input; never continues",async()=>{
    const f=fixture();await f.client.parents();const summary=await f.client.prepare(parent,"A private goal");await f.client.approve();
    const post=f.calls.filter(x=>x.body);const complete=post.find(x=>x.url===CUSTODY_PATHS.complete_path).body;
    expect(Object.keys(complete).sort()).toEqual(["approval_id","ceremony_id","intent_digest","operation_id","response"]);
    expect(complete).toMatchObject({approval_id:"approval-A",ceremony_id:"ceremony-B",operation_id:"operation-C"});
    expect(post.every(x=>x.options.credentials==="same-origin"&&x.options.headers["X-Oasis7-CSRF"]==="real-csrf")).toBe(true);
    const options=post.find(x=>x.url===CUSTODY_PATHS.options_path).body;
    const approve=post.find(x=>x.url.endsWith("/approve")).body;
    expect(options).toEqual(summary.prepared);expect(approve.input.request).toEqual(options);expect(approve.input.operation_id).toBe(summary.plan.setup_id);
    expect(approve.input.approval.signature).toBe("signature");expect(approve.plan.resource_refs).toEqual([]);
    expect(approve).not.toHaveProperty("proof_bytes");expect(f.calls.some(x=>/continue|\/ceremonies\/|\/actions\//.test(x.url))).toBe(false);
  });
  it("accepts identical Rust context with Native JSON object keys in a different order",async()=>{
    const f=fixture({approvalContext:context=>Object.fromEntries(Object.entries(context).sort(([a],[b])=>a.localeCompare(b)))});
    await f.client.prepare(parent,"Goal");await f.client.approve();
    expect(f.calls.filter(x=>x.url.endsWith("/approve"))).toHaveLength(1);
  });
  it("rejects changed or extra Native approval context fields before Owner I/O",async()=>{
    for(const approvalContext of [context=>({...context,custody_account_id:"other"}),context=>({...context,unexpected:true})]){
      const f=fixture({approvalContext});await f.client.prepare(parent,"Goal");
      await expect(f.client.approve()).rejects.toMatchObject({code:"custody_approval_invalid"});
      expect(f.calls.some(x=>x.url.endsWith("/approve"))).toBe(false);
    }
  });
  it("keeps unknown complete in RAM and explicitly resends its identical body without another assertion or challenge",async()=>{
    const f=fixture();await f.client.prepare(parent,"Goal");f.loseComplete();
    await expect(f.client.approve()).rejects.toMatchObject({code:"custody_outcome_unknown"});
    expect(f.client.summary().completeUnknown).toBe(true);await f.client.retry();
    const complete=f.calls.filter(x=>x.url===CUSTODY_PATHS.complete_path);
    expect(complete).toHaveLength(2);expect(complete[0].options.body).toBe(complete[1].options.body);
    expect(f.get).toHaveBeenCalledTimes(1);expect(f.calls.filter(x=>x.url===CUSTODY_PATHS.options_path)).toHaveLength(1);
  });
  it("shares a double click and discards a late authenticator after account switch",async()=>{
    let release;const get=vi.fn(()=>new Promise(resolve=>{release=resolve;}));const f=fixture({get});await f.client.prepare(parent,"Goal");
    const first=f.client.approve();const second=f.client.approve();expect(first).toBe(second);
    await vi.waitFor(()=>expect(get).toHaveBeenCalledTimes(1));
    f.client.setSession({account_id:"other"},{method:"webauthn"},cap);release(credential());
    await expect(first).rejects.toMatchObject({code:"custody_session_changed"});expect(f.client.summary()).toBeNull();
    expect(f.calls.some(x=>x.url===CUSTODY_PATHS.complete_path)).toBe(false);
  });
  it("does not call ordinary cancel/status after UV cancellation",async()=>{
    const f=fixture({get:vi.fn(async()=>{throw new DOMException("cancelled","NotAllowedError");})});await f.client.prepare(parent,"Goal");
    await expect(f.client.approve()).rejects.toMatchObject({code:"custody_authenticator_cancelled"});
    expect(f.client.summary().cancelled).toBe(true);expect(f.calls.some(x=>/cancel|\/operations\//.test(x.url))).toBe(false);
  });
  it("blocks native-key, caller-picked endpoints and unmatched pagination before I/O",async()=>{
    const f=fixture();f.client.setSession({account_id:"manager"},{method:"native_key"},cap);
    await expect(f.client.parents()).rejects.toMatchObject({code:"custody_session_changed"});expect(f.calls).toHaveLength(0);
    f.client.setSession({account_id:"manager"},{method:"webauthn"},{...cap,options_path:"https://attacker.example"});
    await expect(f.client.parents()).rejects.toMatchObject({code:"custody_session_changed"});expect(f.calls).toHaveLength(0);
    f.client.setSession({account_id:"manager"},{method:"webauthn"},cap);
    await expect(f.client.parents("account",null)).rejects.toMatchObject({code:"custody_cursor_invalid"});expect(f.calls).toHaveLength(0);
  });
  it("mounts the actual account surface with explicit metadata loading and no background custody requests",async()=>{
    const f=fixture();vi.stubGlobal("fetch",f.fetch);
    const accountClient={capabilities:async()=>({passkey:{available:true},custody_management:cap}),
      session:async()=>({account:{account_id:"manager",display_name:"Manager"},authentication:{method:"webauthn"}}),
      credentials:async()=>({credentials:[]}),sessions:async()=>({sessions:[]}),clearImportedKey:vi.fn()};
    const root=document.createElement("section");const surface=mountPasskeyAccount(root,{client:accountClient});
    try {await surface.refresh();expect(root.textContent).toContain("受控 OwnerSetup");expect(f.calls).toHaveLength(0);
      const button=[...root.querySelectorAll("button")].find(x=>x.textContent==="加载本人可见父项");button.click();
      await vi.waitFor(()=>expect(root.textContent).toContain("为此父项准备批准摘要"));
      expect(f.calls.filter(x=>x.url===CUSTODY_PATHS.parents_query_path)).toHaveLength(1);
    } finally {surface.dispose();vi.unstubAllGlobals();}
  });
});

describe("cancelled custody approval expiry recovery",()=>{
  function cancelledFixture() {
    let clock=1000;
    const get=vi.fn().mockRejectedValueOnce(new DOMException("cancelled","NotAllowedError")).mockImplementation(async()=>credential());
    const f=fixture({now:()=>clock,get,fetch:async(url)=> url===CUSTODY_PATHS.options_path
      ? new Response(JSON.stringify({api_version:1,phase:"pending",approval_id:"approval-A",ceremony_id:"ceremony-B",operation_id:"operation-C",
        intent_digest:"78".repeat(32),audience:"custody",ceremony_expires_at:String(clock+1000),options:{publicKey:{challenge:"AQ",allowCredentials:[]}}}),{status:200}) : undefined});
    return {...f,setClock(value){clock=value;}};
  }
  it("only explicitly discards a never-submitted cancellation after the original challenge expires, with zero discard I/O",async()=>{
    const f=cancelledFixture();await f.client.prepare(parent,"First goal");
    await expect(f.client.approve()).rejects.toMatchObject({code:"custody_authenticator_cancelled"});
    const original=f.client.summary();const before=f.calls.length;
    await expect(f.client.discardCancelled()).rejects.toMatchObject({code:"custody_cancelled_not_expired"});
    expect(f.client.summary()).toEqual(original);expect(f.calls).toHaveLength(before);
    f.setClock(2000);await f.client.discardCancelled();
    expect(f.client.summary()).toBeNull();expect(f.calls).toHaveLength(before);
    const fresh=await f.client.prepare(parent,"Second goal");
    expect(fresh.plan.setup_id).not.toBe(original.plan.setup_id);
    await f.client.approve();expect(f.get).toHaveBeenCalledTimes(2);
    expect(f.calls.filter(x=>x.url===CUSTODY_PATHS.options_path)).toHaveLength(2);
    expect(f.calls.filter(x=>x.url===CUSTODY_PATHS.complete_path)).toHaveLength(1);
  });
  it("cannot discard complete unknown after challenge and plan expiry or replace its exact retry original",async()=>{
    let clock=1000;const f=fixture({now:()=>clock});await f.client.prepare(parent,"Goal");f.loseComplete();
    await expect(f.client.approve()).rejects.toMatchObject({code:"custody_outcome_unknown"});
    const original=f.client.summary();const before=f.calls.length;clock=99999999999999;
    await expect(f.client.discardCancelled()).rejects.toMatchObject({code:"custody_original_pending"});
    expect(f.client.summary()).toEqual(original);expect(f.calls).toHaveLength(before);
    await f.client.retry();const complete=f.calls.filter(x=>x.url===CUSTODY_PATHS.complete_path);
    expect(complete[0].options.body).toBe(complete[1].options.body);expect(f.get).toHaveBeenCalledTimes(1);
  });
  it("cannot discard approve unknown after expiry; the original approved body remains the only retry",async()=>{
    let clock=1000,lose=true;
    const f=fixture({now:()=>clock,fetch:async(url)=>{if(url.endsWith("/approve")&&lose){lose=false;throw new Error("lost");}}});
    await f.client.prepare(parent,"Goal");await expect(f.client.approve()).rejects.toMatchObject({code:"custody_outcome_unknown"});
    const original=f.client.summary();const before=f.calls.length;clock=99999999999999;
    await expect(f.client.discardCancelled()).rejects.toMatchObject({code:"custody_original_pending"});
    expect(f.client.summary()).toEqual(original);expect(f.calls).toHaveLength(before);
    await f.client.retry();const approvals=f.calls.filter(x=>x.url.endsWith("/approve"));
    expect(approvals[0].options.body).toBe(approvals[1].options.body);expect(f.get).toHaveBeenCalledTimes(1);
  });
});

describe("cancelled recovery expiry validation and mounted controls",()=>{
  const options = expiry => ({api_version:1,phase:"pending",approval_id:"approval-A",ceremony_id:"ceremony-B",operation_id:"operation-C",
    intent_digest:"78".repeat(32),audience:"custody",ceremony_expires_at:expiry,options:{publicKey:{challenge:"AQ",allowCredentials:[]}}});
  it("rejects absent, noncanonical, overflow and already-expired Native expiry before get or complete",async()=>{
    for(const expiry of [undefined,1001,"0","01","-1","1e9","18446744073709551616","1000"]){
      const f=fixture({fetch:async url=>url===CUSTODY_PATHS.options_path?new Response(JSON.stringify(options(expiry)),{status:200}):undefined});
      await f.client.prepare(parent,"Goal");await expect(f.client.approve()).rejects.toMatchObject({code:"custody_options_invalid"});
      expect(f.get).not.toHaveBeenCalled();expect(f.calls.some(x=>x.url===CUSTODY_PATHS.complete_path)).toBe(false);
    }
  });
  it("keeps a valid u64 expiry beyond Number precision lossless and refuses premature discard",async()=>{
    const f=fixture({get:vi.fn(async()=>{throw new DOMException("cancelled","NotAllowedError");}),
      fetch:async url=>url===CUSTODY_PATHS.options_path?new Response(JSON.stringify(options("18446744073709551615")),{status:200}):undefined});
    await f.client.prepare(parent,"Goal");await expect(f.client.approve()).rejects.toMatchObject({code:"custody_authenticator_cancelled"});
    const before=f.calls.length;expect(f.client.summary().ceremonyExpiresAt).toBe("18446744073709551615");
    await expect(f.client.discardCancelled()).rejects.toMatchObject({code:"custody_cancelled_not_expired"});expect(f.calls).toHaveLength(before);
  });
  it("retains prepare unknown and rejects discard even after the original plan deadline",async()=>{
    let clock=1000;const f=fixture({now:()=>clock,fetch:async url=>{if(url.endsWith("/prepare"))throw new Error("lost");}});
    await expect(f.client.prepare(parent,"Goal")).rejects.toMatchObject({code:"custody_outcome_unknown"});
    const original=f.client.summary(),before=f.calls.length;clock=99999999999999;
    await expect(f.client.discardCancelled()).rejects.toMatchObject({code:"custody_original_pending"});
    expect(f.client.summary()).toEqual(original);expect(f.calls).toHaveLength(before);
  });
  it("mounts an explicit zero-I/O expiry recovery button and restores the parent-loading flow",async()=>{
    let clock=1000;const f=fixture({now:()=>clock,get:vi.fn(async()=>{throw new DOMException("cancelled","NotAllowedError");}),
      fetch:async url=>url===CUSTODY_PATHS.options_path?new Response(JSON.stringify(options("2000")),{status:200}):undefined});
    const root=document.createElement("div");const panel=createOwnerSetupPanel({client:f.client});
    panel.update({account_id:"manager"},{method:"webauthn"},cap);
    await f.client.prepare(parent,"Goal");await expect(f.client.approve()).rejects.toMatchObject({code:"custody_authenticator_cancelled"});
    panel.renderInto(root);
    const before=f.calls.length;const click=()=>[...root.querySelectorAll("button")].find(x=>x.textContent==="挑战到期后丢弃未提交原件").click();
    try {
      click();await vi.waitFor(()=>expect(root.textContent).toContain("原挑战尚未到期"));expect(f.calls).toHaveLength(before);
      clock=2000;click();await vi.waitFor(()=>expect(root.textContent).toContain("已丢弃本页未提交原件"));
      expect(root.textContent).toContain("加载本人可见父项");expect(root.textContent).not.toContain("确认同一份 OwnerSetup V1 原件");
      expect(f.client.summary()).toBeNull();expect(f.calls).toHaveLength(before);
    } finally {panel.dispose();}
  });
});

// Component mocks exercise ordering and rendering, not Native signature/UV or
// actual Account/World/inference qualification.
const summaryContract = {api_version:1,summary_digest:"91".repeat(32)};
function v2Draft() {
  return {api_version:2,approval_request_id:"v2-approval",plan:{api_version:2,setup_id:"v2-setup",provision_id:parent.provision_id,
    world_identity:world,participant_receipt_digest:"34".repeat(32),owner_key_ref:"owner",executor_key_ref:"executor",executor_public_key:"12".repeat(32),
    bootstrap_config_version:"1",goal_content:{goal_text:"Bound private goal",soft_constraints:[]},create_operation_id:"v2-create",prepare_operation_id:"v2-prepare",prepare_nonce:"21".repeat(32),
    publish_operation_id:"v2-publish",delegation_operation_id:"v2-delegate",bind_operation_id:"v2-bind",executor_id:"v2-executor",delegation_generation:"1",delegation_nonce:"1",
    selection:{kind:"build",site:"site",factory:"factory",factory_kind:"smelter"},deadline:"100000",request_limit:"6",transport_limit:"584",write_limit:"4",
    model_budget:{api_version:1,approval_contract:"native_custody_model_budget_v1",native_account_id:"native",native_public_key:"11".repeat(32),native_instance_public_key:"22".repeat(32),
      ledger_id:"ledger",preset_id:"fixed-preset",preset_revision:"1",model:"fixed-model",runtime_digest:"33".repeat(32),skill_digest:"44".repeat(32),tools_digest:"55".repeat(32),context_policy_digest:"66".repeat(32),
      physical_graph_digest:"77".repeat(32),probe_physical_http:["10","10"],inference_physical_http:"10",max_calls:"3",max_input_tokens:"100",max_output_tokens:"50",max_total_tokens:"450",max_concurrent:"1",deadline:"90000",
      price_policy:{currency:"USD",input_micro_units_per_million:"1000000",output_micro_units_per_million:"2000000",max_total_micro_units:"600",quote_digest:"88".repeat(32),valid_until:"100000"}}}};
}
function v2Fixture(change={}) {
  let prepared;
  return fixture({now:change.now,fetch(url,options,body){
    let value;
    if(url.endsWith("/prepare")&&body.api_version===2){prepared={approval_request_id:body.approval_request_id,
      context:{environment:"fixture",custody_account_id:parent.custody_account_id,audience:"custody",policy_version:"1",security_version:"1",service_epoch:"1"},
      intent:{kind:"owner_setup_v2",plan:body.plan}};value=prepared;}
    else if(url===CUSTODY_PATHS.options_path){value={api_version:1,phase:"pending",approval_id:"A",ceremony_id:"B",operation_id:"C",intent_digest:"78".repeat(32),
      audience:"custody",ceremony_expires_at:"99000",options:{publicKey:{challenge:"AQ",allowCredentials:[]}},model_budget_summary:structuredClone(prepared.intent.plan.model_budget),
      model_budget_summary_contract:summaryContract};change.options?.(value);}
    else if(url===CUSTODY_PATHS.complete_path)value={api_version:1,phase:"completed",approval:{proof:{approval_id:"A",intent_digest:"78".repeat(32),context:prepared.context,model_budget_summary_contract:summaryContract},signature:"fixture-only"},proof_bytes:"not-authority"};
    else if(url.endsWith("/approve"))value={api_version:2,...parent,setup_id:prepared.intent.plan.setup_id,phase:"completed",model_budget_digest:"92".repeat(32)};
    if(value!==undefined)return new Response(JSON.stringify(value),{status:200});
  }});
}
describe("V2 Native model budget summary contract",()=>{
  it("does not invoke get when Native contract or actual confirmation consumer is missing",async()=>{
    for(const change of [{},{options:v=>delete v.model_budget_summary_contract},{options:v=>v.model_budget_summary.native_public_key="99".repeat(32)}]){
      const f=v2Fixture(change);await f.client.prepareV2(parent,v2Draft());
      await expect(f.client.approve()).rejects.toMatchObject({code:"custody_budget_summary_required"});
      expect(f.get).not.toHaveBeenCalled();expect(f.calls.some(c=>c.url===CUSTODY_PATHS.complete_path||c.url.endsWith("/approve"))).toBe(false);
    }
  });
  it("awaits explicit summary completion before actual get and preserves versioned original through Owner approve",async()=>{
    const f=v2Fixture();await f.client.prepareV2(parent,v2Draft());let release;
    const confirmModelBudget=vi.fn(()=>new Promise(resolve=>{release=resolve;}));
    const result=f.client.approve({confirmModelBudget});await vi.waitFor(()=>expect(confirmModelBudget).toHaveBeenCalledTimes(1));
    expect(f.get).not.toHaveBeenCalled();expect(confirmModelBudget.mock.calls[0][0].budget).toEqual(v2Draft().plan.model_budget);
    release();await result;expect(f.get).toHaveBeenCalledTimes(1);
    expect(f.calls.find(c=>c.url.endsWith("/approve")).body.api_version).toBe(2);
    expect(f.calls.some(c=>c.url.endsWith("/continue"))).toBe(false);
  });
  it("account switch or deadline during confirmation prevents get, complete and approve",async()=>{
    for(const switched of [true,false]){let now=1000;const f=v2Fixture({now:()=>now});await f.client.prepareV2(parent,v2Draft());let release;
      const promise=f.client.approve({confirmModelBudget:()=>new Promise(resolve=>{release=resolve;})});await vi.waitFor(()=>expect(release).toBeTypeOf("function"));
      if(switched)f.client.setSession({account_id:"other"},{method:"webauthn"},cap);else now=95000;
      release();await expect(promise).rejects.toMatchObject({code:switched?"custody_session_changed":"custody_budget_summary_expired"});
      expect(f.get).not.toHaveBeenCalled();expect(f.calls.some(c=>c.url===CUSTODY_PATHS.complete_path||c.url.endsWith("/approve"))).toBe(false);
    }
  });
  it("renders the actual panel budget and target before explicit confirmation, then gets once",async()=>{
    const f=v2Fixture();const panel=createOwnerSetupPanel({client:f.client});const root=document.createElement("div");document.body.append(root);
    panel.update({account_id:"manager"},{method:"webauthn"},cap);panel.renderInto(root);
    try{await panel.prepareV2(parent,v2Draft());[...root.querySelectorAll("button")].find(b=>b.textContent==="确认摘要并使用通行密钥批准").click();
      await vi.waitFor(()=>expect(root.querySelector('[role="dialog"]')).not.toBeNull());expect(f.get).not.toHaveBeenCalled();
      const text=root.querySelector('[role="dialog"]').textContent;
      for(const value of ["Bound private goal","fixed-preset","fixed-model","包含两次真实探测","物理 HTTP","USD / 600","90000"])expect(text).toContain(value);
      [...root.querySelectorAll("button")].find(b=>b.textContent==="已核对预算，继续通行密钥").click();
      await vi.waitFor(()=>expect(f.get).toHaveBeenCalledTimes(1));await vi.waitFor(()=>expect(f.client.summary().state).toBe("approved"));
    }finally{panel.dispose();root.remove();}
  });
});
