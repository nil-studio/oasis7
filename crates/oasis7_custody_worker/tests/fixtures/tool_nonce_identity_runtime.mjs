// Test-only physical-port observation; all requests reach the real implementation.
import deployed, {ClientEnrollmentAuthority as RealClientEnrollmentAuthority} from './test_session_runtime.mjs';
export * from './test_session_runtime.mjs';
export default {async fetch(request,env,ctx){
  if(request.method==='POST'&&/^\/v1\/account-provisions\/[^/]+\/accounts\/[^/]+\/client\/proof-challenges$/.test(new URL(request.url).pathname)){
    let body;try{body=await request.clone().json();}catch{}
    if(body?.kind?.kind==='tool')console.warn('fixture.tool_nonce.public');
  }
  return deployed.fetch(request,env,ctx);
}};
export class ClientEnrollmentAuthority extends RealClientEnrollmentAuthority {
  async fetch(request){
    if(request.method==='POST'&&new URL(request.url).pathname==='/internal/abuse'){
      let body;try{body=await request.clone().json();}catch{}
      if(body?.kind==='client_http'&&['provider','realm'].includes(body.scope))console.warn(`fixture.tool_nonce.${body.scope}`);
    }
    return super.fetch(request);
  }
}
