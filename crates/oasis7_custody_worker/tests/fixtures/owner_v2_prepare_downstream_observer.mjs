// Observe only the actual named RPC result; never replace authority/response.
import deployed,{OwnerManagement as RealOwnerManagement,OwnerNativePermits as RealOwnerNativePermits,ManagerAssociations as RealManagerAssociations,AccountAuthority as RealAccountAuthority} from './tool_nonce_owner_runtime.mjs';
export * from './tool_nonce_owner_runtime.mjs';
export default deployed;
function same(a,b){if(a===b)return true;if(!a||!b||typeof a!=='object'||typeof b!=='object')return false;if(Array.isArray(a)||Array.isArray(b))return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every((v,i)=>same(v,b[i]));const keys=Object.keys(a);return keys.length===Object.keys(b).length&&keys.every(k=>Object.hasOwn(b,k)&&same(a[k],b[k]));}
export class OwnerManagement extends RealOwnerManagement {
  async prepareOwnerSetup(raw){
    let request;try{request=JSON.parse(raw)?.request;}catch{}
    if(request?.api_version!==2)return super.prepareOwnerSetup(raw);
    console.warn('fixture.owner_v2_prepare.received_v2');
    let original;
    try{original=await super.prepareOwnerSetup(raw);}catch(error){console.warn('fixture.owner_v2_prepare.rpc_failed');throw error;}
    if(typeof original!=='string'){console.warn('fixture.owner_v2_prepare.returned_nontext');return original;}
    console.warn('fixture.owner_v2_prepare.returned_text');
    let value;try{value=JSON.parse(original);}catch{return original;}
    if(value?.intent?.kind==='owner_setup_v2')console.warn('fixture.owner_v2_prepare.closed_v2');
    if(value?.intent?.kind==='owner_setup_v2'&&value.approval_request_id===request.approval_request_id&&same(value.intent.plan,request.plan))console.warn('fixture.owner_v2_prepare.exact_original');
    if(typeof value?.context?.custody_account_id==='string'&&value.context.custody_account_id.length>0&&typeof value.context.security_version==='string'&&/^[1-9][0-9]*$/.test(value.context.security_version))console.warn('fixture.owner_v2_prepare.valid_context');
    return original;
  }
}

// Observe the actual named Identity-to-Owner preflight RPC unchanged.
export class OwnerNativePermits extends RealOwnerNativePermits {
  async clientChallenge(raw){
    let value;try{value=JSON.parse(raw);}catch{}
    const action=value?.action?.kind==='admit'?'admit':null;
    if(action)console.warn('fixture.credential_source.'+action+'.entered');
    try{const reply=await super.clientChallenge(raw);if(action)console.warn('fixture.credential_source.'+action+'.returned');return reply;}
    catch(error){if(action)console.warn('fixture.credential_source.'+action+'.threw');throw error;}
  }
  async clientV2CredentialChallenge(raw){
    let value;try{value=JSON.parse(raw);}catch{}
    const known=['admit','claim_source','record_source','observe','claim_lifetime','complete'];
    const action=value?.api_version===2&&known.includes(value?.action?.stage)?value.action.stage:null;
    if(action)console.warn('fixture.credential_source.'+action+'.entered');
    try{const reply=await super.clientV2CredentialChallenge(raw);if(action)console.warn('fixture.credential_source.'+action+'.returned');return reply;}
    catch(error){if(action)console.warn('fixture.credential_source.'+action+'.threw');throw error;}
  }
  async testSession(raw){
    let value;try{value=JSON.parse(raw);}catch{}
    const known=['describe_preflight','claim_source','record_source','get_permit','claim_step','reserve_source_read','record_access','record_lifetime','publish_owner_read','activate'];
    const action=value?.request?.api_version===2&&known.includes(value?.action?.kind)?value.action.kind:null;
    if(action)console.warn('fixture.session_preflight.'+action+'.entered');
    try{const reply=await super.testSession(raw);if(action)console.warn('fixture.session_preflight.'+action+'.returned');return reply;}
    catch(error){if(action)console.warn('fixture.session_preflight.'+action+'.threw');throw error;}
  }
}

// The real DO receiver and its response are unchanged; fixed inspection status only.
export class ManagerAssociations extends RealManagerAssociations {
  async fetch(request){
    const inspect=new URL(request.url).pathname==='/internal/provision-inspect-gate';
    if(inspect)console.warn('fixture.session_inspection.entered');
    try{const response=await super.fetch(request);if(inspect)console.warn('fixture.session_inspection.'+(response.status===200?'status_200':response.status===409?'status_409':'status_other'));return response;}
    catch(error){if(inspect)console.warn('fixture.session_inspection.threw');throw error;}
  }
}

// Test-only inspection of actual Account SQL after the genuine first paid Admit.
// It never changes the stored Source, response or dispatch result.
export class AccountAuthority extends RealAccountAuthority {
  constructor(state,env){super(state,env);this.fixtureSourceState=state;}
  fixtureSources(session){
    const row=this.fixtureSourceState.storage.sql.exec('SELECT payload FROM custody_records WHERE id = ?','custody-provision-approval-v1').one();
    const state=JSON.parse(row.payload).state;
    return state.test_sessions_v2?.[session]?.sources??{};
  }
  async fetch(request){
    let session=null,before=null;
    if(request.method==='POST'&&new URL(request.url).pathname==='/internal/test-session-client-challenge'){
      let rpc;try{rpc=await request.clone().json();}catch{}
      if(rpc?.action?.kind==='admit'&&rpc?.admission?.http?.selector?.api_version===1){
        const selected=rpc.admission.http.selector.session_id;
        try{before=new Set(Object.keys(this.fixtureSources(selected)));session=selected;}
        catch{console.warn('fixture.credential_profile.inspection_error');}
      }
    }
    const response=await super.fetch(request);
    if(session!==null&&response.status===200){
      try{
        const created=Object.entries(this.fixtureSources(session)).filter(([key])=>!before.has(key));
        if(created.length===1){
          console.warn('fixture.credential_profile.first_commit');
          const source=created[0][1];
          console.warn('fixture.credential_profile.'+(!Object.hasOwn(source,'observation_profile')?'missing':source.observation_profile==='client_credential_five_oracle_v2'?'credential_five':'wrong'));
        }else console.warn('fixture.credential_profile.no_unique_source');
      }catch{console.warn('fixture.credential_profile.inspection_error');}
    }
    return response;
  }
}
