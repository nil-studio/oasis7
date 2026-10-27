// Test-only provider ingress. Synthetic Host metadata does not prove OS isolation.
import deployed from '../../artifacts/identity/entry.mjs';
export * from '../../artifacts/identity/entry.mjs';
export default {async fetch(request,env,ctx){
    const url=new URL(request.url);
    if(url.pathname.startsWith('/v1/client-enrollments')) {
        const headers=new Headers(request.headers);
        headers.set('CF-Connecting-IP','192.0.2.29');
        headers.set('Host',url.host);
        request=new Request(request,{headers,cf:{colo:'TEST'}});
    }
    return deployed.fetch(request,env,ctx);
}};
