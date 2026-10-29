// Test-only scheduling barrier on one real, already-paid Bind result callback.
// Real bindings are always forwarded; no success, proof, clock or Current is forged.
import deployed,{AccountAuthority as ActualAccount} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
function bound(target,name){const value=Reflect.get(target,name,target);return typeof value==='function'&&name!=='constructor'?value.bind(target):value;}
function saved(state,id){
  const row=state.storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one();
  const account=JSON.parse(row.payload).state;
  const setup=account.owner_setups[id];
  if(!setup)throw new Error('fixed setup fixture missing');
  return {setup,stage:setup.stages.bind};
}
function batch(state,id,mode){
  const before=saved(state,id),source=JSON.stringify(before.stage.source),spent=before.setup.transport.spent;
  const facts={selected_calls:1,gate_reads:0,fence_reads:0,paid_confirmations:0,callback_before:before.stage.callback_claimed_at!==null,
    callback_claimed:false,result_empty:false,stage_prepaid:false,stage_count:0,setup_transport_spent:null,
    native_source_present:false,canonical_committed:false,source_unchanged:null,spent_unchanged:null,
    result_after:null,gate_status:null,released:null,elapsed_ms:0,first_source_age_ms:null,source_fresh_at_start:false,gate_faulted:false,fence_disabled:false,sql_faults:0,update_attempts:0,target_candidate_seen:false,candidate_source_fresh:false};
  let started=null,timer,resolve;const ready=new Promise(done=>{resolve=done;});
  function release(reason){if(facts.released!==null)return;facts.released=reason;facts.elapsed_ms=Date.now()-started;clearTimeout(timer);resolve();}
  const control={facts,setupId:id,beforeRaw:state.storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload,
    beforeAudit:state.storage.sql.exec("SELECT COUNT(*) AS n FROM custody_audit WHERE record_id='custody-provision-approval-v1'").one().n,
    async arrive(kind){
    // SQL-fault mode must observe/forward a mistaken outgoing call, not hide it
    // behind this fixture's paid-state assertion.
    if(mode==='prepay_sql_fail'){facts[kind]++;return;}

    const {setup,stage}=saved(state,id),transport=setup.transport;
    const count=Object.keys(setup.stages).length;
    const paid=BigInt(transport.spent)===8n+96n*BigInt(count)
      &&BigInt(transport.limit)===BigInt(setup.plan.transport_limit)
      &&Object.keys(transport.reservations).length===0;
    const committed=Object.values(stage.canonical_history).some(fact=>fact.response?.payload?.outcome?.status==='committed');
    Object.assign(facts,{callback_claimed:stage.callback_claimed_at!==null,result_empty:stage.result===null,
      stage_prepaid:paid,stage_count:count,setup_transport_spent:Number(transport.spent),
      native_source_present:stage.source.verification!==null,canonical_committed:committed});
    if(facts.callback_before||!facts.callback_claimed||!facts.result_empty||!paid
      ||!facts.native_source_present||!committed||stage.claimed_at===null
      ||JSON.stringify(stage.source)!==source||transport.spent!==spent){
      throw new Error('fixed paid Bind callback precondition failed');
    }
    facts.paid_confirmations++;facts[kind]++;
    if(facts[kind]!==1)throw new Error('fixed Bind readonly input cannot retry');
    if(started===null){
      started=Date.now();
      const observed=Number(stage.source.verification.verification.observed_at);
      facts.first_source_age_ms=Number.isSafeInteger(observed)&&observed<=started?started-observed:null;
      facts.source_fresh_at_start=facts.first_source_age_ms!==null&&facts.first_source_age_ms<=5000;
      timer=setTimeout(()=>release('bounded_timeout'),5100);
    }
    if(mode!=='over_window'&&facts.gate_reads===1&&facts.fence_reads===1)release('independent_reads_started');
    await ready;
  },finish(){
    const {setup,stage}=saved(state,id);
    facts.source_unchanged=JSON.stringify(stage.source)===source;
    facts.spent_unchanged=setup.transport.spent===spent;
    facts.result_after=stage.result!==null;
    if(mode==='prepay_sql_fail'){
      const sql=state.storage.sql;
      facts.record_unchanged=sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload===control.beforeRaw;
      facts.audit_unchanged=sql.exec("SELECT COUNT(*) AS n FROM custody_audit WHERE record_id='custody-provision-approval-v1'").one().n===control.beforeAudit;
      facts.trigger_removed=sql.exec("SELECT COUNT(*) AS n FROM sqlite_master WHERE type='trigger' AND name='fixture_setup_callback_fail'").one().n===0;
      facts.callback_after=stage.callback_claimed_at!==null;
    }
  }};
  return control;
}
export class AccountAuthority extends ActualAccount {
  constructor(state,env){
    const control={active:null,last:null};
    const wrapped=new Proxy(env,{get(target,name){
      if(name==='MANAGER_ASSOCIATIONS')return new Proxy(target[name],{get(namespace,method){
        if(method==='get')return(...args)=>new Proxy(namespace.get(...args),{get(stub,operation){
          if(operation==='fetch')return async(...input)=>{
            const raw=input[0],url=new URL(typeof raw==='string'?raw:raw.url),active=control.active;
            if(active&&url.origin==='https://directory.internal'&&url.pathname==='/internal/provision-gate'){
              await active.arrive('gate_reads');
              const response=await stub.fetch(...input);active.facts.gate_status=response.status;
              if(env.FIXTURE_SETUP_PAID_RESULT_MODE==='reject_gate'&&response.status===200){
                await response.arrayBuffer();active.facts.gate_faulted=true;
                throw new Error('fixed fault after actual successful Bind gate response');
              }
              return response;
            }
            return stub.fetch(...input);
          };
          return bound(stub,operation);
        }});
        return bound(namespace,method);
      }});
      if(name==='RECOVERY_FENCE')return new Proxy(target[name],{get(service,method){
        if(method==='read')return async()=>{
          const active=control.active;
          if(active){await active.arrive('fence_reads');if(env.FIXTURE_SETUP_PAID_RESULT_MODE==='freeze_fence')await env.FIXTURE_FENCE_CONTROL.disable();}
          const result=await service.read();if(active)active.facts.fence_disabled=JSON.parse(result).disabled===true;return result;
        };
        return bound(service,method);
      }});
      return bound(target,name);
    }});
    const storage=state.storage;
    const sql=new Proxy(storage.sql,{get(target,name){
      if(name==='exec')return(...args)=>{
        const active=control.active;
        if(env.FIXTURE_SETUP_PAID_RESULT_MODE==='prepay_sql_fail'&&active&&active.facts.sql_faults===0
          &&typeof args[0]==='string'&&args[0].startsWith('UPDATE custody_records SET payload')
          &&args[2]==='custody-provision-approval-v1'){
          const setup=JSON.parse(args[1]).state.owner_setups[active.setupId],stage=setup?.stages.bind;
          if(stage?.callback_claimed_at!==null&&stage?.callback_claimed_at!==undefined&&stage.result===null){
            active.facts.update_attempts++;active.facts.target_candidate_seen=true;
            active.facts.stage_prepaid=BigInt(setup.transport.spent)===8n+96n*BigInt(Object.keys(setup.stages).length);
            active.facts.native_source_present=stage.source.verification!==null;
            const observed=Number(stage.source.verification?.verification.observed_at),now=Date.now();
            active.facts.candidate_source_fresh=Number.isSafeInteger(observed)&&observed<=now&&now-observed<=5000&&now<Number(stage.source.expires_at);
            active.facts.canonical_committed=Object.values(stage.canonical_history).some(fact=>fact.response?.payload?.outcome?.status==='committed');
            // Actual SQLite UPDATE aborts after Rust finalchecks and audit INSERT;
            // the transaction must roll back its record, audit and trigger DDL.
            target.exec("CREATE TRIGGER fixture_setup_callback_fail BEFORE UPDATE ON custody_records WHEN OLD.id = 'custody-provision-approval-v1' BEGIN SELECT RAISE(ABORT, 'fixed setup callback SQL failure'); END");
            try{return target.exec(...args);}catch(error){active.facts.sql_faults++;throw error;}
          }
        }
        return target.exec(...args);
      };
      return bound(target,name);
    }});
    const wrappedStorage=new Proxy(storage,{get(target,name){if(name==='sql')return sql;return bound(target,name);}});
    const wrappedState=new Proxy(state,{get(target,name){if(name==='storage')return wrappedStorage;return bound(target,name);}});
    super(wrappedState,wrapped);this.fixtureState=state;this.control=control;
    this.fixtureEnabled=env.FIXTURE_SETUP_PAID_RESULT_BARRIER==='true';this.fixtureMode=env.FIXTURE_SETUP_PAID_RESULT_MODE??'together';
    if(!['together','reject_gate','freeze_fence','over_window','prepay_sql_fail'].includes(this.fixtureMode))throw new Error('fixed Bind fixture mode invalid');
  }
  async fetch(request){
    const url=new URL(request.url);
    if(this.fixtureEnabled&&request.method==='POST'&&url.origin==='https://account.internal'
      &&url.pathname==='/fixture/setup-paid-result-status'&&!url.search&&!url.hash){
      return Response.json(this.control.last?.facts??null);
    }
    let selected=null;
    if(this.fixtureEnabled&&request.method==='POST'&&url.origin==='https://account.internal'
      &&url.pathname==='/internal/owner-setup-identity'&&!url.search&&!url.hash){
      const input=await request.clone().json();
      if(input.stage==='bind'&&input.action?.kind==='record_result'){
        if(this.control.last)throw new Error('fixed Bind result callback must be once');
        selected=batch(this.fixtureState,input.request.setup_id,this.fixtureMode);
        this.control.active=selected;this.control.last=selected;
      }
    }
    try{return await super.fetch(request);}finally{
      if(selected){selected.finish();if(this.control.active===selected)this.control.active=null;}
    }
  }
}
