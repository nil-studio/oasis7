// Test-only provider ingress. Synthetic Host metadata does not prove OS isolation.
import deployed from '../../artifacts/identity/entry.mjs';
export * from '../../artifacts/identity/entry.mjs';
export default {async fetch(request,env,ctx){
    const url=new URL(request.url);
    if(request.method==='POST' && ['/fixture/test-session-attestation','/fixture/native-history'].includes(url.pathname)) {
        try {
            const raw=await request.text();
            const result=url.pathname==='/fixture/test-session-attestation'
                ?await env.TEST_SESSION_PRIVATE_HISTORY.attest(raw)
                :await env.TEST_SESSION_NATIVE_HISTORY.read(raw);
            return new Response(result,{headers:{'content-type':'application/json'}});
        } catch {return new Response('private metadata rejected',{status:409});}
    }
    if(url.pathname.startsWith('/v1/client-enrollments')) {
        const headers=new Headers(request.headers);
        headers.set('CF-Connecting-IP','192.0.2.29');
        headers.set('Host',url.host);
        request=new Request(request,{headers,cf:{colo:'TEST'}});
    }
    return deployed.fetch(request,env,ctx);
}};
