// Explicit trusted-provider simulation for local workerd; never deployed.
import RustWorker from '../../artifacts/native-harness/index.js';
export {ClientEnrollmentAuthority} from '../../artifacts/native-harness/index.js';
export default {async fetch(request,env,ctx){
  const url=new URL(request.url);
  if(url.pathname.startsWith('/fixture/read/')) {
    const raw=await new RustWorker(ctx,env).identity_read_client_enrollment(env,url.pathname.slice('/fixture/read/'.length));
    return new Response(raw,{headers:{'content-type':'application/json'}});
  }
  const headers=new Headers(request.headers);
  headers.set('CF-Connecting-IP','192.0.2.8');
  // dispatchFetch uses a loopback Host; this provider fixture supplies the public authority.
  headers.set('Host',headers.get('x-fixture-wrong-host')==='true'?'other.example':url.host);
  headers.delete('x-fixture-wrong-host');
  const providerRequest=new Request(request,{headers,cf:{colo:'TEST'}});
  if(url.pathname.endsWith('/prove')) {
    const bytes=new TextEncoder().encode(await providerRequest.clone().text()).byteLength;
    console.warn('enrollment.fixture.headers',JSON.stringify({count:[...headers].length,transfer_encoding:headers.has('transfer-encoding'),content_encoding:headers.has('content-encoding'),trailer:headers.has('trailer'),host_matches:!headers.has('host')||headers.get('host')===url.host,length_matches:!headers.has('content-length')||headers.get('content-length')===String(bytes)}));
  }
  return new RustWorker(ctx,env).fetch(providerRequest);
}};
