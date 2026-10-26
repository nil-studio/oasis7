// Test-only faults around the actual Rust actor and actual SQLite transaction.
import deployed,{RecoveryFence as RealFence} from '../../artifacts/recovery-fence/entry.mjs';
import {WorkerEntrypoint} from 'cloudflare:workers';
export * from '../../artifacts/recovery-fence/entry.mjs';
export default deployed;
let verifyCalls=0,delayMs=0;
const verify=crypto.subtle.verify.bind(crypto.subtle);
crypto.subtle.verify=async(...args)=>{
  verifyCalls++;
  const result=await verify(...args);
  if(delayMs)await new Promise(resolve=>setTimeout(resolve,delayMs));
  return result;
};
export class FixtureStats extends WorkerEntrypoint {
  stats(){return {verification_calls:verifyCalls};}
}
export class RecoveryFence extends RealFence {
  constructor(state,env) {
    delayMs=Number(env.FIXTURE_VERIFY_DELAY??0);
    let fail=env.FIXTURE_FAIL_COMMIT==='true';
    const storage=state.storage;
    const sql=new Proxy(storage.sql,{get(target,key){
      if(key==='exec')return(...args)=>{
        const result=target.exec(...args);
        if(fail&&args[0].startsWith('UPDATE custody_records SET payload=')) {
          fail=false;throw new Error('fixture failed after actual audit and ledger writes');
        }
        return result;
      };
      const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
    }});
    const wrappedStorage=new Proxy(storage,{get(target,key){
      if(key==='sql')return sql;
      const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
    }});
    const wrappedState=new Proxy(state,{get(target,key){
      if(key==='storage')return wrappedStorage;
      const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
    }});
    super(wrappedState,env);
  }
}
