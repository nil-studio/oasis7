// Isolated Account-only scheduling fault; every read still targets its real binding.
import deployed,{AccountAuthority as ActualAccount} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
function bound(target,name){const value=Reflect.get(target,name,target);return typeof value==='function'&&name!=='constructor'?value.bind(target):value;}
function batch(mode,state,sessionId){
  const facts={gate_reads:0,fence_reads:0,paid_confirmations:0,oracle_ticket:null,source_ticket:null,released:null,elapsed_ms:0,gate_status:null,gate_faulted:false,fence_disabled:false};
  let started=null,timer,resolve;const ready=new Promise(done=>{resolve=done;});
  function release(reason){if(facts.released!==null)return;facts.released=reason;facts.elapsed_ms=Date.now()-started;clearTimeout(timer);resolve();}
  return {facts,async arrive(kind){
    const row=state.storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").toArray()[0];
    const session=JSON.parse(row.payload).state.test_sessions[sessionId];
    const reads=Object.values(session.source_oracle_reads);
    if(session.preflight_oracle_reads!==3||reads.length!==1||reads[0]!==3)throw new Error('fixed factory oracle must be committed before readonly I/O');
    facts.oracle_ticket=session.preflight_oracle_reads;facts.source_ticket=reads[0];facts.paid_confirmations++;
    facts[kind]++;if(facts[kind]!==1)throw new Error('fixed readonly input cannot retry');
    if(started===null){started=Date.now();timer=setTimeout(()=>release('bounded_timeout'),5100);}
    if(mode!=='over_window'&&facts.gate_reads===1&&facts.fence_reads===1)release('independent_reads_started');
    await ready;
  }};
}
export class AccountAuthority extends ActualAccount {
  constructor(state,env){
    const control={active:null,last:null};
    const wrapped=new Proxy(env,{get(target,name){
      if(name==='MANAGER_ASSOCIATIONS')return new Proxy(target[name],{get(namespace,method){
        if(method==='get')return(...args)=>new Proxy(namespace.get(...args),{get(stub,operation){
          if(operation==='fetch')return async(...input)=>{
            const raw=input[0];const path=new URL(typeof raw==='string'?raw:raw.url).pathname;const active=control.active;
            if(active&&path==='/internal/provision-gate'){
              await active.arrive('gate_reads');const response=await stub.fetch(...input);active.facts.gate_status=response.status;
              if(env.FIXTURE_PAID_DIRECTORY_MODE==='reject_gate'&&response.status===200){active.facts.gate_faulted=true;throw new Error('fixed fault after actual successful gate response');}
              return response;
            }
            return stub.fetch(...input);
          };
          return bound(stub,operation);
        }});
        return bound(namespace,method);
      }});
      if(name==='RECOVERY_FENCE')return new Proxy(target[name],{get(service,method){
        if(method==='read')return async()=>{const active=control.active;
          if(active){await active.arrive('fence_reads');if(env.FIXTURE_PAID_DIRECTORY_MODE==='freeze_fence')await env.FIXTURE_FENCE_CONTROL.disable();}
          const result=await service.read();if(active)active.facts.fence_disabled=JSON.parse(result).disabled===true;return result;
        };
        return bound(service,method);
      }});
      return bound(target,name);
    }});
    super(state,wrapped);this.control=control;this.fixtureState=state;this.fixtureMode=env.FIXTURE_PAID_DIRECTORY_MODE;
  }
  async fetch(request){
    const url=new URL(request.url);
    if(this.fixtureMode&&request.method==='POST'&&url.origin==='https://account.internal'&&url.pathname==='/fixture/paid-directory-fence-status'&&!url.search&&!url.hash)return Response.json(this.control.last?.facts??null);
    let selected=null;
    if(this.fixtureMode&&request.method==='POST'&&url.pathname==='/internal/test-session-owner'){
      const input=await request.clone().json();
      if(input.action?.kind==='get_permit'&&input.action.access!==null&&input.action.access!==undefined){selected=batch(this.fixtureMode,this.fixtureState,input.request.session_id);this.control.active=selected;this.control.last=selected;}
    }
    try{return await super.fetch(request);}finally{if(selected&&this.control.active===selected)this.control.active=null;}
  }
}
