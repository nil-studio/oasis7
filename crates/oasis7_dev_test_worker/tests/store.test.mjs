import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
function runtime(persist,kek='17'.repeat(32)){
 const modules=['tests/fixtures/store.mjs','platform/store.mjs','platform/native.mjs'].map(path=>({type:'ESModule',path:resolve(root,path)}));
 return new Miniflare({...convertV4MiniflareOptions({name:'dev-test-store',modules,modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,durableObjects:{STORE:{className:'StoreHarness',useSQLite:true}},bindings:{DEV_TEST_STATE_KEK:kek}}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
}
async function call(mf,cmd){const {STORE}=await mf.getBindings();const r=await STORE.get(STORE.idFromName('one')).fetch('https://fixture.test',{method:'POST',body:JSON.stringify(cmd)});return {status:r.status,...await r.json()};}
test('SQLite atomic CAS, restart and missing policy cannot reset budget',{timeout:60000},async()=>{
 const dir=mkdtempSync(resolve(tmpdir(),'oasis7-worker-store-'));let mf=runtime(dir);
 try{
  assert.equal((await call(mf,{op:'load',key:'policy-meta'})).value,'');
  assert.equal((await call(mf,{op:'meta',old:'',next:'meta1'})).status,200);
  assert.equal((await call(mf,{op:'ledger',metaOld:'meta1',metaNext:'meta2',key:'ledger',old:'',next:'used1'})).status,200);
  assert.equal((await call(mf,{op:'ledger',metaOld:'meta1',metaNext:'bad',key:'ledger',old:'used1',next:'bad'})).status,409);
  assert.equal((await call(mf,{op:'load',key:'ledger'})).value,'used1');
  await mf.dispose();mf=runtime(dir);
  assert.equal((await call(mf,{op:'load',key:'policy-meta'})).value,'meta2');
  await call(mf,{op:'delete',key:'policy-meta'});
  assert.equal((await call(mf,{op:'load',key:'policy-meta'})).status,409);
  assert.equal((await call(mf,{op:'meta',old:'',next:'reset'})).status,409);
  assert.equal((await call(mf,{op:'load',key:'ledger'})).value,'used1');
 }finally{await mf.dispose();rmSync(dir,{recursive:true,force:true});}
});
test('AES-GCM immutable originals preserve full u64 and wrong key never replaces them',{timeout:60000},async()=>{
 const dir=mkdtempSync(resolve(tmpdir(),'oasis7-worker-private-'));let mf=runtime(dir);const key='native:scope:action:original';const original='{"height":18446744073709551615,"goal":"私有"}';
 try{
  await call(mf,{op:'meta',old:'',next:'meta'});
  const raced=await Promise.all([call(mf,{op:'private',key,next:original}),call(mf,{op:'private',key,next:'different'})]);
  assert.equal(raced.filter(r=>r.status===200).length,1);
  const winner=(await call(mf,{op:'readPrivate',key})).value;
  assert.ok([original,'different'].includes(winner));
  assert.equal((await call(mf,{op:'private',key,next:winner})).status,200);
  const ciphertext=(await call(mf,{op:'load',key})).value;
  assert.ok(!ciphertext.includes('私有'));assert.ok(!ciphertext.includes('height'));
  await mf.dispose();mf=runtime(dir,'18'.repeat(32));
  assert.equal((await call(mf,{op:'readPrivate',key})).status,409);
  assert.equal((await call(mf,{op:'private',key,next:winner})).status,409);
  assert.equal((await call(mf,{op:'load',key})).value,ciphertext);
 }finally{await mf.dispose();rmSync(dir,{recursive:true,force:true});}
});
