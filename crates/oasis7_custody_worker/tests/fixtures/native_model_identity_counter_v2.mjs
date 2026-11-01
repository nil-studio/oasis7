// Component-only trusted provider context; no production ingress imports it.
import deployed from '../../artifacts/identity/entry.mjs';
export * from '../../artifacts/identity/entry.mjs';
let publicIngress=0;
export default {async fetch(request,env,ctx) {
 const u=new URL(request.url);
 if(u.pathname==='/fixture/n-model-counts'&&request.method==='GET')return Response.json({publicIngress});
 if(u.pathname!=='/fixture/n-model-refusal'||request.method!=='POST')return new Response('fixed fixture route',{status:404});
 publicIngress++;
 const target=new URL(env.FIXTURE_N_MODEL_TARGET);const headers=new Headers(request.headers);
 headers.set('CF-Connecting-IP','192.0.2.141');headers.set('Host',target.host);
 return deployed.fetch(new Request(target,{method:'POST',headers,body:request.body,duplex:'half',cf:{colo:'TEST'}}),env,ctx);
}};
