// Observe only the actual named RPC result; never replace authority/response.
import deployed,{OwnerManagement as RealOwnerManagement,OwnerNativePermits as RealOwnerNativePermits,ManagerAssociations as RealManagerAssociations} from './tool_nonce_owner_runtime.mjs';
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
