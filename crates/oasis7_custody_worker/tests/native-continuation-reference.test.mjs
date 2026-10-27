import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';

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
    assert.equal(progress.stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/custody/managed-bindings/complete'],1);
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
    await runtime.restart('native-chain');
    assert.equal((await runtime.drive('stage-gap-original')).stage,'native_session_ready');
    assert.equal(runtime.nativeRequests()['/native-key/complete'],1);
  });
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
