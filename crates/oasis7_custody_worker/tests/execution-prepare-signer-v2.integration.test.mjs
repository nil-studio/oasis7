import assert from 'node:assert/strict';
import test from 'node:test';
import {isDeepStrictEqual} from 'node:util';
import {readFileSync, mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname, resolve, join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = JSON.parse(readFileSync(resolve(root, 'artifacts/execution-prepare-signer-fixture.json'), 'utf8'));
assert.equal(fixture.profile, 'historical-core-fixture-not-paid-worker-current');
async function withFixture(run) {
  const persist = mkdtempSync(join(tmpdir(), 'oasis7-execution-signer-v2-'));
  const options = () => ({...convertV4MiniflareOptions({
    name:'execution-signer-fixture', modulesRoot:root, compatibilityDate:'2026-10-25', cf:false,
    modules:[
      {type:'ESModule',path:resolve(root,'artifacts/execution-signer-harness/entry.mjs')},
      {type:'ESModule',path:resolve(root,'artifacts/execution-signer-harness/index.js')},
      {type:'CompiledWasm',path:resolve(root,'artifacts/execution-signer-harness/index_bg.wasm')},
    ],
    durableObjects:{ROLE_KEYS:{className:'ExecutionPrepareSignerHarness',useSQLite:true}},
    serviceBindings:{EXECUTOR_PROVISION_KEYS:{name:'execution-signer-fixture',entrypoint:'RoleKeyProvision'}},
    bindings:{CUSTODY_TEST_SESSION_V2_ENABLED:'true',CUSTODY_ROLE_KEY_ENVIRONMENT:fixture.proof.permit.recipient_scope.context.identity.environment,
      CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS:JSON.stringify([fixture.pin]),
      CUSTODY_ACCOUNT_SERVICE_ISSUER_FLOOR:JSON.stringify(fixture.floor)},
  }), resourcePersistencePath:persist, unsafeDevRegistryPath:resolve(persist,'registry')});
  let mf = new Miniflare(options());
  const call = async (operation, extra={}) => {
    const {ROLE_KEYS:FIXTURE} = await mf.getBindings();
    const response = await FIXTURE.get(FIXTURE.idFromName(fixture.actor_name)).fetch('https://fixture.invalid/execution', {
      method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify({operation,
        proof:operation==='initialize'?fixture.initialize_proof:fixture.proof,
        bundle:fixture.bundle, fault:null, ...extra}),
    });
    const status = response.status;
    const value = status===200 ? await response.json() : (await response.arrayBuffer(), null);
    return {status,value};
  };
  const ok = async (operation, extra) => {const r=await call(operation,extra);assert.equal(r.status,200);return r.value;};
  const rejected = async (operation, extra) => assert.equal((await call(operation,extra)).status,409);
  const restart = async () => {await mf.dispose();mf=new Miniflare(options());};
  const serviceCalls = async()=>{const r=await mf.dispatchFetch("https://fixture.invalid/fixture-service-calls");assert.equal(r.status,200);return r.json();};
  const fixedRpc = async proof=>{const r=await mf.dispatchFetch("https://fixture.invalid/fixture-fixed-rpc",{method:"POST",body:JSON.stringify({proof,bundle:fixture.bundle})});if(r.status!==200){await r.arrayBuffer();throw new Error("fixed historical RPC rejected");}return r.json();};
  try {await run({call,ok,rejected,restart,fixedRpc,serviceCalls});} finally {await mf.dispose();rmSync(persist,{recursive:true,force:true});}
}
const unknown = {known:false};
const original = {known:true,original_digest:fixture.expected_original_digest};
test('execution signer actual Rust crypto commits one claim and cold lookup returns exact public original', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected,restart})=>{
    await ok('seed');const before=await ok('counts');
    await rejected('sign'); // no implicit V2 state bootstrap or old-signer fallback
    assert.deepEqual(await ok('counts'),before);
    await ok('initialize');assert.deepEqual(await ok('sign'),original);
    const committed=await ok('counts');assert.equal(committed.counts.once,1);assert.equal(committed.counts.originals,1);
    assert.equal(committed.slot_digest,before.slot_digest);
    await rejected('sign');assert.deepEqual(await ok('counts'),committed);
    await restart();assert.deepEqual(await ok('lookup'),original);
    await rejected('sign');assert.equal((await ok('counts')).counts.once,1);
  });
});
test('execution signer actual SQLite UPDATE CHECK failure rolls back INSERT and header atomically', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected})=>{
    await ok('seed');await ok('initialize');await ok('fault',{fault:'claim_update_check'});
    const before=await ok('counts');assert.equal(before.counts.update_faults,0);
    await rejected('sign');const after=await ok('counts');
    assert.equal(after.counts.update_faults,1);assert.equal(after.counts.once,0);assert.equal(after.counts.originals,0);
    assert.equal(after.slot_digest,before.slot_digest);
  });
});
test('execution signer bad KEK keeps committed once and unknown cold lookup never reopens or resigns', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected,restart})=>{
    await ok('seed');await ok('initialize');await rejected('sign',{fault:'wrong_kek'});
    const saved=await ok('counts');assert.equal(saved.counts.once,1);assert.equal(saved.counts.originals,0);
    await restart();assert.deepEqual(await ok('lookup'),unknown);
    await rejected('sign');assert.equal((await ok('counts')).counts.once,1);
  });
});
test('execution signer private Current revocation after real open preserves once without signature publication', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected})=>{
    await ok('seed');await ok('initialize');await rejected('sign',{fault:'frozen_after_open'});
    const saved=await ok('counts');assert.equal(saved.counts.once,1);assert.equal(saved.counts.originals,0);
    await rejected('sign');assert.deepEqual(await ok('lookup'),unknown);
  });
});
test('execution signer current credential generation mismatch consumes physical once and never publishes', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected})=>{
    await ok('seed');await ok('initialize');await rejected('sign',{fault:'credential_generation_changed'});
    const saved=await ok('counts');assert.equal(saved.counts.once,1);assert.equal(saved.counts.originals,0);
    await rejected('sign');assert.deepEqual(await ok('lookup'),unknown);
  });
});
test('execution signer failed original INSERT remains durable unknown without releasing physical once', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected,restart})=>{
    await ok('seed');await ok('initialize');await ok('fault',{fault:'publication_check'});
    assert.equal((await ok('counts')).counts.publication_faults,0);
    await rejected('sign');const saved=await ok('counts');assert.equal(saved.counts.once,1);assert.equal(saved.counts.originals,0);
    assert.equal(saved.counts.publication_faults,1);
    await restart();assert.deepEqual(await ok('lookup'),unknown);await rejected('sign');
  });
});
test('execution signer rejects wrong proof, init/sign cross-use and unknown input before any claim', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected})=>{
    await ok('seed');await ok('initialize');const before=await ok('counts');
    const bad=structuredClone(fixture.proof);bad.signature_hex='00'.repeat(64);
    await rejected('sign',{proof:bad});await rejected('sign',{proof:fixture.initialize_proof});
    await rejected('initialize',{proof:fixture.proof});await rejected('sign',{verified:true});
    const changed=structuredClone(fixture.bundle);changed.stage_operation_id+='-different';
    await rejected('sign',{bundle:changed});assert.deepEqual(await ok('counts'),before);
    await rejected('production_expired');assert.deepEqual(await ok('counts'),before);
  });
});
test('execution signer partial state and corrupted chain fail closed across restart', {timeout:120000}, async()=>{
  for (const fault of ['remove_state','corrupt_head']) await withFixture(async({ok,rejected,restart})=>{
    await ok('seed');await ok('initialize');await ok('fault',{fault});await restart();
    await rejected('sign');await rejected('initialize');await rejected('lookup');
  });
});

