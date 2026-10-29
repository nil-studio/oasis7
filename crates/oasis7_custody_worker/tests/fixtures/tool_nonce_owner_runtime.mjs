// Fixed test-only faults/port observations around actual Rust and workerd SQL.
import deployed, {OwnerNativePermits as RealOwnerNativePermits,AccountAuthority as RealAccountAuthority} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
const recordKey='custody-provision-approval-v1';
function bound(target,name){const value=Reflect.get(target,name,target);return typeof value==='function'&&name!=='constructor'?value.bind(target):value;}
export class OwnerNativePermits extends RealOwnerNativePermits {
  async clientToolNonce(raw){
    console.warn('fixture.tool_nonce.owner');
    const result=await super.clientToolNonce(raw);
    if(this.env.FIXTURE_TOOL_NONCE_FAULT==='drop_reply'){
      const body=JSON.parse(result);
      if(body.kind?.kind!=='tool_metadata'||typeof body.nonce!=='string')throw new Error('fixed metadata response prerequisite failed');
      // Account returned its actual committed metadata. The client receives no
      // substitute body; this named RPC now loses that real reply.
      console.warn('fixture.tool_nonce.reply_lost');
      throw new Error('fixed committed metadata reply lost');
    }
    return result;
  }
}
export class AccountAuthority extends RealAccountAuthority {
  constructor(state,env){
    const control={active:null,failed:false},storage=state.storage;
    const sql=new Proxy(storage.sql,{get(target,name){
      if(name==='exec')return (...args)=>{
        if(env.FIXTURE_TOOL_NONCE_FAULT==='sql_once'&&control.active&&!control.failed
          &&typeof args[0]==='string'&&args[0].startsWith('UPDATE custody_records SET payload')&&args[2]===recordKey){
          const next=JSON.parse(args[1]).state.test_sessions[control.active.session];
          const previous=JSON.parse(target.exec('SELECT payload FROM custody_records WHERE id = ?',recordKey).one().payload).state.test_sessions[control.active.session];
          const key=control.active.request;
          if(next?.tool_nonce_links?.[key]&&!previous.tool_nonce_links?.[key]){
            const entries=Object.entries(previous.tool_nonce_ingresses).filter(([,v])=>v.request.request_id===key);
            if(entries.length!==1)throw new Error('fixed fee prerequisite failed');
            const [ingress,fee]=entries[0],host=previous.purpose_http_admissions[ingress];
            if(host?.class?.kind!=='tool_nonce_three'||host.class.owner_attempt!==fee.owner_attempt
              ||host.class.account_attempt!==fee.account_attempt
              ||previous.purpose_transports[fee.owner_attempt]!==fee.request.purpose
              ||previous.purpose_transports[fee.account_attempt]!==fee.request.purpose)throw new Error('fixed three-hop prerequisite failed');
            console.warn('fixture.tool_nonce.sql_fee_confirmed');
            control.failed=true;
            // Audit has already been inserted by the real transition. Create
            // this trigger inside that same transaction, then let actual SQLite
            // reject the UPDATE; audit, state and trigger DDL must all roll back.
            target.exec("CREATE TRIGGER fixture_tool_nonce_second_update_fail BEFORE UPDATE ON custody_records WHEN OLD.id = 'custody-provision-approval-v1' BEGIN SELECT RAISE(ABORT, 'fixed metadata second SQL failure'); END");
            try{return target.exec(...args);}catch(error){console.warn('fixture.tool_nonce.sql_update_fault');throw error;}
          }
        }
        return target.exec(...args);
      };
      return bound(target,name);
    }});
    const wrappedStorage=new Proxy(storage,{get(target,name){if(name==='sql')return sql;return bound(target,name);}});
    const wrappedState=new Proxy(state,{get(target,name){if(name==='storage')return wrappedStorage;return bound(target,name);}});
    super(wrappedState,env);this.control=control;
  }
  async fetch(request){
    const selected=request.method==='POST'&&new URL(request.url).pathname==='/internal/test-session-client-tool-nonce';
    if(!selected)return super.fetch(request);
    console.warn('fixture.tool_nonce.account');
    const rpc=await request.clone().json();
    if(rpc.action?.kind!=='tool_nonce')throw new Error('fixed metadata action prerequisite failed');
    const original=JSON.parse(rpc.admission.http.original.body);
    this.control.active={session:rpc.admission.http.selector.session_id,request:original.request_id};
    try{return await super.fetch(request);}finally{this.control.active=null;}
  }
}
