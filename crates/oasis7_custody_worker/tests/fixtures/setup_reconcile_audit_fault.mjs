// Test-only failure after real new and original setup state and audit writes, inside SQLite.
import deployed, {AccountAuthority as RealAccount, OwnerNativePermits as RealOwnerNativePermits} from '../../artifacts/owner/entry.mjs';
export * from '../../artifacts/owner/entry.mjs';
const facts={faults:0,attempted_result:false};
export default {fetch(request,env,ctx) {
  if(new URL(request.url).pathname==='/_fixture/reconcile-audit-fault') return Response.json(facts);
  return deployed.fetch(request,env,ctx);
}};
export class AccountAuthority extends RealAccount {
  constructor(state,env) {
    const storage=state.storage;
    let finalAudit=false;
    const sql=new Proxy(storage.sql,{get(target,key) {
      if(key==='exec') return (...args)=>{
        const result=target.exec(...args);
        if(args[0].startsWith('INSERT INTO custody_audit') && String(args[3]).includes('"complete_owner_setup_reconcile"')) finalAudit=true;
        if(finalAudit && args[0].startsWith('UPDATE custody_records SET payload')) {
          const row=target.exec("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").toArray()[0];
          const saved=JSON.parse(row.payload).state;
          const setup=Object.values(saved.owner_setups)[0];
          const reconcile=Object.values(saved.owner_setup_reconciles)[0];
          facts.attempted_result=!!setup.stages.create_task.result && !!Object.values(reconcile.attempts)[0].result;
          facts.faults++;
          throw new Error('fixture rejects final reconcile audit commit');
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

export class OwnerNativePermits extends RealOwnerNativePermits {
  async ownerSetup(raw) {
    const rpc=JSON.parse(raw);
    if(this.env.FIXTURE_SETUP_ARCHIVE_EXPIRY==='final_observation' && rpc.stage==='final_observation' && rpc.action.kind==='record_result') {
      throw new Error('fixture loses original final observation result before Account commit');
    }
    const result=await super.ownerSetup(raw);
    if(rpc.stage===this.env.FIXTURE_SETUP_ARCHIVE_EXPIRY &&
        rpc.action.kind==='archive_canonical' && rpc.action.fact.source==='submit') {
      const response=await fetch('https://fixture.test/_after-setup-submit-archive',{method:'POST'});
      if(response.status!==204) throw new Error('fixture archive expiry failed');
    }
    return result;
  }
  async call(raw) {
    const rpc=JSON.parse(raw);
    if(rpc.action.kind==='record_result' && (
      this.env.FIXTURE_DROP_NATIVE_RESULT==='true' && rpc.action.result.kind==='session_status'
      || this.env.FIXTURE_DROP_MISSING_RESULT==='true' && rpc.action.result.kind==='explicit_login_required')) {
      throw new Error('fixture lost Account submission before commit');
    }
    const result=await super.call(raw);
    if(this.env.FIXTURE_STAGE_GAP==='true' && rpc.action.kind==='record_result' && rpc.action.result.kind==='binding_receipt') {
      await fetch('https://fixture.test/_after-binding-commit',{method:'POST'});
    }
    return result;
  }
}
