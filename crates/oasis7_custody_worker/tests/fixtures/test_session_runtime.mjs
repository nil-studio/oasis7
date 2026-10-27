// Test-only provider ingress. Synthetic Host metadata does not prove OS isolation.
import deployed from '../../artifacts/identity/entry.mjs';
export * from '../../artifacts/identity/entry.mjs';
export default {async fetch(request,env,ctx){
    const url=new URL(request.url);
    const hostFaultPrefix='/fixture/client-wrong-host';
    if(url.pathname.startsWith(hostFaultPrefix)) {
        const actualPath=url.pathname.slice(hostFaultPrefix.length);
        if(request.method!=='POST'||url.search||!/^\/v1\/account-provisions\/[^/]+\/accounts\/[^/]+\/client\/(?:proof-challenges|client-enrollments\/[^/]+\/claim|test-sessions\/[^/]+\/(?:initialize|tools-list|heartbeat|finish|abort))$/.test(actualPath))return new Response('invalid fixed fault',{status:400});
        const headers=new Headers(request.headers);
        headers.set('CF-Connecting-IP','192.0.2.29');headers.set('Host','wrong.example');
        // Inject after Miniflare ingress normalization. The signed actual target,
        // body and proof are unchanged; real Rust must reject this literal Host.
        const actual=new Request(new URL(actualPath,url.origin),{method:'POST',headers,body:request.body,cf:{colo:'TEST'},duplex:'half'});
        return deployed.fetch(actual,env,ctx);
    }
    if(request.method==='POST' && ['/fixture/test-session-attestation','/fixture/native-history'].includes(url.pathname)) {
        try {
            const raw=await request.text();
            const result=url.pathname==='/fixture/test-session-attestation'
                ?await env.TEST_SESSION_PRIVATE_HISTORY.attest(raw)
                :await env.TEST_SESSION_NATIVE_HISTORY.read(raw);
            return new Response(result,{headers:{'content-type':'application/json'}});
        } catch {return new Response('private metadata rejected',{status:409});}
    }
    if(/^\/v1\/account-provisions\/[^/]+\/accounts\/[^/]+\/client\/(?:proof-challenges|client-enrollments\/[^/]+\/claim|test-sessions\/[^/]+\/(?:initialize|tools-list|heartbeat|finish|abort))$/.test(url.pathname)) {
        const headers=new Headers(request.headers);
        headers.set('CF-Connecting-IP','192.0.2.29');
        // A literal wrong Host negative must reach the real validator unchanged.
        // Loopback dispatch transport otherwise receives the same trusted Host fix
        // as the established public Enrollment fixture.
        if(headers.get('Host')!=='wrong.example')headers.set('Host',url.host);
        request=new Request(request,{headers,cf:{colo:'TEST'}});
    }
    if(url.pathname.startsWith('/v1/client-enrollments')) {
        const headers=new Headers(request.headers);
        headers.set('CF-Connecting-IP','192.0.2.29');
        headers.set('Host',url.host);
        request=new Request(request,{headers,cf:{colo:'TEST'}});
    }
    return deployed.fetch(request,env,ctx);
}};
