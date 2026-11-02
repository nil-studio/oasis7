// Private SQL component only: Rust signature/Current/Account authority is not mocked as qualified.
import {claimExecutionPendingActorV2} from '../../platform/execution_pending_actor_sql_v2.mjs';
const floor='custody_pending_current_floor_v2',once='custody_pending_current_once_v2';
export class DirectoryPartialSqlHarness {
 constructor(state){this.state=state;}
 async fetch(request){const q=await request.json(),sql=this.state.storage.sql;
  if(q.op==='drop-floor'){sql.exec(`DROP TABLE ${floor}`);return Response.json({fault_installed:true});}
  if(q.op==='inventory'){const exists=sql.exec("SELECT name FROM sqlite_master WHERE type='table' AND name=?",floor).toArray().length===1;return Response.json({floor_exists:exists,claims:sql.exec(`SELECT COUNT(*) AS n FROM ${once}`).one().n});}
  const material={namespace:'sql-component',service_id:'account-service',environment:'test',physical_attempt_id:q.attempt,proof_digest:'ab'.repeat(32),issuer_epoch:'1',config_generation:'1'};
  try{claimExecutionPendingActorV2(this.state,JSON.stringify(material));return Response.json({claimed:true});}catch{return Response.json({rejected:true},{status:409});}
 }
}
export default {fetch(request,env){return env.HARNESS.get(env.HARNESS.idFromName(new URL(request.url).pathname)).fetch(request);}};
