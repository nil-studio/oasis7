import { describe, it, expect, vi } from "vitest";
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
  const client=createOwnerSetupClient({fetch,credentials:{get},browserPasskeyOptions,serializePasskey,now:()=>1000});
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
