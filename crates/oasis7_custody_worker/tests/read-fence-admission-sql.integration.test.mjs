// Actual Miniflare SQLite, storage-only test-domain signatures. No CF/Core
// Current or paid graph claim; independent Core tests verify actual Read codecs.
import {generateKeyPairSync,sign} from 'node:crypto';
import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const key=generateKeyPairSync('ed25519'),publicKey=key.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('hex');
test('Read FENCES SQL once/cold/dual-profile/partial and ordinary projection retain bytes',async()=>{
 const persist=await mkdtemp(resolve(tmpdir(),'oasis7-read-fences-sql-'));
 const options=(cap='2',enabled='true')=>({...convertV4MiniflareOptions({name:'read-fences-sql',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,modules:[{type:'ESModule',path:'tests/fixtures/read_fence_admission_sql_harness_v3.mjs'},{type:'ESModule',path:'platform/read_fence_admission_sql_v3.mjs'}],durableObjects:{HARNESS:{className:'ReadFenceSqlHarnessV3',useSQLite:true}},bindings:{FIXTURE_READ_CAPACITY:cap,FIXTURE_EXEC_CAPACITY:'2',FIXTURE_READ_ENABLED:enabled,FIXTURE_STORAGE_PUBLIC_KEY:publicKey}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=new Miniflare(options());const call=(name,op,proof)=>mf.dispatchFetch(`https://component.invalid/${name}`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op,...(proof?{proof}:{})})});
 const signed=async(name,attempt,profile='reconcile_read_v3')=>{const {object_id}=await(await call(name,'identity')).json();const claim={object_id,attempt,profile,expires_at:Date.now()+5000};return{claim,signature_hex:sign(null,Buffer.from(JSON.stringify(['oasis7.test.read-fence-sql-storage.v3',claim])),key.privateKey).toString('hex')};};
 const summary=async name=>(await call(name,'summary')).json();
 try{
  for(const name of ['normal','floor','header','birth','business','unknown','schema','denied','late','both','drift','corrupt','wrong-object'])assert.equal((await call(name,'prepare')).status,200);
  assert.equal((await call('normal','read')).status,200,'legacy ordinary read before new profiles');
  const wrong=await summary('wrong-object');assert.equal((await call('wrong-object','claim',await signed('normal','cross-object'))).status,409);assert.deepEqual(await summary('wrong-object'),wrong);
  for(const name of ['normal','floor','header','birth','business','unknown','schema','both','drift','corrupt'])assert.equal((await call(name,'claim',await signed(name,'first'))).status,200);
  assert.equal((await call('both','seed-execution',await signed('both','execution-first','execution_v2'))).status,200);
  assert.equal((await call('both','read')).status,200,'both known births plus two business records');assert.equal((await call('both','no-op-transition')).status,200);
  const beforeDenied=await summary('denied');assert.equal((await call('denied','reject-before-write',await signed('denied','first'))).status,409);assert.deepEqual(await summary('denied'),beforeDenied);
  assert.equal((await call('late','late-denial',await signed('late','first'))).status,409);assert.equal((await summary('late')).profiles[1].once.length,1,'post-commit denial preserves permanent once');
  await mf.dispose();mf=new Miniflare(options());
  assert.equal((await call('normal','read')).status,200,'cold ordinary validates installed history');
  assert.equal((await call('normal','claim',await signed('normal','first'))).status,409,'same physical attempt cannot reissue');
  assert.equal((await call('normal','claim',await signed('normal','second'))).status,200);assert.equal((await call('normal','claim',await signed('normal','third'))).status,409);
  for(const [name,op] of [['floor','drop-floor'],['header','drop-header'],['birth','drop-birth'],['business','drop-fence'],['unknown','unknown'],['schema','unknown-table'],['corrupt','corrupt-birth']]){
   assert.equal((await call(name,op)).status,200);const before=await summary(name);assert.equal((await call(name,'read')).status,409);assert.equal((await call(name,'no-op-transition')).status,409);assert.equal((await call(name,'claim',await signed(name,'next'))).status,409);assert.deepEqual(await summary(name),before,'no partial initializer or once repair');
  }
  assert.equal((await call('both','unknown')).status,200);const both=await summary('both');assert.equal(both.records.length,5);assert.equal((await call('both','read')).status,409,'unknown fifth probe remains visible');assert.deepEqual(await summary('both'),both);
  await mf.dispose();mf=new Miniflare(options('2','false'));assert.equal((await call('drift','read')).status,200,'disabled still verifies retained history');const disabled=await summary('drift');assert.equal((await call('drift','claim',await signed('drift','disabled-next'))).status,409);assert.deepEqual(await summary('drift'),disabled);
  await mf.dispose();mf=new Miniflare(options('3','false'));assert.equal((await call('drift','read')).status,409,'ordinary must not ignore installed capacity drift');
 }finally{await mf.dispose();await rm(persist,{recursive:true,force:true});}
});
