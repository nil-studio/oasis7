// Synthetic trusted Native metadata; tests only the real Owner RPC/DO projection.
import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from '../../artifacts/owner/index.js';
import { ManagerAssociations as RustDirectory, AccountAuthority } from '../../artifacts/owner/entry.mjs';
export { AccountAuthority };
let rpcCalls = 0, directoryCalls = 0;
export class ManagerAssociations extends RustDirectory {
  async fetch(request) { directoryCalls++; return super.fetch(request); }
}
export class OwnerManagement extends WorkerEntrypoint {
  async queryParents(raw) { rpcCalls++; return new RustWorker(this.ctx,this.env).owner_query_parents(this.env,raw); }
  async reserveDraft(raw) { return new RustWorker(this.ctx,this.env).owner_reserve_draft(this.env,raw); }
}
export default { async fetch(request,env) {
  const path=new URL(request.url).pathname;
  if(path==='/fixture/counts') return Response.json({rpc:rpcCalls,directory:directoryCalls});
  if(path==='/fixture/reset') {rpcCalls=0;directoryCalls=0;return new Response(null,{status:204});}
  if(path==='/fixture/reserve') return new Response(await env.SELF_MANAGEMENT.reserveDraft(await request.text()),{headers:{'Content-Type':'application/json'}});
  return new Response(null,{status:404});
}};
