// Local component contract gate: input must be emitted by the canonical Rust test.
// This exercises real JS consumers, not WebAuthn, Native HTTP or World execution.
import { readFileSync } from "node:fs";
import { isDeepStrictEqual } from "node:util";
import { createOwnerSetupClient, CUSTODY_PATHS } from "./viewer_custody_owner_setup_client.js";
let stage="fixture";
const requireTrue = value => { if (!value) throw Error("contract_gate_failed"); };
const same=isDeepStrictEqual;
try {
  const exported=JSON.parse(readFileSync(process.argv[2],"utf8"));
  requireTrue(exported.api_version===1&&exported.fixture_only===true&&exported.cases.length===2);
  for(const item of exported.cases){
    const {original,prepared,contract,summary,budget,input}=item;
    const approval=input.approval,approvalId=approval.proof.approval_id,intentDigest=approval.proof.intent_digest;
    requireTrue(same(input.request,prepared)&&same(approval.proof.model_budget_summary_contract,contract));
    stage="typed_shape";
    requireTrue(!Object.hasOwn(original.plan,"provision_id")&&Object.hasOwn(original.plan,"model_budget"));
    const parent={...original.scope.client_pairing.target,environment:prepared.context.environment,world_id:original.scope.world_identity.world_id};
    const capability={api_version:1,available:true,audience:prepared.context.audience,environment:parent.environment,...CUSTODY_PATHS};
    let gets=0,confirms=0,approves=0,options=0;
    const fetch=async(url,request)=>{
      const body=request.body?JSON.parse(request.body):null;let response;
      if(url.endsWith("/capabilities")) response={csrf_token:"fixture-csrf",custody_management:capability};
      else if(url.endsWith("/prepare")){requireTrue(same(body,original));response=prepared;}
      else if(url===CUSTODY_PATHS.options_path){++options;requireTrue(same(body,prepared));response={api_version:1,phase:"pending",approval_id:approvalId,ceremony_id:"fixture-ceremony",operation_id:"fixture-operation",intent_digest:intentDigest,audience:capability.audience,ceremony_expires_at:"5000",options:{},model_budget_summary_contract:contract,custody_session_summary_v2:summary,model_budget_summary:budget};}
      else if(url===CUSTODY_PATHS.complete_path){requireTrue(body.approval_id===approvalId&&body.intent_digest===intentDigest&&body.ceremony_id==="fixture-ceremony"&&body.operation_id==="fixture-operation"&&request.headers["X-Oasis7-CSRF"]==="fixture-csrf");response={api_version:1,phase:"completed",approval};}
      else if(url.endsWith("/approve")){++approves;requireTrue(same(body.input,input)&&!Object.hasOwn(body,"plan"));response={api_version:2,custody_account_id:parent.custody_account_id,provision_id:parent.provision_id,session_id:original.scope.session_id,phase:"completed"};}
      else throw Error("closed_route");
      return new Response(JSON.stringify(response));
    };
    const client=createOwnerSetupClient({fetch,credentials:{get:async()=>{++gets;requireTrue(confirms===1);return{};}},browserPasskeyOptions:x=>x,serializePasskey:()=>({}),now:()=>exported.now_ms});
    client.setSession({account_id:approval.proof.manager_account_id},{method:"webauthn"},capability);
    stage="prepare";await client.prepareSessionV2(parent,original);
    stage="confirm_get_complete_approve";const result=await client.approve({confirmSession:async actual=>{++confirms;requireTrue(gets===0&&same(actual.plan,original.plan)&&same(actual.scope,original.scope));}});
    requireTrue(gets===1&&options===1&&approves===1&&result.execution_started===false);
  }
  console.log("typed_session_ui_contract_cases=2 passed=true browser_native_qualification=false");
}catch{console.error(`typed_session_ui_contract_failed stage=${stage}`);process.exitCode=1;}
