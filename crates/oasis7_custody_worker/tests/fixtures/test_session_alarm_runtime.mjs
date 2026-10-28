// Actual workerd alarm fixture. It never edits Account records or their clock.
import deployed,{AccountAuthority as ActualAccount} from './native_owner_result_loss.mjs';
export * from './native_owner_result_loss.mjs';
export default deployed;
export class AccountAuthority extends ActualAccount {
  constructor(state,env){super(state,env);this.fixtureState=state;this.fixtureEnv=env;this.alarmEvents=0;}
  async alarm(){this.alarmEvents++;return super.alarm();}
  async fetch(request){
    const url=new URL(request.url);
    const paths=['/fixture/account-alarm-observe','/fixture/account-alarm-early'];
    if(!paths.includes(url.pathname))return super.fetch(request);
    if(this.fixtureEnv.FIXTURE_TEST_SESSION_ALARM!=='true'||request.method!=='POST'||url.origin!=='https://account.internal'||url.search||url.hash)return new Response('fixed fixture unavailable',{status:404});
    const input=await request.json();
    if(!input||Object.keys(input).length!==1||typeof input.session_id!=='string')return new Response('fixed fixture request rejected',{status:400});
    const storage=this.fixtureState.storage;
    const raw=storage.sql.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").one().payload;
    const saved=JSON.parse(raw),session=saved.state.test_sessions[input.session_id];
    if(!session)return new Response('fixed session missing',{status:404});
    if(url.pathname==='/fixture/account-alarm-early'){
      // Only the real scheduling timestamp changes. All signed and persisted
      // deadlines, leases and budgets remain their actual original values.
      await storage.setAlarm(Date.now()+100);
      return Response.json({armed:true});
    }
    const scheduled=await storage.getAlarm();
    const audits=storage.sql.exec("SELECT COUNT(*) AS count FROM custody_audit WHERE record_id='custody-provision-approval-v1'").one().count;
    const remaining=b=>Number(b.limit)-Number(b.spent)-Object.values(b.reservations).reduce((sum,v)=>sum+Number(v),0);
    return Response.json({events:this.alarmEvents,scheduled_at:scheduled,lease_until:session.lease_until,
      execution_deadline:session.roots.execution.deadline,closing_reason:session.closing?.[0]??null,
      closing_at:session.closing?.[1]??null,audits,
      termination_originals:Object.keys(session.termination_slots).length,
      termination_results:Object.values(session.termination_slots).filter(v=>v.result!==null).length,
      budgets:{host:remaining(session.control.transport),native:remaining(session.identity_transport),
        purpose:Object.fromEntries(Object.entries(session.roots).map(([key,root])=>[key,remaining(root.budgets.transport)]))}});
  }
}
