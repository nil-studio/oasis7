// Explicit trusted-provider simulation for local workerd; never deployed.
import RustWorker from '../../artifacts/identity/index.js';
export {ClientEnrollmentAuthority} from '../../artifacts/identity/entry.mjs';
export default {async fetch(request,env,ctx){
  const url=new URL(request.url);
  if(url.pathname.startsWith('/fixture/read/')) {
    const raw=await new RustWorker(ctx,env).identity_read_client_enrollment(env,url.pathname.slice('/fixture/read/'.length));
    return new Response(raw,{headers:{'content-type':'application/json'}});
  }
  const headers=new Headers(request.headers);
  headers.set('CF-Connecting-IP','192.0.2.8');
  const providerRequest=new Request(request,{headers,cf:{colo:'TEST'}});
  return new RustWorker(ctx,env).fetch(providerRequest);
}};
