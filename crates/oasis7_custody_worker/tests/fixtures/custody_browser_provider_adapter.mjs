// Local-only trusted provider boundary. Browser CF claims are discarded twice.
import RustWorker from '../../artifacts/identity/index.js';
export * from '../../artifacts/identity/entry.mjs';
let requests=0;
export default { async fetch(request,env,ctx) {
  const headers=new Headers(request.headers);
  for(const key of [...headers.keys()])if(key.toLowerCase().startsWith('cf-')||key.toLowerCase().startsWith('x-forwarded-'))headers.delete(key);
  headers.set('CF-Connecting-IP','127.0.0.1');requests++;
  const trusted=new Request(request,{headers,cf:{colo:'LOCAL'}});
  return new RustWorker(ctx,env).fetch(trusted);
}};
