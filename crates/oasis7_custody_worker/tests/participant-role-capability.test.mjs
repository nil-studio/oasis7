import assert from 'node:assert/strict';
import test from 'node:test';
import {withRoleRuntime,persistedRuntime} from './fixtures/role_runtime_native_support.mjs';

test('actual named participant entries reject cross-role actions before consuming shared oracle or callback capacity', {timeout:180000}, async()=>{
  await withRoleRuntime('native-participant-role-capability',async runtime=>{
    const progress=await runtime.drive('participant-role-capability');
    assert.equal(progress.stage,'participant_bound',JSON.stringify({progress,requests:runtime.nativeRequests()}));
    const response=await runtime.participantRoleProbes();
    assert.equal(response.status,200);
    assert.deepEqual(await response.json(),{
      identity_sign_bundle_attempts:1,
      identity_sign_bundle_rejected:true,
      owner_record_policy_attempts:1,
      owner_record_policy_rejected:true,
    });
    await runtime.suspend();
    const saved=persistedRuntime(runtime.persist,runtime.account);
    assert.equal(saved.participants.length,1);
    const original=saved.participants[0];
    assert.equal(original.reads,8,'Identity SignBundle rejection must leave all eight legal oracle reads available');
    assert.deepEqual(original.callbacks,['admission','final','policy'],
      'Owner RecordPolicy rejection must not preclaim the legal policy callback');
    assert.equal(original.has_result,true,'all fixed-role production calls must still settle after the two probes');
    assert.equal(runtime.nativeRequests()['/participant/policy-original'],1);
    assert.equal(runtime.nativeRequests()['/participant/authorize'],1);
    assert.equal(runtime.nativeRequests()['/participant/submit'],1);
  });
});