test('paid fixed Prepare profile executes two actual service RPCs and two actual Actor requests', {timeout:120000}, async()=>{
  await withFixture(async({ok,fixedRpc,serviceCalls})=>{
    await ok('seed');
    const before=await ok('counts'), serviceBefore=await serviceCalls();
    const initialize=await fixedRpc(fixture.initialize_proof);
    assert.equal(initialize.original,null);
    const signed=await fixedRpc(fixture.proof);
    assert.equal(isDeepStrictEqual(signed.original,fixture.expected_original),true);
    const after=await ok('counts');
    assert.equal((await serviceCalls())-serviceBefore,2);
    assert.equal(after.physical_actor_calls-before.physical_actor_calls,2);
    assert.equal(after.counts.initializations,1); assert.equal(after.counts.once,1); assert.equal(after.counts.originals,1);
    assert.equal(new Set([...initialize.port_ids,...signed.port_ids]).size,4);
  });
});
test('fixed gateway validates the actual signature before any Actor request and cold init replay rejects', {timeout:120000}, async()=>{
  await withFixture(async({ok,rejected,restart,fixedRpc})=>{
    await ok('seed'); const before=await ok('counts');
    const wrong=structuredClone(fixture.proof);wrong.signature_hex='00'.repeat(64);
    await assert.rejects(()=>fixedRpc(wrong));
    assert.equal((await ok('counts')).physical_actor_calls,before.physical_actor_calls);
    await fixedRpc(fixture.initialize_proof);
    await restart();
    await rejected('initialize');
    assert.equal((await ok('counts')).counts.initializations,1);
    assert.equal((await ok('counts')).counts.once,0);
  });
});
