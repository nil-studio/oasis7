// Explicit provider-origin simulation only. No caller IP/Forwarded is trusted.
import deployed from '../../artifacts/identity/entry.mjs';
export * from '../../artifacts/identity/entry.mjs';
export default {async fetch(request,env,ctx){
  const url=new URL(request.url);
  const peers={'/fixture/provider-a/create':'192.0.2.41','/fixture/provider-b/create':'192.0.2.42'};
  if(request.method!=='POST'||!Object.hasOwn(peers,url.pathname))return new Response('fixed fixture route',{status:404});
  const headers=new Headers(request.headers);
  headers.set('CF-Connecting-IP',peers[url.pathname]);headers.set('Host',url.host);
  const actual=new Request(new URL('/v1/client-enrollments',url.origin),{method:'POST',headers,body:request.body,cf:{colo:'TEST'},duplex:'half'});
  return deployed.fetch(actual,env,ctx);
}};
