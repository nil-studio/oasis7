// Fixed, once-only faults around actual workerd Storage/SQLite operations.
import deployed,{AccountAuthority as ActualAccount} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
export class AccountAuthority extends ActualAccount {
  constructor(state,env){
    const mode=env.FIXTURE_TEST_SESSION_ALARM_FAULT;
    const facts={set_faults:0,sql_faults:0,final_update_seen:false};
    const storage=state.storage;
    let pendingAudit=false;
    const sql=new Proxy(storage.sql,{get(target,name){
      if(name==='exec')return (...args)=>{
        const result=target.exec(...args);
        if(mode==='sql-once'&&facts.sql_faults===0){
          if(args[0].startsWith('INSERT INTO custody_audit')&&String(args[3]).includes('"test_session_pending"'))pendingAudit=true;
          if(pendingAudit&&args[0].startsWith('UPDATE custody_records SET payload')){
            const saved=JSON.parse(target.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload);
            facts.final_update_seen=Object.keys(saved.state.test_sessions).length===1;
            facts.sql_faults++;
            throw new Error('fixed test session pending SQL failure');
          }
        }
        return result;
      };
      const value=Reflect.get(target,name,target);return typeof value==='function'?value.bind(target):value;
    }});
    const wrappedStorage=new Proxy(storage,{get(target,name){
      if(name==='sql')return sql;
      if(name==='setAlarm')return async value=>{
        if(mode==='set-once'&&facts.set_faults===0){facts.set_faults++;throw new Error('fixed test session alarm scheduling failure');}
        return target.setAlarm(value);
      };
      const value=Reflect.get(target,name,target);return typeof value==='function'?value.bind(target):value;
    }});
    const wrappedState=new Proxy(state,{get(target,name){if(name==='storage')return wrappedStorage;const value=Reflect.get(target,name,target);return typeof value==='function'?value.bind(target):value;}});
    super(wrappedState,env);
    this.fixtureState=state;this.fixtureMode=mode;this.facts=facts;
  }
  async fetch(request){
    const url=new URL(request.url);
    if(!['/fixture/account-alarm-observe','/fixture/account-alarm-unknown'].includes(url.pathname))return super.fetch(request);
    if(!['set-once','sql-once','unknown-inventory'].includes(this.fixtureMode)||request.method!=='POST'||url.origin!=='https://account.internal'||url.search||url.hash)return new Response('fixed fixture unavailable',{status:404});
    const input=await request.json();
    if(!input||Object.keys(input).length!==1||typeof input.session_id!=='string')return new Response('fixed fixture request rejected',{status:400});
    const storage=this.fixtureState.storage;
    if(url.pathname==='/fixture/account-alarm-unknown'){
      if(this.fixtureMode!=='unknown-inventory')return new Response('fixed fault disabled',{status:409});
      storage.sql.exec('CREATE TABLE fixture_unknown_alarm_inventory (marker INTEGER NOT NULL)');
      return Response.json({inserted:true});
    }
    const raw=storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload;
    const saved=JSON.parse(raw),sessions=Object.values(saved.state.test_sessions);
    const digested=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(raw))),v=>v.toString(16).padStart(2,'0')).join('');
    const audit=storage.sql.exec("SELECT COUNT(*) AS count FROM custody_audit WHERE record_id='custody-provision-approval-v1'").one().count;
    const beginAudit=storage.sql.exec("SELECT COUNT(*) AS count FROM custody_audit WHERE record_id='custody-provision-approval-v1' AND payload LIKE '%\"test_session_pending\"%'").one().count;
    return Response.json({...this.facts,sessions:sessions.length,roots:sessions.reduce((n,s)=>n+Object.keys(s.roots).length,0),consumed:sessions.filter(s=>s.consumed_at!==null).length,audits:audit,begin_audits:beginAudit,record_digest:digested,scheduled_at:await storage.getAlarm()});
  }
}
