// Actual workerd SQLite recovery invariant. No portable proof/Current/139 qualification.
import {generateKeyPairSync,sign} from 'node:crypto';
import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const key=generateKeyPairSync('ed25519'),publicKey=key.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('hex');
const signed=attempt=>{const claim={physical_attempt_id:attempt,scope:'private-sql-component-only'};return{claim,signature_hex:sign(null,Buffer.from(JSON.stringify(['oasis7.test.directory-sql-storage',claim])),key.privateKey).toString('hex')};};
test('Directory SQL installed partial floor loss cannot silently repair on a later physical attempt',async()=>{
 const persist=await mkdtemp(resolve(tmpdir(),'oasis7-directory-partial-'));
 const options=(capacity='2')=>({...convertV4MiniflareOptions({name:'directory-partial',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
  modules:[{type:'ESModule',path:'tests/fixtures/execution_directory_partial_sql_harness.mjs'},{type:'ESModule',path:'platform/execution_pending_actor_sql_v2.mjs'}],durableObjects:{HARNESS:{className:'DirectoryPartialSqlHarness',useSQLite:true}},bindings:{FIXTURE_DIRECTORY_CAPACITY:capacity,FIXTURE_STORAGE_PUBLIC_KEY:publicKey}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=new Miniflare(options());const call=(name,op,attempt)=>mf.dispatchFetch(`https://component.invalid/${name}`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op,...(attempt?{proof:signed(attempt)}:{})})});
 try{
  for(const name of ['normal','partial','both','birth','config','corrupt'])assert.equal((await call(name,'prepare')).status,200);
  assert.equal((await call('normal','claim','normal-first')).status,200);
  assert.equal((await call('partial','claim','partial-first')).status,200);
  assert.equal((await call('both','claim','both-first')).status,200);
  assert.equal((await call('birth','claim','birth-first')).status,200);
  assert.equal((await call('config','claim','config-first')).status,200);
  assert.equal((await call('corrupt','claim','corrupt-first')).status,200);
  await mf.dispose();mf=new Miniflare(options());
  assert.equal((await call('normal','claim','normal-next')).status,200,'legitimate distinct attempt after cold restart');
  assert.deepEqual(await (await call('partial','inventory')).json(),{floor_exists:true,claims:1,birth_exists:true,ordinary_unchanged:true},'cold positive before fault');
  assert.equal((await call('partial','drop-floor')).status,200);
  const partial=await (await call('partial','inventory')).json();assert.deepEqual(partial,{floor_exists:false,claims:1,birth_exists:true,ordinary_unchanged:true});
  const result=await call('partial','claim','partial-next');
  assert.equal(result.status,409,'installed profile with missing floor must reject, never CREATE IF NOT EXISTS repair');
  assert.deepEqual(await (await call('partial','inventory')).json(),partial,'rejection keeps partial inventory unchanged');
  assert.equal((await call('both','drop-both')).status,200);const both=await(await call('both','inventory')).json();assert.deepEqual(both,{floor_exists:false,claims:null,birth_exists:true,ordinary_unchanged:true});
  assert.equal((await call('both','claim','both-next')).status,409,'birth forbids reinstall after both sidecars vanish');assert.deepEqual(await(await call('both','inventory')).json(),both);
  assert.equal((await call('birth','drop-birth')).status,200);const missingBirth=await(await call('birth','inventory')).json();assert.deepEqual(missingBirth,{floor_exists:true,claims:1,birth_exists:false,ordinary_unchanged:true});
  assert.equal((await call('birth','claim','birth-next')).status,409,'sidecars cannot recreate missing birth');assert.deepEqual(await(await call('birth','inventory')).json(),missingBirth);
  assert.equal((await call('normal','claim','normal-over-capacity')).status,409);assert.deepEqual(await(await call('normal','inventory')).json(),{floor_exists:true,claims:2,birth_exists:true,ordinary_unchanged:true});
  assert.equal((await call('corrupt','corrupt-birth')).status,200);assert.equal((await call('corrupt','claim','corrupt-next')).status,409,'cold birth signature remains authenticated');assert.equal((await(await call('corrupt','inventory')).json()).claims,1);
  await mf.dispose();mf=new Miniflare(options('3'));assert.equal((await call('config','claim','config-next')).status,409,'deployment capacity drift is not migration');assert.equal((await(await call('config','inventory')).json()).claims,1);

 }finally{await mf.dispose();await rm(persist,{recursive:true,force:true});}
});
