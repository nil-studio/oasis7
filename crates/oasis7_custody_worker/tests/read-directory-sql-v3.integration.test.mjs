// Actual workerd SQLite storage component; no Core purpose/CF/Read60 evidence.
import {generateKeyPairSync,sign,randomBytes} from 'node:crypto';
import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtemp,rm} from 'node:fs/promises';import {tmpdir} from 'node:os';import {resolve} from 'node:path';import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const keys=generateKeyPairSync('ed25519'),publicKey=keys.publicKey.export({format:'der',type:'spki'}).subarray(-32).toString('hex');
const proof=(profile,attempt=randomBytes(32).toString('hex'))=>{const claim={profile,attempt,until:String(Date.now()+60000)};return{claim,signature_hex:sign(null,Buffer.from(JSON.stringify(['oasis7.test.read-directory-storage-v3',claim])),keys.privateKey).toString('hex')};};
test('Read Directory SQL cold dual-family once partial unknown capacity and immutable business',async()=>{
 const persist=await mkdtemp(resolve(tmpdir(),'oasis7-read-directory-v3-'));
 const options=()=>({...convertV4MiniflareOptions({name:'read-directory-v3',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,modules:[{type:'ESModule',path:'tests/fixtures/read_directory_sql_v3_harness.mjs'},{type:'ESModule',path:'platform/read_directory_admission_sql_v3.mjs'}],durableObjects:{HARNESS:{className:'ReadDirectorySqlV3Harness',useSQLite:true}},bindings:{CAPACITY:'2',PUBLIC_KEY:publicKey}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
 let mf=new Miniflare(options());const call=(name,body)=>mf.dispatchFetch(`https://component.invalid/${name}`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
 try {
  const names=['normal','birth','header','once','unknown','tamper','execpartial'];const firsts=new Map();
  for(const name of names){assert.equal((await call(name,{op:'prepare'})).status,200);assert.equal((await call(name,{op:'claim',profile:'execution',proof:proof('execution')})).status,200);const first=proof('read');firsts.set(name,first);assert.equal((await call(name,{op:'claim',profile:'read',proof:first})).status,200);assert.equal((await call(name,{op:'ordinary'})).status,200);}
  await mf.dispose();mf=new Miniflare(options());
  assert.equal((await call('normal',{op:'ordinary'})).status,200,'both independent histories survive cold restart');
  const before=await(await call('normal',{op:'inventory'})).json();
  assert.equal((await call('normal',{op:'claim',profile:'read',proof:firsts.get('normal')})).status,409,'same callback cannot resend');assert.deepEqual(await(await call('normal',{op:'inventory'})).json(),before);
  assert.equal((await call('normal',{op:'claim',profile:'read',proof:proof('read')})).status,200);assert.equal((await call('normal',{op:'claim',profile:'read',proof:proof('read')})).status,409,'independent Read capacity');
  for(const kind of ['birth','header','once','unknown','tamper']){assert.equal((await call(kind,{op:'fault',profile:'read',kind})).status,200);const damaged=await(await call(kind,{op:'inventory'})).json();assert.equal((await call(kind,{op:'ordinary'})).status,409);assert.equal((await call(kind,{op:'claim',profile:'read',proof:proof('read')})).status,409);assert.deepEqual(await(await call(kind,{op:'inventory'})).json(),damaged,'rejection does not repair');}
  assert.equal((await call('execpartial',{op:'fault',profile:'execution',kind:'header'})).status,200);assert.equal((await call('execpartial',{op:'ordinary'})).status,409);assert.equal((await call('execpartial',{op:'claim',profile:'read',proof:proof('read')})).status,409,'Read cannot filter damaged Exec family');
 }finally{await mf.dispose();await rm(persist,{recursive:true,force:true});}
});
