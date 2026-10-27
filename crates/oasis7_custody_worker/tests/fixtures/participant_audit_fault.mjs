// Test-only failure after real participant state and audit writes, inside SQLite.
import deployed, {AccountAuthority as RealAccount} from '../../artifacts/owner/entry.mjs';
export * from '../../artifacts/owner/entry.mjs';
const facts={faults:0,attempted_bound:false};
export default {fetch(request,env,ctx) {
  if(new URL(request.url).pathname==='/_fixture/participant-audit-fault') return Response.json(facts);
  return deployed.fetch(request,env,ctx);
}};
export class AccountAuthority extends RealAccount {
  constructor(state,env) {
    const storage=state.storage;
    let finalAudit=false;
    const sql=new Proxy(storage.sql,{get(target,key) {
      if(key==='exec') return (...args)=>{
        const result=target.exec(...args);
        if(args[0].startsWith('INSERT INTO custody_audit') && String(args[3]).includes('"complete_participant"')) finalAudit=true;
        if(finalAudit && args[0].startsWith('UPDATE custody_records SET payload')) {
          const row=target.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").toArray()[0];
          facts.attempted_bound=JSON.parse(row.payload).state.provision.stage==='participant_bound';
          facts.faults++;
          throw new Error('fixture rejects final participant audit commit');
        }
        return result;
      };
      const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
    }});
    const wrappedStorage=new Proxy(storage,{get(target,key){if(key==='sql')return sql;const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;}});
    const wrappedState=new Proxy(state,{get(target,key){if(key==='storage')return wrappedStorage;const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;}});
    super(wrappedState,env);
  }
}
