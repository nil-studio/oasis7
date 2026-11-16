// Local workerd fixture only. Production entry never imports this module.
import RustWorker,{DevTestAccounts as RustAccounts} from '../../build/index.js';
import {commitRecord} from '../../platform/store.mjs';
export class DevTestAccounts extends RustAccounts {
 constructor(state,env){super(state,env);this.fixtureState=state;this.fixtureEnv=env;}
 async fetch(request){
  if(new URL(request.url).pathname==='/fixture/seed-session'){
   const grant=JSON.parse(this.fixtureEnv.DEV_TEST_POLICY).grants[0];
   const epoch='ab'.repeat(32),reference=epoch+'-1';
   // BLAKE3(serde_json tuple ["service", reference]); frozen by Rust unit test.
   const key='dev-test-session-v1:066b532ac8b80dcc2a40fd854432a5c056ccbfd4b2cb3471d745b05d7c33494c';
   commitRecord(this.fixtureState,key,'',JSON.stringify({version:1,epoch,reference,service_grant_id:'service',grant,caller:grant.caller_id,issued_at:10,expires:4102444700,closed:false}));
   return Response.json({reference});
  }
  return super.fetch(request);
 }
}
export default {fetch(request,env,ctx){
 if(new URL(request.url).pathname==='/fixture/seed-session')return env.DEV_TEST_ACCOUNTS.get(env.DEV_TEST_ACCOUNTS.idFromName('dev-test-accounts-v1')).fetch(request);
 return new RustWorker(ctx,env).fetch(request);
}};
