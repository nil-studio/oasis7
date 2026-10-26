import assert from 'node:assert/strict';
import test from 'node:test';
import {sign} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdtempSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare,convertV4MiniflareOptions} from 'miniflare';
import {fixtureRecoveryKeys as keys,fixtureRecoveryPolicy as policy,encodeFixtureRecoveryCbor as cbor,signFixtureRecoveryActivation as signed} from "./fixtures/fence_activation_signer.mjs";
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
async function runtime(run,enabled=true,mode='normal',configuredPolicy=policy) {
  const persist=mkdtempSync(resolve(tmpdir(),'oasis7-fence-activation-'));
  let mf;
  const start=()=>new Miniflare({...convertV4MiniflareOptions({workers:[{
    name:'fence',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
    modules:[...(mode==='normal'?[]:[{type:'ESModule',path:resolve(root,'tests/fixtures/fence_fault_actor.mjs')}]),{type:'ESModule',path:resolve(root,'artifacts/recovery-fence/entry.mjs')},{type:'ESModule',path:resolve(root,'artifacts/recovery-fence/index.js')},{type:'CompiledWasm',path:resolve(root,'artifacts/recovery-fence/index_bg.wasm')}],
    bindings:{CUSTODY_INITIAL_SERVICE_EPOCH:'0',...(mode==='commit-loss'?{FIXTURE_FAIL_COMMIT:'true'}:{}),...(mode==='slow-verify'?{FIXTURE_VERIFY_DELAY:'200'}:{}),...(enabled?{CUSTODY_RECOVERY_CONTROL_ENABLED:'true',CUSTODY_RECOVERY_AUTHORITY_POLICY:JSON.stringify(configuredPolicy)}:{})},
    durableObjects:{FENCES:{className:'RecoveryFence',useSQLite:true}},
  },{name:'controller',modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/fence_controller.mjs')}],
    serviceBindings:{CONTROL:{name:'fence',entrypoint:'RecoveryFenceControl'},READER:{name:'fence',entrypoint:'RecoveryFenceReader'},...(mode==='normal'?{}:{STATS:{name:'fence',entrypoint:'FixtureStats'}})}}]}),resourcePersistencePath:persist,unsafeDevRegistryPath:resolve(persist,'registry')});
  try {
    mf=start();
    const call=async(path,body)=>{const worker=await mf.getWorker('controller');return worker.fetch(`https://fixture.test/${path}`,{method:'POST',...(body?{body:JSON.stringify(body)}:{})});};
    await run({call,restart:async()=>{await mf.dispose();mf=start();},deployed:async()=>mf.getWorker('fence'),corruptAudit:async kind=>{
      await mf.dispose();
      const count=execFileSync('python3',['-c',`
import sqlite3,sys
from pathlib import Path
changed=0
for path in Path(sys.argv[1]).rglob('*.sqlite'):
 db=sqlite3.connect(path)
 try:
  rows=db.execute("SELECT record_id,sequence FROM custody_audit WHERE record_id='recovery-fence-ledger-v1'").fetchall()
  if rows:
   if sys.argv[2]=='payload': db.execute("UPDATE custody_audit SET payload='{}' WHERE record_id=? AND sequence=?",rows[0])
   elif sys.argv[2]=='sequence': db.execute("UPDATE custody_audit SET sequence='99' WHERE record_id=? AND sequence=?",rows[0])
   elif sys.argv[2]=='foreign': db.execute("INSERT INTO custody_audit(record_id,sequence,payload) VALUES('foreign-history','1','{}')")
   else: raise RuntimeError('unknown fixture corruption')
   db.commit();changed+=1
 except sqlite3.OperationalError: pass
 db.close()
print(changed)
`,persist,kind],{encoding:'utf8'}).trim();
      assert.equal(count,'1','exactly the independent authority audit was corrupted');mf=start();
    }});
  } finally {await mf?.dispose();rmSync(persist,{recursive:true,force:true});}
}
test('actual independent fence consumes one quorum original and never thaws on historical retry', {timeout:120000}, async()=>{
  await runtime(async r=>{
    assert.deepEqual(await (await r.call('read')).json(),{epoch:'0',disabled:true});
    const before=await (await r.call('describe')).json();
    const original=signed(before);
    const activated=await r.call('activate',original);assert.equal(activated.status,200);
    assert.deepEqual((await activated.json()).fence,{epoch:'1',disabled:false});
    await r.restart();
    assert.deepEqual(await (await r.call('read')).json(),{epoch:'1',disabled:false});
    await r.call('disable');
    const retry=await r.call('activate',original);assert.equal(retry.status,200);
    const retained=await retry.json();assert.equal(retained.fence.disabled,true);assert.equal(retained.activations.length,1);
    const next=signed(await (await r.call('describe')).json(),'activation-two');
    const advanced=await (await r.call('activate',next)).json();
    assert.deepEqual(advanced.frozen_epochs,['0','1']);assert.equal(advanced.fence.epoch,'2');
  });
});
test('a failed actual SQL commit rolls back audit, ledger and the legacy fence together',{timeout:120000},async()=>{
  await runtime(async r=>{
    const before=await (await r.call('describe')).json();const original=signed(before);
    assert.equal((await r.call('activate',original)).status,409);
    assert.deepEqual(await (await r.call('describe')).json(),before);
    assert.deepEqual(await (await r.call('read')).json(),{epoch:'0',disabled:true});
    assert.equal((await r.call('activate',original)).status,200);
    const after=await (await r.call('describe')).json();assert.equal(after.ledger.revision,'1');
  },true,'commit-loss');
});
test('clock expiry during actual cryptography is rejected by final SQL admission',{timeout:120000},async()=>{
  await runtime(async r=>{
    const before=await (await r.call('describe')).json();const original=signed(before);
    original.manifest.expires_at=String(Date.now()+150);
    const bytes=cbor(['oasis7.custody.recovery-activation.v1',original.manifest]);
    original.signatures=keys.map((key,i)=>({key_id:`offline-${i}`,signature:sign(null,bytes,key).toString('base64url')}));
    assert.equal((await r.call('activate',original)).status,409);
    assert.equal((await (await r.call('stats')).json()).verification_calls,2,'both real crypto checks finished before final expiry');
    assert.deepEqual(await (await r.call('describe')).json(),before);
  },true,'slow-verify');
});
test('concurrent different signed operations against one current digest cannot both advance',{timeout:120000},async()=>{
  await runtime(async r=>{
    const before=await (await r.call('describe')).json();
    const replies=await Promise.all([r.call('activate',signed(before,'concurrent-one')),r.call('activate',signed(before,'concurrent-two'))]);
    assert.deepEqual(replies.map(v=>v.status).sort(),[200,409]);
    const after=await (await r.call('describe')).json();assert.equal(after.ledger.activations.length,1);assert.equal(after.ledger.fence.epoch,'1');
  });
});
test('a low-order configured point cannot forge an independent quorum signature',{timeout:120000},async()=>{
  const weakPolicy=structuredClone(policy);
  weakPolicy.keys[0].public_key='01'+'00'.repeat(31);
  await runtime(async r=>{
    const before=await (await r.call('describe')).json();const forged=signed(before);
    forged.signatures[0].signature=Buffer.concat([Buffer.from(weakPolicy.keys[0].public_key,'hex'),Buffer.alloc(32)]).toString('base64url');
    assert.equal((await r.call('activate',forged)).status,409);
    assert.equal((await (await r.call('stats')).json()).verification_calls,0,'strict Rust rejection precedes WebCrypto');
    assert.deepEqual(await (await r.call('describe')).json(),before);
  },true,'observe',weakPolicy);
});
for(const kind of ['payload','sequence','foreign']) {
  test(`partial ${kind} audit corruption cannot preserve an apparently active fence`,{timeout:120000},async()=>{
    await runtime(async r=>{
      const original=signed(await (await r.call('describe')).json());
      assert.equal((await r.call('activate',original)).status,200);
      assert.deepEqual(await (await r.call('read')).json(),{epoch:'1',disabled:false});
      await r.corruptAudit(kind);
      assert.equal((await r.call('read')).status,409);
      assert.equal((await r.call('describe')).status,409);
      assert.equal((await r.call('activate',original)).status,409,'exact retry cannot hide damaged history');
    });
  });
}
test('nonquorum, duplicate signer, tampered physical scope and default HTTP ingress never activate', {timeout:120000},async()=>{
  await runtime(async r=>{
    const before=await (await r.call('describe')).json();
    const original=signed(before);
    const missing=structuredClone(original);missing.signatures.pop();assert.equal((await r.call('activate',missing)).status,409);
    const duplicate=structuredClone(original);duplicate.signatures[1]=duplicate.signatures[0];assert.equal((await r.call('activate',duplicate)).status,409);
    const tampered=structuredClone(original);tampered.manifest.object_id='other-physical-object';assert.equal((await r.call('activate',tampered)).status,409);
    assert.deepEqual(await (await r.call('describe')).json(),before);
    const publicReply=await (await r.deployed()).fetch('https://fixture.test/internal/recovery-activate',{method:'POST',body:JSON.stringify(original)});
    assert.equal(publicReply.status,404);
    assert.deepEqual(await (await r.call('read')).json(),{epoch:'0',disabled:true});
  });
  await runtime(async r=>{
    assert.equal((await r.call('describe')).status,409);
    assert.deepEqual(await (await r.call('read')).json(),{epoch:'0',disabled:true});
  },false);
});
