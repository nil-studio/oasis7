// Explicit trusted-provider component fixture; never a deployment entrypoint.
import RustWorker from '../../artifacts/identity/index.js';
import { ClientEnrollmentAuthority as RustAbuse } from '../../artifacts/identity/entry.mjs';
let abuseCalls = 0;
export class ClientEnrollmentAuthority extends RustAbuse {
  async fetch(request) { abuseCalls++; return super.fetch(request); }
}
export default { async fetch(request, env, ctx) {
  const url = new URL(request.url);
  if (url.pathname === '/fixture/counts') return Response.json({abuse: abuseCalls});
  if (url.pathname === '/fixture/reset') { abuseCalls = 0; return new Response(null, {status: 204}); }
  const headers = new Headers(request.headers);
  headers.set('CF-Connecting-IP', '192.0.2.87');
  const trusted = new Request(request, {headers, cf: {colo:'TEST'}});
  return new RustWorker(ctx, env).fetch(trusted);
}};
