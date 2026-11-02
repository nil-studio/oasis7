// Actual workerd SQLite recovery invariant. No portable proof/Current/139 qualification.
import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
test('Directory SQL installed partial floor loss cannot silently repair on a later physical attempt',async()=>{
 const persist=await mkdtemp(resolve(tmpdir(),'oasis7-directory-partial-'));
 const options=()=>({...convertV4MiniflareOptions({name:'directory-partial',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
  modules:[{type:'ESModule',path:'tests/fixtures/execution_directory_partial_sql_harness.mjs'},{type:'ESModule',path:'platform/execution_pending_actor_sql_v2.mjs'}],durableObjects:{HARNESS:{className:'DirectoryPartialSqlHarness',useSQLite:true}}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=new Miniflare(options());const call=(name,op,attempt)=>mf.dispatchFetch(`https://component.invalid/${name}`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op,attempt})});
 try{
  assert.equal((await call('normal','claim','normal-first')).status,200);
  assert.equal((await call('normal','claim','normal-next')).status,200,'legitimate later distinct attempt positive');
  assert.equal((await call('partial','claim','partial-first')).status,200);
  await mf.dispose();mf=new Miniflare(options());
  assert.deepEqual(await (await call('partial','inventory')).json(),{floor_exists:true,claims:1},'cold positive before fault');
  assert.equal((await call('partial','drop-floor')).status,200);
  const partial=await (await call('partial','inventory')).json();assert.deepEqual(partial,{floor_exists:false,claims:1});
  const result=await call('partial','claim','partial-next');
  assert.equal(result.status,409,'installed profile with missing floor must reject, never CREATE IF NOT EXISTS repair');
  assert.deepEqual(await (await call('partial','inventory')).json(),partial,'rejection keeps partial inventory unchanged');
 }finally{await mf.dispose();await rm(persist,{recursive:true,force:true});}
});
