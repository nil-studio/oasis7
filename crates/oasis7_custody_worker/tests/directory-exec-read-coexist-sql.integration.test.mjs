// Actual workerd SQLite storage component; no Core purpose/CF/Read60 evidence.
import {generateKeyPairSync,sign,randomBytes} from 'node:crypto';
import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const keys=generateKeyPairSync('ed25519'),publicKey=keys.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('hex');
const proof=(profile,attempt=randomBytes(32).toString('hex'))=>{const claim={profile,attempt,until:String(Date.now()+60000)};return{claim,signature_hex:sign(null,Buffer.from(JSON.stringify(['oasis7.test.read-directory-storage-v3',claim])),keys.privateKey).toString('hex')};};
test('production Exec paid Directory accepts genuine storage Read history and rejects partial history',async()=>{
 const persist=await mkdtemp(resolve(tmpdir(),'oasis7-exec-read-coexist-'));
 const options=()=>({...convertV4MiniflareOptions({name:'exec-read-coexist',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,modules:[{type:'ESModule',path:'tests/fixtures/directory_exec_read_coexist_sql_harness.mjs'},{type:'ESModule',path:'platform/read_directory_admission_sql_v3.mjs'},{type:'ESModule',path:'platform/execution_pending_actor_sql_v2.mjs'}],durableObjects:{HARNESS:{className:'ReadDirectorySqlV3Harness',useSQLite:true}},bindings:{CAPACITY:'3',PUBLIC_KEY:publicKey}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=new Miniflare(options());const call=(name,body)=>mf.dispatchFetch(`https://component.invalid/${name}`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
 const inv=async name=> (await call(name,{op:'inventory'})).json();
 try{
  // Authentic signed test-domain histories, actual production JS and SQLite.
  // This is not Rust authorization, Core paid proof or first14 qualification.
  assert.equal((await call('exec-only',{op:'prepare'})).status,200);
  assert.equal((await call('exec-only',{op:'exec-paid',profile:'execution',proof:proof('execution')})).status,200,'production Exec single-family positive');
  assert.equal((await call('exec-only',{op:'ordinary'})).status,200);
  assert.equal((await call('exec-only',{op:'claim',profile:'read',proof:proof('read')})).status,200,'Exec then Read preserves both histories');
  for(const name of ['read-first','partial','unknown','disabled']){
   assert.equal((await call(name,{op:'prepare'})).status,200);
   assert.equal((await call(name,{op:'claim',profile:'read',proof:proof('read')})).status,200,'actual Read signed birth/header/once positive');
   assert.equal((await call(name,{op:'ordinary'})).status,200);
  }
  await mf.dispose();mf=new Miniflare(options());
  assert.equal((await call('read-first',{op:'ordinary'})).status,200,'cold Read positive');
  for(const [name,kind]of [['partial','header'],['unknown','unknown']]){
   assert.equal((await call(name,{op:'fault',profile:'read',kind})).status,200);const before=await inv(name);
   assert.equal((await call(name,{op:'exec-paid',profile:'execution',proof:proof('execution')})).status,409);assert.deepEqual(await inv(name),before,'failed admission cannot repair damaged Read');
  }
  const disabled=await inv('disabled');assert.equal((await call('disabled',{op:'exec-paid',profile:'execution',proof:proof('execution'),disabled:true})).status,409);assert.deepEqual(await inv('disabled'),disabled);assert.equal((await call('disabled',{op:'ordinary'})).status,200,'disabled admission does not erase historical validation');
  assert.equal((await call('read-first',{op:'exec-paid',profile:'execution',proof:proof('execution')})).status,200,'valid Read history cannot categorically block production Exec paid admission');
  assert.equal((await call('read-first',{op:'ordinary'})).status,200);
 }finally{await mf.dispose();await rm(persist,{recursive:true,force:true});}
});
