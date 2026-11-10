import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';

async function assertNativeReady(runtime,progress){
  let diagnostic;
  if(progress.stage!=='native_session_ready'){
    const requests=runtime.nativeRequests();
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    diagnostic=JSON.stringify({stage:progress.stage,requests,native_results:saved.native_results,native_reads:saved.native_reads,transport_remaining:saved.transport_remaining});
  }
  assert.equal(progress.stage,'native_session_ready',diagnostic);
}

function diagnosticOwner(options){
  const production=fileURLToPath(new URL('../artifacts/owner/',import.meta.url));
  const diagnostic=fileURLToPath(new URL('../artifacts/approval-harness/',import.meta.url));
  const wrapper=fileURLToPath(new URL('./fixtures/native_owner_result_loss.mjs',import.meta.url));
  const owner=options.workers.find(worker=>worker.name==='owner');
  let wrappers=0;
  owner.modules=owner.modules.map(module=>{
    if(module.path.startsWith(production))return {...module,path:diagnostic+module.path.slice(production.length)};
    if(module.path===wrapper){
      const contents=readFileSync(wrapper,'utf8'),marker='../../artifacts/owner/entry.mjs';
      assert.equal(contents.split(marker).length-1,2);
      wrappers++;return {...module,contents:contents.replaceAll(marker,'../../artifacts/approval-harness/entry.mjs')};
    }
    return module;
  });
  assert.equal(wrappers,1);return options;
}

test('the public continuation rejects a stale entry observation before spending provision budget',{timeout:120000},async()=>{
  await withRoleRuntime('native-stage-gap-stale-entry',async runtime=>{
    const reply=await runtime.driveResponse('stage-gap-stale');
    assert.equal(reply.status,503);
    assert.equal((await reply.json()).code,'not_ready');
    assert.equal(runtime.nativeRequests()['/machine-registration/options']??0,0);
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.transport_remaining,'400');
    assert.equal(saved.slots.length,0);
  });
});

test('a committed stage may cross the HTTP observation window before a separately fresh next-stage grant',{timeout:120000},async()=>{
  await withRoleRuntime('native-stage-gap-success',async runtime=>{
    const progress=await runtime.drive('stage-gap-original');
    await assertNativeReady(runtime,progress);
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/complete'],1);
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    await runtime.restart('native-chain');
    assert.equal((await runtime.drive('stage-gap-original')).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
  });
});

test('actual Native current checks reuse exact Account bytes in synchronous SQL callbacks',{timeout:120000},async()=>{
  await withRoleRuntime('native-chain',async runtime=>{
    await assertNativeReady(runtime,await runtime.drive('native-current-reuse'));
    const observed=await runtime.readAccountRestoreCallbacks();
    assert.equal(observed.actor_identity_available,true);
    assert.equal(observed.attribution_complete,true);
    for(const event of ['native_source_verified','native_stage_claimed','native_oracle_authorized','native_stage_result_recorded']){
      const callbacks=observed.recent_callbacks.filter(callback=>callback.event===event);
      assert.ok(callbacks.length>0,`actual ${event} callback is required`);
      for(const callback of callbacks)assert.equal(callback.decode_count,0,`${event} must reuse the exact already-restored Account bytes`);
    }
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/complete'],1);
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
  },{configureOptions:diagnosticOwner});
});

for(const invalidation of ['revoke','expire']) {
  test(`an actual manager session ${invalidation} between stages cannot obtain another native grant`,{timeout:120000},async()=>{
    await withRoleRuntime('native-stage-gap-'+invalidation,async runtime=>{
      const reply=await runtime.driveResponse('stage-gap-invalid');
      // The binding fact is already committed. Rejection of the next grant is
      // reported as historical progress with a pending Login, never as ready.
      assert.equal(reply.status,200);
      const progress=await reply.json();
      assert.equal(progress.stage,'native_registered');
      assert.ok(progress.pending_operations.length>0);
      assert.equal(runtime.nativeRequests()['/native-key/complete']??0,0);
      await runtime.suspend();
      const saved=persistedRuntime(runtime.persist,runtime.account);
      assert.equal(saved.provision_stage,'native_registered');
      assert.ok(saved.native_binding);
      assert.equal(saved.native_reads.find(v=>v.stage==='login')?.reads,1,
        'the next stage reaches its paid fresh inspection and receives no grant');
    });
  });
}
