// Isolated Account-only scheduling fault; every read still targets its real binding.
import deployed,{AccountAuthority as ActualAccount} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
function bound(target,name){const value=Reflect.get(target,name,target);return typeof value==='function'&&name!=='constructor'?value.bind(target):value;}
function batch(mode,state,sessionId,ticket){
  const facts={gate_reads:0,fence_reads:0,paid_confirmations:0,oracle_ticket:null,source_ticket:null,released:null,elapsed_ms:0,gate_status:null,gate_faulted:false,fence_disabled:false};
  let started=null,timer,resolve;const ready=new Promise(done=>{resolve=done;});
  function release(reason){if(facts.released!==null)return;facts.released=reason;facts.elapsed_ms=Date.now()-started;clearTimeout(timer);resolve();}
  return {facts,async arrive(kind){
    if(mode==='prepay_sql_fail'){facts[kind]++;return;}
    const row=state.storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").toArray()[0];
    const session=JSON.parse(row.payload).state.test_sessions[sessionId];
    const reads=Object.values(session.source_oracle_reads);
    if(session.preflight_oracle_reads!==ticket||reads.length!==1||reads[0]!==ticket)throw new Error('fixed selected role oracle must be committed before readonly I/O');
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
    const storage=state.storage;
    const sql=new Proxy(storage.sql,{get(target,name){
      if(name==='exec')return (...args)=>{
        const active=control.active;
        if(env.FIXTURE_PAID_DIRECTORY_MODE==='prepay_sql_fail'&&active&&active.facts.sql_faults===0
          &&typeof args[0]==='string'&&args[0].startsWith('UPDATE custody_records SET payload')
          &&args[2]==='custody-provision-approval-v1'){
          const next=JSON.parse(args[1]).state.test_sessions[active.sessionId];
          const paid=Object.values(next.source_oracle_reads);
          if(next.preflight_oracle_reads===active.ticket&&paid.length===1&&paid[0]===active.ticket){
            active.facts.update_attempts++;active.facts.target_candidate_seen=true;
            // Installed only AFTER Rust's strict inventory/transition checks and
            // audit INSERT, inside that real transaction. The actual SQLite
            // UPDATE fails; the whole transaction must roll back audit+record+DDL.
            target.exec("CREATE TRIGGER fixture_paid_ticket_fail BEFORE UPDATE ON custody_records WHEN OLD.id = 'custody-provision-approval-v1' BEGIN SELECT RAISE(ABORT, 'fixed paid oracle SQL failure'); END");
            try{return target.exec(...args);}catch(error){active.facts.sql_faults++;throw error;}
          }
        }
        return target.exec(...args);
      };
      return bound(target,name);
    }});
    const wrappedStorage=new Proxy(storage,{get(target,name){if(name==='sql')return sql;return bound(target,name);}});
    const wrappedState=new Proxy(state,{get(target,name){if(name==='storage')return wrappedStorage;return bound(target,name);}});
    super(wrappedState,wrapped);this.control=control;this.fixtureState=state;this.fixtureMode=env.FIXTURE_PAID_DIRECTORY_MODE;this.fixtureTicket=Number(env.FIXTURE_PAID_DIRECTORY_TICKET??3);if(![3,4,5].includes(this.fixtureTicket))throw new Error('fixed oracle selection invalid');
  }
  async fetch(request){
    const url=new URL(request.url);
    if(this.fixtureMode&&request.method==='POST'&&url.origin==='https://account.internal'&&url.pathname==='/fixture/paid-directory-fence-status'&&!url.search&&!url.hash)return Response.json(this.control.last?.facts??null);
    let selected=null;
    if(this.fixtureMode&&request.method==='POST'&&url.pathname==='/internal/test-session-owner'){
      const input=await request.clone().json();
      const action=input.action;
      const matches=this.fixtureTicket===3
        ? action?.kind==='get_permit'&&action.access!==null&&action.access!==undefined
        : this.fixtureTicket===4
          ? action?.kind==='get_permit'&&(action.access===null||action.access===undefined)
          : action?.kind==='publish_owner_read';
      if(matches){
        // This selects a fixed private Owner port only. arrive() independently
        // verifies that Rust has actually committed the target paid ticket.
        selected=batch(this.fixtureMode,this.fixtureState,input.request.session_id,this.fixtureTicket);
        if(this.fixtureMode==='prepay_sql_fail'){
          const raw=this.fixtureState.storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload;
          const session=JSON.parse(raw).state.test_sessions[input.request.session_id];
          selected.sessionId=input.request.session_id;selected.ticket=this.fixtureTicket;
          selected.beforeRaw=raw;selected.beforeRoots=JSON.stringify(session.roots);
          selected.beforeAudit=this.fixtureState.storage.sql.exec("SELECT COUNT(*) AS n FROM custody_audit WHERE record_id='custody-provision-approval-v1'").one().n;
          Object.assign(selected.facts,{sql_faults:0,update_attempts:0,target_candidate_seen:false,oracle_before:session.preflight_oracle_reads,source_before:Object.values(session.source_oracle_reads)[0]});
        }
        this.control.active=selected;this.control.last=selected;
      }
    }
    try{return await super.fetch(request);}finally{
      if(selected&&this.fixtureMode==='prepay_sql_fail'){
        const sql=this.fixtureState.storage.sql;
        const raw=sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload;
        const session=JSON.parse(raw).state.test_sessions[selected.sessionId];
        const audit=sql.exec("SELECT COUNT(*) AS n FROM custody_audit WHERE record_id='custody-provision-approval-v1'").one().n;
        Object.assign(selected.facts,{oracle_after:session.preflight_oracle_reads,source_after:Object.values(session.source_oracle_reads)[0],record_unchanged:raw===selected.beforeRaw,roots_unchanged:JSON.stringify(session.roots)===selected.beforeRoots,audit_unchanged:audit===selected.beforeAudit,trigger_removed:sql.exec("SELECT COUNT(*) AS n FROM sqlite_master WHERE type='trigger' AND name='fixture_paid_ticket_fail'").one().n===0});
      }
      if(selected&&this.control.active===selected)this.control.active=null;
    }
  }
}
