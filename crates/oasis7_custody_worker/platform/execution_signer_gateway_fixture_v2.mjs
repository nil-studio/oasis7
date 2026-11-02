// Test-only driver: historical Core proof, real service RPC and real SQLite Actor.
import { WorkerEntrypoint } from 'cloudflare:workers';
import RustWorker from './index.js';
export { ExecutionPrepareSignerHarness } from './index.js';
let physicalServiceCalls = 0;
export class RoleKeyProvision extends WorkerEntrypoint {
  async executionPrepareV2(raw) {
    physicalServiceCalls++;
    return new RustWorker(this.ctx,this.env).execution_prepare_role_historical_fixture_v2(this.env,raw);
  }
}
export default {async fetch(request,env) {
  const url=new URL(request.url);
  if(url.pathname==='/fixture-service-calls'&&request.method==='GET') return Response.json(physicalServiceCalls);
  if(url.pathname!=='/fixture-fixed-rpc'||request.method!=='POST') return new Response('not found',{status:404});
  const raw=await request.text();
  if(new TextEncoder().encode(raw).byteLength>32768) return new Response('rejected',{status:409});
  try{return new Response(await env.EXECUTOR_PROVISION_KEYS.executionPrepareV2(raw),{headers:{'content-type':'application/json'}});}
  catch{return new Response('fixed historical RPC rejected',{status:409});}
}};
