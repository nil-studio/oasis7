import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync,mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const token='56'.repeat(32);
test('real Rust session survives actor replacement; current grant and durable close still deny',{timeout:60000},async()=>{
 const persist=mkdtempSync(resolve(tmpdir(),'oasis7-worker-session-'));let nativeCalls=0;
 const config=JSON.parse(readFileSync(resolve(root,'tests/fixtures/policy.json'),'utf8'));config.max_calls=8;
 const make=()=>new Miniflare({...convertV4MiniflareOptions({name:'dev-test-session',modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/session-harness.mjs')},{type:'ESModule',path:resolve(root,'build/index.js')},{type:'CompiledWasm',path:resolve(root,'build/index_bg.wasm')},{type:'ESModule',path:resolve(root,'platform/store.mjs')}],modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,durableObjects:{DEV_TEST_ACCOUNTS:{className:'DevTestAccounts',useSQLite:true}},bindings:{DEV_TEST_POLICY:JSON.stringify(config),DEV_TEST_TOKEN:token,DEV_TEST_STATE_KEK:'18'.repeat(32),DEV_TEST_MATERIAL:JSON.stringify({executor_seed:'17'.repeat(32),identity_cookie:'__Host-oasis7_sid=private'})},outboundService:()=>{nativeCalls++;return new Response('fixture native unavailable',{status:503});}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=make();
 const invoke=async args=>{const r=await mf.dispatchFetch('https://worker.example.test/mcp',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+token},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/call',params:{name:'dev_test_accounts_call',arguments:args}})});return (await r.json()).result.structuredContent;};
 try{
  await mf.dispatchFetch('https://worker.example.test/mcp',{method:'POST',headers:{'Content-Type':'application/json',Authorization:'Bearer '+token},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'ping'})});
  const seeded=await(await mf.dispatchFetch('https://worker.example.test/fixture/seed-session')).json();
  await mf.dispose();mf=make();
  const read=await invoke({method:'describe',session_ref:seeded.reference});
  assert.equal(read.code,'native_outcome_unknown','stored session must pass current auth and reach native transport after restart');assert.ok(nativeCalls>0);
  const count=nativeCalls;
  await mf.dispose();config.generation++;config.grants[0].binding.authorization_version++;mf=make();
  assert.equal((await invoke({method:'describe',session_ref:seeded.reference})).code,'session_scope_denied');assert.equal(nativeCalls,count);
  assert.equal((await invoke({method:'close',session_ref:seeded.reference})).status,'ok');
  await mf.dispose();mf=make();
  assert.equal((await invoke({method:'describe',session_ref:seeded.reference})).code,'session_unavailable');
  assert.equal((await invoke({method:'describe',session_ref:'cd'.repeat(32)+'-1'})).code,'session_unavailable');assert.equal(nativeCalls,count);
 }finally{await mf.dispose();rmSync(persist,{recursive:true,force:true});}
});
