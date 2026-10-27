import assert from 'node:assert/strict';
import {test} from 'node:test';
import {persistedRuntime,withRoleRuntime} from './fixtures/role_runtime_native_support.mjs';

test('actual role factory rejects fixed-role and canonical-request-digest mismatches before vault creation', {timeout:120000}, async()=>{
  for(const mode of ['wrong-role','wrong-digest']) await withRoleRuntime(mode,async runtime=>{
    const pending=await runtime.drive('mismatched-permit-drive');
    assert.equal(pending.stage,'approved');
    assert.equal(pending.pending_operations.length,1);
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.deepEqual(saved.slots,[],'a mismatched authority reply cannot create a key');
    assert.equal(saved.dispatches.length,1);
    assert.equal(saved.dispatches[0].reads,1,'factory must fail before the vault pre-read');
    assert.equal(saved.dispatches[0].has_result,false);
    assert.equal(saved.inspections,1);
    assert.equal(runtime.nativeCalls(),2,'one proof consume and one original source inspection only');
  });
});

test('fourth actual Account oracle read cannot renew a permit and Executor default RPC remains unavailable', {timeout:120000}, async()=>withRoleRuntime('normal',async runtime=>{
  const ready=await runtime.drive('quota-drive');
  assert.equal(ready.stage,'role_keys_ready');
  const before=persistedRuntime(runtime.persist,runtime.account);
  const dispatch=before.dispatches.find(value=>value.role==='executor');
  assert.ok(dispatch);
  assert.equal(dispatch.reads,3);
  assert.ok(BigInt(Date.now())<BigInt(dispatch.expires_at),'test must exercise exhausted quota before expiry');
  assert.equal((await runtime.oracle('identity',dispatch.request)).status,409,'a fixed-role alias cannot inspect another role permit');
  assert.equal((await runtime.oracle('executor',dispatch.request)).status,409,'the fourth real permit query must fail');
  const after=persistedRuntime(runtime.persist,runtime.account);
  const original=after.dispatches.find(value=>value.request.permit_id===dispatch.request.permit_id);
  assert.equal(original.reads,3);
  assert.equal(original.expires_at,dispatch.expires_at,'failed reads cannot renew the original deadline');
  assert.equal(after.inspections,before.inspections,'quota failure cannot create a fresh source observation');
  assert.equal(runtime.nativeCalls(),2);
  for(const kind of ['generated','named','owner-permit']) {
    const response=await runtime.defaultExecutor(kind,dispatch.request);
    assert.equal(response.status,409);
    assert.equal((await response.json()).missing_rpc,true,'Executor default binding must not expose '+kind+' RPC');
  }
  assert.equal(runtime.nativeCalls(),2,'default-method probes must not reach the native authority');
}));

test('actual role runtime rechecks the independent fence after sealing and cannot publish a candidate', {timeout:120000}, async()=>withRoleRuntime('freeze-after-seal',async runtime=>{
  const pending=await runtime.drive('post-seal-freeze-drive');
  assert.equal(pending.stage,'approved');
  const saved=persistedRuntime(runtime.persist,runtime.account);
  assert.deepEqual(saved.slots,[],'the newly sealed candidate must not reach SQLite after fence closure');
  assert.equal(saved.dispatches.length,1);
  assert.equal(saved.dispatches[0].reads,3,'freeze is injected after the actual post-seal permit read');
  assert.equal(saved.dispatches[0].has_result,false);
}));

test('actual runtime recovery rejects wrong KEK and damaged ciphertext without replacing a committed unknown key', {timeout:180000}, async()=>{
  for(const fault of ['wrong-kek','ciphertext']) await withRoleRuntime('lost',async runtime=>{
    const pending=await runtime.drive('lost-before-recovery');
    assert.equal(pending.stage,'approved');
    const committed=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(committed.slots.length,1);
    assert.equal(committed.slots[0].role,'identity');
    assert.equal(committed.dispatches[0].has_result,false);
    await runtime.suspend();
    let expected=committed.slots;
    if(fault==='ciphertext') {
      expected=persistedRuntime(runtime.persist,runtime.account,true).slots;
      assert.notEqual(expected[0].payload_digest,committed.slots[0].payload_digest,'the test must change the ciphertext');
      assert.equal(expected[0].public_key,committed.slots[0].public_key);
    }
    await runtime.restart(fault==='wrong-kek'?'wrong-kek':'normal');
    const blocked=await runtime.drive('fresh-failed-recovery');
    assert.equal(blocked.stage,'approved');
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.deepEqual(saved.slots,expected,'failure must preserve the original slot, including damaged bytes');
    assert.ok(saved.dispatches.every(value=>!value.has_result));
    if(fault==='wrong-kek') {
      await runtime.restart('normal');
      const recovered=await runtime.drive('fresh-correct-key-recovery');
      assert.equal(recovered.stage,'role_keys_ready');
      const final=persistedRuntime(runtime.persist,runtime.account);
      assert.equal(final.slots.find(value=>value.role==='identity').public_key,committed.slots[0].public_key,'correct-key recovery must retain the original public key');
    }
  });
});
