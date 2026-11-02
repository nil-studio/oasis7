// Test-only wrappers around the actual named service and actual Account Actor.
import deployed,{OwnerNativePermits as RealPermits,AccountAuthority as RealAccount} from '../../artifacts/owner/entry.mjs';
export * from '../../artifacts/owner/entry.mjs';
let serviceCalls=0;
export class OwnerNativePermits extends RealPermits {
 async nativeModelReservationV2(raw){serviceCalls++;return super.nativeModelReservationV2(raw);}
}
export class AccountAuthority extends RealAccount {
 constructor(ctx,env){super(ctx,env);this.traceState=ctx;}
 async fetch(request){const u=new URL(request.url);
  if(u.pathname==='/fixture/n-model-account-counts'&&request.method==='GET'){
   let authorityRows=0;try{authorityRows=this.traceState.storage.sql.exec("SELECT count(*) AS n FROM custody_records").one().n;}catch{}
   return Response.json({accountCalls:await this.traceState.storage.get('n-model-account-calls')??0,authorityRows});
  }
  if(u.pathname==='/internal/native-model-reservation-v2')await this.traceState.storage.put('n-model-account-calls',(await this.traceState.storage.get('n-model-account-calls')??0)+1);
  return super.fetch(request);
 }
}
export default {fetch(request,env,ctx){if(new URL(request.url).pathname==='/fixture/n-model-service-counts')return Response.json({serviceCalls});return deployed.fetch(request,env,ctx);}};
