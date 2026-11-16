import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const policy=readFileSync(resolve(root,'tests/fixtures/policy.json'),'utf8');
const token='56'.repeat(32);
async function mcp(mf,method,args,auth=token){return mf.dispatchFetch('https://worker.example.test/mcp',{method:'POST',headers:{'Content-Type':'application/json','Authorization':'Bearer '+auth},body:JSON.stringify({jsonrpc:'2.0',id:1,method,...(args?{params:args}:{})})});}
test('real Rust Wasm auth, metadata and pre-network durable budget survive restart',{timeout:60000},async()=>{
 const persist=mkdtempSync(resolve(tmpdir(),'oasis7-worker-platform-'));let nativeCalls=0;
 const make=()=>new Miniflare({...convertV4MiniflareOptions({name:'dev-test-platform',modules:[{type:'ESModule',path:resolve(root,'build/entry.mjs')},{type:'ESModule',path:resolve(root,'build/index.js')},{type:'CompiledWasm',path:resolve(root,'build/index_bg.wasm')}],modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,durableObjects:{DEV_TEST_ACCOUNTS:{className:'DevTestAccounts',useSQLite:true}},bindings:{DEV_TEST_POLICY:policy,DEV_TEST_TOKEN:token,DEV_TEST_STATE_KEK:'18'.repeat(32),DEV_TEST_MATERIAL:JSON.stringify({executor_seed:'17'.repeat(32),identity_cookie:'__Host-oasis7_sid=private'})},outboundService:()=>{nativeCalls++;return new Response('unexpected native call',{status:503});}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=make();
 try{
  assert.equal((await mcp(mf,'ping',{},'invalid')).status,401);
  for(let n=0;n<3;n++)assert.equal((await mcp(mf,'ping')).status,200);
  const account=await(await mcp(mf,'tools/call',{name:'dev_test_accounts_call',arguments:{method:'accounts'}})).json();
  assert.equal(account.result.structuredContent.status,'ok');
  for(let n=0;n<2;n++){
   const denied=await(await mcp(mf,'tools/call',{name:'dev_test_accounts_call',arguments:{method:'open',grant_ref:'grant',expires_at_unix:4102444700}})).json();
   assert.equal(denied.result.structuredContent.code,'call_limit_exhausted');
  }
  assert.equal(nativeCalls,0,'exhausted open must not contact native auth/view');
  await mf.dispose();mf=make();
  const denied=await(await mcp(mf,'tools/call',{name:'dev_test_accounts_call',arguments:{method:'accounts'}})).json();
  assert.equal(denied.result.structuredContent.code,'call_limit_exhausted');
  assert.equal((await mcp(mf,'tools/list')).status,200,'metadata has no business charge');
  assert.equal(nativeCalls,0);
 }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
for(const advance of [0,4000])test(advance ? 'real Rust auth-hop deadline retains charged admission and never sends control POST' : 'real Rust auth-hop clock negative control reaches control POST without expiry',{timeout:60000},async()=>{
 const persist=mkdtempSync(resolve(tmpdir(),'oasis7-worker-deadline-'));let posts=0,gets=0,capabilityGets=0;const hops=[];
 const config=JSON.parse(policy);config.timeout_ms=10000;config.max_calls=1;
 // Only this Miniflare instance loads the clock shim. Rust/Wasm, durable SQL,
 // encryption and nativeHttp remain real; advance 4s at each authentication response. Each hop is within the 10s
 // budget but the third crosses the original deadline; no cold-runner race.
 const entry=readFileSync(resolve(root,'build/entry.mjs'),'utf8');
 const clockEntry=`
 const realNow=Date.now.bind(Date);let clock;
 Date.now=()=>clock ??= realNow();
 const realFetch=globalThis.fetch.bind(globalThis);
 globalThis.fetch=async (...args)=>{
  const response=await realFetch(...args);
  const input=args[0];const url=typeof input==='string'?input:input.url;
  if(['/api/public/hosted-account/v2/session','/api/public/hosted-account/v2/capabilities'].includes(new URL(url).pathname))clock=Date.now()+${advance};
  return response;
 };
 ${entry}`;
 const mf=new Miniflare({...convertV4MiniflareOptions({name:'dev-test-deadline',modules:[{type:'ESModule',path:resolve(root,'build/entry.mjs'),contents:clockEntry},{type:'ESModule',path:resolve(root,'build/index.js')},{type:'CompiledWasm',path:resolve(root,'build/index_bg.wasm')}],modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,durableObjects:{DEV_TEST_ACCOUNTS:{className:'DevTestAccounts',useSQLite:true}},bindings:{DEV_TEST_POLICY:JSON.stringify(config),DEV_TEST_TOKEN:token,DEV_TEST_STATE_KEK:'18'.repeat(32),DEV_TEST_MATERIAL:JSON.stringify({executor_seed:'17'.repeat(32),identity_cookie:'__Host-oasis7_sid=private'})},outboundService:async request=>{
  const path=new URL(request.url).pathname;
  if(request.method==='POST'){posts++;return Response.json({unexpected:true});}
  gets++;hops.push(path);
  if(path==='/api/public/hosted-account/v2/session')return Response.json({account:{account_id:'identity'}});
  assert.equal(path,'/api/public/hosted-account/v2/capabilities');capabilityGets++;
  return Response.json({csrf_token:'19'.repeat(32)});
 }}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 try{
  const open=await(await mcp(mf,'tools/call',{name:'dev_test_accounts_call',arguments:{method:'open',grant_ref:'grant',expires_at_unix:4102444700}})).json();
  const diagnostic=JSON.stringify({gets,capabilityGets,posts,hops,result:open.result.structuredContent});
  assert.equal(open.result.structuredContent.status,'unavailable',diagnostic);
  assert.deepEqual(hops,[
   '/api/public/hosted-account/v2/session',
   '/api/public/hosted-account/v2/session',
   '/api/public/hosted-account/v2/capabilities',
  ],diagnostic);
  assert.equal(capabilityGets,1,diagnostic);
  if(advance)assert.ok(['native_outcome_unknown','native_deadline_unknown'].includes(open.result.structuredContent.code),diagnostic);
  // The no-expiry control must reach the mocked POST (its unsigned reply is
  // deliberately unavailable), proving the deadline case is not false-green.
  assert.equal(posts,advance?0:1,diagnostic);
  const again=await(await mcp(mf,'tools/call',{name:'dev_test_accounts_call',arguments:{method:'open',grant_ref:'grant',expires_at_unix:4102444700}})).json();
  assert.equal(again.result.structuredContent.code,'call_limit_exhausted');
  assert.equal(posts,advance?0:1,'exhausted retry must not send a new POST');
 }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
